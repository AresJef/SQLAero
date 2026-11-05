//! Discarding one streaming set preserves validation, facts and the next set.

use mysql_common::constants::{CapabilityFlags, ColumnType, StatusFlags};
use mysql_common::proto::MySerialize;
use sqlaero_core::memory::MemoryError;
use sqlaero_core::protocol::framing::{self, Sequence, WireError};
use sqlaero_core::result::text::{ResultError, StreamEvent};
use sqlaero_core::session::query::{ConfirmedPrefix, Query, QueryError};
use sqlaero_core::transport::channel::{Channel, ChannelError, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::net::TcpListener;
use std::num::NonZeroUsize;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_PACKET: usize = 16 * 1024;

fn request(binary: bool) -> Vec<u8> {
    if binary {
        vec![0x17, 1, 0, 0, 0, 0, 1, 0, 0, 0]
    } else {
        b"\x03SELECT value".to_vec()
    }
}

fn header(legacy: bool) -> Vec<Vec<u8>> {
    let mut column = Vec::new();
    mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
        .with_name(b"value")
        .serialize(&mut column);
    let mut packets = vec![vec![1], column];
    if legacy {
        packets.push(vec![0xfe, 0, 0, 2, 0]);
    }
    packets
}

fn row(binary: bool, size: usize) -> Vec<u8> {
    let mut bytes = if binary { vec![0, 0] } else { Vec::new() };
    if size < 251 {
        bytes.push(size as u8);
    } else {
        bytes.extend_from_slice(&[0xfc, size as u8, (size >> 8) as u8]);
    }
    bytes.resize(bytes.len() + size, b'x');
    bytes
}

fn end(legacy: bool, more: bool) -> Vec<u8> {
    let status = (StatusFlags::SERVER_STATUS_AUTOCOMMIT
        | if more {
            StatusFlags::SERVER_MORE_RESULTS_EXISTS
        } else {
            StatusFlags::empty()
        })
    .bits()
    .to_le_bytes();
    if legacy {
        vec![0xfe, 3, 0, status[0], status[1]]
    } else {
        vec![0xfe, 0, 0, status[0], status[1], 3, 0]
    }
}

fn peer(binary: bool, packets: Vec<Vec<u8>>) -> (Channel, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, MAX_PACKET).unwrap(),
            request(binary)
        );
        for packet in packets {
            framing::write_message(&mut socket, &packet, &mut sequence, MAX_PACKET).unwrap();
        }
    });
    (
        Channel::new(Transport::tcp(address).unwrap(), false),
        server,
    )
}

fn budget() -> IoBudget {
    IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(3)),
        None,
        None,
    )
}

fn query(binary: bool, legacy: bool, limit: usize) -> Query {
    let mut flags = CapabilityFlags::CLIENT_PROTOCOL_41;
    flags.insert(if binary {
        CapabilityFlags::CLIENT_PS_MULTI_RESULTS
    } else {
        CapabilityFlags::CLIENT_MULTI_RESULTS
    });
    if !legacy {
        flags.insert(CapabilityFlags::CLIENT_DEPRECATE_EOF);
    }
    Query::new(request(binary), flags, MAX_PACKET, MAX_PACKET, limit)
        .unwrap()
        .with_streaming()
        .unwrap()
}

fn next(
    query: &mut Query,
    channel: &mut Channel,
    budget: &mut IoBudget,
    rows: usize,
    controls: &mut usize,
) -> Result<StreamEvent, QueryError> {
    while !channel.connected()? {
        channel.wait(Direction::Write, budget)?;
    }
    loop {
        match query.advance_stream(channel, budget, NonZeroUsize::new(rows).unwrap())? {
            Step::Ready(event) => return Ok(event),
            Step::Control => {
                *controls += 1;
                assert!(!query.at_stream_boundary());
                assert!(query.discard_current_rows().is_err());
                assert!(
                    !query.is_finished(),
                    "local rejection cannot terminate the command"
                );
            }
            Step::Wait(_) => panic!("Query drives concrete readiness"),
        }
    }
}

fn confirmed_rows(query: &mut Query) -> u128 {
    let mut prefix = ConfirmedPrefix::default();
    prefix
        .record_confirmation(0..0, query.take_confirmation().unwrap())
        .unwrap();
    prefix.row_count
}

#[test]
fn discard_only_at_a_real_current_row_boundary_and_preserve_delivered_owners() {
    for binary in [false, true] {
        for legacy in [false, true] {
            let mut packets = header(legacy);
            packets.extend([row(binary, 7), row(binary, 8), end(legacy, true)]);
            packets.extend(header(legacy));
            packets.extend([row(binary, 9), end(legacy, false)]);
            let (mut channel, server) = peer(binary, packets);
            let mut query = query(binary, legacy, 32 * 1024);
            assert!(
                query.discard_current_rows().is_err(),
                "not before the Header"
            );
            assert!(!query.is_finished());
            let mut budget = budget();
            let mut controls = 0;
            let StreamEvent::Header(first_header) =
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("first header");
            };
            let StreamEvent::Rows(first) =
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("first stored row");
            };
            let bytes = first.bytes.to_vec();
            let first_used = first_header.columns.scope().used();
            query.discard_current_rows().unwrap();
            assert_eq!(
                first_header.columns.scope().used(),
                first_used,
                "switching cannot release a host-owned batch"
            );
            let StreamEvent::End {
                tail,
                total_rows,
                completion,
                ..
            } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("discard must not manufacture a RowBatch");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, Some(2));
            assert_eq!(completion.warnings(), 3);
            assert!(
                !query.is_finished(),
                "current set end is not whole-command end"
            );
            assert!(query.take_confirmation().is_none());
            assert!(
                query.discard_current_rows().is_err(),
                "ended set cannot change the following set"
            );
            let StreamEvent::Header(second_header) =
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("next header");
            };
            assert_eq!(second_header.columns.len(), 1);
            let StreamEvent::Rows(second) =
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("next set must still store its rows");
            };
            assert_eq!(second.row_count, 1);
            assert_eq!(
                second.row(0).unwrap()[0]
                    .get(&second.bytes)
                    .unwrap()
                    .unwrap(),
                &[b'x'; 9]
            );
            let StreamEvent::End {
                tail, total_rows, ..
            } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("final end");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, Some(1));
            assert!(query.is_finished());
            assert_eq!(confirmed_rows(&mut query), 1);
            assert_eq!(
                first.bytes.as_slice(),
                bytes,
                "delivered storage outlives discard and next sets"
            );
            server.join().unwrap();
        }
    }
}

#[test]
fn discard_large_unread_set_with_small_capacity_keeps_control_yields_and_real_count() {
    for binary in [false, true] {
        let mut packets = header(false);
        packets.extend((0..600).map(|_| row(binary, 1024)));
        packets.push(end(false, false));
        let (mut channel, server) = peer(binary, packets);
        let mut query = query(binary, false, 16 * 1024);
        let mut budget = budget();
        let mut controls = 0;
        let StreamEvent::Header(header) =
            next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
        else {
            panic!("header");
        };
        query.discard_current_rows().unwrap();
        let StreamEvent::End {
            tail,
            total_rows,
            completion,
            ..
        } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
        else {
            panic!("no discarded Rows event");
        };
        assert!(tail.is_none());
        assert_eq!(total_rows, Some(600));
        assert!(
            controls >= 2,
            "roughly 256KiB host checkpoints remain in the parser loop"
        );
        assert!(header.columns.scope().used() < 16 * 1024);
        assert_eq!(completion.warnings(), 3);
        assert_eq!(confirmed_rows(&mut query), 600);
        assert!(query.is_finished());
        server.join().unwrap();
    }
}

#[test]
fn discarded_rows_still_reject_truncation_and_trailing_fields() {
    for binary in [false, true] {
        for trailing in [false, true] {
            let mut packets = header(false);
            let mut invalid = row(binary, 7);
            if trailing {
                invalid.push(0);
            } else {
                invalid.pop();
            }
            packets.push(invalid);
            let (mut channel, server) = peer(binary, packets);
            let mut query = query(binary, false, 16 * 1024);
            let mut budget = budget();
            let mut controls = 0;
            assert!(matches!(
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap(),
                StreamEvent::Header(_)
            ));
            query.discard_current_rows().unwrap();
            let error = next(&mut query, &mut channel, &mut budget, 1, &mut controls)
                .err()
                .unwrap();
            assert!(matches!(error, QueryError::Result(ResultError::Wire(_))));
            assert!(query.is_finished());
            assert!(query.take_confirmation().is_none());
            server.join().unwrap();
        }
    }
}

#[test]
fn discard_preserves_native_packet_capacity_failure() {
    for binary in [false, true] {
        let mut packets = header(false);
        packets.push(row(binary, 12 * 1024));
        let (mut channel, server) = peer(binary, packets);
        let mut query = query(binary, false, 4096);
        let mut budget = budget();
        let mut controls = 0;
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap(),
            StreamEvent::Header(_)
        ));
        query.discard_current_rows().unwrap();
        let error = next(&mut query, &mut channel, &mut budget, 1, &mut controls)
            .err()
            .unwrap();
        let QueryError::Channel(ChannelError::Wire(WireError::Memory(MemoryError::Result {
            required,
            limit,
        }))) = error
        else {
            panic!("expected real receive-capacity failure, got {error:?}");
        };
        assert_eq!(limit, 4096);
        assert!(required > limit);
        assert!(query.is_finished());
        assert!(query.take_confirmation().is_none());
        server.join().unwrap();
    }
}

#[test]
fn discard_rejects_buffered_query_without_changing_its_delivery_mode() {
    let mut query = Query::new(
        request(false),
        CapabilityFlags::CLIENT_PROTOCOL_41,
        MAX_PACKET,
        MAX_PACKET,
        16 * 1024,
    )
    .unwrap();
    assert!(query.discard_current_rows().is_err());
    assert!(!query.is_finished());
    let mut query = query.with_streaming().unwrap();
    assert!(query.discard_current_rows().is_err());
    assert!(!query.is_finished());
}

#[test]
fn discarded_empty_set_has_a_known_zero_count_and_no_synthetic_tail() {
    for binary in [false, true] {
        for legacy in [false, true] {
            let mut packets = header(legacy);
            packets.push(end(legacy, false));
            let (mut channel, server) = peer(binary, packets);
            let mut query = query(binary, legacy, 16 * 1024);
            let mut budget = budget();
            let mut controls = 0;
            assert!(matches!(
                next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap(),
                StreamEvent::Header(_)
            ));
            query.discard_current_rows().unwrap();
            let StreamEvent::End {
                tail, total_rows, ..
            } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
            else {
                panic!("empty end");
            };
            assert!(tail.is_none());
            assert_eq!(total_rows, Some(0));
            assert_eq!(confirmed_rows(&mut query), 0);
            assert!(query.is_finished());
            assert!(query.discard_current_rows().is_err());
            assert!(query.is_finished());
            server.join().unwrap();
        }
    }
}

#[test]
fn server_error_in_a_discarded_tail_keeps_its_original_terminal_facts() {
    for binary in [false, true] {
        let mut packets = header(false);
        packets.extend([
            row(binary, 7),
            b"\xff\x7a\x04#42S02discarded tail failure".to_vec(),
        ]);
        let (mut channel, server) = peer(binary, packets);
        let mut query = query(binary, false, 16 * 1024);
        let mut budget = budget();
        let mut controls = 0;
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap(),
            StreamEvent::Header(_)
        ));
        query.discard_current_rows().unwrap();
        let error = next(&mut query, &mut channel, &mut budget, 1, &mut controls)
            .err()
            .unwrap();
        let QueryError::Result(ResultError::Server(error)) = error else {
            panic!("original server error");
        };
        assert_eq!(error.error_code(), 1146);
        assert_eq!(error.message_ref(), b"discarded tail failure");
        assert!(query.received_server_err());
        assert!(query.is_finished());
        assert!(query.take_confirmation().is_none());
        server.join().unwrap();
    }
}

#[test]
fn discarded_legacy_out_rows_still_wait_for_the_real_command_tail() {
    let mut packets = header(true);
    let metadata = StatusFlags::SERVER_STATUS_AUTOCOMMIT
        | StatusFlags::SERVER_PS_OUT_PARAMS
        | StatusFlags::SERVER_MORE_RESULTS_EXISTS;
    let bits = metadata.bits().to_le_bytes();
    *packets.last_mut().unwrap() = vec![0xfe, 0, 0, bits[0], bits[1]];
    packets.extend([row(true, 7), end(true, false), vec![0, 5, 0, 2, 0, 0, 0]]);
    let (mut channel, server) = peer(true, packets);
    let mut query = query(true, true, 16 * 1024);
    let mut budget = budget();
    let mut controls = 0;
    let StreamEvent::Header(header) =
        next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
    else {
        panic!("legacy OUT header");
    };
    assert_eq!(header.metadata_status, Some(metadata));
    query.discard_current_rows().unwrap();
    let StreamEvent::End {
        tail,
        total_rows,
        completion,
        metadata_status,
    } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
    else {
        panic!("OUT end");
    };
    assert!(tail.is_none());
    assert_eq!(total_rows, Some(1));
    assert_eq!(metadata_status, Some(metadata));
    assert_eq!(
        completion.status_flags(),
        StatusFlags::SERVER_STATUS_AUTOCOMMIT
    );
    assert!(
        !query.is_finished(),
        "OUT terminator is not the whole-command tail"
    );
    assert!(query.take_confirmation().is_none());
    let StreamEvent::End {
        tail,
        total_rows,
        completion,
        ..
    } = next(&mut query, &mut channel, &mut budget, 1, &mut controls).unwrap()
    else {
        panic!("actual command tail");
    };
    assert!(tail.is_none());
    assert_eq!(total_rows, None);
    assert_eq!(completion.affected_rows(), 5);
    assert_eq!(confirmed_rows(&mut query), 5);
    assert!(query.is_finished());
    server.join().unwrap();
}
