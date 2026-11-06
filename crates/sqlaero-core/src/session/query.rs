//! One native command exchange over the existing channel and original budget.

use crate::memory::{Pool, Reservation, Scope};
use crate::protocol::framing::{MessageReader, MessageWriter, WireError};
use crate::protocol::local_infile::{LocalError, Source, Upload, UploadProgress};
use crate::result::text::{
    self, Completion, Delivery, Header, Receiver, Response, ResultError, RowBatch, StreamEvent,
};
use crate::result::value::RowEncoding;
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use mysql_common::constants::{CapabilityFlags, Command, StatusFlags};
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut, Range};
use std::sync::Arc;

/// A command failure whose server/transport/resource origin remains explicit.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// The explicitly authorized LOCAL source or upload failed.
    #[error(transparent)]
    Local(#[from] LocalError),
    /// A complete result failed protocol, data or resource validation.
    #[error(transparent)]
    Result(#[from] ResultError),
    /// A real channel failure occurred.
    #[error(transparent)]
    Channel(#[from] ChannelError),
    /// Native request or message storage was invalid.
    #[error(transparent)]
    Wire(#[from] WireError),
}

pub use crate::result::text::CommandConfirmation;

/// Managed CALL delivery keeps an internal output's actual status independent
/// of business results and their counters. Ordinary query delivery is unchanged.
pub enum Managed<T> {
    /// One original business result or streaming boundary.
    Public(T),
    /// An actual validated native output terminator, never a business set.
    /// Apply its status/session tracking before continuing the original CALL.
    InternalOutput {
        /// The original completion, without synthesized status flags.
        completion: Completion,
        /// Original legacy metadata flags, when that packet existed.
        metadata_status: Option<StatusFlags>,
    },
}

struct ManagedOutputs {
    expected: usize,
    output: Option<RowBatch>,
    seen: bool,
    header: Option<Header>,
    rows: [Option<RowBatch>; 2],
    end: Option<StreamEvent>,
    publish: bool,
    business: bool,
    failed_completion: Option<(Completion, Option<StatusFlags>)>,
}

impl ManagedOutputs {
    fn new(expected: usize) -> Self {
        Self {
            expected,
            output: None,
            seen: false,
            header: None,
            rows: [None, None],
            end: None,
            publish: false,
            business: false,
            failed_completion: None,
        }
    }

    fn pending(&self) -> bool {
        self.header.is_some() || self.rows.iter().any(Option::is_some) || self.end.is_some()
    }

    fn pop(&mut self) -> Option<StreamEvent> {
        if !self.publish {
            return None;
        }
        if let Some(header) = self.header.take() {
            return Some(StreamEvent::Header(header));
        }
        if let Some(rows) = self.rows[0].take() {
            self.rows[0] = self.rows[1].take();
            return Some(StreamEvent::Rows(rows));
        }
        self.publish = false;
        if let Some(end) = self.end.take() {
            self.business = false;
            return Some(end);
        }
        None
    }

    fn retain(&mut self, rows: RowBatch) -> Result<(), WireError> {
        if self.seen {
            return Err(WireError::Malformed("duplicate managed output result"));
        }
        if rows.row_count != 1 || rows.columns.len() != self.expected {
            return Err(WireError::Malformed(
                "managed output row or column count mismatch",
            ));
        }
        self.seen = true;
        self.output = Some(rows);
        Ok(())
    }

    fn internal<T>(
        &mut self,
        rows: Option<RowBatch>,
        completion: Completion,
        metadata_status: Option<StatusFlags>,
    ) -> Result<Managed<T>, QueryError> {
        let result = rows
            .ok_or(WireError::Malformed(
                "managed output requires exactly one row",
            ))
            .and_then(|rows| self.retain(rows));
        match result {
            Ok(()) => Ok(Managed::InternalOutput {
                completion,
                metadata_status,
            }),
            Err(error) => Err(self.invalid(completion, metadata_status, error)),
        }
    }

    fn invalid(
        &mut self,
        completion: Completion,
        metadata_status: Option<StatusFlags>,
        error: WireError,
    ) -> QueryError {
        self.failed_completion = Some((completion, metadata_status));
        error.into()
    }

    fn clear_pending(&mut self) {
        self.header = None;
        self.rows = [None, None];
        self.end = None;
        self.publish = false;
        self.business = false;
    }
}

// One concrete opt-in control owner, not a result registry or growable queue.
// Keep one checked allocation: ordinary queries carry only its empty handle.
struct ManagedStorage {
    state: Vec<ManagedOutputs>,
    _owner: Reservation,
}
impl ManagedStorage {
    fn new(expected: usize, scope: &Arc<Scope>) -> Result<Self, ResultError> {
        let mut owner = scope.charge(std::mem::size_of::<ManagedOutputs>())?;
        let mut state = Vec::new();
        state
            .try_reserve_exact(1)
            .map_err(crate::memory::MemoryError::from)?;
        owner.resize(
            state
                .capacity()
                .checked_mul(std::mem::size_of::<ManagedOutputs>())
                .ok_or(WireError::Malformed("managed control capacity overflow"))?,
        )?;
        state.push(ManagedOutputs::new(expected));
        Ok(Self {
            state,
            _owner: owner,
        })
    }
}
impl Deref for ManagedStorage {
    type Target = ManagedOutputs;
    fn deref(&self) -> &Self::Target {
        &self.state[0]
    }
}
impl DerefMut for ManagedStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state[0]
    }
}

fn output_status(status: StatusFlags, metadata_status: Option<StatusFlags>) -> bool {
    (status | metadata_status.unwrap_or_default()).contains(StatusFlags::SERVER_PS_OUT_PARAMS)
}

/// Actual request-transfer progress, independent of response or commit facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestProgress {
    /// No request bytes have been accepted by the transport writer.
    NotSent,
    /// Sending started, including plaintext accepted by a TLS writer.
    Sending,
    /// The complete request and required TLS records have been flushed.
    AwaitingResponse,
}

/// Actual completed prefix of one user batch. These facts are attached to a
/// later error; they never masquerade as the failed call's success counters
/// and do not assert that any transaction was committed.
#[derive(Debug, Default)]
pub struct ConfirmedPrefix {
    /// Number of complete commands whose full responses were consumed.
    pub commands: usize,
    /// Contiguous source-row range already confirmed; an empty root can issue
    /// one zero-position command without inventing a source row.
    pub source_rows: usize,
    /// Sum of actual affected rows, or actual returned rows for row responses.
    pub row_count: u128,
    /// Sum of each actual command's warning count, exactly once per command.
    pub warnings: u128,
    /// Insert ID of the most recent completed command, not a guessed range.
    pub last_insert_id: Option<u64>,
}

impl ConfirmedPrefix {
    /// Add only a complete response in source order. A send, partial response,
    /// server ERR or local encoding failure must not call this method.
    ///
    /// # Errors
    /// Returns inconsistent source-range or command-count overflow failures
    /// without partially mutating the existing confirmed prefix.
    pub fn record(&mut self, rows: Range<usize>, response: &Response) -> Result<(), WireError> {
        self.record_confirmation(rows, CommandConfirmation::from_response(response))
    }
    /// Retain confirmation even when subsequent local state/result publication
    /// fails. A confirmation says execution completed, not transaction commit.
    ///
    /// # Errors
    /// Returns noncontiguous-source or command-count overflow failures.
    pub fn record_confirmation(
        &mut self,
        rows: Range<usize>,
        confirmation: CommandConfirmation,
    ) -> Result<(), WireError> {
        if rows.start != self.source_rows || rows.end < rows.start {
            return Err(WireError::Malformed(
                "batch confirmation is not a contiguous source prefix",
            ));
        }
        let commands = self
            .commands
            .checked_add(1)
            .ok_or(WireError::Malformed("batch command count overflow"))?;
        self.commands = commands;
        self.source_rows = rows.end;
        self.row_count += confirmation.count;
        self.warnings += u128::from(confirmation.warnings);
        self.last_insert_id = confirmation.last_insert_id;
        Ok(())
    }

    /// Concise factual diagnostic. The current failing/unknown range and stage
    /// are supplied separately by the boundary that owns that actual request.
    #[must_use]
    pub fn diagnostic(&self) -> String {
        format!(
            "confirmed {} commands covering source rows 0..{}; actual cumulative row count {}, warnings {}, final insert ID {:?}; confirmation does not imply transaction commit",
            self.commands, self.source_rows, self.row_count, self.warnings, self.last_insert_id
        )
    }
}

/// An owned complete request beginning with its actual MySQL command byte.
pub struct Query {
    request: Vec<u8>,
    sender: MessageWriter,
    incoming: MessageReader,
    collector: Receiver,
    capabilities: CapabilityFlags,
    encoding: RowEncoding,
    max_receive: usize,
    max_send: usize,
    local_upload: Option<Upload>,
    local_progress: Option<UploadProgress>,
    scope: Arc<Scope>,
    sent: bool,
    started: bool,
    next_set: bool,
    discard: bool,
    memory_limit: usize,
    pool: Arc<Pool>,
    finished: bool,
    streaming: bool,
    stream_boundary: bool,
    managed: Option<ManagedStorage>,
}

impl Query {
    /// Whether the complete command ended, rather than just one result set.
    /// A validated server ERR also ends this exchange without inventing success.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
            && !self
                .managed
                .as_ref()
                .is_some_and(|managed| managed.pending())
    }

    /// Read existing framing facts only when needed for failure handling.
    #[must_use]
    pub fn progress(&self) -> RequestProgress {
        if self.sent {
            RequestProgress::AwaitingResponse
        } else if self.sender.has_started() {
            RequestProgress::Sending
        } else {
            RequestProgress::NotSent
        }
    }
    /// A validated ERR is a complete failure response, not a lost response.
    #[must_use]
    pub fn received_server_err(&self) -> bool {
        self.collector.received_server_err()
    }

    /// Prepare a native request that already includes its command byte. SQL
    /// binding writes directly into this request before the network phase.
    ///
    /// # Errors
    /// Returns invalid/oversize request failures before transmission.
    pub fn new(
        request: Vec<u8>,
        capabilities: CapabilityFlags,
        max_send: usize,
        max_receive: usize,
        memory_limit: usize,
    ) -> Result<Self, QueryError> {
        Self::with_pool(
            request,
            capabilities,
            max_send,
            max_receive,
            memory_limit,
            Pool::new(usize::MAX),
            false,
        )
    }

    /// Execute under the same aggregate pool as other live connection results.
    /// Discard only row storage for known intermediate commands; validation,
    /// real counts and session-state parsing remain identical.
    ///
    /// # Errors
    /// Returns actual request, native-owner or configured capacity failures.
    /// COM_SET_OPTION must be exactly its opcode and supported little-endian
    /// u16 operation (0 enables multi-statements, 1 disables them).
    pub fn with_pool(
        request: Vec<u8>,
        capabilities: CapabilityFlags,
        max_send: usize,
        max_receive: usize,
        memory_limit: usize,
        pool: Arc<Pool>,
        discard: bool,
    ) -> Result<Self, QueryError> {
        if request.is_empty() {
            return Err(WireError::Malformed("request has no command byte").into());
        }
        let set_option = request[0] == Command::COM_SET_OPTION as u8;
        if set_option && !matches!(request.as_slice(), [0x1b, 0 | 1, 0]) {
            return Err(
                WireError::Malformed("COM_SET_OPTION requires one supported u16 option").into(),
            );
        }
        let sender = MessageWriter::new(request.len(), max_send)?;
        let scope = Scope::new(Arc::clone(&pool), memory_limit).map_err(ResultError::from)?;
        let encoding = if request[0] == Command::COM_STMT_EXECUTE as u8 {
            RowEncoding::Binary
        } else {
            RowEncoding::Text
        };
        let mut collector =
            Receiver::with_encoding(capabilities, Arc::clone(&scope), discard, encoding)?;
        if set_option {
            collector.enable_set_option_ack();
        }
        Ok(Self {
            request,
            sender,
            incoming: MessageReader::tracked(max_receive, &scope),
            collector,
            capabilities,
            encoding,
            max_receive,
            max_send,
            local_upload: None,
            local_progress: None,
            scope,
            sent: false,
            started: false,
            next_set: false,
            discard,
            memory_limit,
            pool,
            finished: false,
            streaming: false,
            stream_boundary: false,
            managed: None,
        })
    }

    /// Attach one already opened source before SQL transmission and old-result
    /// handoff. The boundary has already verified single plain-text LOAD shape.
    ///
    /// # Errors
    /// Rejects late, repeated, non-text or unnegotiated permission and reserves
    /// bounded work storage before the shared channel is touched.
    pub fn with_local_infile(mut self, source: Source) -> Result<Self, QueryError> {
        if self.started || self.local_upload.is_some() || self.request[0] != 3 {
            return Err(WireError::Malformed("invalid LOCAL source command").into());
        }
        if !self
            .capabilities
            .contains(CapabilityFlags::CLIENT_LOCAL_FILES)
        {
            return Err(WireError::Malformed("LOCAL client capability is disabled").into());
        }
        self.local_upload = Some(Upload::new(source, self.max_send, Arc::clone(&self.scope))?);
        Ok(self)
    }

    /// Actual LOCAL observations even if the final server response is unknown.
    #[must_use]
    pub fn local_progress(&self) -> Option<UploadProgress> {
        self.local_upload
            .as_ref()
            .map(Upload::progress)
            .or(self.local_progress)
    }

    /// Select resumable row delivery before transmission. Row parsing, native
    /// ownership, encoding, limits and the original channel remain identical.
    ///
    /// # Errors
    /// Rejects changing a started query or a discard-only collector.
    pub fn with_streaming(mut self) -> Result<Self, QueryError> {
        if self.started || self.streaming {
            return Err(WireError::Malformed("invalid streaming command state").into());
        }
        self.collector.enable_streaming()?;
        self.streaming = true;
        Ok(self)
    }

    /// Hide only a prepared CALL's explicitly managed native OUT/INOUT result.
    /// This opt-in belongs to callproc with known output directions. Direct
    /// execute CALL and calls without outputs must keep ordinary delivery.
    ///
    /// # Errors
    /// Rejects a started, non-EXECUTE, zero-output, duplicate or discard-only
    /// setup before sending bytes. The caller supplies the declared count;
    /// this does not inspect SQL or query a routine signature.
    pub fn with_managed_outputs(mut self, expected: usize) -> Result<Self, QueryError> {
        if self.started
            || self.managed.is_some()
            || self.encoding != RowEncoding::Binary
            || expected == 0
            || self.discard
        {
            return Err(WireError::Malformed("invalid managed output command").into());
        }
        self.managed = Some(ManagedStorage::new(expected, &self.scope)?);
        Ok(self)
    }

    /// Consume one completely received and shape-validated native output row.
    /// A later error does not erase it. This alone does not assert that the
    /// full CALL succeeded or that a public output mapping is ready.
    pub fn take_call_output(&mut self) -> Option<RowBatch> {
        self.managed
            .as_mut()
            .and_then(|managed| managed.output.take())
    }

    /// Consume a real non-business terminator saved only when subsequent OUT
    /// shape/duplicate validation failed. Publish its actual session status
    /// before reporting that error; it is never a whole-CALL confirmation.
    /// Normal InternalOutput events do not duplicate their completion here.
    pub fn take_managed_completion(&mut self) -> Option<(Completion, Option<StatusFlags>)> {
        self.managed
            .as_mut()
            .and_then(|managed| managed.failed_completion.take())
    }

    /// A Header/Rows/End was returned and no later logical message was started.
    /// The host can reserve its accumulation slot here before calling advance:
    /// a local reservation failure outside Query leaves this state and all
    /// already delivered batches intact. Native receive failures are separate.
    #[must_use]
    pub const fn at_stream_boundary(&self) -> bool {
        self.streaming && self.stream_boundary
    }

    /// Stop storing subsequent rows of this current streaming set only.
    /// Validation, actual counts, channel ownership and budget remain intact;
    /// a following set gets the original delivery policy and a new Receiver.
    ///
    /// # Errors
    /// Rejects calls outside a delivered Header/Rows boundary, including an
    /// already ended set. This local rejection does not advance the exchange.
    pub fn discard_current_rows(&mut self) -> Result<(), QueryError> {
        if !self.at_stream_boundary() {
            return Err(
                WireError::Malformed("discard requires a streaming result boundary").into(),
            );
        }
        if let Some(managed) = self.managed.as_mut() {
            if !managed.business || managed.header.is_some() {
                return Err(
                    WireError::Malformed("discard requires a published business header").into(),
                );
            }
            // Its real End may already be queued after a zero/one-row
            // lookahead. Only drop business rows; keep that original End.
            managed.rows = [None, None];
            if managed.end.is_some() {
                return Ok(());
            }
        }
        self.collector.discard_current_rows()?;
        Ok(())
    }

    /// Receive one buffered business set or one actual managed OUT completion.
    /// It uses the same complete-set parser, without a second network driver.
    ///
    /// # Errors
    /// Returns real exchange errors and native output shape/duplicate errors.
    /// After the latter, take_managed_completion retains its real terminator.
    pub fn advance_managed(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<Managed<Response>>, QueryError> {
        if self.managed.is_none() || self.streaming {
            return Err(WireError::Malformed("query needs managed buffered delivery").into());
        }
        let result = self.advance_buffered_raw(channel, budget).and_then(|step| {
            let Step::Ready(response) = step else {
                return Ok(match step {
                    Step::Control => Step::Control,
                    Step::Wait(direction) => Step::Wait(direction),
                    Step::Ready(_) => unreachable!(),
                });
            };
            let metadata_status = match &response {
                Response::Rows(rows) => rows.metadata_status,
                _ => None,
            };
            if !output_status(response.completion().status_flags(), metadata_status) {
                return Ok(Step::Ready(Managed::Public(response)));
            }
            let managed = self.managed.as_mut().expect("validated managed mode");
            let (rows, completion) = match response {
                Response::Rows(rows) => (Some(rows.rows), rows.completion),
                Response::Command(completion) | Response::Counted { completion, .. } => {
                    (None, completion)
                }
            };
            managed
                .internal(rows, completion, metadata_status)
                .map(Step::Ready)
        });
        if result.is_err() {
            self.finished = true;
        }
        result
    }

    /// Receive managed streaming delivery. Only this explicit mode waits for
    /// up to two native rows or the real set End before publishing its header.
    /// Two business rows are replayed in order without reading a third row.
    ///
    /// # Errors
    /// Returns actual parser/storage/transport errors without replaying SQL.
    /// Completed output and its real error completion remain independently
    /// takeable after a later failure or host signal.
    pub fn advance_managed_stream(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
        row_limit: NonZeroUsize,
    ) -> Result<Step<Managed<StreamEvent>>, QueryError> {
        if self.managed.is_none() || !self.streaming {
            return Err(WireError::Malformed("query needs managed streaming delivery").into());
        }
        self.stream_boundary = false;
        let result = self.advance_managed_stream_inner(channel, budget, row_limit);
        match &result {
            Ok(Step::Ready(Managed::Public(_))) => self.stream_boundary = true,
            Err(_) => {
                self.finished = true;
                self.managed
                    .as_mut()
                    .expect("validated managed mode")
                    .clear_pending();
            }
            _ => self.stream_boundary = false,
        }
        result
    }

    fn advance_managed_stream_inner(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
        row_limit: NonZeroUsize,
    ) -> Result<Step<Managed<StreamEvent>>, QueryError> {
        loop {
            let managed = self.managed.as_mut().expect("validated managed mode");
            if let Some(event) = managed.pop() {
                return Ok(Step::Ready(Managed::Public(event)));
            }
            let limit = if managed.business {
                row_limit
            } else {
                NonZeroUsize::MIN
            };
            let event = match self.advance_stream_raw(channel, budget, limit)? {
                Step::Ready(event) => event,
                Step::Control => return Ok(Step::Control),
                Step::Wait(direction) => return Ok(Step::Wait(direction)),
            };
            let managed = self.managed.as_mut().expect("validated managed mode");
            match event {
                StreamEvent::Header(header) => managed.header = Some(header),
                StreamEvent::Rows(rows) if managed.business => {
                    return Ok(Step::Ready(Managed::Public(StreamEvent::Rows(rows))));
                }
                StreamEvent::Rows(rows) => {
                    if rows.row_count != 1 || managed.header.is_none() {
                        return Err(
                            WireError::Malformed("invalid managed lookahead row boundary").into(),
                        );
                    }
                    if managed.rows[0].is_none() {
                        managed.rows[0] = Some(rows);
                    } else {
                        if managed
                            .header
                            .as_ref()
                            .and_then(|header| header.metadata_status)
                            .is_some_and(|status| {
                                status.contains(StatusFlags::SERVER_PS_OUT_PARAMS)
                            })
                        {
                            return Err(WireError::Malformed(
                                "managed output has more than one row",
                            )
                            .into());
                        }
                        managed.rows[1] = Some(rows);
                        managed.publish = true;
                        managed.business = true;
                    }
                }
                StreamEvent::End {
                    tail,
                    completion,
                    total_rows,
                    metadata_status,
                } => {
                    if output_status(completion.status_flags(), metadata_status) {
                        if managed.business {
                            return Err(managed.invalid(
                                completion,
                                metadata_status,
                                WireError::Malformed("managed output has more than one row"),
                            ));
                        }
                        if tail.is_some() {
                            return Err(managed.invalid(
                                completion,
                                metadata_status,
                                WireError::Malformed("invalid managed lookahead tail"),
                            ));
                        }
                        let rows = managed.rows[0].take();
                        managed.header = None;
                        return managed
                            .internal(rows, completion, metadata_status)
                            .map(Step::Ready);
                    }
                    let end = StreamEvent::End {
                        tail,
                        completion,
                        total_rows,
                        metadata_status,
                    };
                    if managed.header.is_some() {
                        managed.end = Some(end);
                        managed.publish = true;
                        managed.business = true;
                    } else {
                        managed.business = false;
                        return Ok(Step::Ready(Managed::Public(end)));
                    }
                }
            }
        }
    }

    /// Advance only to completed column definitions, a bounded complete-row
    /// batch, or an actual set terminator. A delivered batch has no fabricated
    /// Completion and remains owned for conversion retry before the next call.
    ///
    /// # Errors
    /// Native framing/required storage/parser failure is terminal. Host-side
    /// batch-slot reservation before this call is the controlled retry branch.
    pub fn advance_stream(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
        row_limit: NonZeroUsize,
    ) -> Result<Step<StreamEvent>, QueryError> {
        if self.managed.is_some() {
            return Err(WireError::Malformed("managed query needs managed delivery").into());
        }
        self.advance_stream_raw(channel, budget, row_limit)
    }

    fn advance_stream_raw(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
        row_limit: NonZeroUsize,
    ) -> Result<Step<StreamEvent>, QueryError> {
        self.stream_boundary = false;
        let result = if self.streaming {
            self.advance_inner(channel, budget, Some(row_limit))
                .and_then(|step| match step {
                    Step::Ready(Delivery::Streaming(event)) => Ok(Step::Ready(event)),
                    Step::Ready(Delivery::Buffered(_)) => {
                        Err(WireError::Malformed("wrong query delivery").into())
                    }
                    Step::Control => Ok(Step::Control),
                    Step::Wait(direction) => Ok(Step::Wait(direction)),
                })
        } else {
            Err(WireError::Malformed("query is not streaming").into())
        };
        if result.is_err() {
            self.finished = true;
        }
        result
    }

    /// Receive the next complete result of this command, retaining framing
    /// across waits, host checks and set boundaries. `Ready` publishes one set;
    /// consult `is_finished` before releasing command ownership. The caller
    /// applies every terminator's session facts before advancing again.
    ///
    /// # Errors
    /// Returns real server, resource, protocol or transport failures. Callers
    /// preserve actual sending/confirmation facts when invalidating a session.
    pub fn advance(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<Response>, QueryError> {
        if self.managed.is_some() {
            return Err(WireError::Malformed("managed query needs managed delivery").into());
        }
        self.advance_buffered_raw(channel, budget)
    }

    fn advance_buffered_raw(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<Response>, QueryError> {
        let result = if self.streaming {
            Err(WireError::Malformed("streaming query needs stream delivery").into())
        } else {
            self.advance_inner(channel, budget, None)
                .and_then(|step| match step {
                    Step::Ready(Delivery::Buffered(response)) => Ok(Step::Ready(response)),
                    Step::Ready(Delivery::Streaming(_)) => {
                        Err(WireError::Malformed("wrong query delivery").into())
                    }
                    Step::Control => Ok(Step::Control),
                    Step::Wait(direction) => Ok(Step::Wait(direction)),
                })
        };
        if result.is_err() {
            self.finished = true;
        }
        result
    }

    /// Take validated full-command facts after a local completion-storage failure.
    /// An intermediate set never masquerades as full-command confirmation.
    pub fn take_confirmation(&mut self) -> Option<CommandConfirmation> {
        self.collector.take_confirmation()
    }

    fn advance_inner(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
        row_limit: Option<NonZeroUsize>,
    ) -> Result<Step<Delivery>, QueryError> {
        if self.finished {
            return Err(WireError::Malformed("command exchange already ended").into());
        }
        if self.next_set {
            // Every set retains its own per-result quota; earlier owners stay
            // charged to the same connection pool. Prepare the next collector
            // only after the previous complete set was returned to its owner.
            let scope =
                Scope::new(Arc::clone(&self.pool), self.memory_limit).map_err(ResultError::from)?;
            let mut collector = Receiver::with_encoding(
                self.capabilities,
                Arc::clone(&scope),
                self.discard,
                self.encoding,
            )?;
            if self.streaming {
                collector.enable_streaming()?;
            }
            self.incoming = MessageReader::tracked(self.max_receive, &scope);
            self.collector = collector;
            self.scope = scope;
            self.next_set = false;
        }
        let mut processed = 0usize;
        if !self.started {
            channel.new_command()?;
            self.started = true;
        }
        loop {
            if !self.sent {
                match channel.send_step(
                    &mut self.sender,
                    &self.request,
                    budget,
                    Some(&self.scope),
                )? {
                    Step::Ready(()) => self.sent = true,
                    Step::Wait(direction) => {
                        if channel.wait(direction, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                        continue;
                    }
                    Step::Control => return Ok(Step::Control),
                }
            }
            if let Some(upload) = self.local_upload.as_mut()
                && upload.is_requested()
            {
                match upload.advance(channel, budget)? {
                    Step::Ready(progress) => {
                        self.local_progress = Some(progress);
                        self.local_upload = None;
                    }
                    Step::Control => return Ok(Step::Control),
                    Step::Wait(direction) => {
                        if channel.wait(direction, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                        continue;
                    }
                }
            }
            let data = match channel.receive_step(&mut self.incoming, budget, Some(&self.scope))? {
                Step::Ready(data) => data,
                Step::Wait(direction) => {
                    if channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                    continue;
                }
                Step::Control => return Ok(Step::Control),
            };
            self.incoming = MessageReader::tracked(self.max_receive, &self.scope);
            // An unused permission belongs only to this command's first
            // response. Drop it on any other packet, before row parsing, so a
            // later text row beginning with a NULL byte cannot trigger upload.
            if let Some(mut upload) = self.local_upload.take() {
                if data.first() == Some(&0xfb) {
                    upload.accept_request()?;
                    self.local_upload = Some(upload);
                    return Ok(Step::Control);
                }
                self.local_progress = Some(upload.progress());
            }
            processed = processed.saturating_add(data.len());
            let delivery = if let Some(limit) = row_limit {
                self.collector
                    .push_stream(&data, limit.get())
                    .map(|event| event.map(Delivery::Streaming))
            } else {
                self.collector
                    .push(&data)
                    .map(|response| response.map(Delivery::Buffered))
            };
            // A parsed final OK/ERR is not trusted until its compression frame
            // has ended. This check also precedes exposing terminal facts on a
            // subsequent local Completion/diagnostic storage failure. Legal
            // intermediate MORE/legacy OUT sets and streaming rows have no
            // whole-command facts and continue under the same framing state.
            if self.collector.has_terminal_facts()
                && let Err(error) = channel.finish_response()
            {
                self.collector.invalidate_terminal_facts();
                return Err(error.into());
            }
            let delivery = delivery?;
            if let Some(delivery) = delivery {
                match &delivery {
                    Delivery::Buffered(response) => {
                        self.next_set =
                            text::has_more(response) || self.collector.legacy_out_has_tail();
                        self.finished = !self.next_set;
                    }
                    Delivery::Streaming(StreamEvent::End { completion, .. }) => {
                        self.next_set = self.collector.legacy_out_has_tail()
                            || completion.status_flags().contains(
                                mysql_common::constants::StatusFlags::SERVER_MORE_RESULTS_EXISTS,
                            );
                        self.finished = !self.next_set;
                    }
                    Delivery::Streaming(_) => (),
                }
                self.stream_boundary = self.streaming;
                return Ok(Step::Ready(delivery));
            }
            if processed >= 256 * 1024 {
                if let Some(rows) = self.collector.take_stream_rows()? {
                    self.stream_boundary = true;
                    return Ok(Step::Ready(Delivery::Streaming(StreamEvent::Rows(rows))));
                }
                return Ok(Step::Control);
            }
        }
    }
}
