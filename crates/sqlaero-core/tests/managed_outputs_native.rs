//! Managed OUT delivery through actual prepared CALL on the owned servers.

use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{ValueRef, ValueSink};
use sqlaero_core::charset::{self, codec::Codec};
use sqlaero_core::memory::{Pool, Scope};
use sqlaero_core::protocol::prepared::{Close, Parameters, Prepare, Statement};
use sqlaero_core::protocol::{CapabilityFlags, StatusFlags, binary};
use sqlaero_core::result::text::{Response, RowBatch, StreamEvent};
use sqlaero_core::result::value::FieldValue;
use sqlaero_core::session::connect::{Authenticated, ConnectParams, Endpoint, Establish};
use sqlaero_core::session::initialize::{Initialize, Options};
use sqlaero_core::session::query::{Managed, Query};
use sqlaero_core::transport::channel::Step;
use sqlaero_core::transport::compression;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;
use zeroize::Zeroizing;

const LIMIT: usize = 16 * 1024 * 1024;
fn budget() -> IoBudget {
    IoBudget::new(
        TimeoutKind::Operation,
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(3)),
        Some(Duration::from_secs(3)),
    )
}
fn connect(label: &str, algorithm: &str) -> Authenticated {
    let root =
        PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").expect("select owned fixtures"));
    let state: mysql_common::serde_json::Value = mysql_common::serde_json::from_slice(
        &std::fs::read(root.join(label).join("state.json")).unwrap(),
    )
    .unwrap();
    let parameters = ConnectParams {
        endpoint: Endpoint::Unix(root.join(format!("{label}.sock"))),
        user: "sqlaero".to_owned(),
        password: Zeroizing::new(state["test_password"].as_str().unwrap().as_bytes().to_vec()),
        database: Some("sqlaero_test".to_owned()),
        tls: None,
        server_public_key: None,
        compression: compression::Options::new(
            compression::Algorithms::from_csv(algorithm).unwrap(),
            3,
        )
        .unwrap(),
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
    let mut job = Initialize::new(Establish::new(parameters).unwrap(), options);
    loop {
        match job.advance().unwrap() {
            Step::Ready(ready) => return ready.transport,
            Step::Control => (),
            Step::Wait(_) => panic!("initialize drives readiness"),
        }
    }
}
fn sql(connection: &mut Authenticated, source: &str) -> Response {
    let mut bytes = vec![3];
    bytes.extend_from_slice(source.as_bytes());
    let mut job = Query::new(bytes, connection.capabilities, LIMIT, LIMIT, LIMIT).unwrap();
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget).unwrap() {
            Step::Ready(response) => {
                assert!(job.is_finished());
                return response;
            }
            Step::Control => (),
            Step::Wait(_) => panic!("query drives readiness"),
        }
    }
}
fn prepare(connection: &mut Authenticated, source: &str) -> Statement {
    let mut bytes = vec![0x16];
    bytes.extend_from_slice(source.as_bytes());
    let mut job = Prepare::new(
        bytes,
        connection.capabilities,
        LIMIT,
        LIMIT,
        LIMIT,
        Pool::new(LIMIT),
    )
    .unwrap();
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget).unwrap() {
            Step::Ready(statement) => return statement,
            Step::Control => (),
            Step::Wait(_) => panic!("prepare drives readiness"),
        }
    }
}
fn values(rows: &RowBatch) -> Vec<Vec<i64>> {
    (0..rows.row_count)
        .map(|index| {
            rows.columns
                .iter()
                .zip(rows.row(index).unwrap())
                .map(|(column, cell)| {
                    let FieldValue::Signed(value) =
                        binary::value(column, cell.get(&rows.bytes).unwrap().unwrap()).unwrap()
                    else {
                        panic!("actual native integer")
                    };
                    value
                })
                .collect()
        })
        .collect()
}
fn close(connection: &mut Authenticated, statement: Statement) {
    let scope = connection
        .channel
        .is_compressed()
        .then(|| Scope::new(Pool::new(LIMIT), LIMIT).unwrap());
    let mut job = Close::new(statement.id, LIMIT, scope).unwrap();
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget).unwrap() {
            Step::Ready(()) => return,
            Step::Control => (),
            Step::Wait(_) => panic!("close drives readiness"),
        }
    }
}

#[test]
#[ignore = "requires C's two owned MySQL fixtures; run with one test thread"]
fn real_managed_call_retains_business_sets_output_status_tracking_and_tail() {
    for label in ["84", "97"] {
        for algorithm in ["uncompressed", "zlib", "zstd"] {
            let mut connection = connect(label, algorithm);
            assert!(connection.capabilities.contains(
                CapabilityFlags::CLIENT_DEPRECATE_EOF | CapabilityFlags::CLIENT_SESSION_TRACK
            ));
            sql(&mut connection, "DROP PROCEDURE IF EXISTS c_managed_native");
            sql(
                &mut connection,
                "CREATE PROCEDURE c_managed_native(IN a BIGINT,OUT b BIGINT,INOUT c BIGINT) BEGIN SELECT a AS same_name WHERE FALSE; SELECT a AS same_name; SELECT a AS same_name UNION ALL SELECT a+1 UNION ALL SELECT a+2; SET b=-9223372036854775808; SET c=c+1; SET SESSION time_zone='+01:00'; END",
            );
            sql(
                &mut connection,
                "CREATE TEMPORARY TABLE c_managed_tx(n INT) ENGINE=InnoDB",
            );
            sql(&mut connection, "INSERT INTO c_managed_tx VALUES(1)");
            let statement = prepare(&mut connection, "CALL c_managed_native(?,?,?)");
            for streaming in [false, true] {
                sql(&mut connection, "SET SESSION time_zone='+00:00'");
                sql(&mut connection, "START TRANSACTION");
                sql(&mut connection, "SELECT n FROM c_managed_tx");
                let context = Context {
                    codec: Codec::new(charset::resolve(None, None).unwrap().charset),
                    no_backslash_escapes: false,
                    utc_confirmed: true,
                    limit: LIMIT,
                };
                let mut parameters = Parameters::new(statement.id, 3, LIMIT).unwrap();
                for value in [ValueRef::Signed(5), ValueRef::Null, ValueRef::Signed(10)] {
                    parameters.write(value, &context).unwrap();
                }
                let query = Query::new(
                    parameters.finish().unwrap(),
                    connection.capabilities,
                    LIMIT,
                    LIMIT,
                    LIMIT,
                )
                .unwrap();
                let query = if streaming {
                    query.with_streaming().unwrap()
                } else {
                    query
                };
                let mut job = query.with_managed_outputs(2).unwrap();
                let mut budget = budget();
                let mut rows = Vec::new();
                let mut ends = Vec::new();
                let mut headers = 0;
                let mut output_status = None;
                let mut tracking = Vec::new();
                while !job.is_finished() {
                    if streaming {
                        match job
                            .advance_managed_stream(
                                &mut connection.channel,
                                &mut budget,
                                NonZeroUsize::new(4).unwrap(),
                            )
                            .unwrap()
                        {
                            Step::Ready(Managed::Public(StreamEvent::Header(header))) => {
                                assert_eq!(header.columns.len(), 1);
                                headers += 1;
                            }
                            Step::Ready(Managed::Public(StreamEvent::Rows(batch))) => {
                                rows.extend(values(&batch))
                            }
                            Step::Ready(Managed::Public(StreamEvent::End {
                                tail,
                                total_rows,
                                completion,
                                ..
                            })) => {
                                if let Some(tail) = tail {
                                    rows.extend(values(&tail));
                                }
                                assert!(
                                    completion
                                        .status_flags()
                                        .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
                                );
                                ends.push(total_rows);
                                tracking.extend_from_slice(
                                    completion.session_state_info_ref().unwrap_or_default(),
                                );
                            }
                            Step::Ready(Managed::InternalOutput {
                                completion,
                                metadata_status,
                            }) => {
                                assert!(metadata_status.is_none());
                                output_status = Some(completion.status_flags());
                                tracking.extend_from_slice(
                                    completion.session_state_info_ref().unwrap_or_default(),
                                );
                            }
                            Step::Control => (),
                            Step::Wait(_) => panic!("managed stream drives readiness"),
                        }
                    } else {
                        match job
                            .advance_managed(&mut connection.channel, &mut budget)
                            .unwrap()
                        {
                            Step::Ready(Managed::Public(response)) => {
                                assert!(
                                    response
                                        .completion()
                                        .status_flags()
                                        .contains(StatusFlags::SERVER_STATUS_IN_TRANS)
                                );
                                if let Response::Rows(batch) = &response {
                                    rows.extend(values(batch));
                                    ends.push(Some(batch.row_count));
                                } else {
                                    ends.push(None);
                                }
                                tracking.extend_from_slice(
                                    response
                                        .completion()
                                        .session_state_info_ref()
                                        .unwrap_or_default(),
                                );
                            }
                            Step::Ready(Managed::InternalOutput {
                                completion,
                                metadata_status,
                            }) => {
                                assert!(metadata_status.is_none());
                                output_status = Some(completion.status_flags());
                                tracking.extend_from_slice(
                                    completion.session_state_info_ref().unwrap_or_default(),
                                );
                            }
                            Step::Control => (),
                            Step::Wait(_) => panic!("managed buffered drives readiness"),
                        }
                    }
                }
                assert_eq!(headers, if streaming { 3 } else { 0 });
                assert_eq!(rows, [vec![5], vec![5], vec![6], vec![7]]);
                assert_eq!(ends, [Some(0), Some(1), Some(3), None]);
                assert!(output_status.unwrap().contains(
                    StatusFlags::SERVER_PS_OUT_PARAMS
                        | StatusFlags::SERVER_MORE_RESULTS_EXISTS
                        | StatusFlags::SERVER_STATUS_IN_TRANS
                ));
                assert!(tracking.windows(6).any(|bytes| bytes == b"+01:00"));
                let output = job.take_call_output().unwrap();
                assert_eq!(values(&output), [vec![i64::MIN, 11]]);
                assert!(job.take_managed_completion().is_none());
                assert!(job.take_confirmation().is_some());
                eprintln!(
                    "MySQL {} {label} {algorithm} streaming={streaming}: 3 business sets + actual tail, output {:?}, tracked timezone confirmed",
                    connection.metadata.version,
                    values(&output)
                );
                sql(&mut connection, "ROLLBACK");
            }
            close(&mut connection, statement);
            sql(&mut connection, "DROP PROCEDURE c_managed_native");
            sql(&mut connection, "DO 1");
        }
    }
}
