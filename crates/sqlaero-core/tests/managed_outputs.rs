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
