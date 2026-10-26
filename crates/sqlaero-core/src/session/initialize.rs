//! Session initialization under the original connection deadline.

use super::connect::{Authenticated, ConnectError, Establish};
use super::query::{Query, QueryError};
use crate::bind::writer;
use crate::charset::{self, CatalogError, Resolved};
use crate::protocol::framing::WireError;
use crate::result::text::{Buffered, Response};
use crate::transport::channel::Step;
use mysql_common::constants::StatusFlags;
use num_bigint::BigUint;

const TRACKED: &str = "autocommit,time_zone,sql_mode,character_set_client,character_set_connection,character_set_results,collation_connection,net_read_timeout,net_write_timeout,wait_timeout,innodb_lock_wait_timeout";
const TIMEOUT_NAMES: [&str; 4] = [
    "net_read_timeout",
    "net_write_timeout",
    "wait_timeout",
    "innodb_lock_wait_timeout",
];
const CONFIRM: &[u8] = b"SELECT @@SESSION.autocommit,@@SESSION.time_zone,@@SESSION.sql_mode,@@SESSION.character_set_client,@@SESSION.character_set_connection,@@SESSION.character_set_results,@@SESSION.collation_connection,@@SESSION.net_read_timeout,@@SESSION.net_write_timeout,@@SESSION.wait_timeout,@@SESSION.innodb_lock_wait_timeout,@@SESSION.max_allowed_packet";

/// Already validated session configuration. Server timeout integers are not
/// narrowed to a platform-specific server range; MySQL decides their validity.
pub struct Options {
    /// One resolved supported charset/collation pair.
    pub encoding: Resolved,
    /// Initial autocommit selection.
    pub autocommit: bool,
    /// None preserves the actual server default; an empty string clears it.
    pub sql_mode: Option<String>,
    /// Optional positive server-side whole-second values in TIMEOUT_NAMES order.
    pub server_timeouts: [Option<BigUint>; 4],
    /// Native result assembly limit, also applied to internal confirmation rows.
    pub result_memory_limit: usize,
}

/// Fully confirmed initialization values, ready to move into the live session
/// authority. No requested value is substituted for an actual server value.
pub struct Confirmed {
    /// Actual final protocol flags.
    pub status: StatusFlags,
    /// Actual selected database, read in the UTF-8 authentication context.
    pub database: Option<String>,
    /// Actual session time-zone token; initialization requires +00:00.
    pub time_zone: String,
    /// Actual server-normalized SQL mode.
    pub sql_mode: String,
    /// Actual initial encoding selection.
    pub encoding: Resolved,
    /// Actual four server-side timeout values, after clamping or normalization.
    pub server_timeouts: [u64; 4],
    /// Actual server receive limit, used with the client's send limit.
    pub server_packet_limit: usize,
}

/// A Ready native session, retaining the original authenticated transport.
pub struct Ready {
    /// Native channel, capabilities and immutable server metadata.
    pub transport: Authenticated,
    /// Confirmed session facts, not a second independent cache.
    pub confirmed: Confirmed,
}

/// Preparation, server or confirmation failure during connection initialization.
#[derive(Debug, thiserror::Error)]
pub enum InitializeError {
    /// Authentication or transport establishment failed.
    #[error(transparent)]
    Connect(#[from] ConnectError),
    /// A real internal command failed.
    #[error(transparent)]
    Query(#[from] QueryError),
    /// Request bytes or returned fields were malformed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// Actual returned encoding identities did not belong to the static catalog.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// A required initialization fact was not established.
    #[error("session initialization could not confirm {0}")]
    Confirmation(&'static str),
}

enum Phase {
    Authenticate,
    Probe,
    Apply,
    Confirm,
    Finished,
}

/// Concrete connection initialization. The first small read learns the server
/// packet bound before a configurable SET is built. All assignments are then
/// combined, and required actual values are confirmed in one final read.
pub struct Initialize {
    establish: Option<Establish>,
    authenticated: Option<Authenticated>,
    options: Options,
    phase: Phase,
    query: Option<Query>,
    maximum: usize,
    database: Option<String>,
}

impl Initialize {
    /// Continue from the existing establishment job; no new timeout is created.
    #[must_use]
    pub fn new(establish: Establish, options: Options) -> Self {
        Self {
            establish: Some(establish),
            authenticated: None,
            options,
            phase: Phase::Authenticate,
            query: None,
            maximum: 0,
            database: None,
        }
    }

    /// Advance until a Ready native session or a host signal-check boundary.
    ///
    /// # Errors
    /// Returns actual authentication, request, server, resource or confirmation
    /// failures. The original connect deadline covers all three internal commands.
    pub fn advance(&mut self) -> Result<Step<Ready>, InitializeError> {
        let result = self.advance_inner();
        if result.is_err() {
            self.phase = Phase::Finished;
            self.query = None;
            self.authenticated = None;
            self.establish = None;
        }
        result
    }

    fn advance_inner(&mut self) -> Result<Step<Ready>, InitializeError> {
        loop {
            if matches!(self.phase, Phase::Authenticate) {
                let establish = self
                    .establish
                    .as_mut()
                    .ok_or(InitializeError::Confirmation("authentication state"))?;
                match establish.advance()? {
                    Step::Control => return Ok(Step::Control),
                    Step::Wait(_) => {
                        return Err(InitializeError::Confirmation("authentication continuation"));
                    }
                    Step::Ready(authenticated) => {
                        self.authenticated = Some(authenticated);
                        self.establish = None;
                        self.phase = Phase::Probe;
                        self.prepare_query(b"SELECT @@SESSION.max_allowed_packet,DATABASE()")?;
                    }
                }
            }
            if matches!(self.phase, Phase::Finished) {
                return Err(InitializeError::Confirmation(
                    "initialization already consumed",
                ));
            }
            let connection = self
                .authenticated
                .as_mut()
                .ok_or(InitializeError::Confirmation("authenticated transport"))?;
            let query = self
                .query
                .as_mut()
                .ok_or(InitializeError::Confirmation("internal query"))?;
            let response = match query.advance(&mut connection.channel, &mut connection.budget)? {
                Step::Ready(response) => response,
                Step::Control => return Ok(Step::Control),
                Step::Wait(_) => return Err(InitializeError::Confirmation("query continuation")),
            };
            self.query = None;
            match self.phase {
                Phase::Probe => {
                    let rows = one_row(response, 2)?;
                    self.maximum = number(&rows, 0)?
                        .try_into()
                        .map_err(|_| InitializeError::Confirmation("server packet limit"))?;
                    if self.maximum == 0 {
                        return Err(InitializeError::Confirmation(
                            "positive server packet limit",
                        ));
                    }
                    self.database = value(&rows, 1)?
                        .map(|bytes| std::str::from_utf8(bytes).map(str::to_owned))
                        .transpose()
                        .map_err(|_| {
                            InitializeError::Confirmation("database text in the handshake charset")
                        })?;
                    let limit = connection.max_send.min(self.maximum);
                    let settings = settings(&self.options, limit)?;
                    self.phase = Phase::Apply;
                    self.prepare_query(&settings)?;
                }
                Phase::Apply => {
                    if !matches!(response, Response::Command(_)) {
                        return Err(InitializeError::Confirmation("SET completion"));
                    }
                    self.phase = Phase::Confirm;
                    self.prepare_query(CONFIRM)?;
                }
                Phase::Confirm => {
                    let rows = one_row(response, 12)?;
                    let client = text(&rows, 3)?;
                    let connection_charset = text(&rows, 4)?;
                    let results = text(&rows, 5)?;
                    let collation = text(&rows, 6)?;
                    let encoding = charset::resolve(Some(client), Some(collation))?;
                    if client != connection_charset
                        || client != results
                        || encoding.charset.name != self.options.encoding.charset.name
                        || encoding.collation.name != self.options.encoding.collation.name
                    {
                        return Err(InitializeError::Confirmation(
                            "requested charset/collation and all three encoding roles",
                        ));
                    }
                    let zone = text(&rows, 1)?.to_owned();
                    if zone != "+00:00" {
                        return Err(InitializeError::Confirmation("UTC session time zone"));
                    }
                    let auto = number(&rows, 0)?;
                    if auto != u64::from(self.options.autocommit) {
                        return Err(InitializeError::Confirmation("autocommit"));
                    }
                    let status = rows.completion.status_flags();
                    if status.contains(StatusFlags::SERVER_STATUS_AUTOCOMMIT)
                        != self.options.autocommit
                    {
                        return Err(InitializeError::Confirmation("protocol autocommit status"));
                    }
                    let confirmed = Confirmed {
                        status,
                        database: self.database.take(),
                        time_zone: zone,
                        sql_mode: text(&rows, 2)?.to_owned(),
                        encoding,
                        server_timeouts: [
                            number(&rows, 7)?,
                            number(&rows, 8)?,
                            number(&rows, 9)?,
                            number(&rows, 10)?,
                        ],
                        server_packet_limit: number(&rows, 11)?
                            .try_into()
                            .map_err(|_| InitializeError::Confirmation("server packet limit"))?,
                    };
                    connection.budget.check().map_err(ConnectError::from)?;
                    self.phase = Phase::Finished;
                    return Ok(Step::Ready(Ready {
                        transport: self
                            .authenticated
                            .take()
                            .ok_or(InitializeError::Confirmation("ready transport"))?,
                        confirmed,
                    }));
                }
                _ => return Err(InitializeError::Confirmation("initialization phase")),
            }
        }
    }

    fn prepare_query(&mut self, sql: &[u8]) -> Result<(), InitializeError> {
        let connection = self
            .authenticated
            .as_ref()
            .ok_or(InitializeError::Confirmation("authenticated transport"))?;
        let limit = if self.maximum == 0 {
            connection.max_send
        } else {
            connection.max_send.min(self.maximum)
        };
        let mut bytes = Vec::new();
        writer::append(&mut bytes, &[3], limit)?;
        writer::append(&mut bytes, sql, limit)?;
        self.query = Some(Query::new(
            bytes,
            connection.capabilities,
            limit,
            connection.max_receive,
            self.options.result_memory_limit,
        )?);
        Ok(())
    }
}

fn settings(options: &Options, limit: usize) -> Result<Vec<u8>, WireError> {
    let mut sql = Vec::new();
    writer::append(
        &mut sql,
        b"SET SESSION session_track_system_variables=",
        limit,
    )?;
    writer::binary(&mut sql, TRACKED.as_bytes(), limit)?;
    writer::append(
        &mut sql,
        b",session_track_schema=ON,session_track_state_change=ON,time_zone=",
        limit,
    )?;
    writer::binary(&mut sql, b"+00:00", limit)?;
    writer::append(
        &mut sql,
        if options.autocommit {
            b",autocommit=1"
        } else {
            b",autocommit=0"
        },
        limit,
    )?;
    for name in [
        "character_set_client",
        "character_set_connection",
        "character_set_results",
    ] {
        writer::append(&mut sql, b",", limit)?;
        writer::append(&mut sql, name.as_bytes(), limit)?;
        writer::append(&mut sql, b"=", limit)?;
        writer::binary(&mut sql, options.encoding.charset.name.as_bytes(), limit)?;
    }
    writer::append(&mut sql, b",collation_connection=", limit)?;
    writer::binary(&mut sql, options.encoding.collation.name.as_bytes(), limit)?;
    if let Some(mode) = &options.sql_mode {
        writer::append(&mut sql, b",sql_mode=", limit)?;
        writer::binary(&mut sql, mode.as_bytes(), limit)?;
    }
    for (name, value) in TIMEOUT_NAMES.into_iter().zip(&options.server_timeouts) {
        if let Some(value) = value {
            writer::append(&mut sql, b",", limit)?;
            writer::append(&mut sql, name.as_bytes(), limit)?;
            writer::append(&mut sql, b"=", limit)?;
            writer::append(&mut sql, value.to_str_radix(10).as_bytes(), limit)?;
        }
    }
    Ok(sql)
}

fn one_row(response: Response, columns: usize) -> Result<Buffered, InitializeError> {
    match response {
        Response::Rows(rows) if rows.row_count == 1 && rows.columns.len() == columns => Ok(rows),
        _ => Err(InitializeError::Confirmation(
            "one complete internal confirmation row",
        )),
    }
}
fn value(rows: &Buffered, column: usize) -> Result<Option<&[u8]>, InitializeError> {
    rows.row(0)
        .and_then(|row| row.get(column))
        .ok_or(InitializeError::Confirmation("confirmation field"))?
        .get(&rows.bytes)
        .map_err(Into::into)
}
fn text(rows: &Buffered, column: usize) -> Result<&str, InitializeError> {
    std::str::from_utf8(
        value(rows, column)?.ok_or(InitializeError::Confirmation("non-NULL session text"))?,
    )
    .map_err(|_| InitializeError::Confirmation("ASCII session variable text"))
}
fn number(rows: &Buffered, column: usize) -> Result<u64, InitializeError> {
    text(rows, column)?
        .parse()
        .map_err(|_| InitializeError::Confirmation("unsigned session number"))
}
