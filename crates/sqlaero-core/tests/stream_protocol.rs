//! The shared parser stops at headers and complete-row resource boundaries.

use mysql_common::constants::{CapabilityFlags, ColumnType, StatusFlags};
use mysql_common::proto::MySerialize;
use sqlaero_core::memory::Buffer;
use sqlaero_core::protocol::framing::{self, Sequence};
use sqlaero_core::result::text::{RowBatch, StreamEvent};
use sqlaero_core::result::value::RowEncoding;
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::{Channel, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::net::TcpListener;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::time::Duration;

fn request(binary: bool) -> Vec<u8> {
    if binary {
        vec![0x17, 1, 0, 0, 0, 0, 1, 0, 0, 0]
    } else {
        b"\x03SELECT value".to_vec()
    }
}
fn row(binary: bool, value: u8) -> Vec<u8> {
    if binary {
        let mut bytes = vec![0, 0];
        bytes.extend_from_slice(&i64::from(value).to_le_bytes());
        bytes
    } else {
        vec![1, b'0' + value]
    }
}
fn next(
    query: &mut Query,
    channel: &mut Channel,
    budget: &mut IoBudget,
) -> Result<StreamEvent, QueryError> {
    loop {
        match query.advance_stream(channel, budget, NonZeroUsize::new(1).unwrap())? {
            Step::Ready(event) => return Ok(event),
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness"),
        }
    }
}
fn check(batch: &RowBatch, expected: u8, binary: bool) {
    assert_eq!(batch.row_count, 1);
    assert_eq!(
        batch.encoding,
        if binary {
            RowEncoding::Binary
        } else {
            RowEncoding::Text
        }
    );
    let raw = batch.row(0).unwrap()[0].get(&batch.bytes).unwrap().unwrap();
    if binary {
        assert_eq!(raw, &i64::from(expected).to_le_bytes());
    } else {
        assert_eq!(raw, &[b'0' + expected]);
    }
}

#[test]
fn header_and_row_boundaries_allow_controlled_smaller_fetch_without_read_ahead() {
    for binary in [false, true] {
        for legacy in [false, true] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let (release, wait) = mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut sequence = Sequence::default();
                assert_eq!(
                    framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
                    request(binary)
                );
                framing::write_message(&mut socket, &[1], &mut sequence, 4096).unwrap();
                let mut column = Vec::new();
                mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_LONGLONG)
                    .with_name(b"value")
                    .serialize(&mut column);
                framing::write_message(&mut socket, &column, &mut sequence, 4096).unwrap();
                if legacy {
                    framing::write_message(&mut socket, &[0xfe, 0, 0, 2, 0], &mut sequence, 4096)
                        .unwrap();
                }
                // No first row exists on the connection until the Header
                // event returned to the host. A pre-read cannot pass this.
                wait.recv_timeout(Duration::from_secs(3)).unwrap();
                framing::write_message(&mut socket, &row(binary, 7), &mut sequence, 4096).unwrap();
                // Nor can a one-row fetch pre-read its next complete row.
                wait.recv_timeout(Duration::from_secs(3)).unwrap();
                framing::write_message(&mut socket, &row(binary, 8), &mut sequence, 4096).unwrap();
                let end = if legacy {
                    vec![0xfe, 0, 0, 2, 0]
                } else {
                    vec![0xfe, 0, 0, 2, 0, 0, 0]
                };
                framing::write_message(&mut socket, &end, &mut sequence, 4096).unwrap();
            });
            let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
            let mut budget = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_secs(2)),
                None,
                None,
            );
            while !channel.connected().unwrap() {
                channel.wait(Direction::Write, &mut budget).unwrap();
            }
            let mut caps = CapabilityFlags::CLIENT_PROTOCOL_41;
            if !legacy {
                caps.insert(CapabilityFlags::CLIENT_DEPRECATE_EOF);
            }
            let mut query = Query::new(request(binary), caps, 4096, 4096, 1_000_000)
                .unwrap()
                .with_streaming()
                .unwrap();
            let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("header first");
            };
            assert_eq!(header.columns.len(), 1);
            assert_eq!(
                header.metadata_status,
                legacy.then_some(StatusFlags::SERVER_STATUS_AUTOCOMMIT)
            );
            assert!(query.at_stream_boundary());
            assert!(!query.is_finished());
            assert!(query.take_confirmation().is_none());
            release.send(()).unwrap();
            let StreamEvent::Rows(first) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("one first row");
            };
            check(&first, 7, binary);
            let mut pending = Buffer::tracked(header.columns.scope());
            pending.push(first).unwrap();
            let before = header.columns.scope().used();
            // The host reserves an accumulation slot before the next native
            // read. Failing here leaves the Query and complete prefix intact.
            assert!(pending.reserve(1_000_000).is_err());
            assert_eq!(header.columns.scope().used(), before);
            assert!(query.at_stream_boundary());
            assert!(!query.is_finished());
            check(&pending[0], 7, binary);
            pending.clear(); // consume the smaller already-owned batch
            release.send(()).unwrap();
            let StreamEvent::Rows(second) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("one next row");
            };
            check(&second, 8, binary);
            assert!(!query.is_finished(), "the row does not invent a terminator");
            assert!(query.take_confirmation().is_none());
            let StreamEvent::End {
                tail,
                completion,
                total_rows,
            } = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("actual terminal packet");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, Some(2));
            assert_eq!(
                completion.status_flags(),
                StatusFlags::SERVER_STATUS_AUTOCOMMIT
            );
            assert!(query.is_finished());
            assert!(query.take_confirmation().is_some());
            check(&second, 8, binary);
            server.join().unwrap();
        }
    }
}
