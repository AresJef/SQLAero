//! Verify the packet parser against the two already isolated local servers.

use sqlaero_core::protocol::{framing, packets};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

fn greeting(mut transport: Transport, prefix: &[u8]) {
    let mut budget = IoBudget::new(
        TimeoutKind::Connect,
        Some(Duration::from_secs(2)),
        None,
        None,
    );
    while !transport.connected().unwrap() {
        transport.wait(Direction::Write, &mut budget).unwrap();
    }
    let mut sequence = framing::Sequence::default();
    let mut message = framing::MessageReader::new(64 * 1024);
    let data = loop {
        match message
            .step(&mut transport.io(&mut budget), &mut sequence)
            .unwrap()
        {
            framing::Progress::Ready(data) => break data,
            framing::Progress::Pending => {
                transport.wait(Direction::Read, &mut budget).unwrap();
            }
            framing::Progress::Yield => {
                budget.check().unwrap();
            }
        }
    };
    let greeting = packets::handshake(&data).unwrap();
    assert!(greeting.server_version_ref().starts_with(prefix));
    assert_eq!(greeting.protocol_version(), 10);
    assert_eq!(
        greeting.auth_plugin_name_ref(),
        Some(b"caching_sha2_password".as_slice())
    );
    assert_eq!(greeting.nonce().len(), 20);
    assert_eq!(sequence.next(), 1);
}

#[test]
#[ignore = "requires the two isolated local MySQL servers"]
fn both_mysql_lines_provide_a_valid_protocol_10_greeting() {
    #[cfg(unix)]
    let root = std::env::var_os("SQLAERO_TEST_MYSQL_ROOT")
        .map(PathBuf::from)
        .expect("set SQLAERO_TEST_MYSQL_ROOT to the isolated fixture directory");
    for (port, prefix) in [(13384, b"8.4.".as_slice()), (13397, b"9.7.".as_slice())] {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        greeting(Transport::tcp(address).unwrap(), prefix);
        #[cfg(unix)]
        {
            let label = if port == 13384 { "84" } else { "97" };
            greeting(
                Transport::unix(&root.join(format!("{label}.sock"))).unwrap(),
                prefix,
            );
        }
    }
}

#[test]
fn no_progress_read_expires_without_resetting_the_connection_budget() {
    use sqlaero_core::transport::socket::TransportError;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, waiting) = mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (_socket, _) = listener.accept().unwrap();
        let _ = waiting.recv();
    });
    let mut transport = Transport::tcp(address).unwrap();
    let mut budget = IoBudget::new(
        TimeoutKind::Connect,
        Some(Duration::from_secs(2)),
        Some(Duration::from_millis(20)),
        None,
    );
    while !transport.connected().unwrap() {
        transport.wait(Direction::Write, &mut budget).unwrap();
    }
    let mut message = framing::MessageReader::new(64);
    assert!(matches!(
        message
            .step(
                &mut transport.io(&mut budget),
                &mut framing::Sequence::default()
            )
            .unwrap(),
        framing::Progress::Pending
    ));
    assert!(matches!(
        transport.wait(Direction::Read, &mut budget),
        Err(TransportError::Timeout(TimeoutKind::Read))
    ));
    stop.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn no_progress_write_expires_against_real_socket_backpressure() {
    use sqlaero_core::transport::channel::{Channel, ChannelError, Step};
    use sqlaero_core::transport::socket::TransportError;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    socket2::SockRef::from(&listener)
        .set_recv_buffer_size(4096)
        .unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, waiting) = mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (_socket, _) = listener.accept().unwrap();
        let _ = waiting.recv_timeout(Duration::from_secs(3));
    });
    let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
    let mut budget = IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(2)),
        None,
        Some(Duration::from_millis(20)),
    );
    while !channel.connected().unwrap() {
        channel.wait(Direction::Write, &mut budget).unwrap();
    }
    let payload = vec![b'x'; 8 * 1024 * 1024];
    let mut writer = framing::MessageWriter::new(payload.len(), payload.len()).unwrap();
    let outcome = loop {
        match channel.send_step(&mut writer, &payload, &mut budget) {
            Ok(Step::Wait(direction)) => {
                if let Err(error) = channel.wait(direction, &mut budget) {
                    break error;
                }
            }
            Ok(Step::Control) => (),
            Ok(Step::Ready(())) => {
                panic!("a non-reading 4KiB peer must not accept the complete 8MiB upload")
            }
            Err(error) => break error,
        }
    };
    assert!(matches!(
        outcome,
        ChannelError::Timeout(TimeoutKind::Write)
            | ChannelError::Transport(TransportError::Timeout(TimeoutKind::Write))
    ));
    stop.send(()).unwrap();
    server.join().unwrap();
}
