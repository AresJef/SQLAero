//! One native command exchange over the existing channel and original budget.

use crate::protocol::framing::{MessageReader, MessageWriter, WireError};
use crate::result::text::{self, Receiver, Response, ResultError};
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use mysql_common::constants::CapabilityFlags;
use mysql_common::packets::OkPacket;
use std::ops::Range;

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
    /// All unexpected result sets were consumed; the channel is synchronized
    /// and its actual final confirmation is available for session updates.
    #[error("multiple results were fully consumed but are not supported by this client")]
    MultipleResults {
        /// Actual final OK/EOF metadata.
        completion: OkPacket<'static>,
    },
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
        if rows.start != self.source_rows || rows.end < rows.start {
            return Err(WireError::Malformed(
                "batch confirmation is not a contiguous source prefix",
            ));
        }
        let commands = self
            .commands
            .checked_add(1)
            .ok_or(WireError::Malformed("batch command count overflow"))?;
        let count = match response {
            Response::Command(ok) => u128::from(ok.affected_rows()),
            Response::Rows(result) => result.row_count as u128,
        };
        self.commands = commands;
        self.source_rows = rows.end;
        self.row_count += count;
        self.warnings += u128::from(response.completion().warnings());
        self.last_insert_id = response.completion().last_insert_id();
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
    memory_limit: usize,
    sent: bool,
    started: bool,
    unsupported_multiple: bool,
    finished: bool,
}

impl Query {
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
        if request.is_empty() {
            return Err(WireError::Malformed("request has no command byte").into());
        }
        let sender = MessageWriter::new(request.len(), max_send)?;
        Ok(Self {
            request,
            sender,
            incoming: MessageReader::new(max_receive),
            collector: Receiver::new(capabilities, memory_limit),
            capabilities,
            max_receive,
            memory_limit,
            sent: false,
            started: false,
            unsupported_multiple: false,
            finished: false,
        })
    }

    /// Advance the same command, retaining all framing state across waits and
    /// host checks. Unexpected additional results are consumed before rejection.
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

    fn advance_inner(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<Response>, QueryError> {
        if self.finished {
            return Err(WireError::Malformed("command exchange already ended").into());
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
            self.incoming = MessageReader::new(self.max_receive);
            processed = processed.saturating_add(data.len());
            if let Some(response) = self.collector.push(&data)? {
                if text::has_more(&response) {
                    self.unsupported_multiple = true;
                    self.collector = Receiver::new(self.capabilities, self.memory_limit);
                } else {
                    self.finished = true;
                    if self.unsupported_multiple {
                        return Err(QueryError::MultipleResults {
                            completion: response.into_completion(),
                        });
                    }
                    return Ok(Step::Ready(response));
                }
            }
            if processed >= 256 * 1024 {
                return Ok(Step::Control);
            }
        }
    }
}
