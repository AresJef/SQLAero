//! Exact COM_SET_OPTION acknowledgement context on the shared Query driver.

use sqlaero_core::protocol::framing::{self, Sequence};
use sqlaero_core::protocol::{CapabilityFlags, StatusFlags};
use sqlaero_core::result::text::Response;
use sqlaero_core::session::query::{Query, QueryError, RequestProgress};
use sqlaero_core::transport::channel::{Channel, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

fn capabilities() -> CapabilityFlags {
    CapabilityFlags::CLIENT_PROTOCOL_41
        | CapabilityFlags::CLIENT_SESSION_TRACK
        | CapabilityFlags::CLIENT_DEPRECATE_EOF
}
fn job(request: Vec<u8>) -> Result<Query, QueryError> {
    Query::new(request, capabilities(), 4096, 4096, 64 * 1024)
}
fn peer(request: Vec<u8>, response: Option<Vec<u8>>, close: bool) -> (Channel, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
            request
        );
        if let Some(response) = response {
            framing::write_message(&mut socket, &response, &mut sequence, 4096).unwrap();
        }
        if !close {
            assert_eq!(
                socket.read(&mut [0]).unwrap(),
                0,
                "control request must not be replayed"
            );
        }
    });
    (
        Channel::new(Transport::tcp(address).unwrap(), false),
        server,
    )
}
fn advance(query: &mut Query, channel: &mut Channel) -> Result<Response, QueryError> {
    let mut budget = IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_millis(100)),
        Some(Duration::from_millis(40)),
        None,
    );
    while !channel.connected()? {
        channel.wait(Direction::Write, &mut budget)?;
    }
    loop {
        match query.advance(channel, &mut budget)? {
            Step::Ready(response) => return Ok(response),
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness"),
        }
    }
}

#[test]
fn only_exact_set_option_requests_accept_legacy_eof_or_compatible_ok_ack() {
    assert_eq!(mysql_common::constants::Command::COM_SET_OPTION as u8, 0x1b);
    for option in [0, 1] {
        for ack in [
            vec![0xfe, 3, 0, 3, 0],
            vec![0, 0, 0, 3, 0, 3, 0, 0],
            vec![0xfe, 0, 0, 3, 0, 3, 0, 0],
        ] {
            let request = vec![0x1b, option, 0];
            let (mut channel, server) = peer(request.clone(), Some(ack), false);
            let mut query = job(request).unwrap();
            let response = advance(&mut query, &mut channel).unwrap();
            assert!(matches!(response, Response::Command(_)));
            assert_eq!(response.completion().warnings(), 3);
            assert_eq!(
                response.completion().status_flags(),
                StatusFlags::SERVER_STATUS_IN_TRANS | StatusFlags::SERVER_STATUS_AUTOCOMMIT
            );
            assert!(query.is_finished());
            assert!(query.take_confirmation().is_some());
            channel.shutdown().unwrap();
            server.join().unwrap();
        }
    }
    for request in [
        vec![0x1b],
        vec![0x1b, 0],
        vec![0x1b, 2, 0],
        vec![0x1b, 0, 1],
        vec![0x1b, 0, 0, 0],
    ] {
        assert!(job(request).is_err());
    }
}

#[test]
fn control_errors_truncation_and_unexpected_headers_cannot_be_success() {
    for response in [
        vec![],
        vec![0xfe],
        vec![0xfe, 0],
        vec![0xfe, 0, 0],
        vec![0xfe, 0, 0, 3],
        vec![0xfe, 0, 0, 3, 0, 0],
        vec![1],
        vec![0xfb],
        vec![0xfe, 0, 0, 10, 0],
    ] {
        let request = vec![0x1b, 0, 0];
        let (mut channel, server) = peer(request.clone(), Some(response), false);
        let mut query = job(request).unwrap();
        let error = advance(&mut query, &mut channel).err().unwrap();
        assert!(!matches!(error, QueryError::Channel(_)));
        assert!(query.is_finished());
        assert!(!query.received_server_err());
        assert!(query.take_confirmation().is_none());
        channel.shutdown().unwrap();
        server.join().unwrap();
    }
    let request = vec![0x1b, 1, 0];
    let (mut channel, server) = peer(
        request.clone(),
        Some(b"\xff\x17\x04#HY000controlled rejection".to_vec()),
        false,
    );
    let mut query = job(request).unwrap();
    assert!(
        matches!(advance(&mut query, &mut channel), Err(QueryError::Result(sqlaero_core::result::text::ResultError::Server(error))) if error.error_code()==1047)
    );
    assert!(query.received_server_err());
    assert!(query.take_confirmation().is_none());
    channel.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn ordinary_query_does_not_inherit_the_control_eof_ack_context() {
    for response in [vec![0xfe, 0, 0, 2, 0], vec![0xfe, 0, 0, 2, 0, 0, 0, 0]] {
        let request = b"\x03SELECT 1".to_vec();
        let (mut channel, server) = peer(request.clone(), Some(response), false);
        let mut query = job(request).unwrap();
        assert!(advance(&mut query, &mut channel).is_err());
        assert!(query.take_confirmation().is_none());
        channel.shutdown().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn after_send_eof_or_timeout_preserves_unknown_control_outcome_without_replay() {
    for close in [false, true] {
        let request = vec![0x1b, 0, 0];
        let (mut channel, server) = peer(request.clone(), None, close);
        let mut query = job(request).unwrap();
        assert!(matches!(
            advance(&mut query, &mut channel),
            Err(QueryError::Channel(_))
        ));
        assert!(query.is_finished());
        assert_eq!(query.progress(), RequestProgress::AwaitingResponse);
        assert!(!query.received_server_err());
        assert!(query.take_confirmation().is_none());
        if !close {
            channel.shutdown().unwrap();
        }
        server.join().unwrap();
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096).unwrap(),
            vec![0x1b, 0, 0]
        );
        // Advertise the full five-byte EOF but end during its body.
        socket.write_all(&[5, 0, 0, 1, 0xfe, 0]).unwrap();
    });
    let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
    let mut query = job(vec![0x1b, 0, 0]).unwrap();
    assert!(matches!(
        advance(&mut query, &mut channel),
        Err(QueryError::Channel(_))
    ));
    assert_eq!(query.progress(), RequestProgress::AwaitingResponse);
    assert!(!query.received_server_err());
    assert!(query.take_confirmation().is_none());
    server.join().unwrap();
}
