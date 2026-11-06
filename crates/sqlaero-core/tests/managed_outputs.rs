//! Managed prepared outputs retain real completions without publishing a set.

use mysql_common::constants::{CapabilityFlags, ColumnType, StatusFlags};
use mysql_common::proto::MySerialize;
use sqlaero_core::protocol::framing::{self, Sequence};
use sqlaero_core::result::text::{Response, RowBatch, StreamEvent};
use sqlaero_core::session::query::{Managed, Query, QueryError};
use sqlaero_core::transport::channel::{Channel, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::net::TcpListener;
use std::num::NonZeroUsize;
use std::thread::JoinHandle;
use std::time::Duration;

const LIMIT: usize = 1_000_000;
fn request() -> Vec<u8> {
    vec![0x17, 1, 0, 0, 0, 0, 1, 0, 0, 0]
}
fn row(value: i64) -> Vec<u8> {
    let mut bytes = vec![0, 0];
    bytes.extend_from_slice(&value.to_le_bytes());
    bytes
}
fn end(status: StatusFlags, legacy: bool) -> Vec<u8> {
    let flags = status.bits().to_le_bytes();
    if legacy {
        vec![0xfe, 0, 0, flags[0], flags[1]]
    } else {
        vec![0xfe, 0, 0, flags[0], flags[1], 0, 0]
    }
}
fn set(values: &[i64], output: bool, legacy: bool) -> Vec<Vec<u8>> {
    let status = StatusFlags::SERVER_STATUS_AUTOCOMMIT | StatusFlags::SERVER_MORE_RESULTS_EXISTS;
    let output_status = status | StatusFlags::SERVER_PS_OUT_PARAMS;
    let mut column = Vec::new();
    mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_LONGLONG)
        .with_name(b"same_name")
        .serialize(&mut column);
    let mut packets = vec![vec![1], column];
    if legacy {
        packets.push(end(if output { output_status } else { status }, true));
    }
    packets.extend(values.iter().map(|value| row(*value)));
    packets.push(end(
        if output {
            if legacy {
                StatusFlags::SERVER_STATUS_AUTOCOMMIT
            } else {
                output_status
            }
        } else {
            status
        },
        legacy,
    ));
    packets
}
fn final_ok() -> Vec<u8> {
    vec![0, 0, 0, 2, 0, 0, 0]
}
fn peer(packets: Vec<Vec<u8>>, legacy: bool) -> (Query, Channel, IoBudget, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
            request()
        );
        for packet in packets {
            framing::write_message(&mut socket, &packet, &mut sequence, 4096).unwrap();
        }
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
    let mut capabilities =
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_PS_MULTI_RESULTS;
    if !legacy {
        capabilities.insert(CapabilityFlags::CLIENT_DEPRECATE_EOF);
    }
    (
        Query::new(request(), capabilities, 4096, 4096, LIMIT).unwrap(),
        channel,
        budget,
        server,
    )
}
fn value(rows: &RowBatch) -> i64 {
    assert_eq!(rows.row_count, 1);
    i64::from_le_bytes(
        rows.row(0).unwrap()[0]
            .get(&rows.bytes)
            .unwrap()
            .unwrap()
            .try_into()
            .unwrap(),
    )
}
fn next(
    query: &mut Query,
    channel: &mut Channel,
    budget: &mut IoBudget,
) -> Result<Managed<StreamEvent>, QueryError> {
    loop {
        match query.advance_managed_stream(channel, budget, NonZeroUsize::new(1).unwrap())? {
            Step::Ready(event) => return Ok(event),
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives readiness"),
        }
    }
}
#[test]
fn buffered_and_streaming_keep_business_sets_and_hide_only_actual_output() {
    for legacy in [false, true] {
        for streaming in [false, true] {
            let mut packets = set(&[7], false, legacy);
            packets.extend(set(&[9], true, legacy));
            packets.push(final_ok());
            let (query, mut channel, mut budget, server) = peer(packets, legacy);
            let query = if streaming {
                query.with_streaming().unwrap()
            } else {
                query
            };
            let mut query = query.with_managed_outputs(1).unwrap();
            let mut headers = 0;
            let mut rows = Vec::new();
            let mut business_ends = 0;
            let mut internal = 0;
            while !query.is_finished() {
                if streaming {
                    match next(&mut query, &mut channel, &mut budget).unwrap() {
                        Managed::Public(StreamEvent::Header(_)) => headers += 1,
                        Managed::Public(StreamEvent::Rows(batch)) => rows.push(value(&batch)),
                        Managed::Public(StreamEvent::End { tail, .. }) => {
                            assert!(tail.is_none());
                            business_ends += 1;
                        }
                        Managed::InternalOutput {
                            completion,
                            metadata_status,
                        } => {
                            assert!(
                                (completion.status_flags() | metadata_status.unwrap_or_default())
                                    .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
                            );
                            assert_eq!(value(&query.take_call_output().unwrap()), 9);
                            internal += 1;
                        }
                    }
                } else {
                    match query.advance_managed(&mut channel, &mut budget).unwrap() {
                        Step::Control => (),
                        Step::Wait(_) => panic!("Query drives readiness"),
                        Step::Ready(Managed::Public(Response::Rows(result))) => {
                            rows.push(value(&result));
                            business_ends += 1;
                        }
                        Step::Ready(Managed::Public(Response::Command(_))) => business_ends += 1,
                        Step::Ready(Managed::Public(Response::Counted { .. })) => {
                            panic!("owned rows")
                        }
                        Step::Ready(Managed::InternalOutput { .. }) => {
                            assert_eq!(value(&query.take_call_output().unwrap()), 9);
                            internal += 1;
                        }
                    }
                }
            }
            assert_eq!(headers, usize::from(streaming));
            assert_eq!(rows, [7]);
            assert_eq!(
                business_ends, 2,
                "one business result and the original tail OK"
            );
            assert_eq!(internal, 1);
            assert!(query.take_call_output().is_none());
            assert!(query.take_confirmation().is_some());
            server.join().unwrap();
        }
    }
}

#[test]
fn queued_short_business_end_is_not_finished_and_discard_preserves_it() {
    for row_count in 0..=2 {
        let values: Vec<i64> = (0..row_count).map(i64::from).collect();
        let mut packets = set(&values, false, false);
        *packets.last_mut().unwrap() = end(StatusFlags::SERVER_STATUS_AUTOCOMMIT, false);
        let (query, mut channel, mut budget, server) = peer(packets, false);
        let mut query = query
            .with_streaming()
            .unwrap()
            .with_managed_outputs(1)
            .unwrap();
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget).unwrap(),
            Managed::Public(StreamEvent::Header(_))
        ));
        assert!(
            !query.is_finished(),
            "pending business events still belong to this Query"
        );
        if row_count < 2 {
            assert!(
                query.take_confirmation().is_some(),
                "wire facts are already real"
            );
        } else {
            assert!(query.take_confirmation().is_none());
        }
        query.discard_current_rows().unwrap();
        let Managed::Public(StreamEvent::End {
            total_rows, tail, ..
        }) = next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("keep actual End")
        };
        assert!(tail.is_none());
        assert_eq!(total_rows, Some(row_count as usize));
        assert!(query.is_finished());
        server.join().unwrap();
    }
}

#[test]
fn discard_business_prefix_does_not_discard_the_following_output() {
    for row_count in 1..=3 {
        let values: Vec<i64> = (0..row_count).map(i64::from).collect();
        let mut packets = set(&values, false, false);
        packets.extend(set(&[42], true, false));
        packets.push(final_ok());
        let (query, mut channel, mut budget, server) = peer(packets, false);
        let mut query = query
            .with_streaming()
            .unwrap()
            .with_managed_outputs(1)
            .unwrap();
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget).unwrap(),
            Managed::Public(StreamEvent::Header(_))
        ));
        query.discard_current_rows().unwrap();
        assert!(
            matches!(next(&mut query, &mut channel, &mut budget).unwrap(), Managed::Public(StreamEvent::End { total_rows: Some(n), tail: None, .. }) if n == row_count as usize)
        );
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget).unwrap(),
            Managed::InternalOutput { .. }
        ));
        assert_eq!(value(&query.take_call_output().unwrap()), 42);
        assert!(query.take_managed_completion().is_none());
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget).unwrap(),
            Managed::Public(StreamEvent::End {
                total_rows: None,
                ..
            })
        ));
        assert!(query.is_finished());
        server.join().unwrap();
    }
}

#[test]
fn shape_error_retains_actual_nonbusiness_completion_without_whole_call_confirmation() {
    for streaming in [false, true] {
        for wrong_rows in [false, true] {
            let mut packets = set(if wrong_rows { &[] } else { &[9] }, true, false);
            packets.push(final_ok());
            let (query, mut channel, mut budget, server) = peer(packets, false);
            let query = if streaming {
                query.with_streaming().unwrap()
            } else {
                query
            };
            let mut query = query
                .with_managed_outputs(if wrong_rows { 1 } else { 2 })
                .unwrap();
            let error = if streaming {
                match next(&mut query, &mut channel, &mut budget) {
                    Err(error) => error,
                    Ok(_) => panic!("invalid output is not a business set"),
                }
            } else {
                loop {
                    match query.advance_managed(&mut channel, &mut budget) {
                        Err(error) => break error,
                        Ok(Step::Control) => (),
                        Ok(_) => panic!("invalid output is not success"),
                    }
                }
            };
            assert!(matches!(error, QueryError::Wire(_)));
            let (completion, metadata) = query.take_managed_completion().unwrap();
            assert!(completion.status_flags().contains(
                StatusFlags::SERVER_PS_OUT_PARAMS | StatusFlags::SERVER_MORE_RESULTS_EXISTS
            ));
            assert!(metadata.is_none());
            assert!(query.take_managed_completion().is_none(), "single consumer");
            assert!(
                query.take_confirmation().is_none(),
                "MORE does not confirm the CALL"
            );
            assert!(query.take_call_output().is_none());
            server.join().unwrap();
        }
    }
}

#[test]
fn managed_header_waits_for_two_rows_but_never_for_the_third() {
    use std::sync::mpsc;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (release, wait) = mpsc::channel();
    let (sent, sent_rows) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
            request()
        );
        let packets = set(&[1, 2, 3], false, false);
        for packet in &packets[..3] {
            framing::write_message(&mut socket, packet, &mut sequence, 4096).unwrap();
        }
        sent.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
        framing::write_message(&mut socket, &packets[3], &mut sequence, 4096).unwrap();
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
        framing::write_message(&mut socket, &packets[4], &mut sequence, 4096).unwrap();
        framing::write_message(
            &mut socket,
            &end(StatusFlags::SERVER_STATUS_AUTOCOMMIT, false),
            &mut sequence,
            4096,
        )
        .unwrap();
    });
    let (result, delivered) = mpsc::channel();
    let client = std::thread::spawn(move || {
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
            request(),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            4096,
            4096,
            LIMIT,
        )
        .unwrap()
        .with_streaming()
        .unwrap()
        .with_managed_outputs(1)
        .unwrap();
        let first = next(&mut query, &mut channel, &mut budget).unwrap();
        result.send((query, channel, budget, first)).ok().unwrap();
    });
    sent_rows.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        delivered.recv_timeout(Duration::from_millis(50)).is_err(),
        "one row is still ambiguous"
    );
    release.send(()).unwrap();
    let (mut query, mut channel, mut budget, event) = delivered
        .recv_timeout(Duration::from_secs(1))
        .expect("Header must not wait for row three");
    assert!(matches!(event, Managed::Public(StreamEvent::Header(_))));
    for expected in [1, 2] {
        let Managed::Public(StreamEvent::Rows(rows)) =
            next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("replay complete native row")
        };
        assert_eq!(value(&rows), expected);
    }
    release.send(()).unwrap();
    let Managed::Public(StreamEvent::Rows(rows)) =
        next(&mut query, &mut channel, &mut budget).unwrap()
    else {
        panic!("third row")
    };
    assert_eq!(value(&rows), 3);
    assert!(matches!(
        next(&mut query, &mut channel, &mut budget).unwrap(),
        Managed::Public(StreamEvent::End {
            total_rows: Some(3),
            ..
        })
    ));
    assert!(query.is_finished());
    client.join().unwrap();
    server.join().unwrap();
}

#[test]
fn managed_control_storage_is_charged_only_when_explicitly_enabled() {
    use sqlaero_core::memory::Pool;
    let pool = Pool::new(LIMIT);
    let query = Query::with_pool(
        request(),
        CapabilityFlags::CLIENT_PROTOCOL_41,
        4096,
        4096,
        LIMIT,
        pool.clone(),
        false,
    )
    .unwrap();
    let ordinary = pool.used();
    let query = query.with_managed_outputs(1).unwrap();
    assert!(
        pool.used() > ordinary,
        "owned lookahead/output control storage must be charged"
    );
    drop(query);
    assert_eq!(pool.used(), 0);
    let small = Pool::new(ordinary);
    let query = Query::with_pool(
        request(),
        CapabilityFlags::CLIENT_PROTOCOL_41,
        4096,
        4096,
        LIMIT,
        small.clone(),
        false,
    )
    .unwrap();
    assert!(query.with_managed_outputs(1).is_err());
    assert_eq!(small.used(), 0);
}

#[test]
fn duplicate_output_and_later_server_error_keep_the_first_completed_output() {
    for streaming in [false, true] {
        for duplicate in [false, true] {
            let mut packets = set(&[9], true, false);
            if duplicate {
                packets.extend(set(&[10], true, false));
                packets.push(final_ok());
            } else {
                packets.push(b"\xff\x28\x04#42000tail error".to_vec());
            }
            let (query, mut channel, mut budget, server) = peer(packets, false);
            let query = if streaming {
                query.with_streaming().unwrap()
            } else {
                query
            };
            let mut query = query.with_managed_outputs(1).unwrap();
            if streaming {
                assert!(matches!(
                    next(&mut query, &mut channel, &mut budget).unwrap(),
                    Managed::InternalOutput { .. }
                ));
                assert!(next(&mut query, &mut channel, &mut budget).is_err());
            } else {
                loop {
                    match query.advance_managed(&mut channel, &mut budget).unwrap() {
                        Step::Ready(Managed::InternalOutput { .. }) => break,
                        Step::Control => (),
                        _ => panic!("output first"),
                    }
                }
                loop {
                    match query.advance_managed(&mut channel, &mut budget) {
                        Err(_) => break,
                        Ok(Step::Control) => (),
                        _ => panic!("real tail failure"),
                    }
                }
            }
            assert_eq!(value(&query.take_call_output().unwrap()), 9);
            assert!(query.take_call_output().is_none());
            assert_eq!(query.take_managed_completion().is_some(), duplicate);
            assert_eq!(query.received_server_err(), !duplicate);
            assert!(query.take_confirmation().is_none());
            assert!(query.is_finished());
            server.join().unwrap();
        }
    }
}

#[test]
fn two_row_output_marker_is_rejected_after_actual_end_not_hidden_as_a_valid_output() {
    let mut packets = set(&[1, 2], true, false);
    packets.push(final_ok());
    let (query, mut channel, mut budget, server) = peer(packets, false);
    let mut query = query
        .with_streaming()
        .unwrap()
        .with_managed_outputs(1)
        .unwrap();
    assert!(matches!(
        next(&mut query, &mut channel, &mut budget).unwrap(),
        Managed::Public(StreamEvent::Header(_))
    ));
    for expected in [1, 2] {
        let Managed::Public(StreamEvent::Rows(rows)) =
            next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("bounded prefix")
        };
        assert_eq!(value(&rows), expected);
    }
    assert!(next(&mut query, &mut channel, &mut budget).is_err());
    assert!(query.take_call_output().is_none());
    assert!(
        query
            .take_managed_completion()
            .unwrap()
            .0
            .status_flags()
            .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
    );
    assert!(query.take_confirmation().is_none());
    server.join().unwrap();
}

#[test]
fn lookahead_uses_original_deadline_and_completed_output_survives_later_timeout() {
    use sqlaero_core::transport::channel::ChannelError;
    use sqlaero_core::transport::socket::TransportError;
    use std::sync::mpsc;
    for completed_output in [false, true] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut sequence = Sequence::default();
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap();
            let mut packets = set(&[9], completed_output, false);
            if !completed_output {
                packets.pop();
            }
            for packet in packets {
                framing::write_message(&mut socket, &packet, &mut sequence, 4096).unwrap();
            }
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
        let mut budget = IoBudget::new(
            TimeoutKind::Operation,
            Some(Duration::from_millis(50)),
            None,
            None,
        );
        while !channel.connected().unwrap() {
            channel.wait(Direction::Write, &mut budget).unwrap();
        }
        let mut query = Query::new(
            request(),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            4096,
            4096,
            LIMIT,
        )
        .unwrap()
        .with_streaming()
        .unwrap()
        .with_managed_outputs(1)
        .unwrap();
        if completed_output {
            assert!(matches!(
                next(&mut query, &mut channel, &mut budget).unwrap(),
                Managed::InternalOutput { .. }
            ));
        }
        let error = match next(&mut query, &mut channel, &mut budget) {
            Err(error) => error,
            Ok(_) => panic!("lookahead must await the missing packet"),
        };
        assert!(
            matches!(
                error,
                QueryError::Channel(
                    ChannelError::Timeout(TimeoutKind::Operation)
                        | ChannelError::Transport(TransportError::Timeout(TimeoutKind::Operation))
                )
            ),
            "{error:?}"
        );
        assert!(query.is_finished());
        assert_eq!(
            query.take_call_output().map(|rows| value(&rows)),
            completed_output.then_some(9)
        );
        assert!(query.take_confirmation().is_none());
        assert!(!query.received_server_err());
        release.send(()).unwrap();
        server.join().unwrap();
    }
}

#[test]
fn two_large_lookahead_rows_keep_shared_pool_limits_and_control_yields() {
    use sqlaero_core::memory::{Buffer, Pool, Scope};
    use sqlaero_core::protocol::framing::WireError;
    use sqlaero_core::result::text::ResultError;
    use sqlaero_core::transport::channel::ChannelError;
    use std::sync::mpsc;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (release, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut sequence = Sequence::default();
        framing::read_message(&mut socket, &mut sequence, LIMIT).unwrap();
        let mut column = Vec::new();
        mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_BLOB)
            .with_character_set(63)
            .serialize(&mut column);
        let mut row = vec![0, 0, 0xfd, 0xa0, 0x86, 1]; // 100000 byte native value.
        row.resize(100006, b'x');
        for payload in [vec![1], column, row.clone()] {
            framing::write_message(&mut socket, &payload, &mut sequence, LIMIT).unwrap();
        }
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
        framing::write_message(&mut socket, &row, &mut sequence, LIMIT).unwrap();
        framing::write_message(
            &mut socket,
            &end(StatusFlags::SERVER_STATUS_AUTOCOMMIT, false),
            &mut sequence,
            LIMIT,
        )
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
    let pool = Pool::new(LIMIT);
    let mut query = Query::with_pool(
        request(),
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
        4096,
        LIMIT,
        700000,
        pool.clone(),
        false,
    )
    .unwrap()
    .with_streaming()
    .unwrap()
    .with_managed_outputs(1)
    .unwrap();
    assert!(matches!(
        query
            .advance_managed_stream(&mut channel, &mut budget, NonZeroUsize::MIN)
            .unwrap(),
        Step::Control
    ));
    assert!(
        !query.at_stream_boundary(),
        "ambiguous native prefix is not a public header"
    );
    assert!(!query.is_finished());
    assert!(
        pool.used() > 100000,
        "first full row remains owned across host signal checks"
    );
    // Another real live allocation in this same connection pool leaves too
    // little capacity for the second logical row. No guessed peak threshold.
    let pressure_scope = Scope::new(pool.clone(), LIMIT).unwrap();
    let mut pressure = Buffer::<u8>::tracked(&pressure_scope);
    pressure.resize(LIMIT - pool.used() - 128, 0).unwrap();
    release.send(()).unwrap();
    let error = match next(&mut query, &mut channel, &mut budget) {
        Err(error) => error,
        Ok(_) => panic!("simultaneous row growth exceeds the real quota"),
    };
    assert!(
        matches!(
            error,
            QueryError::Result(ResultError::Memory(_))
                | QueryError::Wire(WireError::Memory(_))
                | QueryError::Channel(ChannelError::Wire(WireError::Memory(_)))
        ),
        "{error:?}"
    );
    assert!(query.is_finished());
    assert!(query.take_call_output().is_none());
    assert!(query.take_confirmation().is_none());
    drop(query);
    drop(pressure);
    drop(pressure_scope);
    assert_eq!(pool.used(), 0);
    server.join().unwrap();
}

#[test]
fn completed_output_survives_a_corrupt_compressed_tail_without_false_confirmation() {
    use sqlaero_core::memory::{Pool, Scope};
    use sqlaero_core::protocol::framing::{MessageReader, Progress};
    use sqlaero_core::transport::compression::{Algorithm, Codec};
    use std::io::Write;
    fn frame(algorithm: Algorithm, sequence: u8, plain: &[u8], corrupt: bool) -> Vec<u8> {
        let mut compressed = match algorithm {
            Algorithm::Zlib => {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(plain).unwrap();
                encoder.finish().unwrap()
            }
            Algorithm::Zstd => zstd::bulk::compress(plain, 3).unwrap(),
        };
        if corrupt {
            *compressed.last_mut().unwrap() ^= 1;
        }
        let mut wire = vec![
            compressed.len() as u8,
            (compressed.len() >> 8) as u8,
            (compressed.len() >> 16) as u8,
            sequence,
            plain.len() as u8,
            (plain.len() >> 8) as u8,
            (plain.len() >> 16) as u8,
        ];
        wire.extend_from_slice(&compressed);
        wire
    }
    for (algorithm, corrupt) in [
        (Algorithm::Zlib, false),
        (Algorithm::Zstd, false),
        (Algorithm::Zlib, true),
    ] {
        let mut sequence = Sequence::new(1);
        let mut output = Vec::new();
        for payload in set(&[9], true, false) {
            framing::write_message(&mut output, &payload, &mut sequence, LIMIT).unwrap();
        }
        let mut tail = Vec::new();
        framing::write_message(&mut tail, &final_ok(), &mut sequence, LIMIT).unwrap();
        if !corrupt {
            tail.push(0xa5);
        }
        let wire = [
            frame(algorithm, 1, &output, false),
            frame(algorithm, 2, &tail, corrupt),
        ]
        .concat();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let scope = Scope::new(Pool::new(LIMIT), LIMIT).unwrap();
            let mut codec = Codec::new(algorithm, 3).unwrap();
            let mut message = MessageReader::tracked(4096, &scope);
            let mut sequence = Sequence::default();
            loop {
                if let Progress::Ready(bytes) = codec
                    .read_message_step(&mut message, &mut socket, &mut sequence, &scope)
                    .unwrap()
                {
                    assert_eq!(bytes.as_slice(), request());
                    break;
                }
            }
            codec.finish_response().unwrap();
            socket.write_all(&wire).unwrap();
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
        channel.enable_compression(algorithm, 3).unwrap();
        let mut query = Query::new(
            request(),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            4096,
            LIMIT,
            LIMIT,
        )
        .unwrap()
        .with_streaming()
        .unwrap()
        .with_managed_outputs(1)
        .unwrap();
        assert!(matches!(
            next(&mut query, &mut channel, &mut budget).unwrap(),
            Managed::InternalOutput { .. }
        ));
        assert!(next(&mut query, &mut channel, &mut budget).is_err());
        assert_eq!(value(&query.take_call_output().unwrap()), 9);
        assert!(query.take_confirmation().is_none());
        assert!(!query.received_server_err());
        assert!(query.take_managed_completion().is_none());
        server.join().unwrap();
    }
}
