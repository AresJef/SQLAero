//! Public connection ownership over the shared native protocol and session facts.

use crate::{
    config::{self, Argument, Ssl, Timeouts},
    cursor::Cursor,
    errors,
    input::{self, ScalarTypes},
};
use num_bigint::BigUint;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::{
    bind::{Context, writer},
    charset::{self, output::OutputCodec},
    error::ClientErrorCode,
    protocol::framing::{MAX_MESSAGE_PAYLOAD, MessageWriter},
    result::{
        decode::Options,
        text::{Response, ResultError},
    },
    session::{
        connect::{ConnectParams, Endpoint, Establish},
        initialize::{Initialize, Options as InitOptions},
        query::{CommandConfirmation, Query, QueryError, RequestProgress},
        shared::{CommandLease, Connection as NativeConnection},
    },
    transport::{
        channel::Step,
        deadline::{IoBudget, TimeoutKind},
        socket::WaitStatus,
    },
};
use std::{
    sync::{Arc, Mutex, MutexGuard, atomic::AtomicBool},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(crate) struct Runtime {
    pub(crate) timeouts: Timeouts,
    pub(crate) options: Options,
}
/// Facts for one concrete internal command, not a guess from transaction flags.
#[derive(Default)]
pub(crate) struct ControlAttempt {
    pub(crate) started: bool,
    pub(crate) confirmed: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandRole {
    UserSql,
    InternalControl,
}
#[derive(Clone, Copy)]
enum FailureOutcome {
    NotSent,
    Sending,
    AwaitingResponse,
    ServerErr,
    Confirmed,
    Consumed,
}
impl FailureOutcome {
    fn unknown(self) -> bool {
        matches!(self, Self::Sending | Self::AwaitingResponse)
    }
    fn diagnostic(self) -> &'static str {
        match self {
            Self::NotSent => "The current request was not sent.",
            Self::Sending => {
                "Sending of the current request began, but no complete response was received; its execution outcome is unknown. Entered sending does not prove server receipt or nonexecution. No replay was sent."
            }
            Self::AwaitingResponse => {
                "The current request was fully sent, but no complete server response was received; its execution outcome is unknown. This does not mean it was not executed. No replay was sent."
            }
            Self::ServerErr => {
                "A complete server ERR was received for the current request; it does not prove zero side effects or a transaction rollback."
            }
            Self::Confirmed => {
                "The final server response was fully confirmed before the later local failure; execution is not unknown. Command confirmation alone does not imply transaction commit."
            }
            Self::Consumed => {
                "All unsupported multiple results were fully consumed; their final session state was confirmed, but no public result was published."
            }
        }
    }
}
/// A real native availability failure is distinguished from an original
/// Python exception, even if a signal handler raises the same exception class.
pub(crate) struct DriveFailure {
    error: PyErr,
    unavailable: bool,
    confirmed: Option<CommandConfirmation>,
    outcome: FailureOutcome,
}
impl DriveFailure {
    fn new(
        py: Python<'_>,
        error: PyErr,
        outcome: FailureOutcome,
        unavailable: bool,
        confirmed: Option<CommandConfirmation>,
    ) -> Self {
        errors::note(py, &error, outcome.diagnostic());
        Self {
            error,
            unavailable,
            confirmed,
            outcome,
        }
    }
    pub(crate) fn record_prefix(
        self,
        py: Python<'_>,
        prefix: &mut sqlaero_core::session::query::ConfirmedPrefix,
        rows: std::ops::Range<usize>,
    ) -> PyErr {
        if let Some(confirmed) = self.confirmed
            && let Err(error) = prefix.record_confirmation(rows.clone(), confirmed)
        {
            errors::note(
                py,
                &self.error,
                format!(
                    "A complete response was received, but cumulative prefix recording failed: {error}"
                ),
            );
        }
        errors::note(
            py,
            &self.error,
            format!(
                "The current source rows {}..{} belong to this request; any subsequent source rows were not attempted.",
                rows.start, rows.end
            ),
        );
        self.error
    }
}
impl From<DriveFailure> for PyErr {
    fn from(failure: DriveFailure) -> Self {
        failure.error
    }
}
fn unavailable(error: &QueryError) -> bool {
    use sqlaero_core::{
        protocol::framing::WireError,
        transport::{channel::ChannelError, socket::TransportError},
    };
    use std::io::ErrorKind;
    match error {
        QueryError::Channel(
            ChannelError::Timeout(_) | ChannelError::Transport(TransportError::Timeout(_)),
        ) => true,
        QueryError::Wire(WireError::Io(error))
        | QueryError::Result(ResultError::Wire(WireError::Io(error)))
        | QueryError::Channel(
            ChannelError::Wire(WireError::Io(error))
            | ChannelError::Transport(TransportError::Io(error)),
        ) => matches!(
            error.kind(),
            ErrorKind::UnexpectedEof
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::BrokenPipe
                | ErrorKind::TimedOut
        ),
        _ => false,
    }
}
/// Fully initialized synchronous MySQL connection. Commands share one native
/// wire owner; buffered Cursor reads retain independent local ownership.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Connection {
    pub(crate) native: Arc<NativeConnection>,
    runtime: Mutex<Runtime>,
    pub(crate) types: Py<ScalarTypes>,
    pub(crate) result_limit: usize,
    max_send: usize,
    max_receive: usize,
    total_result_limit: usize,
    ssl_mode: &'static str,
    tls_version: Option<&'static str>,
    pub(crate) transaction_scope: Arc<AtomicBool>,
}
impl Connection {
    pub(crate) fn runtime(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Runtime>> {
        self.runtime.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Connection configuration was invalidated by a native failure",
            )
        })
    }
    pub(crate) fn open(&self, py: Python<'_>) -> PyResult<()> {
        if self.native.is_closed() {
            Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Connection is closed or permanently retired",
            ))
        } else {
            Ok(())
        }
    }
    fn modify(&self, py: Python<'_>, name: &str) -> PyResult<MutexGuard<'_, Runtime>> {
        let runtime = self.runtime(py)?;
        if self.native.is_closed() {
            drop(runtime);
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                format!(
                    "Cannot modify Connection.{name} because this connection is closed or retired. Its saved value remains readable."
                ),
            ));
        }
        Ok(runtime)
    }
    pub(crate) fn ensure_context(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        let needed = self
            .native
            .facts
            .lock()
            .map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "confirmed session facts are unavailable",
                )
            })?
            .needs_context();
        if !needed {
            return Ok(());
        }
        input::check(py, budget)?;
        // This fixed, unquoted ASCII command is valid independently of SQL
        // mode under MySQL's admitted ASCII-compatible client charsets. Its
        // fields use their returned collation IDs, never a guessed old codec.
        let request = b"\x03SELECT @@SESSION.character_set_client,@@SESSION.character_set_connection,@@SESSION.character_set_results,@@SESSION.collation_connection,@@SESSION.sql_mode,@@SESSION.time_zone".to_vec();
        let job = self.query_job(py, lease, request, false)?;
        let response = self.drive(py, lease, job, budget, CommandRole::InternalControl)?;
        let Response::Rows(rows) = response else {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "execution context confirmation returned no row result",
            ));
        };
        let confirmed = self
            .native
            .facts
            .lock()
            .map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "confirmed session facts are unavailable",
                )
            })?
            .confirm_context(&rows);
        confirmed.map_err(|error| errors::wire(py, error))?;
        input::check(py, budget)
    }
    pub(crate) fn context(&self, py: Python<'_>) -> PyResult<(Context, bool, OutputCodec)> {
        let context: Result<_, (ClientErrorCode, &'static str)> = {
            let facts = self.native.facts.lock().map_err(|error| {
                drop(error.into_inner());
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "confirmed session facts are unavailable",
                )
            })?;
            (|| {
                let codec = facts.client_codec().ok_or((ClientErrorCode::UnsupportedCharset, "The actual client charset is unknown or unsupported; it is not reinterpreted as utf8mb4"))?;
                let mode = facts.sql_mode.as_ref().ok_or((
                    ClientErrorCode::SessionState,
                    "SQL mode is not confirmed; safe binding requires a confirmed session context",
                ))?;
                let has_mode = |token: &str| mode.split(',').any(|part| part == token);
                Ok((
                    Context {
                        codec,
                        no_backslash_escapes: has_mode("NO_BACKSLASH_ESCAPES"),
                        utc_confirmed: facts.utc_confirmed(),
                        limit: self.max_send.min(facts.server_packet_limit),
                    },
                    has_mode("ANSI_QUOTES"),
                    facts.metadata_codec().ok_or((
                        ClientErrorCode::UnsupportedCharset,
                        "actual result metadata encoding is unknown or unsupported",
                    ))?,
                ))
            })()
        };
        context.map_err(|(code, message)| errors::client(py, code, message))
    }
    pub(crate) fn query_job(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        request: Vec<u8>,
        discard: bool,
    ) -> PyResult<Query> {
        let session = lease.session().map_err(|error| errors::access(py, error))?;
        Query::with_pool(
            request,
            session.capabilities,
            session.max_send,
            session.max_receive,
            self.result_limit,
            Arc::clone(&self.native.result_pool),
            discard,
        )
        .map_err(|error| errors::query(py, error))
    }
    fn publish(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        completion: &sqlaero_core::protocol::OkPacket<'_>,
    ) -> PyResult<()> {
        let applied = match self.native.facts.lock() {
            Ok(mut facts) => {
                let result = facts.apply(completion);
                if result.is_err() {
                    // The terminator's status is valid, but partially decoded
                    // SESSION_TRACK values must never remain a mixed snapshot.
                    facts.invalidate_settings();
                    facts.database = None;
                }
                result
            }
            Err(_) => {
                lease.invalidate();
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The connection was retired after confirmed session state was invalidated by a native failure",
                ));
            }
        };
        if let Err(error) = applied {
            lease.invalidate();
            return Err(errors::wire(py, error));
        }
        Ok(())
    }
    /// Retire an incomplete exchange using its actual framing/ACK facts. This
    /// common exit also handles original Python control exceptions unchanged.
    fn query_failure(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        error: PyErr,
        unavailable: bool,
        keep_unsent: bool,
    ) -> DriveFailure {
        let progress = job.progress();
        let confirmed = job.take_confirmation();
        let outcome = if let Some(confirmation) = confirmed {
            if let Ok(mut facts) = self.native.facts.lock() {
                facts.invalidate_settings();
                facts.database = None;
                facts.status = Some(confirmation.status());
            }
            FailureOutcome::Confirmed
        } else {
            if progress != RequestProgress::NotSent
                && let Ok(mut facts) = self.native.facts.lock()
            {
                facts.invalidate_unconfirmed();
            }
            if job.received_server_err() {
                FailureOutcome::ServerErr
            } else {
                match progress {
                    RequestProgress::NotSent => FailureOutcome::NotSent,
                    RequestProgress::Sending => FailureOutcome::Sending,
                    RequestProgress::AwaitingResponse => FailureOutcome::AwaitingResponse,
                }
            }
        };
        if !keep_unsent || progress != RequestProgress::NotSent {
            lease.invalidate();
        }
        DriveFailure::new(py, error, outcome, unavailable, confirmed)
    }
    pub(crate) fn drive(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        mut job: Query,
        budget: &mut IoBudget,
        role: CommandRole,
    ) -> Result<Response, DriveFailure> {
        // A user command can change result encoding and then return ERR without
        // SESSION_TRACK. Its entry codec cannot certify these diagnostic bytes.
        // Fixed internal controls cannot perform such an unreported change.
        let error_codec = (role == CommandRole::InternalControl)
            .then(|| {
                self.native
                    .facts
                    .lock()
                    .ok()
                    .and_then(|facts| facts.metadata_codec())
            })
            .flatten();
        loop {
            // Only the native session reference crosses detach. The !Send wire
            // MutexGuard remains on this thread, and no Python input is borrowed.
            let session = match lease.session() {
                Ok(session) => session,
                Err(error) => {
                    return Err(self.query_failure(
                        py,
                        lease,
                        &mut job,
                        errors::access(py, error),
                        false,
                        false,
                    ));
                }
            };
            let step = py.detach(|| job.advance(&mut session.channel, budget));
            match step {
                Ok(Step::Ready(response)) => {
                    if let Err(error) = self.publish(py, lease, response.completion()) {
                        return Err(DriveFailure::new(
                            py,
                            error,
                            FailureOutcome::Confirmed,
                            false,
                            Some(CommandConfirmation::from_response(&response)),
                        ));
                    }
                    return Ok(response);
                }
                Ok(Step::Control) => {
                    if let Some(completion) = job.take_completion()
                        && let Err(error) = self.publish(py, lease, &completion)
                    {
                        return Err(self.query_failure(py, lease, &mut job, error, false, false));
                    }
                    if let Err(error) = input::check(py, budget) {
                        return Err(self.query_failure(py, lease, &mut job, error, false, true));
                    }
                }
                Ok(Step::Wait(_)) => {
                    let error = errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "native query returned an undriven I/O wait",
                    );
                    return Err(self.query_failure(py, lease, &mut job, error, false, false));
                }
                Err(error) => {
                    match &error {
                        QueryError::Result(ResultError::Server(_)) => {
                            if let Ok(mut facts) = self.native.facts.lock() {
                                facts.invalidate_status();
                                if role == CommandRole::UserSql {
                                    // ERR carries no tracked variable changes.
                                    // Arbitrary SQL/triggers may already have
                                    // applied settings before this error.
                                    facts.invalidate_settings();
                                    facts.database = None;
                                }
                            }
                            let mapped = errors::query_with_codec(py, error, error_codec);
                            return Err(DriveFailure::new(
                                py,
                                mapped,
                                FailureOutcome::ServerErr,
                                false,
                                None,
                            ));
                        }
                        QueryError::MultipleResults { completion } => {
                            if let Err(error) = self.publish(py, lease, completion) {
                                return Err(
                                    self.query_failure(py, lease, &mut job, error, false, false)
                                );
                            }
                            return Err(DriveFailure::new(
                                py,
                                errors::query_with_codec(py, error, error_codec),
                                FailureOutcome::Consumed,
                                false,
                                None,
                            ));
                        }
                        _ => (),
                    }
                    let unavailable = unavailable(&error);
                    let keep_unsent = matches!(
                        error,
                        QueryError::Channel(
                            sqlaero_core::transport::channel::ChannelError::Timeout(_)
                                | sqlaero_core::transport::channel::ChannelError::Transport(
                                    sqlaero_core::transport::socket::TransportError::Timeout(_)
                                )
                        )
                    );
                    return Err(self.query_failure(
                        py,
                        lease,
                        &mut job,
                        errors::query_with_codec(py, error, error_codec),
                        unavailable,
                        keep_unsent,
                    ));
                }
            }
        }
    }
    pub(crate) fn control(&self, py: Python<'_>, sql: &[u8]) -> PyResult<()> {
        let mut budget = self.runtime(py)?.timeouts.operation();
        let mut lease = self
            .native
            .try_command()
            .map_err(|error| errors::access(py, error))?;
        self.control_with(py, &mut lease, sql, &mut budget)
    }
    pub(crate) fn control_with(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        sql: &[u8],
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        self.control_recorded(py, lease, sql, budget, &mut ControlAttempt::default())
    }
    pub(crate) fn control_recorded(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        sql: &[u8],
        budget: &mut IoBudget,
        attempt: &mut ControlAttempt,
    ) -> PyResult<()> {
        input::check(py, budget)?;
        let mut request = vec![3];
        writer::append(&mut request, sql, self.max_send)
            .map_err(|error| errors::wire(py, error))?;
        let job = self.query_job(py, lease, request, true)?;
        input::check(py, budget)?;
        if let Err(failure) = self.drive(py, lease, job, budget, CommandRole::InternalControl) {
            attempt.started = !matches!(failure.outcome, FailureOutcome::NotSent);
            attempt.confirmed = failure.confirmed.is_some();
            if failure.outcome.unknown() {
                // These are this library's fixed controls, not a parser or
                // inference over arbitrary user SQL.
                let action = match sql {
                    b"COMMIT AND NO CHAIN NO RELEASE" => Some("COMMIT"),
                    b"ROLLBACK AND NO CHAIN NO RELEASE" => Some("ROLLBACK"),
                    b"START TRANSACTION" => Some("BEGIN"),
                    _ => None,
                };
                if let Some(action) = action {
                    errors::note(
                        py,
                        &failure.error,
                        format!(
                            "{action} outcome is unknown; no complete confirmation was received. The action was not replayed and must not be assumed unexecuted."
                        ),
                    );
                }
            }
            return Err(failure.error);
        }
        attempt.started = true;
        attempt.confirmed = true;
        input::check(py, budget)?;
        Ok(())
    }
    fn server_timeout(&self, py: Python<'_>, index: usize, name: &str) -> PyResult<u64> {
        let timeout = self
            .native
            .facts
            .lock()
            .map_err(|error| {
                drop(error.into_inner());
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "confirmed session facts are unavailable",
                )
            })?
            .server_timeouts[index];
        timeout.ok_or_else(|| errors::client(py, ClientErrorCode::SessionState, format!("MySQL SESSION {name} is no longer reliably confirmed; its previous value is not returned as current")))
    }
    fn set_server_timeout(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        index: usize,
        name: &str,
        stage: &'static str,
    ) -> PyResult<()> {
        (|| {
            let mut budget = self.runtime(py)?.timeouts.operation();
            let value = config::positive(value, name)?;
            let mut lease = self
                .native
                .try_command()
                .map_err(|error| errors::access(py, error))?;
            if self
                .server_timeout(py, index, name)
                .ok()
                .map(BigUint::from)
                .as_ref()
                == Some(&value)
            {
                return Ok(());
            }
            let mut sql = Vec::new();
            writer::append(&mut sql, b"SET SESSION ", self.max_send)
                .map_err(|error| errors::wire(py, error))?;
            writer::append(&mut sql, name.as_bytes(), self.max_send)
                .map_err(|error| errors::wire(py, error))?;
            writer::append(&mut sql, b"=", self.max_send)
                .map_err(|error| errors::wire(py, error))?;
            writer::append(&mut sql, value.to_str_radix(10).as_bytes(), self.max_send)
                .map_err(|error| errors::wire(py, error))?;
            self.control_with(py, &mut lease, &sql, &mut budget)
        })()
        .map_err(|error| errors::at(py, error, stage))
    }
    pub(crate) fn selector(py: Python<'_>, selector: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        if selector.is_some_and(|value| {
            !value.is_none()
                && !value.is(py.get_type::<Cursor>())
                && !value.is(py.get_type::<PyTuple>())
        }) {
            return Err(PyTypeError::new_err(
                "cursor must be None, SQLAero Cursor, or tuple; arbitrary cursor classes and formats are not supported",
            ));
        }
        Ok(())
    }
}
macro_rules! connection_api {
    ($(($name:ident,$set:ident,$doc:literal)),* $(,)?) => {
        #[pymethods]
        impl Connection {
            $(#[getter] #[doc=$doc]
            fn $name(&self, py: Python<'_>) -> PyResult<bool> { Ok(self.runtime(py)?.options.$name) }
            #[setter] fn $set(&self, py: Python<'_>, value:&Bound<'_,PyAny>) -> PyResult<()> {
                let value=config::boolean(value,stringify!($name))?;
                self.modify(py,stringify!($name))?.options.$name=value; Ok(())
            })*
            /// Create an independent buffered Cursor. Its decode defaults are
            /// copied now; later Connection property assignments do not change it.
            #[pyo3(signature=(cursor=None))]
            fn cursor(slf:PyRef<'_,Self>, py:Python<'_>, cursor:Option<&Bound<'_,PyAny>>) -> PyResult<Cursor> {
                slf.open(py)?; Self::selector(py,cursor)?;
                let options=slf.runtime(py)?.options;
                let types=slf.types.clone_ref(py);
                Ok(Cursor::new(slf.into(), types, options))
            }
            /// Enter a managed transaction using a new Cursor of the selected
            /// built-in format. Existing server transactions or managed scopes
            /// are rejected locally. Manual transaction boundaries inside the
            /// body remain allowed; only the current transaction is ended on exit.
            #[pyo3(signature=(cursor=None))]
            fn transaction(slf:PyRef<'_,Self>,py:Python<'_>,cursor:Option<&Bound<'_,PyAny>>)->PyResult<crate::transaction::Transaction> {
                slf.open(py)?;Self::selector(py,cursor)?;
                let scope=Arc::clone(&slf.transaction_scope);
                Ok(crate::transaction::Transaction::new(slf.into(),scope))
            }
            /// Permanently retire this connection. COM_QUIT is sent without a response.
            /// Only write_timeout and operation_timeout apply; read_timeout is not used.
            /// If both applicable limits are None, there is no library-imposed deadline.
            /// Sending failure still performs local transport cleanup and keeps the connection retired.
            /// Timeout or interruption grants no fresh network budget or cleanup grace.
            /// Repeated completed close returns None; overlapping operations raise InterfaceBusyError.
            fn close(&self, py:Python<'_>) -> PyResult<()> {
                let mut lease=match self.native.try_command() {
                    Ok(lease)=>lease,
                    Err(sqlaero_core::session::shared::AccessError::Closed)=>return Ok(()),
                    Err(error)=>return Err(errors::at(py,errors::access(py,error),"connection.close")),
                };
                let snapshot=self.runtime(py)?.timeouts;
                let mut budget=IoBudget::new(TimeoutKind::Operation,snapshot.operation,None,snapshot.write);
                lease.begin_close();
                let result=(|| {
                    let session=lease.session().map_err(|error|errors::access(py,error))?;
                    session.channel.new_command();
                    let mut sender=MessageWriter::new(1,session.max_send).map_err(|error|errors::wire(py,error))?;
                    loop {
                        let step=py.detach(|| {
                            match session.channel.send_step(&mut sender,&[1],&mut budget)? {
                                Step::Wait(direction)=> match session.channel.wait(direction,&mut budget)? {
                                    WaitStatus::Control=>Ok(Step::Control),WaitStatus::Ready=>Ok(Step::Control),
                                },
                                other=>Ok(other),
                            }
                        }).map_err(|error|errors::channel(py,error))?;
                        match step {
                            Step::Ready(())=>return Ok(()),
                            _=>input::check(py,&budget)?,
                        }
                    }
                })();
                lease.invalidate(); result.map_err(|error|errors::at(py,error,"connection.close"))
            }
            /// True after irreversible local retirement begins. This does not
            /// confirm server transaction completion or finished cleanup.
            #[getter] fn closed(&self)->bool { self.native.is_closed() }
            /// Actual handshake server version; remains readable after close.
            #[getter] fn server_version(&self)->&str { &self.native.metadata.version }
            /// Actual server connection ID; remains readable after close.
            #[getter] fn connection_id(&self)->u32 { self.native.metadata.connection_id }
            /// Resolved TLS mode for this connection, still readable after close.
            #[getter] fn ssl_mode(&self)->&str { self.ssl_mode }
            /// Actual negotiated TLS version, or None for plain transport.
            #[getter] fn tls_version(&self)->Option<&str> { self.tls_version }
            /// Client-side establishment timeout in whole seconds. None disables
            /// its total limit. Read-only after creation and readable after close.
            /// Default 10; positive int or None, excluding bool. Invalid type/value
            /// raises TypeError/ValueError. Covers configuration, authentication
            /// and required initialization, without an extra network cleanup grace.
            #[getter] fn connect_timeout(&self,py:Python<'_>)->PyResult<Option<u64>> { Ok(self.runtime(py)?.timeouts.connect.map(|v|v.as_secs())) }
            /// Client-side read wait without progress, in whole seconds. None
            /// disables it. Local assignment affects only subsequent operations,
            /// sends no SQL, and is rejected after this connection is retired.
            /// Default None; positive int or None, excluding bool. Invalid type/value
            /// raises TypeError/ValueError. Progress restarts only this interval;
            /// readiness does not. Does not cap a complete response. Saved values
            /// remain readable after close; expiry does not prove SQL cancellation.
            #[getter] fn read_timeout(&self,py:Python<'_>)->PyResult<Option<u64>> { Ok(self.runtime(py)?.timeouts.read.map(|v|v.as_secs())) }
            #[setter] fn set_read_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::timeout(Some(value),"read_timeout")?; self.modify(py,"read_timeout")?.timeouts.read=value; Ok(())
            }
            /// Client-side write wait without progress, in whole seconds. None
            /// disables it; actual progress restarts only this wait interval.
            /// Local assignment affects subsequent operations, sends no SQL,
            /// and fails after close while the saved value remains readable.
            /// Default None; positive int or None, excluding bool. Invalid type/value
            /// raises TypeError/ValueError. Does not cap a complete upload; the
            /// connect/operation deadline may expire earlier. Expiry does not
            /// prove server cancellation or confirmed transaction rollback.
            #[getter] fn write_timeout(&self,py:Python<'_>)->PyResult<Option<u64>> { Ok(self.runtime(py)?.timeouts.write.map(|v|v.as_secs())) }
            #[setter] fn set_write_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::timeout(Some(value),"write_timeout")?; self.modify(py,"write_timeout")?.timeouts.write=value; Ok(())
            }
            /// Client-side total timeout for one operation in whole seconds,
            /// beginning before binding and shared by all batches and internal
            /// confirmations. Progress does not restart it. None disables it.
            /// Local assignment affects subsequent operations and sends no SQL;
            /// after close modification fails but the saved value stays readable.
            /// Default None; positive int or None, excluding bool. Invalid type/value
            /// raises TypeError/ValueError. Includes request preparation, reception
            /// and state/result publication, but not buffered fetching or user
            /// transaction-block code. Expiry grants no new network cleanup period
            /// and does not prove SQL cancellation or confirmed rollback.
            #[getter] fn operation_timeout(&self,py:Python<'_>)->PyResult<Option<u64>> { Ok(self.runtime(py)?.timeouts.operation.map(|v|v.as_secs())) }
            #[setter] fn set_operation_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::timeout(Some(value),"operation_timeout")?; self.modify(py,"operation_timeout")?.timeouts.operation=value; Ok(())
            }
            /// Configured client send limit in bytes; server/protocol limits may
            /// be lower. Read-only and reliably readable after close.
            #[getter] fn max_allowed_packet(&self)->usize { self.max_send }
            /// Configured client logical receive limit in bytes, readable after close.
            #[getter] fn max_received_packet(&self)->usize { self.max_receive }
            /// Native per-result capacity limit in bytes, readable after close.
            #[getter] fn result_memory_limit(&self)->usize { self.result_limit }
            /// Aggregate native result capacity limit in bytes, readable after close.
            #[getter] fn total_result_memory_limit(&self)->usize { self.total_result_limit }
            /// Confirmed MySQL autocommit mode; no network read is performed.
            /// Unknown facts raise InterfaceError, including after close.
            #[getter] fn autocommit(&self,py:Python<'_>)->PyResult<bool> {
                let status=self.native.facts.lock().map_err(|error|{drop(error.into_inner());errors::client(py,ClientErrorCode::InternalState,"confirmed session facts are unavailable")})?.status;
                status.map(|status|status.contains(sqlaero_core::protocol::StatusFlags::SERVER_STATUS_AUTOCOMMIT))
                    .ok_or_else(||errors::client(py,ClientErrorCode::SessionState,"autocommit has not been reliably confirmed"))
            }
            /// Change server SESSION autocommit after acquiring command ownership.
            /// A reliably confirmed same value sends no request and is not a ping.
            fn set_autocommit(&self,py:Python<'_>,enabled:&Bound<'_,PyAny>)->PyResult<()> {
                (|| {
                    let mut budget=self.runtime(py)?.timeouts.operation();
                    let enabled=config::boolean(enabled,"enabled")?;
                    let mut lease=self.native.try_command().map_err(|error|errors::access(py,error))?;
                    if self.autocommit(py).ok()==Some(enabled) {return Ok(())}
                    self.control_with(py,&mut lease,if enabled {b"SET SESSION autocommit=1"}else{b"SET SESSION autocommit=0"},&mut budget)
                })().map_err(|error|errors::at(py,error,"connection.set_autocommit"))
            }
            /// MySQL server-side session seconds for waiting for more uploaded
            /// client data. The initial actual value is read even when omitted
            /// from connect. Cached reads send no SQL and remain valid after
            /// close; unknown facts raise InterfaceError rather than return stale data.
            /// connect(None) preserves then confirms the actual server default;
            /// it does not disable the server timeout. Reading sends no SQL.
            #[getter] fn net_read_timeout(&self,py:Python<'_>)->PyResult<u64>{self.server_timeout(py,0,"net_read_timeout")}
            /// Set MySQL SESSION net_read_timeout to a positive whole-second
            /// int. None/bool/float are rejected. Reliable same-value updates
            /// acquire command ownership but send no SQL; changes use the shared
            /// operation deadline and cache the server's actual confirmed value.
            /// Invalid type/value raises TypeError/ValueError; 0/negative fail.
            /// Server-specific upper bounds/normalization are decided by MySQL.
            /// Reliable same-value updates send no SQL after lifecycle/ownership
            /// checks. Closed/busy calls raise InterfaceError/InterfaceBusyError.
            fn set_net_read_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{self.set_server_timeout(py,value,0,"net_read_timeout","connection.set_net_read_timeout")}
            /// MySQL server-side session seconds for writing to this client;
            /// distinct from client-side write_timeout. Reliable cached reads
            /// remain available after close and never send SQL. Unknown raises InterfaceError.
            /// connect(None) preserves then confirms the actual server default;
            /// it does not disable the server timeout. Reading sends no SQL.
            #[getter] fn net_write_timeout(&self,py:Python<'_>)->PyResult<u64>{self.server_timeout(py,1,"net_write_timeout")}
            /// Set MySQL SESSION net_write_timeout in positive whole seconds.
            /// Only int is accepted, excluding bool/None. A true change sends
            /// one SET and stores the server-confirmed value; closed/busy calls fail.
            /// Invalid type/value raises TypeError/ValueError; 0/negative fail.
            /// Server-specific upper bounds/normalization are decided by MySQL.
            /// Reliable same-value updates send no SQL after lifecycle/ownership
            /// checks. Closed/busy calls raise InterfaceError/InterfaceBusyError.
            fn set_net_write_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{self.set_server_timeout(py,value,1,"net_write_timeout","connection.set_net_write_timeout")}
            /// MySQL server-side session idle timeout in whole seconds. This
            /// cached fact is not a client operation timer or a live health check.
            /// Reliable values remain readable after close; unknown raises InterfaceError.
            /// connect(None) preserves then confirms the actual server default;
            /// it does not disable the server timeout. Reading sends no SQL.
            #[getter] fn wait_timeout(&self,py:Python<'_>)->PyResult<u64>{self.server_timeout(py,2,"wait_timeout")}
            /// Set MySQL SESSION wait_timeout using a positive int in whole
            /// seconds, excluding bool/None. A true change sends one SET, with
            /// actual server normalization retained. Closed/busy operations fail.
            /// Invalid type/value raises TypeError/ValueError; 0/negative fail.
            /// Server-specific upper bounds/normalization are decided by MySQL.
            /// Reliable same-value updates send no SQL after lifecycle/ownership
            /// checks. Closed/busy calls raise InterfaceError/InterfaceBusyError.
            fn set_wait_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{self.set_server_timeout(py,value,2,"wait_timeout","connection.set_wait_timeout")}
            /// MySQL server-side InnoDB row-lock wait timeout in whole seconds;
            /// it does not control metadata locks or guarantee transaction rollback.
            /// Reliable cached values remain readable after close; unknown raises InterfaceError.
            /// connect(None) preserves then confirms the actual server default;
            /// it does not disable the server timeout. Reading sends no SQL.
            #[getter] fn innodb_lock_wait_timeout(&self,py:Python<'_>)->PyResult<u64>{self.server_timeout(py,3,"innodb_lock_wait_timeout")}
            /// Set MySQL SESSION innodb_lock_wait_timeout using a positive
            /// whole-second int, excluding bool/None. Changes send one SET and
            /// retain actual confirmed server values; closed/busy operations fail.
            /// Invalid type/value raises TypeError/ValueError; 0/negative fail.
            /// Server-specific upper bounds/normalization are decided by MySQL.
            /// Reliable same-value updates send no SQL after lifecycle/ownership
            /// checks. Closed/busy calls raise InterfaceError/InterfaceBusyError.
            fn set_innodb_lock_wait_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{self.set_server_timeout(py,value,3,"innodb_lock_wait_timeout","connection.set_innodb_lock_wait_timeout")}
            /// Send START TRANSACTION. Repeating this follows MySQL semantics,
            /// which may commit an existing transaction before opening another.
            fn begin(&self,py:Python<'_>)->PyResult<()> { self.control(py,b"START TRANSACTION").map_err(|error|errors::at(py,error,"connection.begin")) }
            /// Confirm COMMIT AND NO CHAIN NO RELEASE; never automatically retry.
            fn commit(&self,py:Python<'_>)->PyResult<()> {self.control(py,b"COMMIT AND NO CHAIN NO RELEASE").map_err(|error|errors::at(py,error,"connection.commit"))}
            /// Confirm ROLLBACK AND NO CHAIN NO RELEASE; never infer prior commits.
            fn rollback(&self,py:Python<'_>)->PyResult<()> {self.control(py,b"ROLLBACK AND NO CHAIN NO RELEASE").map_err(|error|errors::at(py,error,"connection.rollback"))}
            /// Actively send COM_PING without reconnecting or replaying SQL.
            fn ping(&self,py:Python<'_>)->PyResult<bool> {
                let mut budget=self.runtime(py)?.timeouts.operation();
                let mut lease=match self.native.try_command() {
                    Ok(value)=>value,Err(sqlaero_core::session::shared::AccessError::Closed)=>return Ok(false),
                    Err(error)=>return Err(errors::access(py,error)),
                };
                let job=self.query_job(py,&mut lease,vec![14],true)?;
                match self.drive(py,&mut lease,job,&mut budget,CommandRole::InternalControl) {
                    Ok(_)=>{ py.check_signals()?; if budget.check().is_err(){Ok(false)}else{Ok(true)} },
                    Err(failure) if failure.unavailable=>Ok(false),
                    Err(failure)=>Err(failure.error),
                }
            }
            fn __enter__<'py>(slf:PyRef<'py,Self>,py:Python<'py>)->PyResult<PyRef<'py,Self>> {slf.open(py)?;Ok(slf)}
            fn __exit__(&self,py:Python<'_>,_typ:&Bound<'_,PyAny>,value:&Bound<'_,PyAny>,_traceback:&Bound<'_,PyAny>)->PyResult<bool>{
                match self.close(py) {
                    Ok(())=>Ok(false),
                    Err(error) if !value.is_none()=>{ errors::additional(py,&PyErr::from_value(value.clone()),error,"connection.close");Ok(false) },
                    Err(error)=>Err(error),
                }
            }
        }
    }
}
connection_api! {
    (decode_json,set_decode_json,"Default False. False returns the original JSON text as str. True parses the complete JSON into Python values; malformed, incomplete or out-of-range content raises DataError without text fallback. This applies to JSON fields, not JSON-looking TEXT. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_set,set_decode_set,"Default False. False returns SET text as str. True returns an ordered tuple[str, ...]; the empty SET is (), and an empty member is preserved. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_bit,set_decode_bit,"Default False. False returns the exact BIT bytes, including leading zero bytes. True returns an unsigned big-endian int, never a bool. Invalid BIT contents raise DataError. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_decimal,set_use_decimal,"Default True. True returns exact decimal.Decimal values for DECIMAL. False uses the native correctly rounded binary64 parser and returns float, potentially losing decimal precision; invalid or non-finite results raise DataError. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_timedelta,set_use_timedelta,"Default True. True returns datetime.timedelta for MySQL TIME, including negative and multi-day durations. False returns datetime.time and raises DataError for negative values or durations at least 24 hours. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_text_strict,set_decode_text_strict,"Default True. True raises UnicodeDecodeError for invalid text bytes. False preserves undecodable bytes through surrogateescape, without replacement characters. Binary fields remain bytes and input encoding is unaffected. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_date_strict,set_decode_date_strict,"Default True. True raises DataError for zero or otherwise unrepresentable DATE/DATETIME values. False returns their original calendar text as str. Valid values retain date/datetime output in both modes. SQL NULL remains None.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
}

fn configured_bool(value: &Argument<'_>, name: &str, default: bool) -> PyResult<bool> {
    value
        .0
        .as_ref()
        .map_or(Ok(default), |value| config::boolean(value, name))
}
fn configured_size(
    value: &Argument<'_>,
    name: &str,
    default: usize,
    maximum: usize,
) -> PyResult<usize> {
    value.0.as_ref().map_or(Ok(default), |value| {
        config::bytes_limit(value, name, maximum)
    })
}

/// Connect synchronously after strict boundary validation. All configuration is
/// keyword-only; default TCP is 127.0.0.1:3306 with VERIFY_IDENTITY, while an
/// explicit Unix socket defaults to plain local transport. Initialization
/// confirms UTC, encoding, autocommit and server timeout values before return.
///
/// Timeouts are positive whole-second integers, excluding bool; type/value
/// errors raise TypeError/ValueError. Client connect_timeout defaults to 10 and
/// covers this entire call; None disables that limit. Client read_timeout and
/// write_timeout default to None and bound network waits without progress.
/// Client operation_timeout defaults to None and bounds each later complete
/// command, not buffered fetching or user transaction-block code. Expiry does
/// not establish server cancellation and grants no new network-cleanup grace.
///
/// MySQL server-side SESSION net_read_timeout (upload), net_write_timeout
/// (download), wait_timeout (idle) and innodb_lock_wait_timeout (InnoDB row lock)
/// all default to None: preserve the server setting, then confirm its actual
/// value. Supplied values are applied during initialization; MySQL decides any
/// version-specific upper limit or normalization. Later remote changes use
/// explicit set_* methods; three local runtime timeouts use property assignment.
#[pyfunction]
#[pyo3(pass_module,signature=(*,user,host=Argument::default(),port=Argument::default(),unix_socket=None,password=None,database=None,charset=None,collation=None,ssl=None,server_public_key_path=None,autocommit=Argument::default(),sql_mode=None,connect_timeout=Argument::default(),read_timeout=None,write_timeout=None,operation_timeout=None,net_read_timeout=None,net_write_timeout=None,wait_timeout=None,innodb_lock_wait_timeout=None,max_allowed_packet=Argument::default(),max_received_packet=Argument::default(),result_memory_limit=Argument::default(),total_result_memory_limit=Argument::default(),decode_json=Argument::default(),decode_set=Argument::default(),decode_bit=Argument::default(),use_decimal=Argument::default(),use_timedelta=Argument::default(),decode_text_strict=Argument::default(),decode_date_strict=Argument::default()))]
#[pyo3(
    text_signature = "(*, user, host='127.0.0.1', port=3306, unix_socket=None, password=None, database=None, charset=None, collation=None, ssl=None, server_public_key_path=None, autocommit=True, sql_mode=None, connect_timeout=10, read_timeout=None, write_timeout=None, operation_timeout=None, net_read_timeout=None, net_write_timeout=None, wait_timeout=None, innodb_lock_wait_timeout=None, max_allowed_packet='64MB', max_received_packet='64MB', result_memory_limit='256MB', total_result_memory_limit='512MB', decode_json=False, decode_set=False, decode_bit=False, use_decimal=True, use_timedelta=True, decode_text_strict=True, decode_date_strict=True)"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "the final approved connect API has explicit keyword-only configuration"
)]
pub(crate) fn connect(
    module: &Bound<'_, PyModule>,
    user: &Bound<'_, PyAny>,
    host: Argument<'_>,
    port: Argument<'_>,
    unix_socket: Option<&Bound<'_, PyAny>>,
    password: Option<&Bound<'_, PyAny>>,
    database: Option<&Bound<'_, PyAny>>,
    charset: Option<&Bound<'_, PyAny>>,
    collation: Option<&Bound<'_, PyAny>>,
    ssl: Option<&Bound<'_, PyAny>>,
    server_public_key_path: Option<&Bound<'_, PyAny>>,
    autocommit: Argument<'_>,
    sql_mode: Option<&Bound<'_, PyAny>>,
    connect_timeout: Argument<'_>,
    read_timeout: Option<&Bound<'_, PyAny>>,
    write_timeout: Option<&Bound<'_, PyAny>>,
    operation_timeout: Option<&Bound<'_, PyAny>>,
    net_read_timeout: Option<&Bound<'_, PyAny>>,
    net_write_timeout: Option<&Bound<'_, PyAny>>,
    wait_timeout: Option<&Bound<'_, PyAny>>,
    innodb_lock_wait_timeout: Option<&Bound<'_, PyAny>>,
    max_allowed_packet: Argument<'_>,
    max_received_packet: Argument<'_>,
    result_memory_limit: Argument<'_>,
    total_result_memory_limit: Argument<'_>,
    decode_json: Argument<'_>,
    decode_set: Argument<'_>,
    decode_bit: Argument<'_>,
    use_decimal: Argument<'_>,
    use_timedelta: Argument<'_>,
    decode_text_strict: Argument<'_>,
    decode_date_strict: Argument<'_>,
) -> PyResult<Connection> {
    let py = module.py();
    let started = Instant::now();
    let timeout_config = Timeouts {
        connect: match &connect_timeout.0 {
            None => Some(Duration::from_secs(10)),
            Some(value) => config::timeout(Some(value), "connect_timeout")?,
        },
        read: config::timeout(read_timeout, "read_timeout")?,
        write: config::timeout(write_timeout, "write_timeout")?,
        operation: config::timeout(operation_timeout, "operation_timeout")?,
    };
    let budget = IoBudget::starting_at(
        started,
        TimeoutKind::Connect,
        timeout_config.connect,
        timeout_config.read,
        timeout_config.write,
    );
    let user = config::text(user, "user", false)?;
    let host_provided = host.0.is_some();
    let port_provided = port.0.is_some();
    let host = host
        .0
        .as_ref()
        .map_or(Ok("127.0.0.1".to_owned()), |value| {
            config::text(value, "host", false)
        })?;
    if host.contains(['\0', '/', '\\', '[', ']'])
        || host.bytes().any(|byte| byte.is_ascii_whitespace())
        || (host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err())
    {
        return Err(PyValueError::new_err(
            "host must be a DNS name or unbracketed IP, without URL, port, path, NUL or whitespace",
        ));
    }
    let port = match &port.0 {
        None => 3306,
        Some(value) => u16::try_from(config::positive(value, "port")?)
            .ok()
            .filter(|value| *value != 0)
            .ok_or_else(|| PyValueError::new_err("port must be an integer in 1..=65535"))?,
    };
    let socket = config::optional_path(unix_socket, "unix_socket")?;
    if socket.is_some() && (host_provided || port_provided) {
        return Err(PyValueError::new_err(
            "unix_socket conflicts with explicitly provided host or port; omit both TCP fields",
        ));
    }
    let unix = socket.is_some();
    let password = Zeroizing::new(
        config::optional_text(password, "password", false)?
            .unwrap_or_default()
            .into_bytes(),
    );
    let database = config::optional_text(database, "database", false)?;
    let encoding = charset::resolve(
        config::optional_text(charset, "charset", false)?.as_deref(),
        config::optional_text(collation, "collation", false)?.as_deref(),
    )
    .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let sql_mode = config::optional_text(sql_mode, "sql_mode", true)?;
    let autocommit = configured_bool(&autocommit, "autocommit", true)?;
    let max_send = configured_size(
        &max_allowed_packet,
        "max_allowed_packet",
        64 * 1024 * 1024,
        MAX_MESSAGE_PAYLOAD,
    )?;
    let max_receive = configured_size(
        &max_received_packet,
        "max_received_packet",
        64 * 1024 * 1024,
        MAX_MESSAGE_PAYLOAD,
    )?;
    let result_limit = configured_size(
        &result_memory_limit,
        "result_memory_limit",
        256 * 1024 * 1024,
        isize::MAX as usize,
    )?;
    let total_limit = configured_size(
        &total_result_memory_limit,
        "total_result_memory_limit",
        512 * 1024 * 1024,
        isize::MAX as usize,
    )?;
    let server_public_key =
        config::optional_path(server_public_key_path, "server_public_key_path")?;
    let server_timeouts = [
        net_read_timeout,
        net_write_timeout,
        wait_timeout,
        innodb_lock_wait_timeout,
    ]
    .into_iter()
    .zip([
        "net_read_timeout",
        "net_write_timeout",
        "wait_timeout",
        "innodb_lock_wait_timeout",
    ])
    .map(|(value, name)| {
        value
            .filter(|value| !value.is_none())
            .map(|value| config::positive(value, name))
            .transpose()
    })
    .collect::<PyResult<Vec<_>>>()?;
    let server_timeouts = server_timeouts
        .try_into()
        .map_err(|_| PyValueError::new_err("invalid internal server timeout count"))?;
    let options = Options {
        decode_json: configured_bool(&decode_json, "decode_json", false)?,
        decode_set: configured_bool(&decode_set, "decode_set", false)?,
        decode_bit: configured_bool(&decode_bit, "decode_bit", false)?,
        use_decimal: configured_bool(&use_decimal, "use_decimal", true)?,
        use_timedelta: configured_bool(&use_timedelta, "use_timedelta", true)?,
        decode_text_strict: configured_bool(&decode_text_strict, "decode_text_strict", true)?,
        decode_date_strict: configured_bool(&decode_date_strict, "decode_date_strict", true)?,
    };
    let default_ssl = Ssl::default_config();
    let ssl = ssl
        .filter(|value| !value.is_none())
        .map(|value| value.extract::<PyRef<'_, Ssl>>())
        .transpose()?;
    let selected = ssl.as_deref().unwrap_or(&default_ssl);
    let ssl_mode = selected.mode_name(unix);
    let tls = selected.prepare(unix, &host)?;
    input::check(py, &budget).map_err(|error| errors::at(py, error, "connect"))?;
    let params = ConnectParams {
        endpoint: match socket {
            Some(socket) => Endpoint::Unix(socket),
            None => Endpoint::Tcp { host, port },
        },
        user,
        password,
        database,
        tls,
        server_public_key,
        connect_timeout: timeout_config.connect,
        read_timeout: timeout_config.read,
        write_timeout: timeout_config.write,
        max_send,
        max_receive,
    };
    let establish = py
        .detach(|| Establish::with_budget(params, budget))
        .map_err(|error| errors::at(py, errors::establish(py, error), "connect"))?;
    let mut initialize = Initialize::new(
        establish,
        InitOptions {
            encoding,
            autocommit,
            sql_mode,
            server_timeouts,
            result_memory_limit: result_limit,
        },
    );
    let ready = loop {
        match py
            .detach(|| initialize.advance())
            .map_err(|error| errors::at(py, errors::initialize(py, error), "connect"))?
        {
            Step::Ready(value) => break value,
            Step::Control => py.check_signals()?,
            Step::Wait(_) => {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "initialization returned an undriven wait",
                ));
            }
        }
    };
    let tls_version = ready
        .transport
        .channel
        .tls_version()
        .and_then(|version| match version {
            rustls::ProtocolVersion::TLSv1_2 => Some("TLSv1.2"),
            rustls::ProtocolVersion::TLSv1_3 => Some("TLSv1.3"),
            _ => None,
        });
    let types = module
        .getattr("_scalar_types")?
        .cast_into::<ScalarTypes>()?
        .unbind();
    Ok(Connection {
        native: Arc::new(NativeConnection::with_result_limit(ready, total_limit)),
        runtime: Mutex::new(Runtime {
            timeouts: timeout_config,
            options,
        }),
        types,
        result_limit,
        max_send,
        max_receive,
        total_result_limit: total_limit,
        ssl_mode,
        tls_version,
        transaction_scope: Arc::new(AtomicBool::new(false)),
    })
}
