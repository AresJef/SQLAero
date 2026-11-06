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
                metadata_status,
            } = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("actual terminal packet");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, Some(2));
            assert_eq!(metadata_status, header.metadata_status);
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

#[test]
fn legacy_out_metadata_keeps_the_real_tail_pending_without_rewriting_end_flags() {
    for streaming in [false, true] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = mpsc::channel();
        let metadata_status = StatusFlags::SERVER_STATUS_AUTOCOMMIT
            | StatusFlags::SERVER_MORE_RESULTS_EXISTS
            | StatusFlags::SERVER_PS_OUT_PARAMS;
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut sequence = Sequence::default();
            assert_eq!(
                framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
                request(true)
            );
            let mut column = Vec::new();
            mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_LONGLONG)
                .with_name(b"out_value")
                .serialize(&mut column);
            let flags = metadata_status.bits().to_le_bytes();
            for payload in [
                vec![1],
                column,
                vec![0xfe, 0, 0, flags[0], flags[1]],
                row(true, 9),
                vec![0xfe, 0, 0, 2, 0],
            ] {
                framing::write_message(&mut socket, &payload, &mut sequence, 4096).unwrap();
            }
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            framing::write_message(&mut socket, &[0, 0, 0, 2, 0, 0, 0], &mut sequence, 4096)
                .unwrap();
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
        let query = Query::new(
            request(true),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_PS_MULTI_RESULTS,
            4096,
            4096,
            1_000_000,
        )
        .unwrap();
        let mut query = if streaming {
            query.with_streaming().unwrap()
        } else {
            query
        };
        if streaming {
            let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("OUT header");
            };
            assert_eq!(header.metadata_status, Some(metadata_status));
            let StreamEvent::Rows(rows) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("OUT row");
            };
            check(&rows, 9, true);
            let StreamEvent::End {
                completion,
                metadata_status: actual_metadata,
                ..
            } = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("actual OUT EOF");
            };
            assert_eq!(actual_metadata, Some(metadata_status));
            assert_eq!(
                completion.status_flags(),
                StatusFlags::SERVER_STATUS_AUTOCOMMIT
            );
        } else {
            let response = loop {
                if let Step::Ready(response) = query.advance(&mut channel, &mut budget).unwrap() {
                    break response;
                }
            };
            assert_eq!(
                response.completion().status_flags(),
                StatusFlags::SERVER_STATUS_AUTOCOMMIT
            );
            let sqlaero_core::result::text::Response::Rows(rows) = response else {
                panic!("buffered OUT row");
            };
            assert_eq!(rows.metadata_status, Some(metadata_status));
        }
        assert!(
            !query.is_finished(),
            "real metadata declared the independent CALL tail despite cleared row EOF flags"
        );
        assert!(
            query.take_confirmation().is_none(),
            "closing OUT EOF cannot certify the full command"
        );
        release.send(()).unwrap();
        if streaming {
            let StreamEvent::End {
                tail,
                total_rows,
                completion,
                ..
            } = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("actual command tail");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, None);
            assert_eq!(
                completion.status_flags(),
                StatusFlags::SERVER_STATUS_AUTOCOMMIT
            );
        } else {
            loop {
                if let Step::Ready(response) = query.advance(&mut channel, &mut budget).unwrap() {
                    assert!(matches!(
                        response,
                        sqlaero_core::result::text::Response::Command(_)
                    ));
                    break;
                }
            }
        }
        assert!(query.is_finished());
        assert!(query.take_confirmation().is_some());
        server.join().unwrap();
    }
}

#[test]
fn large_fetch_all_yields_complete_rows_at_the_existing_work_quantum() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
            request(false)
        );
        let mut column = Vec::new();
        mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
            .with_name(b"value")
            .with_column_length(100_000)
            .serialize(&mut column);
        framing::write_message(&mut socket, &[1], &mut sequence, 4096).unwrap();
        framing::write_message(&mut socket, &column, &mut sequence, 4096).unwrap();
        let mut row = vec![0xfd, 0xa0, 0x86, 1];
        row.resize(100_004, b'x');
        for _ in 0..3 {
            framing::write_message(&mut socket, &row, &mut sequence, 128 * 1024).unwrap();
        }
        framing::write_message(&mut socket, &[0xfe, 0, 0, 2, 0, 0, 0], &mut sequence, 4096)
            .unwrap();
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
    let mut query = Query::new(
        request(false),
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
        4096,
        128 * 1024,
        1_000_000,
    )
    .unwrap()
    .with_streaming()
    .unwrap();
    let limit = NonZeroUsize::new(usize::MAX).unwrap();
    let mut header = false;
    let mut batches = 0;
    loop {
        match query
            .advance_stream(&mut channel, &mut budget, limit)
            .unwrap()
        {
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness"),
            Step::Ready(StreamEvent::Header(_)) => {
                assert!(!header);
                header = true;
            }
            Step::Ready(StreamEvent::Rows(rows)) => {
                assert!(header);
                batches += 1;
                assert_eq!(rows.row_count, 3);
                assert!(rows.bytes.len() >= 256 * 1024);
                for index in 0..3 {
                    let bytes = rows.row(index).unwrap()[0]
                        .get(&rows.bytes)
                        .unwrap()
                        .unwrap();
                    assert_eq!(bytes.len(), 100_000);
                    assert!(bytes.iter().all(|byte| *byte == b'x'));
                }
                assert!(!query.is_finished());
                assert!(query.take_confirmation().is_none());
                assert!(query.at_stream_boundary());
            }
            Step::Ready(StreamEvent::End {
                tail, total_rows, ..
            }) => {
                assert!(tail.is_none());
                assert_eq!(total_rows, Some(3));
                assert_eq!(batches, 1);
                assert!(query.is_finished());
                break;
            }
        }
    }
    server.join().unwrap();
}

#[test]
fn row_storage_exhaustion_keeps_complete_pending_for_explicit_smaller_fetch() {
    use std::io::Write;
    use std::sync::Arc;
    eprintln!(
        "native sizes Query={}, Receiver={}",
        std::mem::size_of::<Query>(),
        std::mem::size_of::<sqlaero_core::result::text::Receiver>()
    );
    for binary in [false, true] {
        for fragmented in [false, true] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut sequence = Sequence::default();
                assert_eq!(
                    framing::read_message(&mut socket, &mut sequence, 8192).unwrap(),
                    request(binary)
                );
                framing::write_message(&mut socket, &[1], &mut sequence, 8192).unwrap();
                let mut column = Vec::new();
                mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
                    .with_name(b"value")
                    .with_column_length(4096)
                    .serialize(&mut column);
                framing::write_message(&mut socket, &column, &mut sequence, 8192).unwrap();
                for value in 1..=8_u8 {
                    let mut row = if binary { vec![0, 0] } else { Vec::new() };
                    row.extend_from_slice(&[0xfc, 0, 16]);
                    row.resize(row.len() + 4096, value);
                    let mut wire = Vec::new();
                    framing::write_message(&mut wire, &row, &mut sequence, 8192).unwrap();
                    if fragmented {
                        for fragment in wire.chunks(97) {
                            socket.write_all(fragment).unwrap();
                        }
                    } else {
                        socket.write_all(&wire).unwrap();
                    }
                }
                framing::write_message(&mut socket, &[0xfe, 0, 0, 2, 0, 0, 0], &mut sequence, 8192)
                    .unwrap();
                // A resume consumes this same response, never another command.
                let mut byte = [0];
                use std::io::Read;
                assert_eq!(socket.read(&mut byte).unwrap(), 0);
            });
            let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
            let mut budget = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_secs(5)),
                None,
                None,
            );
            while !channel.connected().unwrap() {
                channel.wait(Direction::Write, &mut budget).unwrap();
            }
            let pool = sqlaero_core::memory::Pool::new(1_000_000);
            let mut query = Query::with_pool(
                request(binary),
                CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
                8192,
                8192,
                20_000,
                Arc::clone(&pool),
                false,
            )
            .unwrap()
            .with_streaming()
            .unwrap();
            let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
            else {
                panic!("header");
            };
            let mut prefix = Vec::new();
            let large = NonZeroUsize::new(usize::MAX).unwrap();
            let error = loop {
                match query.advance_stream(&mut channel, &mut budget, large) {
                    Ok(Step::Control) => (),
                    Ok(Step::Ready(StreamEvent::Rows(rows))) => prefix.push(rows),
                    Ok(_) => panic!("resource refusal must precede the end"),
                    Err(error) => break error,
                }
            };
            assert!(
                !query.is_finished(),
                "row storage refusal must preserve a complete pending message: {error}"
            );
            assert!(matches!(error, QueryError::Paused(_)));
            assert!(query.resource_paused());
            assert!(header.columns.scope().used() <= 20_000);
            assert!(
                !prefix.is_empty(),
                "a complete prefix must be flushed before refusal"
            );
            assert!(query.take_confirmation().is_none());
            let mut expected = 1_u8;
            for rows in prefix.drain(..) {
                for index in 0..rows.row_count {
                    let value = rows.row(index).unwrap()[0]
                        .get(&rows.bytes)
                        .unwrap()
                        .unwrap();
                    assert_eq!(value, vec![expected; 4096]);
                    expected += 1;
                }
            }
            loop {
                match next(&mut query, &mut channel, &mut budget).unwrap() {
                    StreamEvent::Rows(rows) => {
                        for index in 0..rows.row_count {
                            let value = rows.row(index).unwrap()[0]
                                .get(&rows.bytes)
                                .unwrap()
                                .unwrap();
                            assert_eq!(value, vec![expected; 4096]);
                            expected += 1;
                        }
                    }
                    StreamEvent::End { total_rows, .. } => {
                        assert_eq!(total_rows, Some(8));
                        break;
                    }
                    _ => panic!("no duplicate header"),
                }
            }
            assert_eq!(expected, 9);
            assert!(!query.resource_paused());
            assert!(query.is_finished() && query.take_confirmation().is_some());
            drop(header);
            drop(query);
            drop(channel);
            assert_eq!(pool.used(), 0);
            server.join().unwrap();
        }
    }
}
