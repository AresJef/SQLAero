//! Cooperative prepared control commands over the one shared channel.

use super::{PrepareReceiver, Statement};
use crate::memory::{Pool, Reservation, Scope};
use crate::protocol::CapabilityFlags;
use crate::protocol::framing::{MessageReader, MessageWriter, WireError};
use crate::result::text::ResultError;
use crate::session::query::{QueryError, RequestProgress};
use crate::transport::channel::{Channel, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use std::mem::size_of;
use std::sync::Arc;

/// One PREPARE request and its complete metadata acknowledgement.
pub struct Prepare {
    request: Vec<u8>,
    sender: MessageWriter,
    incoming: MessageReader,
    receiver: PrepareReceiver,
    scope: Arc<Scope>,
    max_receive: usize,
    started: bool,
    sent: bool,
    finished: bool,
    _owner: Reservation,
}

impl Prepare {
    /// Prepare a complete request beginning with COM_STMT_PREPARE (0x16).
    /// The result scope belongs to the existing connection allocation pool.
    ///
    /// # Errors
    /// Returns local command, request-limit or actual allocation failures.
    pub fn new(
        request: Vec<u8>,
        capabilities: CapabilityFlags,
        max_send: usize,
        max_receive: usize,
        memory_limit: usize,
        pool: Arc<Pool>,
    ) -> Result<Self, QueryError> {
        if request.first() != Some(&0x16) {
            return Err(WireError::Malformed("expected COM_STMT_PREPARE request").into());
        }
        let sender = MessageWriter::new(request.len(), max_send)?;
        let scope = Scope::new(pool, memory_limit).map_err(ResultError::from)?;
        let owner = scope.charge(size_of::<Self>()).map_err(ResultError::from)?;
        let receiver = PrepareReceiver::new(capabilities, Arc::clone(&scope));
        Ok(Self {
            request,
            sender,
            incoming: MessageReader::tracked(max_receive, &scope),
            receiver,
            scope,
            max_receive,
            started: false,
            sent: false,
            finished: false,
            _owner: owner,
        })
    }

    /// Actual request transfer facts; preparing is not execution of target SQL.
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

    /// A provisional ID if the server allocated it before a later failure.
    #[must_use]
    pub fn statement_id(&self) -> Option<u32> {
        self.receiver.statement_id()
    }

    /// Whether a complete server ERR was received, independently of its storage.
    #[must_use]
    pub fn received_server_err(&self) -> bool {
        self.receiver.received_server_err()
    }

    /// Whether the control exchange has ended, including a terminal failure.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Advance bounded protocol work using the original command's remaining
    /// deadline. Every Control return lets the boundary check host signals.
    ///
    /// # Errors
    /// Returns the shared server/wire/resource/channel error without retrying.
    pub fn advance(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<Statement>, QueryError> {
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
    ) -> Result<Step<Statement>, QueryError> {
        if self.finished {
            return Err(WireError::Malformed("prepare exchange already ended").into());
        }
        if !self.started {
            channel.new_command();
            self.started = true;
        }
        let mut processed = 0_usize;
        loop {
            if !self.sent {
                match channel.send_step(&mut self.sender, &self.request, budget)? {
                    Step::Ready(()) => self.sent = true,
                    Step::Control => return Ok(Step::Control),
                    Step::Wait(direction) => {
                        if channel.wait(direction, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                        continue;
                    }
                }
            }
            let packet = match channel.receive_step(&mut self.incoming, budget)? {
                Step::Ready(packet) => packet,
                Step::Control => return Ok(Step::Control),
                Step::Wait(direction) => {
                    if channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                    continue;
                }
            };
            self.incoming = MessageReader::tracked(self.max_receive, &self.scope);
            processed = processed.saturating_add(packet.len());
            if let Some(statement) = self.receiver.push(&packet)? {
                self.finished = true;
                return Ok(Step::Ready(statement));
            }
            if processed >= 256 * 1024 {
                return Ok(Step::Control);
            }
        }
    }
}

/// One no-response COM_STMT_CLOSE command. Completion confirms only that all
/// request bytes were flushed; MySQL sends no release acknowledgement.
pub struct Close {
    request: [u8; 5],
    sender: MessageWriter,
    started: bool,
    finished: bool,
}

impl Close {
    /// Prepare the current connection's explicitly owned statement handle.
    ///
    /// # Errors
    /// Returns a send-limit failure before a channel is touched.
    pub fn new(statement_id: u32, max_send: usize) -> Result<Self, WireError> {
        let mut request = [0x19, 0, 0, 0, 0];
        request[1..].copy_from_slice(&statement_id.to_le_bytes());
        Ok(Self {
            request,
            sender: MessageWriter::new(5, max_send)?,
            started: false,
            finished: false,
        })
    }

    /// Actual transfer entry, without claiming that the peer applied CLOSE.
    #[must_use]
    pub fn has_started(&self) -> bool {
        self.sender.has_started()
    }

    /// Advance one no-response command at an already safe command boundary.
    ///
    /// # Errors
    /// Preserves real send/channel/timeout errors; never receives a fake OK.
    pub fn advance(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<()>, QueryError> {
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
    ) -> Result<Step<()>, QueryError> {
        if self.finished {
            return Err(WireError::Malformed("close exchange already ended").into());
        }
        if !self.started {
            channel.new_command();
            self.started = true;
        }
        loop {
            match channel.send_step(&mut self.sender, &self.request, budget)? {
                Step::Ready(()) => {
                    self.finished = true;
                    return Ok(Step::Ready(()));
                }
                Step::Control => return Ok(Step::Control),
                Step::Wait(direction) => {
                    if channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                }
            }
        }
    }
}
