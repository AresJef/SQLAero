//! Actual LOCAL uploads over the existing plain/TLS/compressed channel.

use sqlaero_core::charset;
use sqlaero_core::protocol::CapabilityFlags;
use sqlaero_core::protocol::local_infile::{Source, UploadProgress};
use sqlaero_core::result::text::{Response, ResultError, StreamEvent};
use sqlaero_core::session::connect::{
    Authenticated, ConnectParams, ConnectTls, Endpoint, Establish,
};
use sqlaero_core::session::initialize::{Initialize, InitializeError, Options};
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::Step;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use sqlaero_core::transport::{compression, tls};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const LIMIT: usize = 16 * 1024 * 1024;
fn connect_mode(label: &str, compression: compression::Options, secure: bool) -> Authenticated {
    try_connect_mode(label, compression, secure).unwrap()
}

fn try_connect_mode(
    label: &str,
    compression: compression::Options,
    secure: bool,
) -> Result<Authenticated, InitializeError> {
    let root = PathBuf::from(
        std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").expect("set the task-owned fixture path"),
    );
    let state: mysql_common::serde_json::Value = mysql_common::serde_json::from_slice(
        &std::fs::read(root.join(label).join("state.json")).unwrap(),
    )
    .unwrap();
    let parameters = ConnectParams {
        endpoint: if secure {
            Endpoint::Tcp {
                host: "127.0.0.1".to_owned(),
                port: state["port"].as_str().unwrap().parse().unwrap(),
            }
        } else {
            Endpoint::Unix(root.join(format!("{label}.sock")))
        },
        user: "sqlaero".to_owned(),
        password: Zeroizing::new(state["test_password"].as_str().unwrap().as_bytes().to_vec()),
        database: Some("sqlaero_test".to_owned()),
        tls: secure.then(|| ConnectTls {
            minimum: tls::Version::Tls12,
            maximum: None,
            config: tls::client_config(
                tls::Mode::VerifyIdentity,
                tls::Version::Tls12,
                None,
                tls::CaMode::Replace,
                Some(tls::certificates(&std::fs::read(root.join("tls/ca.pem")).unwrap()).unwrap()),
                None,
            )
            .unwrap()
            .unwrap(),
            name: "127.0.0.1".to_owned().try_into().unwrap(),
        }),
        server_public_key: None,
        compression,
        multi_statements: false,
        local_infile: true,
        connect_timeout: Some(Duration::from_secs(5)),
        read_timeout: Some(Duration::from_secs(3)),
        write_timeout: Some(Duration::from_secs(3)),
        max_send: LIMIT,
        max_receive: LIMIT,
    };
    let options = Options {
        encoding: charset::resolve(None, None).unwrap(),
        autocommit: true,
        sql_mode: None,
        server_timeouts: [None, None, None, None],
        result_memory_limit: LIMIT,
    };
    let mut job = Initialize::new(Establish::new(parameters)?, options);
    loop {
        match job.advance()? {
            Step::Ready(ready) => return Ok(ready.transport),
            Step::Control => (),
            Step::Wait(_) => panic!("initialization drives its own concrete wait"),
        }
    }
}

// Match the existing isolated fixture's administration mechanism. Permit all
// supported algorithms so restoration still works when raw transport is denied.
fn fixture_sql(label: &str, sql: &str) -> Result<String, String> {
    let root = PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").unwrap());
    let state: mysql_common::serde_json::Value = mysql_common::serde_json::from_slice(
        &std::fs::read(root.join(label).join("state.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut child = Command::new(state["mysql"].as_str().unwrap())
        .arg(format!(
            "--defaults-file={}",
            root.join(label).join("root.cnf").display()
        ))
        .args([
            "--no-login-paths",
            "--batch",
            "--skip-column-names",
            "--compression-algorithms=zlib,zstd,uncompressed",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(sql.as_bytes())
        .map_err(|error| error.to_string())?;
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "isolated fixture administration failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn budget() -> IoBudget {
    IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(3)),
        Some(Duration::from_secs(3)),
    )
}

fn query(connection: &mut Authenticated, request: Vec<u8>) -> Result<Response, QueryError> {
    let mut query = Query::new(request, connection.capabilities, LIMIT, LIMIT, LIMIT)?;
    let mut budget = budget();
    loop {
        match query.advance(&mut connection.channel, &mut budget)? {
            Step::Ready(result) => {
                assert!(query.is_finished());
                return Ok(result);
            }
            Step::Control => (),
            Step::Wait(_) => panic!("query drives its own concrete wait"),
        }
    }
}

fn sql(connection: &mut Authenticated, sql: &str) -> Result<Response, QueryError> {
    let mut request = vec![3];
    request.extend_from_slice(sql.as_bytes());
    query(connection, request)
}

struct LocalFile {
    path: PathBuf,
    original: PathBuf,
}
impl LocalFile {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").unwrap());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!(
            "C导入🙂-{}-{nonce}-{}.tsv",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = Self {
            original: path.with_extension("opened"),
            path,
        };
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file.path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        file
    }
    fn replace_path(&self) {
        fs::rename(&self.path, &self.original).unwrap();
        fs::write(&self.path, b"replacement must not be uploaded").unwrap();
    }
}
impl Drop for LocalFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(&self.original);
    }
}

fn load(
    connection: &mut Authenticated,
    source: Source,
    sql: &str,
    mut budget: IoBudget,
) -> (Result<Response, QueryError>, UploadProgress, bool) {
    let mut request = vec![3];
    request.extend_from_slice(sql.as_bytes());
    let mut job = Query::new(request, connection.capabilities, 4096, LIMIT, LIMIT)
        .unwrap()
        .with_local_infile(source)
        .unwrap();
    let result = loop {
        match job.advance(&mut connection.channel, &mut budget) {
            Ok(Step::Ready(response)) => break Ok(response),
            Ok(Step::Control) => (),
            Ok(Step::Wait(_)) => panic!("Query drives concrete readiness"),
            Err(error) => break Err(error),
        }
    };
    assert!(job.is_finished());
    (
        result,
        job.local_progress().unwrap(),
        job.received_server_err(),
    )
}

fn local_cases(label: &str) {
    for (algorithm, secure) in [("uncompressed", false), ("zlib", false), ("zstd", true)] {
        let options =
            compression::Options::new(compression::Algorithms::from_csv(algorithm).unwrap(), 3)
                .unwrap();
        let mut connection = connect_mode(label, options, secure);
        assert!(
            connection
                .capabilities
                .contains(CapabilityFlags::CLIENT_LOCAL_FILES)
        );
        sql(
            &mut connection,
            "CREATE TEMPORARY TABLE c_local_data(id INT PRIMARY KEY, txt TEXT)",
        )
        .unwrap();
        let mut text = String::new();
        for id in 1..=10_000 {
            use std::fmt::Write;
            writeln!(&mut text, "{id}\t中文🙂-{id}").unwrap();
        }
        let file = LocalFile::new(text.as_bytes());
        let operation = budget();
        operation.check().unwrap();
        let source = Source::open(&file.path).unwrap();
        operation.check().unwrap();
        file.replace_path();
        let (result, progress, server_error) = load(
            &mut connection,
            source,
            "LOAD DATA LOCAL INFILE '/this/server/label/does/not/select/a/local/path/🙂' INTO TABLE c_local_data CHARACTER SET utf8mb4",
            operation,
        );
        assert!(!server_error);
        assert_eq!(result.unwrap().completion().affected_rows(), 10_000);
        assert!(progress.opening_length > 4096);
        assert_eq!(
            progress,
            UploadProgress {
                opening_length: text.len() as u64,
                read_bytes: text.len() as u64,
                flushed_bytes: text.len() as u64,
                eof_flushed: true
            }
        );
        let Response::Rows(rows) = sql(
            &mut connection,
            "SELECT COUNT(*),MIN(id),MAX(id),SUM(txt=CONCAT('中文🙂-',id)) FROM c_local_data",
        )
        .unwrap() else {
            panic!("expected actual imported rows");
        };
        let values: Vec<_> = rows
            .row(0)
            .unwrap()
            .iter()
            .map(|cell| cell.get(&rows.bytes).unwrap().unwrap())
            .collect();
        assert_eq!(values, [b"10000".as_slice(), b"1", b"10000", b"10000"]);
        sql(&mut connection, "TRUNCATE TABLE c_local_data").unwrap();
        let empty = LocalFile::new(b"");
        let operation = budget();
        let source = Source::open(&empty.path).unwrap();
        operation.check().unwrap();
        let (result, progress, _) = load(
            &mut connection,
            source,
            "LOAD DATA LOCAL INFILE 'any-empty-label' INTO TABLE c_local_data CHARACTER SET utf8mb4",
            operation,
        );
        assert_eq!(result.unwrap().completion().affected_rows(), 0);
        assert_eq!(
            progress,
            UploadProgress {
                opening_length: 0,
                read_bytes: 0,
                flushed_bytes: 0,
                eof_flushed: true
            }
        );
        let mut wrapped = String::new();
        for id in 1..=70_000 {
            use std::fmt::Write;
            writeln!(&mut wrapped, "{id}\tstreamed-sequence-wrap").unwrap();
        }
        assert!(wrapped.len() > 256 * 4096);
        let data = LocalFile::new(wrapped.as_bytes());
        let mut operation = budget();
        let source = Source::open(&data.path).unwrap();
        operation.check().unwrap();
        let mut job = Query::new(b"\x03LOAD DATA LOCAL INFILE 'stream-source' INTO TABLE c_local_data CHARACTER SET utf8mb4".to_vec(), connection.capabilities, 4096, LIMIT, LIMIT)
            .unwrap().with_local_infile(source).unwrap().with_streaming().unwrap();
        loop {
            match job
                .advance_stream(
                    &mut connection.channel,
                    &mut operation,
                    NonZeroUsize::new(1).unwrap(),
                )
                .unwrap()
            {
                Step::Control => (),
                Step::Wait(_) => panic!("Query drives concrete readiness"),
                Step::Ready(StreamEvent::End {
                    tail,
                    completion,
                    total_rows,
                    metadata_status,
                }) => {
                    assert!(tail.is_none());
                    assert_eq!(total_rows, None);
                    assert_eq!(metadata_status, None);
                    assert_eq!(completion.affected_rows(), 70_000);
                    assert!(job.is_finished());
                    assert!(job.at_stream_boundary());
                    assert!(job.local_progress().unwrap().eof_flushed);
                    assert_eq!(
                        job.local_progress().unwrap().flushed_bytes,
                        wrapped.len() as u64
                    );
                    break;
                }
                Step::Ready(_) => panic!("LOAD must not invent a row header or row batch"),
            }
        }
        sql(&mut connection, "DO 1").unwrap();
    }
}

fn local_server_errors(label: &str) {
    let mut connection = connect_mode(label, compression::Options::default(), false);
    sql(
        &mut connection,
        "CREATE TEMPORARY TABLE c_local_disabled(id INT)",
    )
    .unwrap();
    fixture_sql(label, "SET GLOBAL local_infile=OFF").unwrap();
    let file = LocalFile::new(b"1\n2\n");
    let operation = budget();
    let source = Source::open(&file.path).unwrap();
    operation.check().unwrap();
    let (result, progress, server_error) = load(
        &mut connection,
        source,
        "LOAD DATA LOCAL INFILE 'server-disabled' INTO TABLE c_local_disabled",
        operation,
    );
    assert!(matches!(
        result,
        Err(QueryError::Result(ResultError::Server(_)))
    ));
    assert!(server_error);
    assert_eq!(progress.read_bytes, 0);
    assert!(!progress.eof_flushed);
    sql(&mut connection, "DO 1").unwrap();
    fixture_sql(label, "SET GLOBAL local_infile=ON").unwrap();

    sql(
        &mut connection,
        "DROP TABLE IF EXISTS c_local_trigger_error",
    )
    .unwrap();
    sql(
        &mut connection,
        "CREATE TABLE c_local_trigger_error(id INT)",
    )
    .unwrap();
    fixture_sql(label, "CREATE TRIGGER sqlaero_test.c_local_error BEFORE INSERT ON sqlaero_test.c_local_trigger_error FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT='actual LOCAL trigger failure'").unwrap();
    let operation = budget();
    let source = Source::open(&file.path).unwrap();
    operation.check().unwrap();
    let (result, progress, server_error) = load(
        &mut connection,
        source,
        "LOAD DATA LOCAL INFILE 'error-after-upload' INTO TABLE c_local_trigger_error",
        operation,
    );
    let Err(QueryError::Result(ResultError::Server(error))) = result else {
        panic!("expected actual post-upload server ERR");
    };
    assert_eq!(error.error_code(), 1644);
    assert!(server_error);
    assert!(progress.eof_flushed);
    assert_eq!(progress.flushed_bytes, file.path.metadata().unwrap().len());
    sql(&mut connection, "DO 1").unwrap();
    sql(&mut connection, "DROP TABLE c_local_trigger_error").unwrap();
}

#[test]
#[ignore = "requires C's two owned MySQL fixtures and changes/restores only their LOCAL setting"]
fn real_local_uses_only_the_opened_source_and_preserves_actual_final_responses() {
    for label in ["84", "97"] {
        let original = fixture_sql(label, "SELECT @@GLOBAL.local_infile")
            .unwrap()
            .trim()
            .to_owned();
        assert!(matches!(original.as_str(), "0" | "1"));
        let outcome = std::panic::catch_unwind(|| {
            fixture_sql(label, "SET GLOBAL local_infile=ON").unwrap();
            local_cases(label);
            local_server_errors(label);
        });
        let restored = fixture_sql(label, &format!("SET GLOBAL local_infile={original}"));
        if let Err(payload) = outcome {
            if let Err(error) = restored {
                eprintln!("LOCAL setting restoration also failed: {error}");
            }
            std::panic::resume_unwind(payload);
        }
        restored.unwrap();
        assert_eq!(
            fixture_sql(label, "SELECT @@GLOBAL.local_infile")
                .unwrap()
                .trim(),
            original
        );
    }
}
