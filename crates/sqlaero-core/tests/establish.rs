//! Actual native authentication/TLS cases against the isolated MySQL servers.

use sqlaero_core::protocol::auth::AuthPath;
use sqlaero_core::session::connect::{
    Authenticated, ConnectError, ConnectParams, ConnectTls, Endpoint, Establish,
};
use sqlaero_core::transport::channel::Step;
use sqlaero_core::transport::tls::{self, MinimumVersion, Mode};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

fn root() -> PathBuf {
    std::env::var_os("SQLAERO_TEST_MYSQL_ROOT")
        .map(PathBuf::from)
        .expect("set SQLAERO_TEST_MYSQL_ROOT to the isolated fixture directory")
}

fn root_sql(root: &Path, label: &str, sql: &str) {
    let state: mysql_common::serde_json::Value = mysql_common::serde_json::from_slice(
        &std::fs::read(root.join(label).join("state.json")).unwrap(),
    )
    .unwrap();
    let mut child = Command::new(state["mysql"].as_str().unwrap())
        .arg(format!(
            "--defaults-file={}",
            root.join(label).join("root.cnf").display()
        ))
        .args(["--no-login-paths", "--batch", "--skip-column-names"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(sql.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "isolated fixture administration failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn configuration(
    root: &Path,
    label: &str,
    local: bool,
    mode: Option<Mode>,
    name: &str,
) -> ConnectParams {
    let values: mysql_common::serde_json::Value = mysql_common::serde_json::from_slice(
        &std::fs::read(root.join(label).join("state.json")).unwrap(),
    )
    .unwrap();
    let password = values["test_password"]
        .as_str()
        .unwrap()
        .as_bytes()
        .to_vec();
    let prepared = mode.map(|mode| {
        let roots = if mode == Mode::Required {
            None
        } else {
            Some(tls::certificates(&std::fs::read(root.join("tls/ca.pem")).unwrap()).unwrap())
        };
        ConnectTls {
            config: tls::client_config(mode, MinimumVersion::Tls12, roots, None)
                .unwrap()
                .unwrap(),
            name: name.to_owned().try_into().unwrap(),
        }
    });
    ConnectParams {
        endpoint: if local {
            Endpoint::Unix(root.join(format!("{label}.sock")))
        } else {
            Endpoint::Tcp {
                host: "127.0.0.1".to_owned(),
                port: values["port"].as_str().unwrap().parse().unwrap(),
            }
        },
        user: "sqlaero".to_owned(),
        password: Zeroizing::new(password),
        database: Some("sqlaero_test".to_owned()),
        tls: prepared,
        server_public_key: if !local && mode.is_none() {
            Some(root.join(label).join("data/public_key.pem"))
        } else {
            None
        },
        connect_timeout: Some(Duration::from_secs(5)),
        read_timeout: Some(Duration::from_secs(2)),
        write_timeout: Some(Duration::from_secs(2)),
        max_send: 64 * 1024 * 1024,
        max_receive: 64 * 1024 * 1024,
    }
}

fn establish(parameters: ConnectParams) -> Result<Authenticated, ConnectError> {
    let mut job = Establish::new(parameters)?;
    loop {
        match job.advance()? {
            Step::Ready(connection) => return Ok(connection),
            Step::Control => (),
            Step::Wait(_) => panic!("establishment driver must handle its concrete readiness wait"),
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers and their private fixture files"]
fn real_cold_and_warm_authentication_uses_the_same_native_path() {
    let root = root();
    for label in ["84", "97"] {
        for (local, mode, name) in [
            (true, None, "localhost"),
            (false, None, "127.0.0.1"),
            (false, Some(Mode::VerifyIdentity), "127.0.0.1"),
            (true, Some(Mode::VerifyIdentity), "localhost"),
            (false, Some(Mode::VerifyCa), "deliberately-wrong.example"),
            (false, Some(Mode::Required), "deliberately-wrong.example"),
        ] {
            if local && !cfg!(unix) {
                // Unix-only channel cases have no Windows counterpart. The
                // TCP authentication and all applicable TLS modes still run.
                continue;
            }
            root_sql(&root, label, "FLUSH PRIVILEGES;");
            let cold = establish(configuration(&root, label, local, mode, name)).unwrap();
            assert_eq!(cold.metadata.auth_path, AuthPath::Full);
            assert_eq!(cold.channel.tls_version().is_some(), mode.is_some());
            let warm = establish(configuration(&root, label, local, mode, name)).unwrap();
            assert_eq!(warm.metadata.auth_path, AuthPath::Fast);
            println!(
                "MySQL {label}: local={local}, mode={mode:?}, cold=Full, warm=Fast, TLS={:?}",
                warm.channel.tls_version()
            );
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers and their private fixture files; run this binary with one test thread"]
fn invalid_identity_and_unsafe_full_authentication_fail_without_downgrade() {
    let root = root();
    for label in ["84", "97"] {
        assert!(
            establish(configuration(
                &root,
                label,
                false,
                Some(Mode::VerifyIdentity),
                "deliberately-wrong.example"
            ))
            .is_err()
        );
        let mut bad_roots =
            configuration(&root, label, false, Some(Mode::VerifyIdentity), "127.0.0.1");
        bad_roots.tls = Some(ConnectTls {
            config: tls::client_config(
                Mode::VerifyIdentity,
                MinimumVersion::Tls12,
                Some(
                    tls::certificates(&std::fs::read(root.join("tls/client.pem")).unwrap())
                        .unwrap(),
                ),
                None,
            )
            .unwrap()
            .unwrap(),
            name: "127.0.0.1".to_owned().try_into().unwrap(),
        });
        assert!(establish(bad_roots).is_err());
        root_sql(&root, label, "FLUSH PRIVILEGES;");
        let mut no_key = configuration(&root, label, false, None, "127.0.0.1");
        no_key.server_public_key = None;
        assert!(matches!(
            establish(no_key),
            Err(ConnectError::Auth(
                sqlaero_core::protocol::auth::AuthError::TrustedKeyRequired
            ))
        ));
        let mut wrong_password =
            configuration(&root, label, false, Some(Mode::VerifyIdentity), "127.0.0.1");
        wrong_password.password = Zeroizing::new(b"intentionally-incorrect".to_vec());
        match establish(wrong_password) {
            Err(ConnectError::Auth(sqlaero_core::protocol::auth::AuthError::Server(error))) => {
                assert_eq!(error.error_code(), 1045)
            }
            _ => panic!("expected real MySQL access-denied ERR"),
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers and their private fixture files; run this binary with one test thread"]
fn encrypted_client_key_is_used_when_mysql_requires_x509() {
    let root = root();
    let provider = tls::provider();
    let password = std::fs::read(root.join("tls/key-password")).unwrap();
    let identity = tls::signing_identity(
        tls::certificates(&std::fs::read(root.join("tls/client.pem")).unwrap()).unwrap(),
        tls::private_key(
            &std::fs::read(root.join("tls/client-key-encrypted.pem")).unwrap(),
            Some(&password),
        )
        .unwrap(),
        &provider,
    )
    .unwrap();
    for label in ["84", "97"] {
        root_sql(
            &root,
            label,
            "ALTER USER 'sqlaero'@'127.0.0.1' REQUIRE X509; ALTER USER 'sqlaero'@'localhost' REQUIRE X509;",
        );
        let missing_identity = establish(configuration(
            &root,
            label,
            false,
            Some(Mode::VerifyIdentity),
            "127.0.0.1",
        ));
        let mut parameters =
            configuration(&root, label, false, Some(Mode::VerifyIdentity), "127.0.0.1");
        parameters.tls = Some(ConnectTls {
            config: tls::client_config(
                Mode::VerifyIdentity,
                MinimumVersion::Tls12,
                Some(tls::certificates(&std::fs::read(root.join("tls/ca.pem")).unwrap()).unwrap()),
                Some(Arc::clone(&identity)),
            )
            .unwrap()
            .unwrap(),
            name: "127.0.0.1".to_owned().try_into().unwrap(),
        });
        let authenticated = establish(parameters);
        root_sql(
            &root,
            label,
            "ALTER USER 'sqlaero'@'127.0.0.1' REQUIRE NONE; ALTER USER 'sqlaero'@'localhost' REQUIRE NONE;",
        );
        assert!(authenticated.is_ok());
        assert!(missing_identity.is_err());
    }
}

#[test]
#[ignore = "requires isolated MySQL administration; run this binary with one test thread"]
fn tls12_negotiates_and_tls13_minimum_never_downgrades() {
    let root = root();
    for label in ["84", "97"] {
        root_sql(
            &root,
            label,
            "SET GLOBAL tls_version='TLSv1.2'; ALTER INSTANCE RELOAD TLS;",
        );
        let compatible = establish(configuration(
            &root,
            label,
            false,
            Some(Mode::VerifyIdentity),
            "127.0.0.1",
        ));
        let mut only13 =
            configuration(&root, label, false, Some(Mode::VerifyIdentity), "127.0.0.1");
        only13.tls = Some(ConnectTls {
            config: tls::client_config(
                Mode::VerifyIdentity,
                MinimumVersion::Tls13,
                Some(tls::certificates(&std::fs::read(root.join("tls/ca.pem")).unwrap()).unwrap()),
                None,
            )
            .unwrap()
            .unwrap(),
            name: "127.0.0.1".to_owned().try_into().unwrap(),
        });
        let rejected = establish(only13);
        root_sql(
            &root,
            label,
            "SET GLOBAL tls_version='TLSv1.2,TLSv1.3'; ALTER INSTANCE RELOAD TLS;",
        );
        assert_eq!(
            compatible.unwrap().channel.tls_version(),
            Some(rustls::ProtocolVersion::TLSv1_2)
        );
        assert!(rejected.is_err());
    }
}

fn query(
    connection: &mut Authenticated,
    sql: &str,
) -> Result<sqlaero_core::result::text::Response, sqlaero_core::session::query::QueryError> {
    query_with_memory(
        connection,
        sql,
        sqlaero_core::memory::Pool::new(usize::MAX),
        16 * 1024 * 1024,
        false,
    )
}

fn query_with_memory(
    connection: &mut Authenticated,
    sql: &str,
    pool: Arc<sqlaero_core::memory::Pool>,
    limit: usize,
    discard: bool,
) -> Result<sqlaero_core::result::text::Response, sqlaero_core::session::query::QueryError> {
    use sqlaero_core::session::query::Query;
    let mut request = Vec::with_capacity(sql.len() + 1);
    request.push(3);
    request.extend_from_slice(sql.as_bytes());
    let mut query = Query::with_pool(
        request,
        connection.capabilities,
        connection.max_send,
        connection.max_receive,
        limit,
        pool,
        discard,
    )?;
    let mut budget = sqlaero_core::transport::deadline::IoBudget::new(
        sqlaero_core::transport::deadline::TimeoutKind::Operation,
        Some(Duration::from_secs(5)),
        None,
        None,
    );
    loop {
        match query.advance(&mut connection.channel, &mut budget)? {
            Step::Ready(response) => return Ok(response),
            Step::Control => (),
            Step::Wait(_) => panic!("query driver must handle readiness internally"),
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn pooled_results_release_rows_without_invalidating_metadata_and_discard_boundedly() {
    use sqlaero_core::memory::Pool;
    use sqlaero_core::result::text::{Label, Response};
    let root = root();
    for label in ["84", "97"] {
        let pool = Pool::new(24_000);
        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        let Response::Rows(first) = query_with_memory(
            &mut connection,
            "SELECT REPEAT('x',8192) AS payload",
            Arc::clone(&pool),
            1_000_000,
            false,
        )
        .unwrap() else {
            panic!("expected first buffered rows")
        };
        let owned = pool.used();
        let metadata = Arc::clone(&first.columns);
        let failure = query_with_memory(
            &mut connection,
            "SELECT REPEAT('y',8192) AS payload",
            Arc::clone(&pool),
            1_000_000,
            false,
        );
        assert!(failure.is_err());
        assert!(
            failure
                .err()
                .unwrap()
                .to_string()
                .contains("total result memory")
        );
        assert_eq!(pool.used(), owned);
        drop(connection); // The incomplete failed response retires this wire.
        assert_eq!(
            first.row(0).unwrap()[0]
                .get(&first.bytes)
                .unwrap()
                .unwrap()
                .len(),
            8192
        );
        drop(first);
        assert!(pool.used() < owned);
        assert_eq!(metadata.label(0, Label::Name), Some(b"payload".as_slice()));
        drop(metadata);
        assert_eq!(pool.used(), 0);

        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        let counted = query_with_memory(&mut connection,
            "WITH RECURSIVE n AS (SELECT 1 AS i UNION ALL SELECT i+1 FROM n WHERE i<300) SELECT REPEAT('x',4096) FROM n",
            Arc::clone(&pool), 24_000, true).unwrap();
        let Response::Counted { rows, .. } = counted else {
            panic!("expected intermediate count")
        };
        assert_eq!(rows, 300);
        drop(counted);
        assert_eq!(pool.used(), 0);
        assert!(query(&mut connection, "SELECT 7").is_ok());
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn json_results_expose_actual_metadata_and_transmitted_encoding() {
    use sqlaero_core::result::text::Response;
    let root = root();
    for label in ["84", "97"] {
        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        for charset in ["utf8mb4", "latin1", "cp1251", "gb18030"] {
            query(&mut connection, &format!("SET NAMES {charset}")).unwrap();
            let Response::Rows(rows) = query(
                &mut connection,
                "SELECT JSON_OBJECT('x',CONVERT(X'C3A9D0AFE4B8AD' USING utf8mb4)) AS payload",
            )
            .unwrap() else {
                panic!("expected JSON rows")
            };
            let data = rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap();
            println!(
                "{label} charset={charset} type={} collation={} data={data:?}",
                rows.columns[0].type_code, rows.columns[0].collation_id
            );
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn native_queries_keep_empty_null_unicode_large_rows_and_recover_after_server_err() {
    use sqlaero_core::result::text::{Response, ResultError};
    use sqlaero_core::session::query::QueryError;
    let root = root();
    for label in ["84", "97"] {
        for mode in [None, Some(Mode::VerifyIdentity)] {
            let mut connection = establish(configuration(
                &root,
                label,
                mode.is_none(),
                mode,
                "127.0.0.1",
            ))
            .unwrap();
            let response = query(
                &mut connection,
                "SELECT 1 AS number, '' AS empty_text, NULL AS missing, '中' AS unicode_text",
            )
            .unwrap();
            let Response::Rows(rows) = response else {
                panic!("expected rows")
            };
            assert_eq!(rows.row_count, 1);
            assert_eq!(rows.columns.len(), 4);
            let row = rows.row(0).unwrap();
            assert_eq!(row[0].get(&rows.bytes).unwrap(), Some(b"1".as_slice()));
            assert_eq!(row[1].get(&rows.bytes).unwrap(), Some(b"".as_slice()));
            assert_eq!(row[2].get(&rows.bytes).unwrap(), None);
            assert_eq!(row[3].get(&rows.bytes).unwrap(), Some("中".as_bytes()));
            let Response::Rows(large) =
                query(&mut connection, "SELECT REPEAT('x',1048576)").unwrap()
            else {
                panic!("expected rows")
            };
            assert_eq!(
                large.row(0).unwrap()[0]
                    .get(&large.bytes)
                    .unwrap()
                    .unwrap()
                    .len(),
                1048576
            );
            match query(&mut connection, "SELECT FROM") {
                Err(QueryError::Result(ResultError::Server(error))) => {
                    assert_eq!(error.error_code(), 1064)
                }
                _ => panic!("expected a complete syntax ERR"),
            }
            let Response::Rows(after) = query(&mut connection, "SELECT 2").unwrap() else {
                panic!("expected rows")
            };
            assert_eq!(
                after.row(0).unwrap()[0].get(&after.bytes).unwrap(),
                Some(b"2".as_slice())
            );
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn initialization_confirms_utc_encoding_timeouts_and_server_limit() {
    use sqlaero_core::charset;
    use sqlaero_core::session::initialize::{Initialize, Options};
    let root = root();
    for label in ["84", "97"] {
        for charset in ["utf8mb4", "latin1", "big5", "sjis", "swe7", "binary"] {
            let parameters = configuration(&root, label, true, None, "localhost");
            let options = Options {
                encoding: charset::resolve(Some(charset), None).unwrap(),
                autocommit: false,
                sql_mode: Some("NO_BACKSLASH_ESCAPES".to_owned()),
                server_timeouts: [
                    Some(11u32.into()),
                    Some(12u32.into()),
                    Some(13u32.into()),
                    Some(14u32.into()),
                ],
                result_memory_limit: 16 * 1024 * 1024,
            };
            let mut initialize = Initialize::new(Establish::new(parameters).unwrap(), options);
            let ready = loop {
                match initialize.advance().unwrap() {
                    Step::Ready(ready) => break ready,
                    Step::Control => (),
                    Step::Wait(_) => panic!("unexpected wait"),
                }
            };
            assert_eq!(ready.confirmed.time_zone, "+00:00");
            assert_eq!(ready.confirmed.encoding.charset.name, charset);
            assert_eq!(ready.confirmed.server_timeouts, [11, 12, 13, 14]);
            assert_eq!(ready.confirmed.database.as_deref(), Some("sqlaero_test"));
            assert_eq!(ready.confirmed.server_packet_limit, 64 * 1024 * 1024);
            assert!(ready.confirmed.status.contains(
                mysql_common::constants::StatusFlags::SERVER_STATUS_NO_BACKSLASH_ESCAPES
            ));
            println!(
                "MySQL {label}: confirmed UTC, {charset}, timeouts, database and server limit"
            );
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn command_ownership_and_close_preserve_confirmed_local_facts() {
    use sqlaero_core::charset;
    use sqlaero_core::session::initialize::{Initialize, Options};
    use sqlaero_core::session::query::Query;
    use sqlaero_core::session::shared::{AccessError, Close, Connection};
    use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
    let root = root();
    for label in ["84", "97"] {
        for tls in [false, true] {
            let parameters = configuration(
                &root,
                label,
                !tls,
                tls.then_some(Mode::VerifyIdentity),
                "127.0.0.1",
            );
            let mut initialize = Initialize::new(
                Establish::new(parameters).unwrap(),
                Options {
                    encoding: charset::resolve(None, None).unwrap(),
                    autocommit: true,
                    sql_mode: None,
                    server_timeouts: [None, None, None, None],
                    result_memory_limit: 16 * 1024 * 1024,
                },
            );
            let ready = loop {
                if let Step::Ready(ready) = initialize.advance().unwrap() {
                    break ready;
                }
            };
            let connection = Connection::new(ready);
            let mut lease = connection.try_command().unwrap();
            assert!(matches!(connection.try_command(), Err(AccessError::Busy)));
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        assert!(matches!(connection.try_command(), Err(AccessError::Busy)));
                        assert_eq!(
                            connection.facts.lock().unwrap().time_zone.as_deref(),
                            Some("+00:00")
                        );
                    })
                    .join()
                    .unwrap();
            });
            let session = lease.session().unwrap();
            let mut query = Query::new(
                b"\x03SELECT 42".to_vec(),
                session.capabilities,
                session.max_send,
                session.max_receive,
                1024 * 1024,
            )
            .unwrap();
            let mut budget = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_secs(2)),
                None,
                Some(Duration::from_secs(1)),
            );
            loop {
                if let Step::Ready(_) = query.advance(&mut session.channel, &mut budget).unwrap() {
                    break;
                }
            }
            drop(lease);
            let mut close = Close::new(connection.try_command().unwrap()).unwrap();
            assert!(connection.is_closed());
            loop {
                if let Step::Ready(()) = close.advance(&mut budget).unwrap() {
                    break;
                }
            }
            drop(close);
            assert!(matches!(connection.try_command(), Err(AccessError::Closed)));
            assert_eq!(
                connection
                    .facts
                    .lock()
                    .unwrap()
                    .database
                    .as_ref()
                    .and_then(|value| value.as_deref()),
                Some(b"sqlaero_test".as_slice())
            );
            println!(
                "MySQL {label}: TLS={tls}, immediate busy, native query and explicit close passed"
            );
        }
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn native_numeric_literals_keep_mysql_approximate_and_exact_categories() {
    use mysql_common::constants::ColumnType;
    use sqlaero_core::bind::scalar;
    use sqlaero_core::result::text::Response;
    let root = root();
    for label in ["84", "97"] {
        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        for value in [0.1, -0.0, f64::from_bits(1), f64::MAX] {
            let mut sql = b"SELECT ".to_vec();
            scalar::float(&mut sql, value, 1024).unwrap();
            let Response::Rows(rows) =
                query(&mut connection, std::str::from_utf8(&sql).unwrap()).unwrap()
            else {
                panic!("expected scalar row")
            };
            assert_eq!(
                rows.columns[0].type_code,
                ColumnType::MYSQL_TYPE_DOUBLE as u8
            );
            let bytes = rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap();
            let actual: f64 = std::str::from_utf8(bytes).unwrap().parse().unwrap();
            // MySQL may normalize the sign of zero, which the input writer
            // still preserves. Every nonzero binary64 value remains exact.
            assert_eq!(actual, value);
            println!(
                "MySQL {label}: approximate literal {:?}, received {:?}",
                std::str::from_utf8(&sql[7..]).unwrap(),
                std::str::from_utf8(bytes).unwrap()
            );
        }
        let mut sql = b"SELECT ".to_vec();
        scalar::decimal(&mut sql, false, &[1, 2, 3, 4, 5, 6, 0, 0], -4, 1024).unwrap();
        let Response::Rows(rows) =
            query(&mut connection, std::str::from_utf8(&sql).unwrap()).unwrap()
        else {
            panic!("expected exact decimal")
        };
        assert_eq!(
            rows.columns[0].type_code,
            ColumnType::MYSQL_TYPE_NEWDECIMAL as u8
        );
        assert_eq!(
            rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(),
            Some(b"1234.56".as_slice())
        );
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn all_client_charsets_quote_complete_values_in_both_real_sql_modes() {
    use sqlaero_core::bind::writer;
    use sqlaero_core::charset::{
        self,
        codec::{Codec, CodecError},
    };
    use sqlaero_core::result::text::Response;
    use sqlaero_core::session::initialize::{Initialize, Options};
    use sqlaero_core::session::query::Query;
    let root = root();

    for label in ["84", "97"] {
        for name in CLIENT_CHARSETS {
            for no_backslash in [false, true] {
                let encoding = charset::resolve(Some(name), None).unwrap();
                let codec = Codec::new(encoding.charset);
                let mut initialize = Initialize::new(
                    Establish::new(configuration(&root, label, true, None, "localhost")).unwrap(),
                    Options {
                        encoding,
                        autocommit: true,
                        sql_mode: Some(
                            if no_backslash {
                                "NO_BACKSLASH_ESCAPES"
                            } else {
                                ""
                            }
                            .to_owned(),
                        ),
                        server_timeouts: [None, None, None, None],
                        result_memory_limit: 1024 * 1024,
                    },
                );
                let mut ready = loop {
                    if let Step::Ready(ready) = initialize.advance().unwrap() {
                        break ready;
                    }
                };
                let mut source = String::from("A'\"\0\n\r\x1a%_");
                for point in [
                    '\\', 'Ö', 'é', 'ß', 'α', 'ש', 'ქ', '中', 'ソ', 'ｱ', '한', '😀', '\u{E7C7}',
                    '\u{FFFD}',
                ] {
                    match codec.encode(&point.to_string(), &mut Vec::new(), 1024) {
                        Ok(()) => source.push(point),
                        Err(CodecError::Encode { .. }) => (),
                        Err(error) => panic!("unexpected route failure {name}: {error}"),
                    }
                }
                let mut request = if name == "binary" {
                    b"\x03SELECT HEX(".to_vec()
                } else {
                    b"\x03SELECT HEX(CONVERT(".to_vec()
                };
                codec
                    .literal(&source, &mut request, no_backslash, 1024 * 1024)
                    .unwrap();
                // This independent server conversion returns a fixed ASCII
                // observation even for a non-ASCII-compatible client charset.
                writer::append(
                    &mut request,
                    if name == "binary" {
                        b"),"
                    } else {
                        b" USING utf8mb4)),"
                    },
                    1024 * 1024,
                )
                .unwrap();
                writer::binary(&mut request, b"12", 1024 * 1024).unwrap();
                writer::append(&mut request, b"+0,HEX(", 1024 * 1024).unwrap();
                writer::binary(&mut request, b"\0\xff'\\", 1024 * 1024).unwrap();
                writer::append(&mut request, b")", 1024 * 1024).unwrap();
                let connection = &mut ready.transport;
                let mut query = Query::new(
                    request,
                    connection.capabilities,
                    connection.max_send,
                    connection.max_receive,
                    1024 * 1024,
                )
                .unwrap();
                let response = loop {
                    if let Step::Ready(response) = query
                        .advance(&mut connection.channel, &mut connection.budget)
                        .unwrap()
                    {
                        break response;
                    }
                };
                let Response::Rows(rows) = response else {
                    panic!("expected literal observation")
                };
                let row = rows.row(0).unwrap();
                // The binary pseudo charset has byte-valued semantics, not
                // Unicode transcoding. Assert its actual bytes directly.
                let expected_bytes = if name == "binary" {
                    source
                        .chars()
                        .map(|point| u32::from(point) as u8)
                        .collect::<Vec<_>>()
                } else {
                    source.as_bytes().to_vec()
                };
                let expected = expected_bytes
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<String>();
                assert_eq!(
                    row[0].get(&rows.bytes).unwrap(),
                    Some(expected.as_bytes()),
                    "{label}/{name}/NBE={no_backslash}"
                );
                assert_eq!(row[1].get(&rows.bytes).unwrap(), Some(b"12".as_slice()));
                assert_eq!(
                    row[2].get(&rows.bytes).unwrap(),
                    Some(b"00FF275C".as_slice())
                );
            }
        }
        println!("MySQL {label}: all 36 client charsets, two SQL modes and binary CAST passed");
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn native_template_and_value_writers_keep_sql_data_separate_on_the_server() {
    use sqlaero_core::{
        bind::{scalar, template::Template},
        charset::{self, codec::Codec},
        result::text::Response,
        session::{
            initialize::{Initialize, Options},
            query::Query,
        },
    };
    let root = root();
    for label in ["84", "97"] {
        for name in ["utf8mb4", "sjis", "gbk", "swe7"] {
            for no_backslash in [false, true] {
                let encoding = charset::resolve(Some(name), None).unwrap();
                let codec = Codec::new(encoding.charset);
                let mut job = Initialize::new(
                    Establish::new(configuration(&root, label, true, None, "localhost")).unwrap(),
                    Options {
                        encoding,
                        autocommit: true,
                        sql_mode: Some(
                            if no_backslash {
                                "NO_BACKSLASH_ESCAPES"
                            } else {
                                ""
                            }
                            .to_owned(),
                        ),
                        server_timeouts: [None, None, None, None],
                        result_memory_limit: 1024 * 1024,
                    },
                );
                let mut ready = loop {
                    if let Step::Ready(ready) = job.advance().unwrap() {
                        break ready;
                    }
                };
                let source = "SELECT %s,'%s',10 %% %s";
                let mut encoded = Vec::new();
                codec.encode(source, &mut encoded, 1024).unwrap();
                let template = Template::compile(encoded, codec, no_backslash, false).unwrap();
                assert_eq!(template.slots(), 2);
                let data = match name {
                    "sjis" => "ソ' OR 1=1 -- %s",
                    "swe7" => "Ö' OR 1=1 -- %s",
                    _ => "' OR 1=1 -- %s\\",
                };
                let mut request = vec![3];
                let mut position = 0;
                assert!(
                    template
                        .append_until_slot(&mut position, &mut request, 1024)
                        .unwrap()
                        .is_some()
                );
                codec
                    .literal(data, &mut request, no_backslash, 1024)
                    .unwrap();
                assert!(
                    template
                        .append_until_slot(&mut position, &mut request, 1024)
                        .unwrap()
                        .is_some()
                );
                scalar::integer(&mut request, 3u8, 1024).unwrap();
                assert!(
                    template
                        .append_until_slot(&mut position, &mut request, 1024)
                        .unwrap()
                        .is_none()
                );
                let connection = &mut ready.transport;
                let mut query = Query::new(
                    request,
                    connection.capabilities,
                    connection.max_send,
                    connection.max_receive,
                    1024 * 1024,
                )
                .unwrap();
                let response = loop {
                    if let Step::Ready(response) = query
                        .advance(&mut connection.channel, &mut connection.budget)
                        .unwrap()
                    {
                        break response;
                    }
                };
                let Response::Rows(rows) = response else {
                    panic!("expected rows")
                };
                let row = rows.row(0).unwrap();
                assert_eq!(
                    codec
                        .decode(row[0].get(&rows.bytes).unwrap().unwrap())
                        .unwrap(),
                    data
                );
                assert_eq!(row[1].get(&rows.bytes).unwrap(), Some(b"%s".as_slice()));
                assert_eq!(row[2].get(&rows.bytes).unwrap(), Some(b"1".as_slice()));
            }
        }
        println!(
            "MySQL {label}: protected positions, modulo percent and injection-shaped values remained data"
        );
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn dash_comment_space_classification_matches_mysql_for_supported_encodings() {
    use sqlaero_core::{
        charset::{self, codec::Codec},
        result::text::{Response, ResultError},
        session::query::{Query, QueryError},
        transport::deadline::{IoBudget, TimeoutKind},
    };
    let root = root();
    let mut mismatches = Vec::new();
    let mut all = Vec::new();
    let evidence =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evidence/rust-sync-kernel/charset");
    std::fs::create_dir_all(&evidence).unwrap();
    for label in ["84", "97"] {
        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        let mut observations = String::new();
        for name in CLIENT_CHARSETS {
            query(&mut connection, &format!("SET NAMES {name}")).unwrap();
            let codec = Codec::new(charset::resolve(Some(name), None).unwrap().charset);
            for byte in 0u8..=255 {
                // LF and CR line behavior is checked separately from the byte class:
                // LF terminates the comment immediately; CR does not.
                if matches!(byte, b'\n' | b'\r') {
                    continue;
                }
                let mut request = b"\x03SELECT 7--".to_vec();
                request.push(byte);
                request.extend_from_slice(b" %s");
                let mut job = Query::new(
                    request,
                    connection.capabilities,
                    connection.max_send,
                    connection.max_receive,
                    1024 * 1024,
                )
                .unwrap();
                let mut budget = IoBudget::new(
                    TimeoutKind::Operation,
                    Some(Duration::from_secs(2)),
                    None,
                    None,
                );
                let response = loop {
                    match job.advance(&mut connection.channel, &mut budget) {
                        Ok(Step::Ready(value)) => break Ok(value),
                        Ok(_) => (),
                        Err(error) => break Err(error),
                    }
                };
                let comment = match response {
                    Ok(Response::Rows(rows)) => {
                        assert_eq!(
                            rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(),
                            Some(b"7".as_slice())
                        );
                        true
                    }
                    Err(QueryError::Result(ResultError::Server(_))) => false,
                    _ => panic!("unexpected comment-classification response"),
                };
                observations.push_str(&format!("{name}\t{byte:02X}\t{}\n", u8::from(comment)));
                if codec.comment_space(byte) != comment {
                    mismatches.push(format!("{label}/{name}/0x{byte:02X}"));
                }
            }
        }
        std::fs::write(
            evidence.join(format!("lexer-space-mysql{label}.tsv")),
            &observations,
        )
        .unwrap();
        for ending in ['\n', '\r'] {
            let sql = format!("SELECT 7--{ending}%s");
            let active = match query(&mut connection, &sql) {
                Ok(Response::Rows(_)) => 0,
                Err(QueryError::Result(ResultError::Server(_))) => 1,
                _ => panic!("unexpected line-ending response"),
            };
            let codec = Codec::new(charset::resolve(Some("ujis"), None).unwrap().charset);
            let template = sqlaero_core::bind::template::Template::compile(
                sql.into_bytes(),
                codec,
                false,
                false,
            )
            .unwrap();
            assert_eq!(
                template.slots(),
                active,
                "MySQL {label} line ending U+{:04X}",
                u32::from(ending)
            );
        }
        all.push(observations);
    }
    assert_eq!(
        all[0], all[1],
        "server byte classes must agree before generating static masks"
    );
    assert!(
        mismatches.is_empty(),
        "comment byte-class differences: {}",
        mismatches.join(", ")
    );
}

const CLIENT_CHARSETS: [&str; 36] = [
    "utf8mb4", "ascii", "binary", "armscii8", "cp1250", "cp1251", "cp1256", "cp1257", "cp850",
    "cp852", "cp866", "dec8", "geostd8", "greek", "hebrew", "hp8", "keybcs2", "koi8r", "koi8u",
    "latin1", "latin2", "latin5", "latin7", "macce", "macroman", "swe7", "tis620", "big5", "cp932",
    "eucjpms", "euckr", "gb18030", "gb2312", "gbk", "sjis", "ujis",
];

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn tracked_session_roles_change_independently_after_actual_confirmations() {
    use sqlaero_core::{
        charset,
        session::{
            initialize::{Initialize, Options},
            query::Query,
            shared::Connection,
        },
        transport::deadline::{IoBudget, TimeoutKind},
    };
    let root = root();
    for label in ["84", "97"] {
        let mut initialize = Initialize::new(
            Establish::new(configuration(&root, label, true, None, "localhost")).unwrap(),
            Options {
                encoding: charset::resolve(None, None).unwrap(),
                autocommit: true,
                sql_mode: None,
                server_timeouts: [None, None, None, None],
                result_memory_limit: 16 * 1024 * 1024,
            },
        );
        let ready = loop {
            match initialize.advance().unwrap() {
                Step::Ready(ready) => break ready,
                Step::Control => (),
                Step::Wait(_) => panic!("unexpected wait"),
            }
        };
        let connection = Connection::new(ready);
        let mut lease = connection.try_command().unwrap();
        for (sql, expected) in [
            ("SET SESSION character_set_results=latin1", 0),
            (
                "SET SESSION time_zone='+08:00',sql_mode='ANSI_QUOTES,NO_BACKSLASH_ESCAPES'",
                1,
            ),
            (
                "SET SESSION net_read_timeout=17,net_write_timeout=18,wait_timeout=19,innodb_lock_wait_timeout=20",
                2,
            ),
            ("SET NAMES cp1251", 3),
            ("SET SESSION character_set_results=NULL", 4),
            ("SET SESSION time_zone='+00:00',autocommit=0", 5),
            ("START TRANSACTION", 6),
            ("COMMIT AND NO CHAIN NO RELEASE", 7),
        ] {
            let session = lease.session().unwrap();
            let mut request = vec![3];
            request.extend_from_slice(sql.as_bytes());
            let mut job = Query::new(
                request,
                session.capabilities,
                session.max_send,
                session.max_receive,
                1024 * 1024,
            )
            .unwrap();
            let mut budget = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_secs(2)),
                None,
                None,
            );
            let response = loop {
                match job.advance(&mut session.channel, &mut budget).unwrap() {
                    Step::Ready(response) => break response,
                    Step::Control => (),
                    Step::Wait(_) => panic!("unexpected wait"),
                }
            };
            let mut facts = connection.facts.lock().unwrap();
            for entry in response.completion().session_state_info().unwrap() {
                println!(
                    "MySQL {label}: {sql}; tracking {:?}: {:?}",
                    entry.data_type(),
                    entry.data_ref()
                );
            }
            facts.apply(response.completion()).unwrap();
            let status = facts.status.unwrap();
            use mysql_common::constants::StatusFlags as S;
            match expected {
                0 => {
                    assert_eq!(facts.client_charset.as_deref(), Some("utf8mb4"));
                    assert_eq!(
                        facts.results_charset.as_ref().unwrap().as_deref(),
                        Some("latin1")
                    );
                    assert_eq!(facts.metadata_codec().unwrap().name(), "latin1");
                }
                1 => {
                    assert!(!facts.utc_confirmed());
                    assert!(facts.sql_mode.as_ref().unwrap().contains("ANSI_QUOTES"));
                    assert!(status.contains(S::SERVER_STATUS_NO_BACKSLASH_ESCAPES));
                }
                2 => assert_eq!(
                    facts.server_timeouts,
                    [Some(17), Some(18), Some(19), Some(20)]
                ),
                3 => {
                    assert_eq!(facts.client_charset.as_deref(), Some("cp1251"));
                    assert_eq!(facts.client_codec().unwrap().name(), "cp1251");
                    assert_eq!(facts.connection_charset.as_deref(), Some("cp1251"));
                    assert_eq!(facts.metadata_codec().unwrap().name(), "cp1251");
                }
                4 => {
                    assert_eq!(facts.results_charset, Some(None));
                    assert_eq!(facts.metadata_codec().unwrap().name(), "utf8mb3");
                    assert_eq!(facts.client_codec().unwrap().name(), "cp1251");
                }
                5 => {
                    assert!(facts.utc_confirmed());
                    assert!(!status.contains(S::SERVER_STATUS_AUTOCOMMIT));
                }
                6 => assert!(status.contains(S::SERVER_STATUS_IN_TRANS)),
                7 => assert!(!status.contains(S::SERVER_STATUS_IN_TRANS)),
                _ => unreachable!(),
            }
        }
        println!(
            "MySQL {label}: independent tracked roles, modes, UTC, server timeouts, transaction status passed"
        );
    }
}

#[test]
#[ignore = "requires both isolated MySQL servers; run this binary with one test thread"]
fn actual_error_text_bytes_under_results_encodings_are_preserved() {
    use sqlaero_core::{result::text::ResultError, session::query::QueryError};
    let root = root();
    for label in ["84", "97"] {
        let mut connection =
            establish(configuration(&root, label, true, None, "localhost")).unwrap();
        for encoding in [
            "utf8mb4", "latin1", "utf16", "utf16le", "ucs2", "utf32", "binary", "NULL",
        ] {
            query(
                &mut connection,
                &format!("SET character_set_results={encoding}"),
            )
            .unwrap();
            match query(&mut connection, "SELECT `é`") {
                Err(QueryError::Result(ResultError::Server(error))) => {
                    println!(
                        "MySQL {label} {encoding}: code {}, raw length {}, bytes {:?}",
                        error.error_code(),
                        error.message_ref().len(),
                        error.message_ref()
                    );
                    assert_eq!(error.error_code(), 1054);
                }
                _ => panic!("expected the actual missing-column ERR"),
            }
        }
    }
}
