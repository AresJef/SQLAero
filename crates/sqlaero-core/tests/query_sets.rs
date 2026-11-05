//! Native result boundaries share one command and one packet sequence.

use mysql_common::constants::CapabilityFlags;
use sqlaero_core::result::text::Response;
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::{Channel, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn peer(responses: Vec<Vec<u8>>) -> (Channel, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        read_request(&mut socket);
        for (index, response) in responses.into_iter().enumerate() {
            let size = response.len();
            socket
                .write_all(&[
                    size as u8,
                    (size >> 8) as u8,
                    (size >> 16) as u8,
                    (index + 1) as u8,
                ])
                .unwrap();
            socket.write_all(&response).unwrap();
        }
    });
    (
        Channel::new(Transport::tcp(address).unwrap(), false),
        server,
    )
}

fn read_request(socket: &mut TcpStream) {
    let mut header = [0; 4];
    socket.read_exact(&mut header).unwrap();
    assert_eq!(header[3], 0, "one command begins at sequence zero");
    let size = usize::from(header[0]) | usize::from(header[1]) << 8 | usize::from(header[2]) << 16;
    let mut request = vec![0; size];
    socket.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"\x03CALL example()");
}

fn next(
    query: &mut Query,
    channel: &mut Channel,
    budget: &mut IoBudget,
) -> Result<Response, QueryError> {
    while !channel.connected()? {
        channel.wait(Direction::Write, budget)?;
    }
    loop {
        match query.advance(channel, budget)? {
            Step::Ready(response) => return Ok(response),
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness internally"),
        }
    }
}

fn query() -> Query {
    Query::new(
        b"\x03CALL example()".to_vec(),
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
        4096,
        4096,
        64 * 1024,
    )
    .unwrap()
}

fn budget() -> IoBudget {
    IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(2)),
        None,
        None,
    )
}

#[test]
fn completed_sets_are_returned_in_order_without_resetting_the_wire_sequence() {
    let (mut channel, server) = peer(vec![
        vec![0, 3, 0, 10, 0, 0, 0], // OK, three affected rows, more results.
        vec![0, 7, 0, 2, 0, 1, 0],  // Independent final OK, seven rows, one warning.
    ]);
    let mut query = query();
    let mut budget = budget();
    let first = next(&mut query, &mut channel, &mut budget).unwrap();
    assert_eq!(first.completion().affected_rows(), 3);
    assert!(!query.is_finished());
    let second = next(&mut query, &mut channel, &mut budget).unwrap();
    assert_eq!(second.completion().affected_rows(), 7);
    assert_eq!(second.completion().warnings(), 1);
    assert!(query.is_finished());
    assert!(next(&mut query, &mut channel, &mut budget).is_err());
    assert_eq!(
        first.completion().affected_rows(),
        3,
        "old native ownership remains valid"
    );
    server.join().unwrap();
}

#[test]
fn later_error_does_not_discard_or_reclassify_the_complete_prefix() {
    let (mut channel, server) = peer(vec![
        vec![0, 3, 0, 10, 0, 0, 0],
        b"\xff\x28\x04#42000later syntax failure".to_vec(),
    ]);
    let mut query = query();
    let mut budget = budget();
    let first = next(&mut query, &mut channel, &mut budget).unwrap();
    let error = next(&mut query, &mut channel, &mut budget).err().unwrap();
    assert!(matches!(
        error,
        QueryError::Result(sqlaero_core::result::text::ResultError::Server(_))
    ));
    assert!(query.received_server_err());
    assert!(query.is_finished());
    assert_eq!(first.completion().affected_rows(), 3);
    server.join().unwrap();
}
