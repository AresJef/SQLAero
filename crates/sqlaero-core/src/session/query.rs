//! One native command exchange over the existing channel and original budget.

use crate::memory::{Pool, Scope};
use crate::protocol::framing::{MessageReader, MessageWriter, WireError};
use crate::result::text::{self, Receiver, Response, ResultError};
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use mysql_common::constants::CapabilityFlags;
use std::ops::Range;
use std::sync::Arc;

/// A command failure whose server/transport/resource origin remains explicit.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
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
    max_receive: usize,
    scope: Arc<Scope>,
    sent: bool,
    started: bool,
    next_set: bool,
    discard: bool,
    memory_limit: usize,
    pool: Arc<Pool>,
    finished: bool,
}

impl Query {
    /// Whether the complete command ended, rather than just one result set.
    /// A validated server ERR also ends this exchange without inventing success.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
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
        let sender = MessageWriter::new(request.len(), max_send)?;
        let scope = Scope::new(Arc::clone(&pool), memory_limit).map_err(ResultError::from)?;
        let collector = Receiver::new(capabilities, Arc::clone(&scope), discard)?;
        Ok(Self {
            request,
            sender,
            incoming: MessageReader::tracked(max_receive, &scope),
            collector,
            capabilities,
            max_receive,
            scope,
            sent: false,
            started: false,
            next_set: false,
            discard,
            memory_limit,
            pool,
            finished: false,
        })
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
        let result = self.advance_inner(channel, budget);
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
    ) -> Result<Step<Response>, QueryError> {
        if self.finished {
            return Err(WireError::Malformed("command exchange already ended").into());
        }
        if self.next_set {
            // Every set retains its own per-result quota; earlier owners stay
            // charged to the same connection pool. Prepare the next collector
            // only after the previous complete set was returned to its owner.
            let scope =
                Scope::new(Arc::clone(&self.pool), self.memory_limit).map_err(ResultError::from)?;
            let collector = Receiver::new(self.capabilities, Arc::clone(&scope), self.discard)?;
            self.incoming = MessageReader::tracked(self.max_receive, &scope);
            self.collector = collector;
            self.scope = scope;
            self.next_set = false;
        }
        let mut processed = 0usize;
        if !self.started {
            channel.new_command();
            self.started = true;
        }
        loop {
            if !self.sent {
                match channel.send_step(&mut self.sender, &self.request, budget)? {
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
            let data = match channel.receive_step(&mut self.incoming, budget)? {
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
            processed = processed.saturating_add(data.len());
            if let Some(response) = self.collector.push(&data)? {
                self.next_set = text::has_more(&response);
                self.finished = !self.next_set;
                return Ok(Step::Ready(response));
            }
            if processed >= 256 * 1024 {
                return Ok(Step::Control);
            }
        }
    }
}
