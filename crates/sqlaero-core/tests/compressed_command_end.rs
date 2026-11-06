//! Whole-command confirmations require a verified current compression frame.

use sqlaero_core::memory::{Pool, Scope};
use sqlaero_core::protocol::CapabilityFlags;
use sqlaero_core::protocol::framing::{self, MessageReader, Progress, Sequence};
use sqlaero_core::protocol::prepared::{Prepare, Statement};
use sqlaero_core::result::text::{Response, ResultError};
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::{Channel, ChannelError, Step};
use sqlaero_core::transport::compression::{Algorithm, Codec};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

fn extra() -> Vec<u8> {
    let mut state = 0x1234_5678_u32;
    (0..100_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn encoded(algorithm: Algorithm, plain: &[u8], corrupt: bool) -> Vec<u8> {
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
        assert_eq!(algorithm, Algorithm::Zlib);
        *compressed.last_mut().unwrap() ^= 1;
    }
    let mut wire = vec![
        compressed.len() as u8,
        (compressed.len() >> 8) as u8,
        (compressed.len() >> 16) as u8,
        1, // The request consumed compressed sequence zero.
        plain.len() as u8,
        (plain.len() >> 8) as u8,
        (plain.len() >> 16) as u8,
    ];
    wire.extend_from_slice(&compressed);
    wire
}

fn peer(
    request: Vec<u8>,
    algorithm: Algorithm,
    packets: &[Vec<u8>],
    corrupt: bool,
) -> (Channel, IoBudget, JoinHandle<()>) {
    let mut plain = Vec::new();
    let mut sequence = Sequence::new(1);
    for packet in packets {
        framing::write_message(&mut plain, packet, &mut sequence, 256 * 1024).unwrap();
    }
    let wire = encoded(algorithm, &plain, corrupt);
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let scope = Scope::new(Pool::new(1_000_000), 1_000_000).unwrap();
        let mut codec = Codec::new(algorithm, 3).unwrap();
        let mut message = MessageReader::tracked(4096, &scope);
        let mut sequence = Sequence::default();
        loop {
            if let Progress::Ready(bytes) = codec
                .read_message_step(&mut message, &mut socket, &mut sequence, &scope)
                .unwrap()
            {
                assert_eq!(bytes.as_slice(), request);
                break;
            }
        }
        codec.finish_response().unwrap();
        socket.write_all(&wire).unwrap();
        // Rejecting unread corrupt data may close TCP with RST instead of FIN.
        // Either close is final; a byte, timeout or other error is not success.
        match socket.read(&mut [0]) {
            Ok(0) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                ) =>
            {
                ()
            }
            other => panic!("no command replay; expected closed peer, got {other:?}"),
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
    channel.enable_compression(algorithm, 3).unwrap();
    (channel, budget, server)
}

fn exchange(
    algorithm: Algorithm,
    packets: &[Vec<u8>],
    corrupt: bool,
    memory_limit: usize,
) -> (Query, Result<Vec<Response>, QueryError>) {
    let (mut channel, mut budget, server) = peer(b"\x03DO 1".to_vec(), algorithm, packets, corrupt);
    let mut query = Query::new(
        b"\x03DO 1".to_vec(),
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
        4096,
        256 * 1024,
        memory_limit,
    )
    .unwrap();
    let result = (|| {
        let mut responses = Vec::new();
        loop {
            match query.advance(&mut channel, &mut budget)? {
                Step::Ready(response) => {
                    responses.push(response);
                    if query.is_finished() {
                        return Ok(responses);
                    }
                    assert!(query.take_confirmation().is_none());
                }
                Step::Control => (),
                Step::Wait(_) => panic!("Query drives concrete readiness"),
            }
        }
    })();
    channel.shutdown().unwrap();
    server.join().unwrap();
    (query, result)
}

#[test]
fn final_ok_or_err_followed_by_same_frame_bytes_has_no_trusted_terminal_facts() {
    for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
        for packet in [
            vec![0, 0, 0, 2, 0, 0, 0],
            b"\xff\x17\x04#HY000controlled error".to_vec(),
        ] {
            for corrupt in [false, true] {
                if corrupt && algorithm == Algorithm::Zstd {
                    continue;
                }
                let (mut query, result) =
                    exchange(algorithm, &[packet.clone(), extra()], corrupt, 1_000_000);
                assert!(matches!(
                    result,
                    Err(QueryError::Channel(ChannelError::Wire(_)))
                ));
                assert!(query.is_finished());
                assert!(!query.received_server_err());
                assert!(query.take_confirmation().is_none());
            }
        }
    }
}

#[test]
fn verified_frame_end_preserves_actual_ok_err_and_legal_more_results() {
    for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
        let (mut query, result) = exchange(
            algorithm,
            &[vec![0, 0, 0, 10, 0, 0, 0], vec![0, 0, 0, 2, 0, 0, 0]],
            false,
            1_000_000,
        );
        let responses = result.unwrap();
        assert_eq!(responses.len(), 2);
        assert!(query.is_finished());
        assert!(query.take_confirmation().is_some());
        let (mut query, result) = exchange(
            algorithm,
            &[b"\xff\x17\x04#HY000controlled error".to_vec()],
            false,
            1_000_000,
        );
        assert!(matches!(
            result,
            Err(QueryError::Result(ResultError::Server(_)))
        ));
        assert!(query.received_server_err());
        assert!(query.take_confirmation().is_none());
    }
}

#[test]
fn only_verified_terminal_facts_survive_later_completion_storage_failure() {
    for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
        let mut packet = vec![0, 0, 0, 2, 0, 0, 0, 0xfd, 0xf0, 0x49, 2];
        packet.resize(150_011, b'a');
        let (mut query, result) = exchange(algorithm, &[packet.clone()], false, 299_000);
        assert!(
            matches!(result, Err(QueryError::Result(ResultError::Memory(_)))),
            "{algorithm:?}: {:?}",
            result.as_ref().err()
        );
        assert!(query.take_confirmation().is_some());
        let (mut query, result) = exchange(algorithm, &[packet, extra()], false, 299_000);
        assert!(matches!(
            result,
            Err(QueryError::Channel(ChannelError::Wire(_)))
        ));
        assert!(!query.received_server_err());
        assert!(query.take_confirmation().is_none());
    }
}

#[test]
fn prepare_validates_its_actual_control_terminator_without_trusting_extra_bytes() {
    for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
        let acknowledgement = vec![0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0];
        let failure = b"\xff\x17\x04#HY000controlled prepare error".to_vec();
        for packet in [acknowledgement, failure] {
            for trailing in [false, true] {
                let mut packets = vec![packet.clone()];
                if trailing {
                    packets.push(extra());
                }
                let request = b"\x16DO 1".to_vec();
                let (mut channel, mut budget, server) =
                    peer(request.clone(), algorithm, &packets, false);
                let mut prepare = Prepare::new(
                    request,
                    CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
                    4096,
                    256 * 1024,
                    1_000_000,
                    Pool::new(1_000_000),
                )
                .unwrap();
                let result = (|| {
                    loop {
                        match prepare.advance(&mut channel, &mut budget)? {
                            Step::Ready(statement) => {
                                return Ok::<Statement, QueryError>(statement);
                            }
                            Step::Control => (),
                            Step::Wait(_) => panic!("Prepare drives concrete readiness"),
                        }
                    }
                })();
                if trailing {
                    assert!(matches!(
                        result,
                        Err(QueryError::Channel(ChannelError::Wire(_)))
                    ));
                    assert_eq!(prepare.statement_id(), None);
                    assert!(!prepare.received_server_err());
                } else if packet[0] == 0xff {
                    assert!(matches!(
                        result,
                        Err(QueryError::Result(ResultError::Server(_)))
                    ));
                    assert!(prepare.received_server_err());
                } else {
                    let statement = result.unwrap();
                    assert_eq!(statement.id, 17);
                    assert_eq!(statement.warnings, 3);
                }
                assert!(prepare.is_finished());
                channel.shutdown().unwrap();
                server.join().unwrap();
            }
        }
    }
}
