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
    protocol::framing::MAX_MESSAGE_PAYLOAD,
    result::{
        decode::Options,
        text::{Response, ResultError},
    },
    session::{
        connect::{ConnectParams, Endpoint, Establish},
        initialize::{Initialize, Options as InitOptions},
        query::{CommandConfirmation, Managed, Query, QueryError, RequestProgress},
        shared::{Close as NativeClose, CloseError, CommandLease, Connection as NativeConnection},
    },
    transport::{
        channel::Step,
        deadline::{IoBudget, TimeoutKind},
    },
};
use std::{
    sync::{Arc, Mutex, MutexGuard, OnceLock, atomic::AtomicBool},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

fn close_error(py: Python<'_>, error: CloseError) -> PyErr {
    match error {
        CloseError::Access(error) => errors::access(py, error),
        CloseError::Wire(error) => errors::wire(py, error),
        CloseError::Channel(error) => errors::channel(py, error),
    }
}

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
/// The two real receiver deliveries share exactly one host-error, state and
/// acknowledgement path. A header or row batch carries no completion claim.
trait Received {
    fn completion(&self) -> Option<&sqlaero_core::protocol::OkPacket<'static>>;
    /// A parsed public no-row set is separate from whole-command completion.
    /// The latter still comes only from Query's actual framing/ACK facts.
    fn command_set_confirmation(&self) -> Option<CommandConfirmation> {
        None
    }
}
impl Received for Response {
    fn completion(&self) -> Option<&sqlaero_core::protocol::OkPacket<'static>> {
        Some(Response::completion(self))
    }
    fn command_set_confirmation(&self) -> Option<CommandConfirmation> {
        matches!(self, Self::Command(_)).then(|| CommandConfirmation::from_response(self))
    }
}
impl Received for sqlaero_core::result::text::StreamEvent {
    fn completion(&self) -> Option<&sqlaero_core::protocol::OkPacket<'static>> {
        match self {
            Self::End { completion, .. } => Some(completion),
            _ => None,
        }
    }
    fn command_set_confirmation(&self) -> Option<CommandConfirmation> {
        match self {
            Self::End {
                completion,
                total_rows: None,
                ..
            } => Some(CommandConfirmation::from_completion(completion, None)),
            _ => None,
        }
    }
}
impl<T: Received> Received for Managed<T> {
    fn completion(&self) -> Option<&sqlaero_core::protocol::OkPacket<'static>> {
        match self {
            Self::Public(response) => response.completion(),
            Self::InternalOutput { completion, .. } => Some(completion),
        }
    }
    fn command_set_confirmation(&self) -> Option<CommandConfirmation> {
        match self {
            Self::Public(response) => response.command_set_confirmation(),
            Self::InternalOutput { .. } => None,
        }
    }
}

#[derive(Clone, Copy)]
enum FailureOutcome {
    NotSent,
    Sending,
    AwaitingResponse,
    ServerErr,
    Confirmed,
    Paused(ResourcePause),
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
            Self::Paused(_) => {
                "The current request was sent and its incomplete response is retained at a verified native resource boundary. This is not a full-command or commit confirmation. No request was replayed."
            }
        }
    }
}
/// A real native availability failure is distinguished from an original
/// Python exception, even if a signal handler raises the same exception class.
pub(crate) struct DriveFailure {
    error: PyErr,
    unavailable: bool,
    facts: FailureFacts,
    outcome: FailureOutcome,
}

/// Only a typed, complete-message preflight pause can retain the response.
/// These categories preserve the common resource error mapping at the host.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourcePause {
    Result,
    Total,
    Allocation,
}
impl ResourcePause {
    pub(crate) fn from_memory(error: &sqlaero_core::memory::MemoryError) -> Self {
        match error {
            sqlaero_core::memory::MemoryError::Result { .. } => Self::Result,
            sqlaero_core::memory::MemoryError::Total { .. } => Self::Total,
            sqlaero_core::memory::MemoryError::Allocation(_) => Self::Allocation,
        }
    }
}
enum FailureFacts {
    None,
    Completion(CommandConfirmation),
    Native(Arc<QueryError>),
}
impl DriveFailure {
    /// One cold host boundary for native LOCAL facts, including an original
    /// Python control exception rather than only typed upload I/O failures.
    fn with_local(self, py: Python<'_>, job: &Query) -> Self {
        crate::local::failure(py, job, &self.error);
        self
    }
    /// A complete ERR/ACK or a request which never entered sending needs no
    /// unread-response retirement merely because a host call returned error.
    pub(crate) fn synchronized(&self) -> bool {
        matches!(
            self.outcome,
            FailureOutcome::NotSent | FailureOutcome::ServerErr | FailureOutcome::Confirmed
        )
    }
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
            facts: confirmed.map_or(FailureFacts::None, FailureFacts::Completion),
            outcome,
        }
    }
    pub(crate) fn resource_pause(&self) -> Option<ResourcePause> {
        match self.outcome {
            FailureOutcome::Paused(kind) => Some(kind),
            _ => None,
        }
    }
    fn with_command_set_confirmation(mut self, value: Option<CommandConfirmation>) -> Self {
        // Ready publication failures carry either the stronger whole-command
        // confirmation or this parsed no-row set. They never need two copies.
        if matches!(self.facts, FailureFacts::None)
            && let Some(value) = value
        {
            self.facts = FailureFacts::Completion(value);
        }
        self
    }
    pub(crate) fn command_set_confirmation(&self) -> Option<CommandConfirmation> {
        match self.facts {
            FailureFacts::Completion(value) if !value.has_rows() => Some(value),
            _ => None,
        }
    }
    fn with_native(mut self, error: Arc<QueryError>, _codec: Option<OutputCodec>) -> Self {
        // Keep parsed completion facts without a second native error owner.
        // Only outcome=Confirmed certifies a whole request; a public set with
        // MORE keeps its original incomplete/retired outcome independently.
        if matches!(self.facts, FailureFacts::None) {
            self.facts = FailureFacts::Native(error);
        }
        self
    }
    pub(crate) fn confirmation(&self) -> Option<CommandConfirmation> {
        match (self.outcome, &self.facts) {
            (FailureOutcome::Confirmed, FailureFacts::Completion(value)) => Some(*value),
            _ => None,
        }
    }
    pub(crate) fn native_failure(&self) -> Option<sqlaero_core::result::chain::Failure> {
        match &self.facts {
            FailureFacts::Native(error) => Some(sqlaero_core::result::chain::Failure {
                error: Arc::clone(error),
                codec: None,
            }),
            _ => None,
        }
    }
    pub(crate) fn record_prefix(
        self,
        py: Python<'_>,
        prefix: &mut sqlaero_core::session::query::ConfirmedPrefix,
        rows: std::ops::Range<usize>,
        first: Option<CommandConfirmation>,
    ) -> PyErr {
        if let Some(confirmed) = self.confirmation()
            && let Err(error) = prefix.record_confirmation(rows.clone(), first.unwrap_or(confirmed))
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
    tls_snapshot: Option<Arc<sqlaero_core::transport::tls::info::Snapshot>>,
    tls_info_value: OnceLock<Py<crate::tls_info::TlsInfo>>,
    compression_snapshot: Option<sqlaero_core::transport::compression::Options>,
    compression_value: OnceLock<Py<crate::compression::Compression>>,
    pub(crate) local_infile: bool,
    pub(crate) call_slots: OnceLock<String>,
    pub(crate) transaction_scope: Arc<AtomicBool>,
}
impl Connection {
    /// Acquire a safe physical command boundary and release only previously
    /// registered native statement IDs within this same operation's budget.
    pub(crate) fn acquire<'a>(
        &'a self,
        py: Python<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<CommandLease<'a>> {
        let mut lease = self
            .native
            .try_command()
            .map_err(|error| errors::access(py, error))?;
        crate::statement::flush(py, self, &mut lease, budget)?;
        Ok(lease)
    }
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
        self.native
            .remember_context(self.execution_context(py)?)
            .map_err(|error| errors::access(py, error))?;
        input::check(py, budget)
    }
    fn execution_context(
        &self,
        py: Python<'_>,
    ) -> PyResult<sqlaero_core::session::state::ExecutionContext> {
        use sqlaero_core::session::state::ContextError;
        let facts = self.native.facts.lock().map_err(|_| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "confirmed session facts are unavailable",
            )
        })?;
        facts.execution_context(self.max_send).map_err(|error| {
            let code = match error {
                ContextError::SqlMode => ClientErrorCode::SessionState,
                ContextError::ClientCharset | ContextError::MetadataCharset => {
                    ClientErrorCode::UnsupportedCharset
                }
            };
            errors::client(py, code, error.to_string())
        })
    }
    pub(crate) fn context(&self, py: Python<'_>) -> PyResult<(Context, bool, OutputCodec)> {
        let value = self.execution_context(py)?;
        Ok((value.binding, value.ansi_quotes, value.metadata))
    }
    /// Copy historical auxiliary facts without acquiring command ownership or
    /// consulting live state. This reference cannot authorize SQL execution.
    pub(crate) fn auxiliary_context(
        &self,
        py: Python<'_>,
    ) -> PyResult<sqlaero_core::session::state::ExecutionContext> {
        self.native
            .auxiliary_reference()
            .map_err(|error| errors::access(py, error))?
            .ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "No completely confirmed encoding and SQL-mode reference is available for local SQL projection",
                )
            })
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
                // Ordinary responses do not change input/metadata policy.
                // Rebuild the historical reference only for tracked changes;
                // explicit context confirmation also publishes its own result.
                result.map(|()| {
                    completion
                        .status_flags()
                        .contains(sqlaero_core::protocol::StatusFlags::SERVER_SESSION_STATE_CHANGED)
                        .then(|| facts.execution_context(self.max_send).ok())
                        .flatten()
                })
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
        let reference = applied.map_err(|error| {
            lease.invalidate();
            errors::wire(py, error)
        })?;
        if let Some(reference) = reference {
            self.native.remember_context(reference).map_err(|error| {
                lease.invalidate();
                errors::access(py, error)
            })?;
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
        DriveFailure::new(py, error, outcome, unavailable, confirmed).with_local(py, job)
    }
    /// Consume a fixed internal exchange completely, keeping its last result.
    /// Public result-chain execution uses drive_set and owns every earlier set.
    pub(crate) fn drive(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        mut job: Query,
        budget: &mut IoBudget,
        role: CommandRole,
    ) -> Result<Response, DriveFailure> {
        loop {
            let response = self.drive_set(py, lease, &mut job, budget, role)?;
            if job.is_finished() {
                return Ok(response);
            }
            input::check(py, budget)
                .map_err(|error| self.query_failure(py, lease, &mut job, error, false, false))?;
        }
    }
    pub(crate) fn drive_set(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        budget: &mut IoBudget,
        role: CommandRole,
    ) -> Result<Response, DriveFailure> {
        self.drive_next(py, lease, job, budget, role, Query::advance)
    }
    pub(crate) fn drive_stream(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        budget: &mut IoBudget,
        role: CommandRole,
        row_limit: std::num::NonZeroUsize,
    ) -> Result<sqlaero_core::result::text::StreamEvent, DriveFailure> {
        self.drive_next(py, lease, job, budget, role, move |job, channel, budget| {
            job.advance_stream(channel, budget, row_limit)
        })
    }
    pub(crate) fn drive_managed_set(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        budget: &mut IoBudget,
        role: CommandRole,
    ) -> Result<Managed<Response>, DriveFailure> {
        self.drive_next(py, lease, job, budget, role, Query::advance_managed)
    }
    pub(crate) fn drive_managed_stream(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        budget: &mut IoBudget,
        role: CommandRole,
        row_limit: std::num::NonZeroUsize,
    ) -> Result<Managed<sqlaero_core::result::text::StreamEvent>, DriveFailure> {
        self.drive_next(py, lease, job, budget, role, move |job, channel, budget| {
            job.advance_managed_stream(channel, budget, row_limit)
        })
    }
    fn drive_next<T: Received + Send>(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        job: &mut Query,
        budget: &mut IoBudget,
        role: CommandRole,
        mut advance: impl FnMut(
            &mut Query,
            &mut sqlaero_core::transport::channel::Channel,
            &mut IoBudget,
        ) -> Result<Step<T>, QueryError>
        + Send,
    ) -> Result<T, DriveFailure> {
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
                        job,
                        errors::access(py, error),
                        false,
                        false,
                    ));
                }
            };
            let step = py.detach(|| advance(job, &mut session.channel, budget));
            match step {
                Ok(Step::Ready(response)) => {
                    if let Some(completion) = response.completion()
                        && let Err(error) = self.publish(py, lease, completion)
                    {
                        // Only Query's actual whole-command facts certify a
                        // final response. A public set or managed OUT may still
                        // have MORE/unread data, or queued business delivery.
                        return Err(self
                            .query_failure(py, lease, job, error, false, false)
                            .with_command_set_confirmation(response.command_set_confirmation()));
                    }
                    return Ok(response);
                }
                Ok(Step::Control) => {
                    if let Err(error) = input::check(py, budget) {
                        return Err(self.query_failure(py, lease, job, error, false, true));
                    }
                }
                Ok(Step::Wait(_)) => {
                    let error = errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "native query returned an undriven I/O wait",
                    );
                    return Err(self.query_failure(py, lease, job, error, false, false));
                }
                Err(error) => {
                    if let QueryError::Paused(memory) = &error
                        && job.resource_paused()
                    {
                        // This exact core state owns the complete unconsumed
                        // row packet. No transport/parser failure is forgiven,
                        // and no session facts or wire lease are invalidated.
                        let kind = ResourcePause::from_memory(memory);
                        let mapped = errors::query_with_codec(py, &error, error_codec);
                        let failure = DriveFailure::new(
                            py,
                            mapped,
                            FailureOutcome::Paused(kind),
                            false,
                            None,
                        )
                        .with_local(py, job);
                        return Err(failure);
                    }
                    // A post-End managed shape error still owns a real set
                    // completion. Publish it before retiring the unfinished
                    // exchange; it does not confirm the complete CALL.
                    let managed_publication_error =
                        job.take_managed_completion()
                            .and_then(|(completion, _metadata_status)| {
                                self.publish(py, lease, &completion).err()
                            });
                    if let QueryError::Result(ResultError::Server(_)) = &error {
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
                        let error = Arc::new(error);
                        let mapped = errors::query_with_codec(py, error.as_ref(), error_codec);
                        if let Some(secondary) = managed_publication_error {
                            errors::additional(
                                py,
                                &mapped,
                                secondary,
                                "publishing managed output state",
                            );
                        }
                        return Err(DriveFailure::new(
                            py,
                            mapped,
                            FailureOutcome::ServerErr,
                            false,
                            None,
                        )
                        .with_local(py, job)
                        .with_native(error, error_codec));
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
                    let error = Arc::new(error);
                    let mapped = errors::query_with_codec(py, error.as_ref(), error_codec);
                    if let Some(secondary) = managed_publication_error {
                        errors::additional(
                            py,
                            &mapped,
                            secondary,
                            "publishing managed output state",
                        );
                    }
                    return Err(self
                        .query_failure(py, lease, job, mapped, unavailable, keep_unsent)
                        .with_native(error, error_codec));
                }
            }
        }
    }
    pub(crate) fn control(&self, py: Python<'_>, sql: &[u8]) -> PyResult<()> {
        let mut budget = self.runtime(py)?.timeouts.operation();
        let mut lease = self.acquire(py, &mut budget)?;
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
            attempt.confirmed = failure.confirmation().is_some();
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
            let mut lease = self.acquire(py, &mut budget)?;
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
    pub(crate) fn selector(
        py: Python<'_>,
        selector: Option<&Bound<'_, PyAny>>,
        streaming: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<(bool, bool)> {
        let (dictionary, selected_stream) = match selector.filter(|value| !value.is_none()) {
            None => (false, false),
            Some(value)
                if value.is(py.get_type::<Cursor>()) || value.is(py.get_type::<PyTuple>()) =>
            {
                (false, false)
            }
            Some(value)
                if value.is(py.get_type::<crate::cursor::DictCursor>())
                    || value.is(py.get_type::<pyo3::types::PyDict>()) =>
            {
                (true, false)
            }
            Some(value) if value.is(py.get_type::<crate::cursor::SSCursor>()) => (false, true),
            Some(value) if value.is(py.get_type::<crate::cursor::SSDictCursor>()) => (true, true),
            _ => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "cursor must be None, a SQLAero Cursor class, tuple, or dict; arbitrary cursor classes and formats are not supported",
                ));
            }
        };
        let streaming = streaming
            .filter(|value| !value.is_none())
            .map_or(Ok(selected_stream), |value| {
                config::boolean(value, "streaming")
            })?;
        Ok((dictionary, streaming))
    }
    fn savepoint(
        &self,
        py: Python<'_>,
        name: &Bound<'_, PyAny>,
        action: input::control::SavepointAction,
    ) -> PyResult<()> {
        let mut budget = self.runtime(py)?.timeouts.operation();
        let mut lease = self.acquire(py, &mut budget)?;
        self.ensure_context(py, &mut lease, &mut budget)?;
        let context = self.context(py)?.0;
        let request = input::control::savepoint(name, action, &context, &budget)?;
        let job = self.query_job(py, &mut lease, request, true)?;
        self.drive(
            py,
            &mut lease,
            job,
            &mut budget,
            CommandRole::InternalControl,
        )?;
        input::check(py, &budget)
    }
    pub(crate) fn confirm_session_state(
        &self,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<bool> {
        // Arbitrary SQL may invalidate the tracker or schema. Reuse the same
        // complete confirmation before further initialization or prepared
        // context reuse; a known ordinary-command context avoids this query.
        let mut request = Vec::new();
        writer::append(&mut request,b"\x03SELECT @@SESSION.character_set_client,@@SESSION.character_set_connection,@@SESSION.character_set_results,@@SESSION.collation_connection,@@SESSION.sql_mode,@@SESSION.time_zone,@@SESSION.net_read_timeout,@@SESSION.net_write_timeout,@@SESSION.wait_timeout,@@SESSION.innodb_lock_wait_timeout,@@SESSION.session_track_system_variables,@@SESSION.session_track_schema,CAST(CONVERT(DATABASE() USING utf8mb4) AS BINARY)",self.max_send).map_err(|error|errors::wire(py,error))?;
        let job = self.query_job(py, lease, request, false)?;
        let response = self.drive(py, lease, job, budget, CommandRole::UserSql)?;
        let Response::Rows(rows) = response else {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "session state confirmation returned no row result",
            ));
        };
        let result = {
            let mut facts = self.native.facts.lock().map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "session confirmation facts are unavailable",
                )
            })?;
            facts.confirm_initialization(&rows)
        };
        let tracking_ready = result.map_err(|error| errors::wire(py, error))?;
        self.native
            .remember_context(self.execution_context(py)?)
            .map_err(|error| errors::access(py, error))?;
        input::check(py, budget)?;
        Ok(tracking_ready)
    }
    fn initialize_sql(
        &self,
        py: Python<'_>,
        units: &[String],
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        let mut lease = self.acquire(py, budget)?;
        let mut tracking_ready = true;
        for (index, sql) in units.iter().enumerate() {
            let mut executed = false;
            let result = (|| {
                self.ensure_context(py, &mut lease, budget)?;
                let context = self.context(py)?.0;
                let mut request = Vec::new();
                writer::append(&mut request, b"\x03", context.limit)
                    .map_err(|error| errors::wire(py, error))?;
                input::sql_utf8(py, sql, &mut request, &context, budget)?;
                let job = self.query_job(py, &mut lease, request, true)?;
                self.drive(py, &mut lease, job, budget, CommandRole::UserSql)?;
                executed = true;
                tracking_ready = self.confirm_session_state(py, &mut lease, budget)?;
                input::check(py, budget)
            })();
            if let Err(primary) = result {
                errors::note_format(
                    py,
                    &primary,
                    format_args!(
                        "while processing init_sql[{index}]; {index} earlier SQL units completed; current SQL unit execution confirmed={executed}. Later initialization units were not sent. Closing the failed connection does not confirm rollback or undo committed effects."
                    ),
                );
                return Err(primary);
            }
        }
        if !tracking_ready {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "init_sql left required session tracking disabled or incomplete (session_track_system_variables / session_track_schema); the Connection was not returned and the configuration was not changed back",
            ));
        }
        self.ensure_context(py, &mut lease, budget)?;
        if !self.context(py)?.0.utc_confirmed {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "init_sql did not leave a reliably confirmed UTC time_zone '+00:00'; the Connection was not returned and the setting was not changed back",
            ));
        }
        input::check(py, budget)
    }
}
macro_rules! connection_api {
    ($(($name:ident,$set:ident,$doc:literal)),* $(,)?) => {
        #[pymethods]
        impl Connection {
            /// Read the current MySQL warning area explicitly as ordered
            /// (Level: str, Code: int, Message: str) tuples. This sends exactly
            /// SHOW WARNINGS, preserves existing Cursor results and never tries
            /// to reconstruct historical warnings. A busy connection is refused.
            /// Only messages still retained by MySQL can be returned.
            fn show_warnings<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyTuple>> {
                (|| {
                    let snapshot=self.runtime(py)?;
                    let mut budget=snapshot.timeouts.operation();
                    let options=snapshot.options;
                    drop(snapshot);
                    let mut lease = self.acquire(py, &mut budget)?;
                    input::check(py,&budget)?;
                    let mut request=Vec::new();
                    writer::append(&mut request,b"\x03SHOW WARNINGS",self.max_send).map_err(|error|errors::wire(py,error))?;
                    let job=self.query_job(py,&mut lease,request,false)?;
                    let response=self.drive(py,&mut lease,job,&mut budget,CommandRole::InternalControl)?;
                    let Response::Rows(rows)=response else {return Err(errors::wire(py,sqlaero_core::protocol::framing::WireError::Malformed("SHOW WARNINGS returned no row result")));};
                    crate::diagnostics::warnings(py,rows,&self.types.borrow(py),options,&budget)
                })().map_err(|error|errors::at(py,error,"connection.show_warnings"))
            }
            /// Create or replace one backtick-quoted MySQL savepoint. No local
            /// savepoint registry or automatic transaction boundary is added.
            /// Success alone does not guarantee a retained savepoint: MySQL
            /// may end transactions through autocommit or implicit DDL commits.
            fn create_savepoint(&self,py:Python<'_>,name:&Bound<'_,PyAny>)->PyResult<()> {
                self.savepoint(py,name,input::control::SavepointAction::Create).map_err(|error|errors::at(py,error,"connection.create_savepoint"))
            }
            /// Roll back the current transaction to one quoted savepoint.
            /// MySQL decides whether the name exists; its error is preserved.
            fn rollback_to_savepoint(&self,py:Python<'_>,name:&Bound<'_,PyAny>)->PyResult<()> {
                self.savepoint(py,name,input::control::SavepointAction::Rollback).map_err(|error|errors::at(py,error,"connection.rollback_to_savepoint"))
            }
            /// Release one quoted savepoint without committing the transaction.
            fn release_savepoint(&self,py:Python<'_>,name:&Bound<'_,PyAny>)->PyResult<()> {
                self.savepoint(py,name,input::control::SavepointAction::Release).map_err(|error|errors::at(py,error,"connection.release_savepoint"))
            }
            /// Encode supported values locally as SQL str fragments, parameter
            /// tuples or row lists. many and itemize are exact bool. This uses
            /// the last confirmed encoding reference, including while busy or
            /// closed, and never executes SQL or changes any Cursor result.
            /// No SQL deadline or result-memory quota applies. Strings that
            /// cannot round-trip losslessly through that encoding raise ValueError.
            #[pyo3(signature=(data,*,many=Argument::default(),itemize=Argument::default()))]
            #[pyo3(text_signature="($self, data, *, many=False, itemize=True)")]
            pub(crate) fn escape<'py>(&self,py:Python<'py>,data:&Bound<'py,PyAny>,many:Argument<'_>,itemize:Argument<'_>)->PyResult<Bound<'py,PyAny>> {
                (|| {
                    let many=many.0.as_ref().map_or(Ok(false),|value|config::boolean(value,"many"))?;
                    let itemize=itemize.0.as_ref().map_or(Ok(true),|value|config::boolean(value,"itemize"))?;
                    let reference=self.auxiliary_context(py)?;
                    crate::binding::escape(data,many,itemize,&self.types.borrow(py),&reference.binding)
                })().map_err(|error|errors::at(py,error,"connection.escape"))
            }
            $(#[getter] #[doc=$doc]
            fn $name(&self, py: Python<'_>) -> PyResult<bool> { Ok(self.runtime(py)?.options.$name) }
            #[setter] fn $set(&self, py: Python<'_>, value:&Bound<'_,PyAny>) -> PyResult<()> {
                let value=config::boolean(value,stringify!($name))?;
                self.modify(py,stringify!($name))?.options.$name=value; Ok(())
            })*
            /// Create a built-in Cursor with fixed row, streaming and prepared
            /// policy. Decode defaults are copied now; later Connection property
            /// assignments do not change this Cursor. prepared accepts exact bool.
            #[pyo3(signature=(cursor=None,*,streaming=None,prepared=Argument::default()))]
            #[pyo3(text_signature="($self, cursor=None, *, streaming=None, prepared=False)")]
            fn cursor(slf:PyRef<'_,Self>, py:Python<'_>, cursor:Option<&Bound<'_,PyAny>>,streaming:Option<&Bound<'_,PyAny>>,prepared:Argument<'_>) -> PyResult<Py<PyAny>> {
                slf.open(py)?; let (dictionary,streaming)=Self::selector(py,cursor,streaming)?;
                let prepared=prepared.0.as_ref().map_or(Ok(false),|value|config::boolean(value,"prepared"))?;
                let options=slf.runtime(py)?.options;
                let types=slf.types.clone_ref(py);
                crate::cursor::CursorCore::create(py,slf.into(),types,options,dictionary,streaming,prepared).map(|value|value.object(py))
            }
            /// Enter a managed transaction using a new Cursor of the selected
            /// built-in format. Existing server transactions or managed scopes
            /// are rejected locally. Manual transaction boundaries inside the
            /// body remain allowed; only the current transaction is ended on exit.
            #[pyo3(signature=(cursor=None,*,streaming=None,prepared=Argument::default()))]
            #[pyo3(text_signature="($self, cursor=None, *, streaming=None, prepared=False)")]
            fn transaction(slf:PyRef<'_,Self>,py:Python<'_>,cursor:Option<&Bound<'_,PyAny>>,streaming:Option<&Bound<'_,PyAny>>,prepared:Argument<'_>)->PyResult<crate::transaction::Transaction> {
                slf.open(py)?;let (dictionary,streaming)=Self::selector(py,cursor,streaming)?;
                let prepared=prepared.0.as_ref().map_or(Ok(false),|value|config::boolean(value,"prepared"))?;
                let scope=Arc::clone(&slf.transaction_scope);
                Ok(crate::transaction::Transaction::new(slf.into(),scope,dictionary,streaming,prepared))
            }
            /// Permanently retire this connection. An idle unread stream is
            /// retired locally without draining or sending a new command. On
            /// a synchronized idle channel, COM_QUIT is sent without a response.
            /// Only write_timeout and operation_timeout apply; read_timeout is not used.
            /// If both applicable limits are None, there is no library-imposed deadline.
            /// Sending failure still performs local transport cleanup and keeps the connection retired.
            /// Timeout or interruption grants no fresh network budget or cleanup grace.
            /// Repeated completed close returns None; overlapping operations raise InterfaceBusyError.
            fn close(&self, py:Python<'_>) -> PyResult<()> {
                let mut lease=match self.native.try_close_command() {
                    Ok(lease)=>lease,
                    Err(sqlaero_core::session::shared::AccessError::Closed)=>return Ok(()),
                    Err(error)=>return Err(errors::at(py,errors::access(py,error),"connection.close")),
                };
                if lease.has_owned_response() {lease.invalidate();return Ok(());}
                let snapshot=self.runtime(py)?.timeouts;
                let mut budget=IoBudget::new(TimeoutKind::Operation,snapshot.operation,None,snapshot.write);
                let result=(|| {
                    let mut job=NativeClose::new(lease).map_err(|error|close_error(py,error))?;
                    loop {
                        let step={
                            let mut io=job.io().map_err(|error|close_error(py,error))?;
                            py.detach(||io.advance(&mut budget)).map_err(|error|close_error(py,error))?
                        };
                        match step {
                            Step::Ready(())=>return Ok(()),
                            _=>input::check(py,&budget)?,
                        }
                    }
                })();
                result.map_err(|error|errors::at(py,error,"connection.close"))
            }
            /// True after irreversible local retirement begins. This does not
            /// confirm server transaction completion or finished cleanup.
            #[getter] fn closed(&self)->bool { self.native.is_closed() }
            /// Actual handshake server version; remains readable after close.
            #[getter] fn server_version(&self)->&str { &self.native.metadata.version }
            /// Actual server connection ID; remains readable after close.
            #[getter] fn connection_id(&self)->u32 { self.native.metadata.connection_id }
            /// Actual negotiated LOCAL upload capability; strict opt-in at
            /// connect, default False. Each upload also requires its own
            /// execute(local_file=...) path. Reading performs no I/O and remains
            /// valid after close; this grants no server-selected file access.
            #[getter] fn local_infile(&self)->bool {self.local_infile}
            /// Actual negotiated compression as an immutable cached snapshot,
            /// or None when uncompressed. This is independent of the original
            /// permission configuration, sends no SQL and remains readable
            /// after close. The zstd level is None for a zlib snapshot.
            #[getter] fn compression(&self,py:Python<'_>)->PyResult<Option<Py<crate::compression::Compression>>> {
                let Some(snapshot)=self.compression_snapshot else {return Ok(None);};
                if let Some(value)=self.compression_value.get() {return Ok(Some(value.clone_ref(py)));}
                let value=Py::new(py,crate::compression::Compression::from_options(py,snapshot)?)?;
                let _=self.compression_value.set(value);
                self.compression_value.get().map(|value|Some(value.clone_ref(py))).ok_or_else(||
                    errors::client(py,ClientErrorCode::InternalState,"Compression representation was not published"))
            }
            /// Actual immutable TLS diagnostics, or None for a plain channel.
            /// Facts and peer certificates remain available after close. Details
            /// are decoded lazily and successful Python representations reused.
            /// Access sends no SQL and never needs the network command lease.
            #[getter] fn tls_info(&self,py:Python<'_>)->PyResult<Option<Py<crate::tls_info::TlsInfo>>> {
                let Some(snapshot)=&self.tls_snapshot else {return Ok(None);};
                if let Some(value)=self.tls_info_value.get() {return Ok(Some(value.clone_ref(py)));}
                let value=Py::new(py,crate::tls_info::TlsInfo::new(Arc::clone(snapshot)))?;
                let _=self.tls_info_value.set(value);
                self.tls_info_value.get().map(|value|Some(value.clone_ref(py))).ok_or_else(||
                    errors::client(py,ClientErrorCode::InternalState,"TLS information representation was not published"))
            }
            /// Client-side establishment timeout in seconds. None disables
            /// its total limit. Read-only after creation and readable after close.
            /// Default 10; exact positive int/float or None, excluding bool and numeric
            /// subclasses. Minimum 1ns, nearest nanosecond with ties-to-even.
            /// Whole saved seconds read as int and subsecond values as float. Invalid type/value
            /// raises TypeError/ValueError. Covers configuration, authentication
            /// and required initialization, without an extra network cleanup grace.
            #[getter] fn connect_timeout<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> { let value=self.runtime(py)?.timeouts.connect; config::timeout_object(py,value) }
            /// Client-side read wait without progress, in seconds. None
            /// disables it. Local assignment affects only subsequent operations,
            /// sends no SQL, and is rejected after this connection is retired.
            /// Default None; exact positive int/float or None, excluding bool and numeric
            /// subclasses. Minimum 1ns, nearest nanosecond with ties-to-even.
            /// Whole saved seconds read as int and subsecond values as float. Invalid type/value
            /// raises TypeError/ValueError. Progress restarts only this interval;
            /// readiness does not. Does not cap a complete response. Saved values
            /// remain readable after close; expiry does not prove SQL cancellation.
            #[getter] fn read_timeout<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> { let value=self.runtime(py)?.timeouts.read; config::timeout_object(py,value) }
            #[setter] fn set_read_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::timeout(Some(value),"read_timeout")?; self.modify(py,"read_timeout")?.timeouts.read=value; Ok(())
            }
            /// Client-side write wait without progress, in seconds. None
            /// disables it; actual progress restarts only this wait interval.
            /// Local assignment affects subsequent operations, sends no SQL,
            /// and fails after close while the saved value remains readable.
            /// Default None; exact positive int/float or None, excluding bool and numeric
            /// subclasses. Minimum 1ns, nearest nanosecond with ties-to-even.
            /// Whole saved seconds read as int and subsecond values as float. Invalid type/value
            /// raises TypeError/ValueError. Does not cap a complete upload; the
            /// connect/operation deadline may expire earlier. Expiry does not
            /// prove server cancellation or confirmed transaction rollback.
            #[getter] fn write_timeout<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> { let value=self.runtime(py)?.timeouts.write; config::timeout_object(py,value) }
            #[setter] fn set_write_timeout(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::timeout(Some(value),"write_timeout")?; self.modify(py,"write_timeout")?.timeouts.write=value; Ok(())
            }
            /// Client-side total timeout for one operation in seconds,
            /// beginning before binding and shared by all batches and internal
            /// confirmations. Progress does not restart it. None disables it.
            /// Local assignment affects subsequent operations and sends no SQL;
            /// after close modification fails but the saved value stays readable.
            /// Default None; exact positive int/float or None, excluding bool and numeric
            /// subclasses. Minimum 1ns, nearest nanosecond with ties-to-even.
            /// Whole saved seconds read as int and subsecond values as float. Invalid type/value
            /// raises TypeError/ValueError. Includes request preparation, reception
            /// and state/result publication, but not buffered fetching or user
            /// transaction-block code. Expiry grants no new network cleanup period
            /// and does not prove SQL cancellation or confirmed rollback.
            #[getter] fn operation_timeout<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> { let value=self.runtime(py)?.timeouts.operation; config::timeout_object(py,value) }
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
            /// Confirmed permission for multiple statements in one text
            /// request. This is read-only and remains readable after close
            /// when known. It does not disable a CALL's multiple results or
            /// the separate prepared-statement protocol. Unknown raises
            /// InterfaceError; reading never sends a command.
            #[getter] fn multi_statements(&self,py:Python<'_>)->PyResult<bool> {
                let value=self.native.facts.lock().map_err(|error|{drop(error.into_inner());errors::client(py,ClientErrorCode::InternalState,"confirmed session facts are unavailable")})?.multi_statements;
                value.ok_or_else(||errors::client(py,ClientErrorCode::SessionState,"multi_statements has not been reliably confirmed after the previous control request"))
            }
            /// Change the protocol permission using COM_SET_OPTION, accepting
            /// exact bool only. The saved value changes only on a complete
            /// acknowledgement. A confirmed equal value sends no command, but
            /// still rejects closed connections and overlapping/unread work.
            /// Requests are never replayed after an unknown response.
            fn set_multi_statements(&self,py:Python<'_>,enabled:&Bound<'_,PyAny>)->PyResult<()> {
                (|| {
                    let mut budget=self.runtime(py)?.timeouts.operation();
                    let enabled=config::boolean(enabled,"enabled")?;
                    let mut lease = self.acquire(py, &mut budget)?;
                    input::check(py,&budget)?;
                    if self.multi_statements(py).ok()==Some(enabled) {return Ok(());}
                    let bytes=if enabled {vec![0x1b,0,0]} else {vec![0x1b,1,0]};
                    let job=self.query_job(py,&mut lease,bytes,false)?;
                    let result=self.drive(py,&mut lease,job,&mut budget,CommandRole::InternalControl);
                    let confirmed=result.as_ref().map_or_else(|failure|failure.confirmation().is_some(), |_|true);
                    let unknown=result.as_ref().err().is_some_and(|failure|failure.outcome.unknown());
                    if confirmed || unknown {
                        self.native.facts.lock().map_err(|error|{drop(error.into_inner());errors::client(py,ClientErrorCode::InternalState,"confirmed multi-statement facts are unavailable")})?.multi_statements=confirmed.then_some(enabled);
                    }
                    result?;
                    input::check(py,&budget)
                })().map_err(|error|errors::at(py,error,"connection.set_multi_statements"))
            }
            /// Change server SESSION autocommit after acquiring command ownership.
            /// A reliably confirmed same value sends no request and is not a ping.
            fn set_autocommit(&self,py:Python<'_>,enabled:&Bound<'_,PyAny>)->PyResult<()> {
                (|| {
                    let mut budget=self.runtime(py)?.timeouts.operation();
                    let enabled=config::boolean(enabled,"enabled")?;
                    let mut lease = self.acquire(py, &mut budget)?;
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
                crate::statement::flush(py,self,&mut lease,&mut budget)?;
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
    (decode_date_strict,set_decode_date_strict,"Default True. True raises DataError for zero or otherwise unrepresentable DATE/DATETIME values. False returns str for unrepresentable calendars: text protocol preserves the received calendar spelling using the result codec; binary protocol has no original text and formats the received components with trustworthy fractional-second precision. For example, a zero binary DATETIME with fsp=6 becomes '0000-00-00 00:00:00.000000'. Malformed wire values still raise protocol errors; components outside the canonical display format or an untrustworthy/inconsistent precision raise DataError. Valid values retain date/datetime output in both modes. SQL NULL remains None; TIME follows use_timedelta.\n\nThis local default is copied only to new Cursors; existing Cursors and results do not change. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
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
/// The four client timeouts accept exact positive int/float seconds or None,
/// excluding bool and numeric subclasses, with nearest nanosecond rounding
/// (ties-to-even) and a 1ns minimum. Server timeouts accept exact positive int
/// seconds or None. Type/value errors raise TypeError/ValueError.
/// Client connect_timeout defaults to 10 and
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
///
/// multi_statements defaults to True and requires an exact bool; later changes
/// use set_multi_statements(). local_infile defaults to False and requires an
/// exact bool. An upload also needs that execute() call's explicit local_file
/// path; a server-supplied filename never authorizes a local file by itself.
///
/// compression accepts an algorithm CSV str, list[str], tuple[str, ...], a
/// reusable Compression configuration, or None to disable compression. Shorthand
/// zstd uses level 3; a different level requires Compression. Uncompressed
/// fallback is allowed only when explicitly listed. This does not weaken TLS.
/// Connection.compression reports an independent snapshot of the actual
/// negotiated compression, or None when no compression is active.
///
/// Explicit utf8mb3 selects BMP-only UTF-8; utf8 and utf-8 still select utf8mb4.
/// Binary parameter and result roles remain binary regardless of this charset.
///
/// init_sql accepts None, one nonempty SQL str, or a finite list/tuple of nonempty
/// SQL str. It runs after normal initialization under the original connect
/// deadline. Each statement and all its results are processed and discarded;
/// strings are not split at semicolons. Confirmed final settings and transaction
/// state are retained without an automatic commit or rollback. Required final
/// UTC/session conditions still apply. Failure stops later steps and does not
/// undo effects that were already committed.
///
/// server_public_key is an explicitly trusted RSA SubjectPublicKeyInfo PEM
/// source. str and PathLike values are paths; top-level bytes are PEM contents,
/// even if they look like a filename. A PathLike returning bytes remains a path.
/// None supplies no key. Invalid explicit material fails preparation even when
/// TLS or fast authentication would not use it. No content guessing, DER/PKCS#1
/// conversion or automatic network key fetching occurs. This key does not
/// encrypt later SQL/results or independently verify the server's identity.
/// The old server_public_key_path keyword is no longer accepted; use
/// server_public_key instead. See SSL and Compression for their configuration
/// contracts, and Connection.tls_info for actual negotiated TLS facts.
#[pyfunction]
#[pyo3(pass_module,signature=(*,user,host=Argument::default(),port=Argument::default(),unix_socket=None,password=None,database=None,charset=None,collation=None,ssl=None,compression=None,server_public_key=None,autocommit=Argument::default(),sql_mode=None,init_sql=None,multi_statements=Argument::default(),local_infile=Argument::default(),connect_timeout=Argument::default(),read_timeout=None,write_timeout=None,operation_timeout=None,net_read_timeout=None,net_write_timeout=None,wait_timeout=None,innodb_lock_wait_timeout=None,max_allowed_packet=Argument::default(),max_received_packet=Argument::default(),result_memory_limit=Argument::default(),total_result_memory_limit=Argument::default(),decode_json=Argument::default(),decode_set=Argument::default(),decode_bit=Argument::default(),use_decimal=Argument::default(),use_timedelta=Argument::default(),decode_text_strict=Argument::default(),decode_date_strict=Argument::default()))]
#[pyo3(
    text_signature = "(*, user, host='127.0.0.1', port=3306, unix_socket=None, password=None, database=None, charset=None, collation=None, ssl=None, compression=None, server_public_key=None, autocommit=True, sql_mode=None, init_sql=None, multi_statements=True, local_infile=False, connect_timeout=10, read_timeout=None, write_timeout=None, operation_timeout=None, net_read_timeout=None, net_write_timeout=None, wait_timeout=None, innodb_lock_wait_timeout=None, max_allowed_packet='64MB', max_received_packet='64MB', result_memory_limit='256MB', total_result_memory_limit='512MB', decode_json=False, decode_set=False, decode_bit=False, use_decimal=True, use_timedelta=True, decode_text_strict=True, decode_date_strict=True)"
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
    compression: Option<&Bound<'_, PyAny>>,
    server_public_key: Option<&Bound<'_, PyAny>>,
    autocommit: Argument<'_>,
    sql_mode: Option<&Bound<'_, PyAny>>,
    init_sql: Option<&Bound<'_, PyAny>>,
    multi_statements: Argument<'_>,
    local_infile: Argument<'_>,
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
    let user_initialization = input::control::init_sql(init_sql, &budget)
        .map_err(|error| errors::at(py, error, "connect"))?;
    let compression = crate::compression::parse(compression, Some(&budget))
        .map_err(|error| errors::at(py, error, "connect"))?;
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
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            "host must be a DNS name or unbracketed IP, without URL, port, path, NUL or whitespace",
        ));
    }
    let port = match &port.0 {
        None => 3306,
        Some(value) => u16::try_from(config::positive(value, "port")?)
            .ok()
            .filter(|value| *value != 0)
            .ok_or_else(|| {
                crate::errors::builtin_message::<PyValueError>(
                    py,
                    "port must be an integer in 1..=65535",
                )
            })?,
    };
    let socket = config::optional_path(unix_socket, "unix_socket")?;
    if socket.is_some() && (host_provided || port_provided) {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            "unix_socket conflicts with explicitly provided host or port; omit both TCP fields",
        ));
    }
    let unix = socket.is_some();
    let password = Zeroizing::new(
        config::secret_utf8(
            py,
            "password",
            config::optional_text(password, "password", false),
        )?
        .unwrap_or_default()
        .into_bytes(),
    );
    let database = config::optional_text(database, "database", false)?;
    let encoding = charset::resolve(
        config::optional_text(charset, "charset", false)?.as_deref(),
        config::optional_text(collation, "collation", false)?.as_deref(),
    )
    .map_err(|error| crate::errors::builtin_message::<PyValueError>(py, error.to_string()))?;
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
    let server_public_key = config::public_key(server_public_key)?;
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
    let server_timeouts = server_timeouts.try_into().map_err(|_| {
        crate::errors::builtin_message::<PyValueError>(py, "invalid internal server timeout count")
    })?;
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
    let tls = selected.prepare(py, unix, &host)?;
    let diagnostic_name = tls
        .as_ref()
        .filter(|_| !unix || selected.has_explicit_server_name())
        .map(|tls| tls.name.clone());
    input::check(py, &budget).map_err(|error| errors::at(py, error, "connect"))?;
    let multi_statements = multi_statements
        .0
        .as_ref()
        .map_or(Ok(true), |value| config::boolean(value, "multi_statements"))?;
    let local_infile = local_infile
        .0
        .as_ref()
        .map_or(Ok(false), |value| config::boolean(value, "local_infile"))?;
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
        compression,
        multi_statements,
        local_infile,
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
    let compression_snapshot = ready.transport.channel.compression_options();
    let local_infile = ready
        .transport
        .capabilities
        .contains(sqlaero_core::protocol::CapabilityFlags::CLIENT_LOCAL_FILES);
    let tls_snapshot = ready
        .transport
        .channel
        .tls_session()
        .map(|session| {
            let name = diagnostic_name
                .as_ref()
                .map(rustls_pki_types::ServerName::to_str);
            sqlaero_core::transport::tls::info::Snapshot::capture(
                session,
                selected.resolved_mode(unix),
                selected.minimum(),
                selected.maximum(),
                name.as_deref(),
            )
            .map(Arc::new)
            .map_err(|error| match error {
                sqlaero_core::transport::tls::info::SnapshotError::Allocation(_) => {
                    errors::memory_error(py)
                }
                error => errors::client_format(
                    py,
                    ClientErrorCode::InternalState,
                    format_args!("Cannot preserve TLS handshake facts: {error}"),
                ),
            })
        })
        .transpose()?;
    let types = module
        .getattr("_scalar_types")?
        .cast_into::<ScalarTypes>()?
        .unbind();
    let (native, mut budget) = NativeConnection::with_retained_budget(ready, total_limit);
    let connection = Connection {
        native: Arc::new(native),
        runtime: Mutex::new(Runtime {
            timeouts: timeout_config,
            options,
        }),
        types,
        result_limit,
        max_send,
        max_receive,
        total_result_limit: total_limit,
        tls_snapshot,
        tls_info_value: OnceLock::new(),
        compression_snapshot,
        compression_value: OnceLock::new(),
        local_infile,
        call_slots: OnceLock::new(),
        transaction_scope: Arc::new(AtomicBool::new(false)),
    };
    if !user_initialization.is_empty()
        && let Err(primary) = connection.initialize_sql(py, &user_initialization, &mut budget)
    {
        connection.native.retire();
        return Err(errors::at(py, primary, "connect"));
    }
    Ok(connection)
}
