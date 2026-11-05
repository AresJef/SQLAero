//! Real prepared commands on the task's isolated MySQL fixtures.

use sqlaero_core::bind::Context;
use sqlaero_core::bind::scalar::{Clock, Date};
use sqlaero_core::bind::value::{ValueRef, ValueSink};
use sqlaero_core::charset::{self, codec::Codec};
use sqlaero_core::memory::Pool;
use sqlaero_core::protocol::prepared::{Close, Parameters, Prepare, Statement};
use sqlaero_core::protocol::{CapabilityFlags, StatusFlags, binary};
use sqlaero_core::result::text::{Response, ResultError};
use sqlaero_core::result::value::{FieldValue, RowEncoding};
use sqlaero_core::session::connect::{Authenticated, ConnectParams, Endpoint, Establish};
use sqlaero_core::session::initialize::{Initialize, Options};
use sqlaero_core::session::query::{Query, QueryError};
use sqlaero_core::transport::channel::Step;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use std::path::PathBuf;
use std::time::Duration;
use zeroize::Zeroizing;

const LIMIT: usize = 16 * 1024 * 1024;

fn connect(label: &str) -> Authenticated {
    let root = PathBuf::from(
        std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").expect("set the task-owned fixture path"),
    );
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
    let mut job = Close::new(statement.id, LIMIT).unwrap();
    let mut budget = budget();
    loop {
        match job.advance(&mut connection.channel, &mut budget).unwrap() {
            Step::Ready(()) => return,
            Step::Control => (),
            Step::Wait(_) => panic!("close drives its own concrete wait"),
        }
    }
}

#[test]
#[ignore = "requires C's two isolated MySQL fixtures; run with one test thread"]
fn real_prepared_parameters_preserve_native_values_long_chunks_and_reusable_handles() {
    for label in ["84", "97"] {
        let mut connection = connect(label);
        sql(&mut connection, "CREATE TEMPORARY TABLE c_prepared (id INT PRIMARY KEY,s BIGINT,u BIGINT UNSIGNED,d DECIMAL(65,12),dt DATETIME(6),tm TIME(6),t LONGTEXT,b LONGBLOB)").unwrap();
        let statement = prepare(
            &mut connection,
            "INSERT INTO c_prepared VALUES (?,?,?,?,?,?,?,?)",
        )
        .unwrap();
        assert_eq!(statement.parameters, 8);
        let big = num_bigint::BigInt::parse_bytes(b"1234567890123456789012345678901234567890", 10)
            .unwrap();
        let text = "中'\\\0🙂".repeat(40_000);
        let binary = vec![0xa5; 500_000];
        let mut parameters = Parameters::new(statement.id, statement.parameters, LIMIT).unwrap();
        for value in [
            ValueRef::Signed(1),
            ValueRef::Signed(i64::MIN),
            ValueRef::Unsigned(u64::MAX),
            ValueRef::BigInteger(&big),
            ValueRef::Datetime {
                date: Date {
                    year: 2026,
                    month: 11,
                    day: 5,
                },
                clock: Clock {
                    hour: 1,
                    minute: 2,
                    second: 3,
                    microsecond: 123_456,
                },
            },
            ValueRef::Duration(-183_845_123_456),
        ] {
            parameters.write(value, &context()).unwrap();
        }
        parameters.text_start(text.len(), &context()).unwrap();
        // Each fragment ends on a known character boundary of this fixture.
        for fragment in text.as_bytes().chunks(64_000) {
            parameters
                .text_fragment(std::str::from_utf8(fragment).unwrap(), &context())
                .unwrap();
        }
        parameters.text_end(&context()).unwrap();
        parameters.bytes_start(binary.len(), &context()).unwrap();
        for fragment in binary.chunks(64 * 1024) {
            parameters.bytes_fragment(fragment, &context()).unwrap();
        }
        parameters.bytes_end(&context()).unwrap();
        let Response::Command(insert) =
            query(&mut connection, parameters.finish().unwrap()).unwrap()
        else {
            panic!("expected insert completion");
        };
        assert_eq!(insert.affected_rows(), 1);
        for id in [2, 3] {
            let mut parameters =
                Parameters::new(statement.id, statement.parameters, LIMIT).unwrap();
            for value in [
                ValueRef::Signed(id),
                ValueRef::Null,
                ValueRef::Unsigned(0),
                ValueRef::Decimal {
                    negative: false,
                    digits: &[1, 2, 3, 4, 5],
                    exponent: -3,
                },
                ValueRef::Date(Date {
                    year: 2026,
                    month: 11,
                    day: 5,
                }),
                ValueRef::Time(Clock {
                    hour: 1,
                    minute: 2,
                    second: 3,
                    microsecond: 0,
                }),
                ValueRef::Signed(7),
                ValueRef::Bytes(b""),
            ] {
                parameters.write(value, &context()).unwrap();
            }
            query(&mut connection, parameters.finish().unwrap()).unwrap();
        }
        close(&mut connection, statement);
        let Response::Rows(rows) = sql(
            &mut connection,
            "SELECT s,u,d,dt,tm,t,b FROM c_prepared WHERE id=1",
        )
        .unwrap() else {
            panic!("expected stored row");
        };
        let values: Vec<_> = rows
            .row(0)
            .unwrap()
            .iter()
            .map(|field| field.get(&rows.bytes).unwrap().unwrap())
            .collect();
        assert_eq!(
            values,
            [
                b"-9223372036854775808".as_slice(),
                b"18446744073709551615",
                b"1234567890123456789012345678901234567890.000000000000",
                b"2026-11-05 01:02:03.123456",
                b"-51:04:05.123456",
                text.as_bytes(),
                &binary
            ]
        );
        let Response::Rows(rows) = sql(
            &mut connection,
            "SELECT s,u,d,dt,tm,t,LENGTH(b) FROM c_prepared WHERE id=3",
        )
        .unwrap() else {
            panic!("expected reused row");
        };
        let values: Vec<_> = rows
            .row(0)
            .unwrap()
            .iter()
            .map(|field| field.get(&rows.bytes).unwrap())
            .collect();
        assert_eq!(
            values,
            [
                None,
                Some(b"0".as_slice()),
                Some(b"12.345000000000".as_slice()),
                Some(b"2026-11-05 00:00:00.000000".as_slice()),
                Some(b"01:02:03.000000".as_slice()),
                Some(b"7".as_slice()),
                Some(b"0".as_slice())
            ]
        );
        assert!(
            matches!(prepare(&mut connection, "SELECT FROM"), Err(QueryError::Result(ResultError::Server(error))) if error.error_code() == 1064)
        );
        sql(&mut connection, "DO 1").unwrap();
    }
}

#[test]
#[ignore = "requires C's two isolated MySQL fixtures; run with one test thread"]
fn real_binary_select_uses_native_fields_and_server_current_metadata() {
    for label in ["84", "97"] {
        let mut connection = connect(label);
        let statement = prepare(&mut connection, "SELECT ?,?,?,?,?,?,?,NULL").unwrap();
        for text in ["中'\\\0🙂", ""] {
            let mut parameters =
                Parameters::new(statement.id, statement.parameters, LIMIT).unwrap();
            for value in [
                ValueRef::Signed(i64::MIN),
                ValueRef::Unsigned(u64::MAX),
                ValueRef::Decimal {
                    negative: false,
                    digits: &[1, 2, 3, 4, 5],
                    exponent: -3,
                },
                ValueRef::Datetime {
                    date: Date {
                        year: 2026,
                        month: 11,
                        day: 5,
                    },
                    clock: Clock {
                        hour: 1,
                        minute: 2,
                        second: 3,
                        microsecond: 123_456,
                    },
                },
                ValueRef::Duration(-183_845_123_456),
                ValueRef::Text(text),
                ValueRef::Bytes(b"\0\xffraw"),
            ] {
                parameters.write(value, &context()).unwrap();
            }
            let Response::Rows(rows) =
                query(&mut connection, parameters.finish().unwrap()).unwrap()
            else {
                panic!("expected binary row");
            };
            assert_eq!(rows.encoding, RowEncoding::Binary);
            let values: Vec<_> = rows
                .columns
                .iter()
                .zip(rows.row(0).unwrap())
                .map(|(column, field)| match field.get(&rows.bytes).unwrap() {
                    Some(raw) => binary::value(column, raw).unwrap(),
                    None => FieldValue::Null,
                })
                .collect();
            assert_eq!(
                values,
                [
                    FieldValue::Signed(i64::MIN),
                    FieldValue::Unsigned(u64::MAX),
                    FieldValue::Bytes(b"12.345"),
                    FieldValue::Datetime {
                        year: 2026,
                        month: 11,
                        day: 5,
                        hour: 1,
                        minute: 2,
                        second: 3,
                        microsecond: 123_456
                    },
                    FieldValue::Duration(-183_845_123_456),
                    FieldValue::Bytes(text.as_bytes()),
                    FieldValue::Bytes(b"\0\xffraw"),
                    FieldValue::Null
                ]
            );
        }
        close(&mut connection, statement);
    }
}

#[test]
#[ignore = "requires C's two isolated MySQL fixtures; run with one test thread"]
fn real_prepared_call_preserves_business_sets_out_marker_and_final_status() {
    for label in ["84", "97"] {
        let mut connection = connect(label);
        assert!(
            connection
                .capabilities
                .contains(CapabilityFlags::CLIENT_PS_MULTI_RESULTS)
        );
        sql(
            &mut connection,
            "DROP PROCEDURE IF EXISTS c_prepared_protocol_call",
        )
        .unwrap();
        sql(&mut connection, "CREATE PROCEDURE c_prepared_protocol_call(IN n INT, OUT v BIGINT, INOUT counter INT) BEGIN SELECT n AS r1; SELECT n+1 AS r2; SET v=-9223372036854775808; SET counter=counter+1; END").unwrap();
        let statement = prepare(&mut connection, "CALL c_prepared_protocol_call(?,?,?)").unwrap();
        let mut parameters = Parameters::new(statement.id, statement.parameters, LIMIT).unwrap();
        for value in [ValueRef::Signed(5), ValueRef::Null, ValueRef::Signed(10)] {
            parameters.write(value, &context()).unwrap();
        }
        let mut query = Query::new(
            parameters.finish().unwrap(),
            connection.capabilities,
            LIMIT,
            LIMIT,
            LIMIT,
        )
        .unwrap();
        let mut budget = budget();
        let mut sets = Vec::new();
        loop {
            match query.advance(&mut connection.channel, &mut budget).unwrap() {
                Step::Ready(response) => {
                    sets.push(response);
                    if query.is_finished() {
                        break;
                    }
                }
                Step::Control => (),
                Step::Wait(_) => panic!("query drives concrete waits"),
            }
        }
        assert_eq!(sets.len(), 4);
        for (index, expected) in [5, 6].into_iter().enumerate() {
            let Response::Rows(rows) = &sets[index] else {
                panic!("expected business row");
            };
            assert_eq!(rows.encoding, RowEncoding::Binary);
            assert!(
                !rows
                    .completion
                    .status_flags()
                    .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
            );
            assert_eq!(
                binary::value(
                    &rows.columns[0],
                    rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap()
                )
                .unwrap(),
                FieldValue::Signed(expected)
            );
        }
        let Response::Rows(rows) = &sets[2] else {
            panic!("expected native output row");
        };
        assert!(
            rows.completion
                .status_flags()
                .contains(StatusFlags::SERVER_PS_OUT_PARAMS)
        );
        let values: Vec<_> = rows
            .columns
            .iter()
            .zip(rows.row(0).unwrap())
            .map(|(column, field)| {
                binary::value(column, field.get(&rows.bytes).unwrap().unwrap()).unwrap()
            })
            .collect();
        assert_eq!(
            values,
            [FieldValue::Signed(i64::MIN), FieldValue::Signed(11)]
        );
        assert!(matches!(sets[3], Response::Command(_)));
        assert!(
            !sets[3]
                .completion()
                .status_flags()
                .contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS)
        );
        close(&mut connection, statement);
        sql(&mut connection, "DROP PROCEDURE c_prepared_protocol_call").unwrap();
        sql(&mut connection, "DO 1").unwrap();
    }
}
