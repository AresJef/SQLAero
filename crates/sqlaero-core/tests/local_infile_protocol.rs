//! LOCAL authorization is distinct from capability and ordinary NULL row data.

use mysql_common::constants::{CapabilityFlags, ColumnType};
use mysql_common::proto::MySerialize;
use sqlaero_core::protocol::framing::{self, Sequence};
use sqlaero_core::protocol::local_infile::{LocalError, Source};
use sqlaero_core::result::text::Response;
use sqlaero_core::session::query::{Query, QueryError, RequestProgress};
use sqlaero_core::transport::channel::{Channel, Step};
use sqlaero_core::transport::deadline::{Direction, IoBudget, TimeoutKind};
use sqlaero_core::transport::socket::Transport;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOAD: &[u8] = b"\x03LOAD DATA LOCAL INFILE 'logical-label' INTO TABLE t";

#[test]
fn native_protocol_states_satisfy_the_host_detached_work_send_bound() {
    fn send<T: Send>() {}
    fn share<T: Send + Sync>() {}
    send::<Source>();
    send::<sqlaero_core::protocol::local_infile::Upload>();
    send::<sqlaero_core::protocol::prepared::Parameters>();
    send::<sqlaero_core::protocol::prepared::Prepare>();
    send::<sqlaero_core::protocol::prepared::Close>();
    send::<sqlaero_core::transport::compression::Codec>();
    send::<Channel>();
    send::<sqlaero_core::result::text::RowBatch>();
    send::<sqlaero_core::result::text::Header>();
    send::<sqlaero_core::result::text::StreamEvent>();
    send::<sqlaero_core::bind::value::ValueRef<'static>>();
    share::<sqlaero_core::transport::compression::Options>();
    share::<sqlaero_core::bind::Context>();
}

struct LocalFile(PathBuf);
impl LocalFile {
    fn open(bytes: &[u8]) -> (Self, Source) {
        use std::io::Write;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sqlaero-c-query-local-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        let source = Source::open(&path).unwrap();
        (Self(path), source)
    }
}
impl Drop for LocalFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn peer(responses: Vec<Vec<u8>>) -> (Channel, JoinHandle<()>) {
    response_peer(responses, b"\x03SELECT NULL")
}

fn response_peer(responses: Vec<Vec<u8>>, expected: &'static [u8]) -> (Channel, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096)
                .unwrap()
                .as_slice(),
            expected
        );
        for response in responses {
            framing::write_message(&mut socket, &response, &mut sequence, 4096).unwrap();
        }
        // The command owner either retires an uncertain unauthorized request
        // or closes the completed test channel. It must never upload a path.
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
    });
    (
        Channel::new(Transport::tcp(address).unwrap(), false),
        server,
    )
}

fn authorized_query(source: Source) -> Query {
    Query::new(
        LOAD.to_vec(),
        CapabilityFlags::CLIENT_PROTOCOL_41
            | CapabilityFlags::CLIENT_DEPRECATE_EOF
            | CapabilityFlags::CLIENT_LOCAL_FILES,
        4096,
        4096,
        256 * 1024,
    )
    .unwrap()
    .with_local_infile(source)
    .unwrap()
}

fn upload_peer(expected: Vec<u8>, normal_eof: bool) -> (Channel, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut sequence = Sequence::default();
        assert_eq!(
            framing::read_message(&mut socket, &mut sequence, 4096)
                .unwrap()
                .as_slice(),
            LOAD
        );
        framing::write_message(&mut socket, b"\xfb/first/request", &mut sequence, 4096).unwrap();
        let mut received = Vec::new();
        loop {
            match framing::read_message(&mut socket, &mut sequence, 4096) {
                Ok(bytes) if bytes.is_empty() => {
                    assert!(normal_eof, "truncated source cannot send a normal EOF");
                    break;
                }
                Ok(bytes) => received.extend_from_slice(&bytes),
                Err(framing::WireError::Io(error))
                    if !normal_eof && error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => panic!("invalid upload continuation: {error}"),
            }
        }
        assert_eq!(received, expected);
        if normal_eof {
            framing::write_message(&mut socket, b"\xfb/second/request", &mut sequence, 4096)
                .unwrap();
            assert_eq!(
                socket.read(&mut [0]).unwrap(),
                0,
                "a second request cannot upload another source or EOF"
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
        Some(Duration::from_secs(2)),
        None,
        None,
    );
    while !channel.connected()? {
        channel.wait(Direction::Write, &mut budget)?;
    }
    loop {
        match query.advance(channel, &mut budget)? {
            Step::Ready(response) => return Ok(response),
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives its concrete readiness waits"),
        }
    }
}

fn query(capability: bool) -> Query {
    let mut capabilities =
        CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF;
    if capability {
        capabilities.insert(CapabilityFlags::CLIENT_LOCAL_FILES);
    }
    Query::new(
        b"\x03SELECT NULL".to_vec(),
        capabilities,
        4096,
        4096,
        64 * 1024,
    )
    .unwrap()
}

#[test]
fn capability_alone_never_authorizes_a_server_selected_source() {
    for capability in [false, true] {
        let (mut channel, server) = peer(vec![b"\xfb/server/cannot/choose/a/source".to_vec()]);
        let mut query = query(capability);
        let error = advance(&mut query, &mut channel).err().unwrap();
        assert!(!matches!(error, QueryError::Channel(_)));
        assert!(query.is_finished());
        assert!(!query.received_server_err());
        assert_eq!(query.progress(), RequestProgress::AwaitingResponse);
        channel.shutdown().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn first_null_text_field_is_row_data_even_when_local_capability_is_enabled() {
    let mut column = Vec::new();
    mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
        .with_name(b"value")
        .with_character_set(45)
        .serialize(&mut column);
    let (mut channel, server) = peer(vec![
        vec![1],
        column,
        vec![0xfb],
        vec![0xfe, 0, 0, 2, 0, 0, 0],
    ]);
    let mut query = query(true);
    let Response::Rows(rows) = advance(&mut query, &mut channel).unwrap() else {
        panic!("expected ordinary text rows");
    };
    assert!(query.is_finished());
    assert_eq!(rows.row_count, 1);
    assert_eq!(rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(), None);
    channel.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn first_non_local_response_consumes_the_source_permission_before_null_rows() {
    let mut column = Vec::new();
    mysql_common::packets::Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
        .with_name(b"value")
        .with_character_set(45)
        .serialize(&mut column);
    let (mut channel, server) = response_peer(
        vec![vec![1], column, vec![0xfb], vec![0xfe, 0, 0, 2, 0, 0, 0]],
        LOAD,
    );
    let (_file, source) = LocalFile::open(b"never read this source");
    let mut query = authorized_query(source);
    let Response::Rows(rows) = advance(&mut query, &mut channel).unwrap() else {
        panic!("expected received row");
    };
    assert!(query.is_finished());
    assert_eq!(rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(), None);
    let progress = query.local_progress().unwrap();
    assert_eq!(progress.read_bytes, 0);
    assert_eq!(progress.flushed_bytes, 0);
    assert!(!progress.eof_flushed);
    channel.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn second_request_after_eof_is_rejected_without_extending_the_permission() {
    let data = b"one authorized file";
    let (_file, source) = LocalFile::open(data);
    let mut query = authorized_query(source);
    let (mut channel, server) = upload_peer(data.to_vec(), true);
    let error = advance(&mut query, &mut channel).err().unwrap();
    assert!(!matches!(error, QueryError::Channel(_)));
    assert!(query.is_finished());
    assert!(!query.received_server_err());
    let progress = query.local_progress().unwrap();
    assert_eq!(progress.flushed_bytes, data.len() as u64);
    assert!(progress.eof_flushed);
    channel.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn mid_upload_read_error_keeps_query_facts_and_never_sends_normal_eof() {
    let (file, source) = LocalFile::open(&vec![b'x'; 100_000]);
    OpenOptions::new()
        .write(true)
        .open(&file.0)
        .unwrap()
        .set_len(50_000)
        .unwrap();
    let mut query = authorized_query(source);
    let (mut channel, server) = upload_peer(vec![b'x'; 50_000], false);
    assert!(
        matches!(advance(&mut query, &mut channel), Err(QueryError::Local(LocalError::Source(error))) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    assert!(query.is_finished());
    assert!(!query.received_server_err());
    assert_eq!(query.progress(), RequestProgress::AwaitingResponse);
    let progress = query.local_progress().unwrap();
    assert_eq!(progress.opening_length, 100_000);
    assert_eq!(progress.read_bytes, 50_000);
    assert_eq!(progress.flushed_bytes, 50_000);
    assert!(!progress.eof_flushed);
    channel.shutdown().unwrap();
    server.join().unwrap();
}
