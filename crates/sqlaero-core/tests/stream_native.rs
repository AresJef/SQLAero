//! Actual streaming over the shared text, binary and compressed exchange.

use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{ValueRef, ValueSink};
use sqlaero_core::charset::{self, codec::Codec};
use sqlaero_core::memory::Pool;
use sqlaero_core::protocol::prepared::{Close, Parameters, Prepare, Statement};
use sqlaero_core::protocol::{CapabilityFlags, StatusFlags, binary};
use sqlaero_core::result::text::{Response, RowBatch, StreamEvent};
use sqlaero_core::result::value::FieldValue;
use sqlaero_core::session::connect::{
    Authenticated, ConnectParams, ConnectTls, Endpoint, Establish,
};
use sqlaero_core::session::initialize::{Initialize, InitializeError, Options};
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::Step;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use sqlaero_core::transport::{compression, tls};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;
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
        endpoint: if secure || cfg!(windows) {
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
            minimum: tls::Version::Tls12,
            maximum: None,
        }),
        server_public_key: (cfg!(windows) && !secure).then(|| {
            sqlaero_core::session::connect::PublicKeySource::Path(
                root.join(label).join("data/public_key.pem"),
            )
        }),
        compression,
        multi_statements: false,
        local_infile: false,
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

fn budget() -> IoBudget {
    IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(3)),
        Some(Duration::from_secs(3)),
    )
}

fn context() -> Context {
    Context {
        codec: Codec::new(charset::resolve(None, None).unwrap().charset),
        no_backslash_escapes: false,
        utc_confirmed: true,
        limit: LIMIT,
    }
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

fn prepare(connection: &mut Authenticated, sql: &str) -> Result<Statement, QueryError> {
    let mut request = vec![0x16];
    request.extend_from_slice(sql.as_bytes());
    let mut job = Prepare::new(
        request,
        connection.capabilities,
        LIMIT,
        LIMIT,
        LIMIT,
        Pool::new(LIMIT),
    )?;
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget)? {
            Step::Ready(statement) => return Ok(statement),
            Step::Control => (),
            Step::Wait(_) => panic!("prepare drives its own concrete wait"),
        }
    }
}

fn close(connection: &mut Authenticated, statement: Statement) {
    let scope = if connection.channel.is_compressed() {
        Some(sqlaero_core::memory::Scope::new(Pool::new(LIMIT), LIMIT).unwrap())
    } else {
        None
    };
    let mut job = Close::new(statement.id, LIMIT, scope).unwrap();
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget).unwrap() {
            Step::Ready(()) => return,
            Step::Control => (),
            Step::Wait(_) => panic!("close drives its own concrete wait"),
        }
    }
}

type Trace = (Vec<Option<StatusFlags>>, Vec<(Option<usize>, StatusFlags)>);

fn check_rows(batch: &RowBatch, expected: &[&[i64]], seen: &mut usize) {
    for index in 0..batch.row_count {
        let cells = batch.row(index).unwrap();
        assert_eq!(cells.len(), expected[*seen].len());
        for ((column, cell), expected) in batch.columns.iter().zip(cells).zip(expected[*seen]) {
            let raw = cell.get(&batch.bytes).unwrap().unwrap();
            let actual = if batch.encoding == sqlaero_core::result::value::RowEncoding::Binary {
                let FieldValue::Signed(value) = binary::value(column, raw).unwrap() else {
                    panic!("expected native signed integer");
                };
                value
            } else {
                std::str::from_utf8(raw).unwrap().parse::<i64>().unwrap()
            };
            assert_eq!(actual, *expected);
        }
        *seen += 1;
    }
}

fn stream(
    connection: &mut Authenticated,
    request: Vec<u8>,
    columns: &[usize],
    expected: &[&[i64]],
) -> Trace {
    let mut query = Query::new(request, connection.capabilities, LIMIT, LIMIT, LIMIT)
        .unwrap()
        .with_streaming()
        .unwrap();
    let mut budget = budget();
    let mut headers = Vec::new();
    let mut ends = Vec::new();
    let mut seen = 0;
    loop {
        match query
            .advance_stream(
                &mut connection.channel,
                &mut budget,
                NonZeroUsize::new(1).unwrap(),
            )
            .unwrap()
        {
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness"),
            Step::Ready(StreamEvent::Header(header)) => {
                assert_eq!(header.columns.len(), columns[headers.len()]);
                assert_eq!(
                    header.metadata_status.is_none(),
                    connection
                        .capabilities
                        .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                );
                headers.push(header.metadata_status);
            }
            Step::Ready(StreamEvent::Rows(batch)) => {
                assert_eq!(batch.row_count, 1);
                check_rows(&batch, expected, &mut seen);
            }
            Step::Ready(StreamEvent::End {
                tail,
                completion,
                total_rows,
                metadata_status,
            }) => {
                assert_eq!(
                    metadata_status,
                    if total_rows.is_some() {
                        *headers.last().unwrap()
                    } else {
                        None
                    }
                );
                if let Some(batch) = tail {
                    check_rows(&batch, expected, &mut seen);
                }
                ends.push((total_rows, completion.status_flags()));
                if query.is_finished() {
                    break;
                }
            }
        }
    }
    assert_eq!(headers.len(), columns.len());
    assert_eq!(seen, expected.len());
    (headers, ends)
}

fn short_tail(connection: &mut Authenticated, request: Vec<u8>, expected: &[&[i64]]) {
    let mut job = Query::new(request, connection.capabilities, LIMIT, LIMIT, LIMIT)
        .unwrap()
        .with_streaming()
        .unwrap();
    let mut budget = budget();
    let mut header = false;
    loop {
        match job
            .advance_stream(
                &mut connection.channel,
                &mut budget,
                NonZeroUsize::new(3).unwrap(),
            )
            .unwrap()
        {
            Step::Control => (),
            Step::Wait(_) => panic!("Query drives concrete readiness"),
            Step::Ready(StreamEvent::Header(_)) => {
                assert!(!header);
                header = true;
                assert!(job.take_confirmation().is_none());
            }
            Step::Ready(StreamEvent::Rows(_)) => {
                panic!("two short rows belong to the actual End tail")
            }
            Step::Ready(StreamEvent::End {
                tail, total_rows, ..
            }) => {
                assert!(header);
                let rows = tail.expect("short final batch");
                assert_eq!(rows.row_count, 2);
                assert_eq!(total_rows, Some(2));
                let mut seen = 0;
                check_rows(&rows, expected, &mut seen);
                assert_eq!(seen, 2);
                assert!(job.is_finished());
                assert!(job.take_confirmation().is_some());
                break;
            }
        }
    }
}

#[test]
#[ignore = "requires C's two owned MySQL fixtures; run with one test thread"]
fn real_stream_text_binary_and_multiple_sets_share_the_original_channel() {
    for label in ["84", "97"] {
        for (algorithm, secure) in [("uncompressed", false), ("zstd", true)] {
            let options =
                compression::Options::new(compression::Algorithms::from_csv(algorithm).unwrap(), 3)
                    .unwrap();
            let mut connection = connect_mode(label, options, secure);
            let (_, ends) = stream(
                &mut connection,
                b"\x03SELECT 1 AS n UNION ALL SELECT 2".to_vec(),
                &[1],
                &[&[1], &[2]],
            );
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].0, Some(2));
            short_tail(
                &mut connection,
                b"\x03SELECT 1 AS n UNION ALL SELECT 2".to_vec(),
                &[&[1], &[2]],
            );
            let statement = prepare(&mut connection, "SELECT ? AS n UNION ALL SELECT ?").unwrap();
            let mut params = Parameters::new(statement.id, 2, LIMIT).unwrap();
            params.write(ValueRef::Signed(3), &context()).unwrap();
            params.write(ValueRef::Signed(4), &context()).unwrap();
            let (_, ends) = stream(
                &mut connection,
                params.finish().unwrap(),
                &[1],
                &[&[3], &[4]],
            );
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].0, Some(2));
            let mut params = Parameters::new(statement.id, 2, LIMIT).unwrap();
            params.write(ValueRef::Signed(3), &context()).unwrap();
            params.write(ValueRef::Signed(4), &context()).unwrap();
            short_tail(&mut connection, params.finish().unwrap(), &[&[3], &[4]]);
            close(&mut connection, statement);
            sql(&mut connection, "DROP PROCEDURE IF EXISTS c_stream_sets").unwrap();
            sql(&mut connection, "CREATE PROCEDURE c_stream_sets(IN n INT) BEGIN SELECT n AS n; SELECT n+1 AS n; END").unwrap();
            let (_, ends) = stream(
                &mut connection,
                b"\x03CALL c_stream_sets(7)".to_vec(),
                &[1, 1],
                &[&[7], &[8]],
            );
            assert_eq!(ends.len(), 3);
            assert_eq!(ends[0].0, Some(1));
            assert_eq!(ends[1].0, Some(1));
            assert_eq!(ends[2].0, None);
            assert!(ends[0].1.contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS));
            assert!(ends[1].1.contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS));
            assert!(!ends[2].1.contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS));
            sql(&mut connection, "DROP PROCEDURE c_stream_sets").unwrap();
            sql(&mut connection, "DO 1").unwrap();
        }
    }
}

#[test]
#[ignore = "requires C's two owned MySQL fixtures; same test records temporary legacy OUT probe"]
fn real_stream_prepared_call_reports_only_actual_out_parameter_markers() {
    for label in ["84", "97"] {
        let mut connection = connect_mode(label, compression::Options::default(), false);
        sql(&mut connection, "DROP PROCEDURE IF EXISTS c_stream_out").unwrap();
        sql(&mut connection, "CREATE PROCEDURE c_stream_out(IN n INT,OUT v BIGINT,INOUT counter INT) BEGIN SELECT n AS n; SELECT n+1 AS n; SET v=-9223372036854775808; SET counter=counter+1; END").unwrap();
        let statement = prepare(&mut connection, "CALL c_stream_out(?,?,?)").unwrap();
        let mut params = Parameters::new(statement.id, 3, LIMIT).unwrap();
        for value in [ValueRef::Signed(5), ValueRef::Null, ValueRef::Signed(10)] {
            params.write(value, &context()).unwrap();
        }
        let (headers, ends) = stream(
            &mut connection,
            params.finish().unwrap(),
            &[1, 1, 2],
            &[&[5], &[6], &[i64::MIN, 11]],
        );
        let legacy = !connection
            .capabilities
            .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF);
        eprintln!(
            "MySQL {} ({label}) legacy={legacy}: actual metadata status={headers:?}, actual set ends={ends:?}",
            connection.metadata.version
        );
        assert_eq!(headers.len(), 3);
        if legacy {
            assert!(
                !headers[0]
                    .unwrap()
                    .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
            );
            assert!(
                !headers[1]
                    .unwrap()
                    .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
            );
            assert!(
                headers[2]
                    .unwrap()
                    .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
            );
            assert_eq!(ends.len(), 4);
            assert_eq!(ends[3].0, None);
            assert!(!ends[2].1.contains(StatusFlags::SERVER_PS_OUT_PARAMS));
            assert!(!ends[2].1.contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS));
        } else {
            assert!(headers.iter().all(Option::is_none));
            assert_eq!(ends.len(), 4);
            assert!(ends[2].1.contains(StatusFlags::SERVER_PS_OUT_PARAMS));
            assert_eq!(ends[3].0, None);
        }
        close(&mut connection, statement);
        sql(&mut connection, "DROP PROCEDURE c_stream_out").unwrap();
        sql(&mut connection, "DO 1").unwrap();
    }
}

#[test]
#[ignore = "requires C's two owned MySQL fixtures; run with one test thread"]
fn real_set_option_changes_multi_sql_without_affecting_calls_or_transaction_status() {
    for label in ["84", "97"] {
        for (algorithm, secure) in [("uncompressed", false), ("zstd", true)] {
            let options =
                compression::Options::new(compression::Algorithms::from_csv(algorithm).unwrap(), 3)
                    .unwrap();
            let mut connection = connect_mode(label, options, secure);
            assert!(
                !connection
                    .capabilities
                    .contains(CapabilityFlags::CLIENT_MULTI_STATEMENTS)
            );
            assert!(
                connection
                    .capabilities
                    .contains(CapabilityFlags::CLIENT_PS_MULTI_RESULTS)
            );
            let Err(QueryError::Result(sqlaero_core::result::text::ResultError::Server(error))) =
                sql(&mut connection, "SELECT 1; SELECT 2")
            else {
                panic!("initial multi SQL must be disabled");
            };
            assert_eq!(error.error_code(), 1064);
            sql(
                &mut connection,
                "CREATE TEMPORARY TABLE c_option_tx(n INT) ENGINE=InnoDB",
            )
            .unwrap();
            sql(&mut connection, "INSERT INTO c_option_tx VALUES(1)").unwrap();
            sql(&mut connection, "START TRANSACTION").unwrap();
            let before = sql(&mut connection, "SELECT n FROM c_option_tx").unwrap();
            assert!(
                before
                    .completion()
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
            );
            let on = query(&mut connection, vec![0x1b, 0, 0]).unwrap();
            assert!(matches!(on, Response::Command(_)));
            assert!(
                on.completion()
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
            );
            let (_, ends) = stream(
                &mut connection,
                b"\x03SELECT 1; SELECT 2".to_vec(),
                &[1, 1],
                &[&[1], &[2]],
            );
            assert_eq!(ends.len(), 2);
            assert!(
                ends.iter()
                    .all(|(_, status)| status.contains(StatusFlags::SERVER_STATUS_IN_TRANS))
            );
            let off = query(&mut connection, vec![0x1b, 1, 0]).unwrap();
            assert!(
                off.completion()
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
            );
            let Err(QueryError::Result(sqlaero_core::result::text::ResultError::Server(error))) =
                sql(&mut connection, "SELECT 1; SELECT 2")
            else {
                panic!("multi SQL must be disabled again");
            };
            assert_eq!(error.error_code(), 1064);
            let after = sql(&mut connection, "SELECT n FROM c_option_tx").unwrap();
            assert!(
                after
                    .completion()
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
            );
            let rolled_back = sql(&mut connection, "ROLLBACK").unwrap();
            assert!(
                !rolled_back
                    .completion()
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
            );
            sql(&mut connection, "DROP PROCEDURE IF EXISTS c_option_calls").unwrap();
            sql(
                &mut connection,
                "CREATE PROCEDURE c_option_calls() BEGIN SELECT 41 AS n; SELECT 42 AS n; END",
            )
            .unwrap();
            let (_, text_ends) = stream(
                &mut connection,
                b"\x03CALL c_option_calls()".to_vec(),
                &[1, 1],
                &[&[41], &[42]],
            );
            assert_eq!(text_ends.len(), 3);
            let statement = prepare(&mut connection, "CALL c_option_calls()").unwrap();
            let request = Parameters::new(statement.id, 0, LIMIT)
                .unwrap()
                .finish()
                .unwrap();
            let (_, binary_ends) = stream(&mut connection, request, &[1, 1], &[&[41], &[42]]);
            assert_eq!(binary_ends.len(), 3);
            close(&mut connection, statement);
            sql(&mut connection, "DROP PROCEDURE c_option_calls").unwrap();
            eprintln!(
                "MySQL {} {algorithm} tls={secure}: actual set-option ON/OFF ack preserved transaction status; next multi SQL toggled and text/prepared CALL stayed enabled",
                connection.metadata.version
            );
        }
    }
}
