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

// The same small capacity response is reused only for the pause/discard cases.
fn capacity_response(
    binary: bool,
    malformed_row: Option<u8>,
) -> (Channel, IoBudget, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
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
            if malformed_row == Some(value) {
                row.pop();
            }
            let mut wire = Vec::new();
            framing::write_message(&mut wire, &row, &mut sequence, 8192).unwrap();
            for fragment in wire.chunks(193) {
                if let Err(error) = socket.write_all(fragment) {
                    assert!(matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                    ));
                    return; // a tested terminal client exit may stop the peer
                }
            }
        }
        let mut wire = Vec::new();
        framing::write_message(&mut wire, &[0xfe, 0, 0, 2, 0, 0, 0], &mut sequence, 8192).unwrap();
        if let Err(error) = socket.write_all(&wire) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
            ));
            return;
        }
        let mut byte = [0];
        match socket.read(&mut byte) {
            Ok(0) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("no replay command is permitted: {other:?}"),
        }
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
    (channel, budget, server)
}

#[test]
fn a_flushed_capacity_failure_is_reported_even_after_space_becomes_available() {
    for binary in [false, true] {
        let (mut channel, mut budget, server) = capacity_response(binary, None);
        let mut query = Query::new(
            request(binary),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            8192,
            8192,
            20_000,
        )
        .unwrap()
        .with_streaming()
        .unwrap();
        let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("header");
        };
        let large = NonZeroUsize::new(usize::MAX).unwrap();
        let rows = loop {
            match query
                .advance_stream(&mut channel, &mut budget, large)
                .unwrap()
            {
                Step::Ready(StreamEvent::Rows(rows)) => break rows,
                Step::Control => (),
                _ => panic!("complete prefix before explicit failure"),
            }
        };
        assert!(
            !query.resource_paused(),
            "a successful Rows event is not the typed pause"
        );
        let prefix = rows.row_count;
        assert!(prefix > 0 && prefix < 8);
        let before = header.columns.scope().used();
        drop(rows);
        assert!(header.columns.scope().used() < before);
        let error = match query.advance_stream(&mut channel, &mut budget, large) {
            Err(error) => error,
            Ok(_) => panic!("the previously encountered capacity failure must be reported"),
        };
        assert!(
            matches!(error, QueryError::Paused(_)),
            "the already encountered failure must not disappear"
        );
        assert!(query.resource_paused() && !query.is_finished());
        let mut received = prefix;
        loop {
            match next(&mut query, &mut channel, &mut budget).unwrap() {
                StreamEvent::Rows(rows) => {
                    assert_eq!(rows.row_count, 1);
                    let bytes = rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap();
                    assert!(bytes.iter().all(|byte| usize::from(*byte) == received + 1));
                    received += 1;
                }
                StreamEvent::End { total_rows, .. } => {
                    assert_eq!(total_rows, Some(8));
                    break;
                }
                _ => panic!("no duplicate metadata"),
            }
        }
        assert_eq!(received, 8);
        drop(header);
        drop(query);
        drop(channel);
        server.join().unwrap();
    }
}

#[test]
fn explicit_discard_clears_reported_or_deferred_pause_but_validates_the_pending_row() {
    for binary in [false, true] {
        for reported in [false, true] {
            for malformed_last in [false, true] {
                let (mut channel, mut budget, server) =
                    capacity_response(binary, malformed_last.then_some(8));
                let mut query = Query::new(
                    request(binary),
                    CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
                    8192,
                    8192,
                    20_000,
                )
                .unwrap()
                .with_streaming()
                .unwrap();
                let StreamEvent::Header(header) =
                    next(&mut query, &mut channel, &mut budget).unwrap()
                else {
                    panic!("header");
                };
                let large = NonZeroUsize::new(usize::MAX).unwrap();
                let rows = loop {
                    match query
                        .advance_stream(&mut channel, &mut budget, large)
                        .unwrap()
                    {
                        Step::Ready(StreamEvent::Rows(rows)) => break rows,
                        Step::Control => (),
                        _ => panic!("a complete prefix is available"),
                    }
                };
                assert!(rows.row_count > 0 && rows.row_count < 8);
                drop(rows);
                if reported {
                    assert!(matches!(
                        query.advance_stream(&mut channel, &mut budget, large),
                        Err(QueryError::Paused(_))
                    ));
                    assert!(query.resource_paused());
                } else {
                    assert!(query.at_stream_boundary());
                }
                query.discard_current_rows().unwrap();
                assert!(!query.resource_paused());
                loop {
                    match query.advance_stream(&mut channel, &mut budget, large) {
                        Ok(Step::Control) => (),
                        Ok(Step::Ready(StreamEvent::End {
                            total_rows, tail, ..
                        })) => {
                            assert!(!malformed_last);
                            assert!(tail.is_none());
                            assert_eq!(total_rows, Some(8));
                            assert!(query.is_finished() && query.take_confirmation().is_some());
                            break;
                        }
                        Err(error) => {
                            assert!(malformed_last);
                            assert!(
                                !matches!(error, QueryError::Paused(_)),
                                "discard still validates the actual row"
                            );
                            assert!(query.is_finished() && !query.resource_paused());
                            assert!(query.take_confirmation().is_none());
                            break;
                        }
                        _ => panic!("discard does not publish further row storage"),
                    }
                }
                drop(header);
                drop(query);
                drop(channel);
                server.join().unwrap();
            }
        }
    }
}

#[test]
fn aggregate_capacity_pause_waits_for_its_actual_other_owner_to_release() {
    use sqlaero_core::memory::{MemoryError, Pool, Scope};
    use std::sync::Arc;
    for binary in [false, true] {
        let (mut channel, mut budget, server) = capacity_response(binary, None);
        let pool = Pool::new(20_000);
        let other = Scope::new(Arc::clone(&pool), 1_000_000).unwrap();
        let occupied = other.charge(12_000).unwrap();
        let mut query = Query::with_pool(
            request(binary),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            8192,
            8192,
            1_000_000,
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
        for _ in 0..2 {
            let error = loop {
                match query.advance_stream(&mut channel, &mut budget, NonZeroUsize::MIN) {
                    Ok(Step::Control) => (),
                    Err(error) => break error,
                    _ => panic!("another owner still holds the required total capacity"),
                }
            };
            assert!(matches!(
                error,
                QueryError::Paused(MemoryError::Total { .. })
            ));
            assert!(query.resource_paused() && !query.is_finished());
            assert!(query.take_confirmation().is_none());
            assert!(pool.used() <= 20_000);
        }
        drop(occupied);
        drop(other);
        let mut expected = 1_u8;
        loop {
            match next(&mut query, &mut channel, &mut budget).unwrap() {
                StreamEvent::Rows(rows) => {
                    assert_eq!(rows.row_count, 1);
                    let bytes = rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap();
                    assert_eq!(bytes, vec![expected; 4096]);
                    expected += 1;
                }
                StreamEvent::End { total_rows, .. } => {
                    assert_eq!(total_rows, Some(8));
                    break;
                }
                _ => panic!("one original stream"),
            }
        }
        assert_eq!(expected, 9);
        drop(header);
        drop(query);
        drop(channel);
        assert_eq!(pool.used(), 0);
        server.join().unwrap();
    }
}

#[test]
fn managed_capacity_pause_requires_an_already_public_business_header() {
    use sqlaero_core::memory::{MemoryError, Pool, Scope};
    use sqlaero_core::session::query::Managed;
    use std::sync::Arc;
    for public_business in [false, true] {
        let (mut channel, mut budget, server) = capacity_response(true, None);
        let pool = Pool::new(20_000);
        let other = Scope::new(Arc::clone(&pool), 1_000_000).unwrap();
        let occupied = (!public_business).then(|| other.charge(12_000).unwrap());
        let mut query = Query::with_pool(
            request(true),
            CapabilityFlags::CLIENT_PROTOCOL_41
                | CapabilityFlags::CLIENT_DEPRECATE_EOF
                | CapabilityFlags::CLIENT_PS_MULTI_RESULTS,
            8192,
            8192,
            1_000_000,
            Arc::clone(&pool),
            false,
        )
        .unwrap()
        .with_streaming()
        .unwrap()
        .with_managed_outputs(1)
        .unwrap();
        let mut header = None;
        let mut prefix = Vec::new();
        let error = loop {
            match query.advance_managed_stream(
                &mut channel,
                &mut budget,
                NonZeroUsize::new(usize::MAX).unwrap(),
            ) {
                Ok(Step::Control) => (),
                Ok(Step::Ready(Managed::Public(StreamEvent::Header(value)))) => {
                    assert!(public_business);
                    assert!(header.is_none());
                    header = Some(value);
                }
                Ok(Step::Ready(Managed::Public(StreamEvent::Rows(rows)))) => {
                    assert!(header.is_some());
                    prefix.push(rows);
                }
                Err(error) => break error,
                _ => panic!("capacity stops this script before a terminal result"),
            }
        };
        if public_business {
            assert!(matches!(
                error,
                QueryError::Paused(MemoryError::Total { .. })
            ));
            assert!(query.resource_paused() && !query.is_finished());
            assert!(query.take_confirmation().is_none());
            let mut expected = 1_u8;
            for rows in prefix.drain(..) {
                for index in 0..rows.row_count {
                    assert_eq!(
                        rows.row(index).unwrap()[0]
                            .get(&rows.bytes)
                            .unwrap()
                            .unwrap(),
                        vec![expected; 4096]
                    );
                    expected += 1;
                }
            }
            loop {
                match query
                    .advance_managed_stream(&mut channel, &mut budget, NonZeroUsize::MIN)
                    .unwrap()
                {
                    Step::Control => (),
                    Step::Ready(Managed::Public(StreamEvent::Rows(rows))) => {
                        assert_eq!(rows.row_count, 1);
                        assert_eq!(
                            rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap(),
                            vec![expected; 4096]
                        );
                        expected += 1;
                    }
                    Step::Ready(Managed::Public(StreamEvent::End { total_rows, .. })) => {
                        assert_eq!(total_rows, Some(8));
                        break;
                    }
                    _ => panic!("no duplicate header, invented output or lost public event"),
                }
            }
            assert_eq!(expected, 9);
            assert!(query.is_finished() && !query.resource_paused());
            assert!(query.take_call_output().is_none());
        } else {
            assert!(matches!(
                error,
                QueryError::Result(sqlaero_core::result::text::ResultError::Memory(
                    MemoryError::Total { .. }
                ))
            ));
            assert!(query.is_finished() && !query.resource_paused());
            assert!(
                header.is_none() && prefix.is_empty(),
                "unpublished OUT lookahead is not user-recoverable business data"
            );
            assert!(query.take_confirmation().is_none() && query.take_call_output().is_none());
        }
        drop(occupied);
        drop(other);
        drop(header);
        drop(query);
        drop(channel);
        assert_eq!(pool.used(), 0);
        server.join().unwrap();
    }
}

#[test]
fn a_capacity_pause_does_not_restart_the_original_operation_budget() {
    for binary in [false, true] {
        let (mut channel, mut budget, server) = capacity_response(binary, None);
        let mut query = Query::new(
            request(binary),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            8192,
            8192,
            20_000,
        )
        .unwrap()
        .with_streaming()
        .unwrap();
        let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("header");
        };
        let mut prefix = Vec::new();
        loop {
            match query.advance_stream(
                &mut channel,
                &mut budget,
                NonZeroUsize::new(usize::MAX).unwrap(),
            ) {
                Ok(Step::Ready(StreamEvent::Rows(rows))) => prefix.push(rows),
                Ok(Step::Control) => (),
                Err(QueryError::Paused(_)) => break,
                _ => panic!("the original capacity pause"),
            }
        }
        assert!(query.resource_paused());
        drop(prefix);
        budget = IoBudget::new(TimeoutKind::Operation, Some(Duration::ZERO), None, None);
        let error = match query.advance_stream(&mut channel, &mut budget, NonZeroUsize::MIN) {
            Err(error) => error,
            Ok(_) => panic!("expired original budget"),
        };
        assert!(matches!(
            error,
            QueryError::Channel(sqlaero_core::transport::channel::ChannelError::Timeout(
                TimeoutKind::Operation
            ))
        ));
        assert!(query.is_finished() && !query.resource_paused());
        assert!(query.take_confirmation().is_none());
        drop(header);
        drop(query);
        drop(channel);
        server.join().unwrap();
    }
}

#[test]
fn malformed_rows_are_terminal_even_when_their_destination_reservation_would_fail() {
    for binary in [false, true] {
        let (mut channel, mut budget, server) = capacity_response(binary, Some(3));
        let mut query = Query::new(
            request(binary),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            8192,
            8192,
            20_000,
        )
        .unwrap()
        .with_streaming()
        .unwrap();
        let StreamEvent::Header(header) = next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("header");
        };
        let error = loop {
            match query.advance_stream(
                &mut channel,
                &mut budget,
                NonZeroUsize::new(usize::MAX).unwrap(),
            ) {
                Ok(Step::Control) => (),
                Err(error) => break error,
                _ => panic!("malformed third row is not a recoverable allocation pause"),
            }
        };
        assert!(matches!(
            error,
            QueryError::Result(sqlaero_core::result::text::ResultError::Wire(_))
        ));
        assert!(query.is_finished() && !query.resource_paused());
        assert!(query.take_confirmation().is_none());
        drop(header);
        drop(query);
        drop(channel);
        server.join().unwrap();
    }
}

#[test]
fn metadata_terminal_packets_and_incomplete_messages_never_become_row_capacity_pauses() {
    use std::io::{Read, Write};
    // malformed metadata, metadata quota, EOF storage, ERR storage,
    // incomplete-message quota, and the real max_received_packet bound.
    for kind in 0..6 {
        for binary in [false, true] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let (release, wait) = mpsc::channel();
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
                if kind == 1 {
                    let mut wire = Vec::new();
                    framing::write_message(&mut wire, &[0xfd, 0xa0, 0x86, 1], &mut sequence, 8192)
                        .unwrap();
                    let mut column = Vec::new();
                    mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
                        .with_name(b"value")
                        .serialize(&mut column);
                    // Metadata is admitted per actual column. A count alone
                    // deliberately does not allocate all claimed definitions.
                    for _ in 0..1024 {
                        framing::write_message(&mut wire, &column, &mut sequence, 8192).unwrap();
                    }
                    if let Err(error) = socket.write_all(&wire) {
                        assert!(matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        ));
                        return;
                    }
                } else {
                    framing::write_message(&mut socket, &[1], &mut sequence, 8192).unwrap();
                    let mut column = Vec::new();
                    if kind == 0 {
                        column.push(1);
                    } else {
                        mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
                            .with_name(b"value")
                            .with_column_length(4096)
                            .serialize(&mut column);
                    }
                    framing::write_message(&mut socket, &column, &mut sequence, 8192).unwrap();
                    if kind >= 2 {
                        wait.recv_timeout(Duration::from_secs(5)).unwrap();
                        let data = match kind {
                            2 => vec![0xfe, 0, 0, 2, 0, 0, 0],
                            3 => b"\xff\x28\x04#42000bad".to_vec(),
                            _ => {
                                let mut row = if binary { vec![0, 0] } else { Vec::new() };
                                row.extend_from_slice(&[0xfc, 0, 16]);
                                row.resize(row.len() + 4096, 7);
                                row
                            }
                        };
                        let mut wire = Vec::new();
                        framing::write_message(&mut wire, &data, &mut sequence, 8192).unwrap();
                        if let Err(error) = socket.write_all(&wire) {
                            assert!(matches!(
                                error.kind(),
                                std::io::ErrorKind::BrokenPipe
                                    | std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionAborted
                            ));
                            return;
                        }
                    }
                }
                let mut byte = [0];
                match socket.read(&mut byte) {
                    Ok(0) => (),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        ) => {}
                    other => panic!("terminal failure must not replay: {other:?}"),
                }
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
            let mut query = Query::new(
                request(binary),
                CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
                8192,
                if kind == 5 { 4096 } else { 8192 },
                20_000,
            )
            .unwrap()
            .with_streaming()
            .unwrap();
            let mut header = None;
            let mut held = None;
            if kind >= 2 {
                let StreamEvent::Header(value) =
                    next(&mut query, &mut channel, &mut budget).unwrap()
                else {
                    panic!("header");
                };
                if kind <= 4 {
                    let leave = match kind {
                        2 => 8,
                        3 => 16,
                        _ => 64,
                    };
                    let mut capacity = Buffer::<u8>::tracked(value.columns.scope());
                    capacity
                        .reserve(20_000 - value.columns.scope().used() - leave)
                        .unwrap();
                    assert_eq!(value.columns.scope().used(), 20_000 - leave);
                    held = Some(capacity);
                }
                header = Some(value);
                release.send(()).unwrap();
            }
            let error = loop {
                match query.advance_stream(&mut channel, &mut budget, NonZeroUsize::MIN) {
                    Ok(Step::Control) => (),
                    Err(error) => break error,
                    _ => panic!("the selected old failure remains terminal"),
                }
            };
            assert!(!matches!(error, QueryError::Paused(_)), "kind={kind}");
            match kind {
                0 => assert!(matches!(
                    &error,
                    QueryError::Result(sqlaero_core::result::text::ResultError::Wire(_))
                )),
                1..=3 => assert!(
                    matches!(
                        &error,
                        QueryError::Result(sqlaero_core::result::text::ResultError::Memory(_))
                    ),
                    "kind={kind}: {error}"
                ),
                _ => assert!(
                    matches!(
                        &error,
                        QueryError::Channel(sqlaero_core::transport::channel::ChannelError::Wire(
                            _
                        ))
                    ),
                    "kind={kind}: {error}"
                ),
            }
            assert!(
                query.is_finished() && !query.resource_paused(),
                "kind={kind}"
            );
            match kind {
                2 => {
                    let ack = query
                        .take_confirmation()
                        .expect("actual EOF facts survive ownership failure");
                    assert!(ack.has_rows());
                    assert_eq!(ack.status(), StatusFlags::SERVER_STATUS_AUTOCOMMIT);
                }
                3 => assert!(query.received_server_err()),
                _ => assert!(query.take_confirmation().is_none()),
            }
            drop(held);
            drop(header);
            drop(query);
            drop(channel);
            server.join().unwrap();
        }
    }
}

#[test]
fn a_multi_frame_logical_row_is_paused_only_after_its_complete_owned_message() {
    use sqlaero_core::memory::{MemoryError, Pool, Scope};
    use std::io::Read;
    use std::sync::Arc;
    const MIB: usize = 1024 * 1024;
    for binary in [false, true] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = mpsc::channel();
        let length = framing::MAX_FRAME_PAYLOAD + 32;
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut sequence = Sequence::default();
            assert_eq!(
                framing::read_message(&mut socket, &mut sequence, 32 * MIB).unwrap(),
                request(binary)
            );
            framing::write_message(&mut socket, &[1], &mut sequence, 32 * MIB).unwrap();
            let mut column = Vec::new();
            mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
                .with_name(b"value")
                .with_column_length(length as u32)
                .serialize(&mut column);
            framing::write_message(&mut socket, &column, &mut sequence, 32 * MIB).unwrap();
            wait.recv_timeout(Duration::from_secs(10)).unwrap();
            let mut row = if binary { vec![0, 0] } else { Vec::new() };
            row.push(0xfe);
            row.extend_from_slice(&(length as u64).to_le_bytes());
            row.resize(row.len() + length, 0x5a);
            assert!(row.len() > framing::MAX_FRAME_PAYLOAD);
            framing::write_message(&mut socket, &row, &mut sequence, 32 * MIB).unwrap();
            framing::write_message(
                &mut socket,
                &[0xfe, 0, 0, 2, 0, 0, 0],
                &mut sequence,
                32 * MIB,
            )
            .unwrap();
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).unwrap(), 0);
        });
        let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
        let mut budget = IoBudget::new(
            TimeoutKind::Operation,
            Some(Duration::from_secs(10)),
            None,
            None,
        );
        while !channel.connected().unwrap() {
            channel.wait(Direction::Write, &mut budget).unwrap();
        }
        let pool = Pool::new(64 * MIB);
        let mut query = Query::with_pool(
            request(binary),
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            8192,
            32 * MIB,
            128 * MIB,
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
        let other = Scope::new(Arc::clone(&pool), 128 * MIB).unwrap();
        let mut held = Buffer::<u8>::tracked(&other);
        // Actual allocated capacity leaves enough for the framing reader's
        // old 16MiB + replacement 32MiB peak, but not the destination copy.
        held.reserve(64 * MIB - pool.used() - 48 * MIB - 64)
            .unwrap();
        assert_eq!(64 * MIB - pool.used(), 48 * MIB + 64);
        release.send(()).unwrap();
        let error = loop {
            match query.advance_stream(&mut channel, &mut budget, NonZeroUsize::MIN) {
                Ok(Step::Control) => (),
                Err(error) => break error,
                _ => panic!("the complete large row still needs destination storage"),
            }
        };
        assert!(
            matches!(error, QueryError::Paused(MemoryError::Total { .. })),
            "{error}"
        );
        assert!(query.resource_paused() && !query.is_finished());
        assert!(query.take_confirmation().is_none());
        assert!(pool.used() <= 64 * MIB);
        drop(held);
        drop(other);
        let StreamEvent::Rows(rows) = next(&mut query, &mut channel, &mut budget).unwrap() else {
            panic!("same complete pending row");
        };
        assert_eq!(rows.row_count, 1);
        let value = rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap();
        assert_eq!(value.len(), length);
        assert!(value.iter().all(|byte| *byte == 0x5a));
        assert!(!query.resource_paused() && !query.is_finished());
        let StreamEvent::End {
            total_rows, tail, ..
        } = next(&mut query, &mut channel, &mut budget).unwrap()
        else {
            panic!("original final packet");
        };
        assert_eq!(total_rows, Some(1));
        assert!(tail.is_none());
        assert!(query.is_finished() && query.take_confirmation().is_some());
        drop(rows);
        drop(header);
        drop(query);
        drop(channel);
        assert_eq!(pool.used(), 0);
        server.join().unwrap();
    }
}
