//! Immediate command ownership and separately accessible confirmed local facts.

use super::connect::ServerMetadata;
use super::initialize::{Confirmed, Ready};
use crate::protocol::framing::{MessageWriter, WireError};
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use mysql_common::constants::CapabilityFlags;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};

/// The live protocol owner, accessible only under the connection command lease.
pub struct WireSession {
    /// Same owned socket, TLS and protocol sequence context.
    pub channel: Channel,
    /// Actual negotiated capabilities.
    pub capabilities: CapabilityFlags,
    /// Minimum confirmed server/client/protocol logical-send limit.
    pub max_send: usize,
    /// Client logical-receive limit.
    pub max_receive: usize,
}

/// A local ownership/lifecycle failure, mapped to the declared Interface family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AccessError {
    /// Another server command currently owns this connection.
    #[error("connection is busy executing another command")]
    Busy,
    /// The local connection was irreversibly disabled or closed.
    #[error("connection is closed or unusable")]
    Closed,
    /// An unexpected native panic invalidated protected protocol state.
    #[error("connection state was invalidated by a native failure")]
    Poisoned,
}

/// Shared native connection identity. The wire mutex is never used for a
/// different Cursor's buffered fetch or for immutable metadata reads.
pub struct Connection {
    wire: Mutex<Option<WireSession>>,
    closed: AtomicBool,
    /// Immutable received metadata, still available after close.
    pub metadata: ServerMetadata,
    /// One authoritative set of confirmed facts. Borrow briefly and release
    /// before invoking Python or waiting for network I/O.
    pub facts: Mutex<Confirmed>,
    /// Shared capacity ownership for all live native results and snapshots.
    pub result_pool: std::sync::Arc<crate::memory::Pool>,
}

impl Connection {
    /// Move a completely initialized session into its shared command owner.
    /// The connection-establishment budget is dropped, not reused for commands.
    #[must_use]
    pub fn new(ready: Ready) -> Self {
        Self::with_result_limit(ready, 512 * 1024 * 1024)
    }

    /// Keep one aggregate result limit for the connection's entire lifetime.
    #[must_use]
    pub fn with_result_limit(ready: Ready, total_result_memory_limit: usize) -> Self {
        let authenticated = ready.transport;
        let wire = WireSession {
            channel: authenticated.channel,
            capabilities: authenticated.capabilities,
            max_send: authenticated
                .max_send
                .min(ready.confirmed.server_packet_limit),
            max_receive: authenticated.max_receive,
        };
        Self {
            wire: Mutex::new(Some(wire)),
            closed: AtomicBool::new(false),
            metadata: authenticated.metadata,
            facts: Mutex::new(ready.confirmed),
            result_pool: crate::memory::Pool::new(total_result_memory_limit),
        }
    }

    /// Return the authoritative local lifecycle flag without network traffic.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Acquire the server-command owner immediately; never queue or wait.
    ///
    /// # Errors
    /// Returns Busy for a true overlapping command, Closed after invalidation,
    /// or Poisoned after disabling native state following an unexpected panic.
    pub fn try_command(&self) -> Result<CommandLease<'_>, AccessError> {
        let wire = match self.wire.try_lock() {
            Ok(wire) => wire,
            Err(TryLockError::WouldBlock) => return Err(AccessError::Busy),
            Err(TryLockError::Poisoned(error)) => {
                error.into_inner().take();
                self.closed.store(true, Ordering::Release);
                self.wire.clear_poison();
                return Err(AccessError::Poisoned);
            }
        };
        if self.is_closed() || wire.is_none() {
            return Err(AccessError::Closed);
        }
        Ok(CommandLease {
            wire,
            closed: &self.closed,
        })
    }
}

/// Scoped native execution ownership. Releasing it never performs network I/O.
pub struct CommandLease<'a> {
    wire: MutexGuard<'a, Option<WireSession>>,
    closed: &'a AtomicBool,
}

impl CommandLease<'_> {
    /// Borrow the live native protocol state under this exclusive lease.
    ///
    /// # Errors
    /// Returns Closed if this lease already invalidated the transport.
    pub fn session(&mut self) -> Result<&mut WireSession, AccessError> {
        self.wire.as_mut().ok_or(AccessError::Closed)
    }

    /// Begin irreversible local close while retaining the transport long enough
    /// for a separately bounded, explicit COM_QUIT attempt.
    pub fn begin_close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Disable and release the owned transport after protocol loss or close.
    /// Immutable metadata and confirmed local facts remain separately owned.
    pub fn invalidate(&mut self) {
        self.begin_close();
        self.wire.take();
    }
}

/// A close failure. The transport is retired even when the notification fails.
#[derive(Debug, thiserror::Error)]
pub enum CloseError {
    /// This command lease no longer owns a live transport.
    #[error(transparent)]
    Access(#[from] AccessError),
    /// The quit packet could not be prepared.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// Sending or flushing the quit notification failed.
    #[error(transparent)]
    Channel(#[from] ChannelError),
}

/// An explicit COM_QUIT send under the caller's existing close budget.
/// No response is expected. Dropping an interrupted job only releases locally;
/// it never grants a fresh network deadline or retries the notification.
pub struct Close<'a> {
    lease: CommandLease<'a>,
    sender: MessageWriter,
    finished: bool,
}

impl<'a> Close<'a> {
    /// Irreversibly enter the closing phase after acquiring command ownership.
    ///
    /// # Errors
    /// Returns unavailable transport or an invalid send-limit failure. Local
    /// retirement still takes place on every preparation failure.
    pub fn new(mut lease: CommandLease<'a>) -> Result<Self, CloseError> {
        lease.begin_close();
        let prepared = lease.session().map(|session| {
            session.channel.new_command();
            MessageWriter::new(1, session.max_send)
        });
        let sender = match prepared {
            Ok(Ok(sender)) => sender,
            Ok(Err(error)) => {
                lease.invalidate();
                return Err(error.into());
            }
            Err(error) => {
                lease.invalidate();
                return Err(error.into());
            }
        };
        Ok(Self {
            lease,
            sender,
            finished: false,
        })
    }

    /// Send and flush the notification, yielding only for host signal checks.
    /// The budget uses write/operation timeouts; no read is performed.
    ///
    /// # Errors
    /// Preserves the actual send, timeout or lifecycle failure after releasing
    /// the transport. A failed close can never make this connection reusable.
    pub fn advance(&mut self, budget: &mut IoBudget) -> Result<Step<()>, CloseError> {
        let result = self.advance_inner(budget);
        if result.is_err() || matches!(result, Ok(Step::Ready(()))) {
            self.lease.invalidate();
            self.finished = true;
        }
        result
    }

    fn advance_inner(&mut self, budget: &mut IoBudget) -> Result<Step<()>, CloseError> {
        if self.finished {
            return Err(AccessError::Closed.into());
        }
        loop {
            let session = self.lease.session()?;
            match session.channel.send_step(&mut self.sender, &[1], budget)? {
                Step::Wait(direction) => {
                    if session.channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                }
                other => return Ok(other),
            }
        }
    }
}

impl Drop for Close<'_> {
    fn drop(&mut self) {
        self.lease.invalidate();
    }
}
