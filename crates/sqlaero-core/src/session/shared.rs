//! Immediate command ownership and separately accessible confirmed local facts.

use super::connect::ServerMetadata;
use super::initialize::Ready;
use super::state::{ExecutionContext, Facts};
use crate::memory::{Pool, Scope};
use crate::protocol::framing::{MessageWriter, WireError};
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use mysql_common::constants::CapabilityFlags;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

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
    owner: Option<Arc<()>>,
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
    pub facts: Mutex<Facts>,
    reference: Mutex<Option<ExecutionContext>>,
    statement_releases: Mutex<Vec<u32>>,
    releases_pending: AtomicBool,
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
        Self::with_retained_budget(ready, total_result_memory_limit).0
    }

    /// Move the ready transport into its native owner while retaining the
    /// original establishment deadline for approved user initialization SQL.
    /// No timeout is restarted; all earlier establishment work remains charged.
    #[must_use]
    pub fn with_retained_budget(
        ready: Ready,
        total_result_memory_limit: usize,
    ) -> (Self, crate::transport::deadline::IoBudget) {
        let authenticated = ready.transport;
        let budget = authenticated.budget;
        let wire = WireSession {
            channel: authenticated.channel,
            capabilities: authenticated.capabilities,
            max_send: authenticated
                .max_send
                .min(ready.confirmed.server_packet_limit),
            max_receive: authenticated.max_receive,
            owner: None,
        };
        let mut facts: Facts = ready.confirmed.into();
        facts.multi_statements = Some(
            authenticated
                .capabilities
                .contains(crate::protocol::CapabilityFlags::CLIENT_MULTI_STATEMENTS),
        );
        let reference = facts.execution_context(wire.max_send).ok();
        (
            Self {
                wire: Mutex::new(Some(wire)),
                closed: AtomicBool::new(false),
                metadata: authenticated.metadata,
                facts: Mutex::new(facts),
                reference: Mutex::new(reference),
                statement_releases: Mutex::new(Vec::new()),
                releases_pending: AtomicBool::new(false),
                result_pool: crate::memory::Pool::new(total_result_memory_limit),
            },
            budget,
        )
    }

    /// Return the authoritative local lifecycle flag without network traffic.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Register an owned, synchronized statement for its next safe release.
    /// This never performs I/O or retains a Cursor, SQL or host object. On a
    /// real allocation/poisoning failure the physical session is retired, so an
    /// unrecorded server handle cannot survive in a reusable connection.
    #[must_use]
    pub fn defer_statement(&self, id: u32) -> bool {
        if self.is_closed() {
            return true;
        }
        let Ok(mut pending) = self.statement_releases.lock() else {
            self.retire();
            return false;
        };
        if pending.try_reserve(1).is_err() {
            self.retire();
            return false;
        }
        pending.push(id);
        self.releases_pending.store(true, Ordering::Release);
        true
    }

    /// Take one recorded handle only under this same physical connection's
    /// synchronized command boundary. No caller may insert CLOSE into another
    /// response. The empty path avoids acquiring the resource-list mutex.
    ///
    /// # Errors
    /// Rejects a foreign/unread lease or poisoned release storage.
    pub fn take_deferred_statement(
        &self,
        lease: &CommandLease<'_>,
    ) -> Result<Option<u32>, AccessError> {
        if !std::ptr::eq(lease.closed, &self.closed) || lease.has_owned_response() {
            return Err(AccessError::Busy);
        }
        if !self.releases_pending.load(Ordering::Acquire) {
            return Ok(None);
        }
        let mut pending = self.statement_releases.lock().map_err(|_| {
            self.retire();
            AccessError::Poisoned
        })?;
        let id = pending.pop();
        if pending.is_empty() {
            // Release the actual allocation once no IDs remain; GC does not
            // turn this thin resource list into retained lifetime history.
            *pending = Vec::new();
            self.releases_pending.store(false, Ordering::Release);
        }
        Ok(id)
    }

    /// Copy the last completely confirmed auxiliary reference, even after
    /// close or current-fact invalidation. It grants no command authority.
    ///
    /// # Errors
    /// Returns a native diagnostic-cache poisoning failure.
    pub fn auxiliary_reference(&self) -> Result<Option<ExecutionContext>, AccessError> {
        self.reference
            .lock()
            .map(|reference| *reference)
            .map_err(|_| AccessError::Poisoned)
    }

    /// Save a complete context derived from confirmed facts under command
    /// ownership. Missing current facts must not call this with a guessed value.
    ///
    /// # Errors
    /// Returns a real protected-state poisoning failure; no guessed context is
    /// published. Callers must never use this reference to repair live facts.
    pub fn remember_context(&self, context: ExecutionContext) -> Result<(), AccessError> {
        *self.reference.lock().map_err(|_| AccessError::Poisoned)? = Some(context);
        Ok(())
    }

    /// Acquire the server-command owner immediately; never queue or wait.
    ///
    /// # Errors
    /// Returns Busy for a true overlapping command, Closed after invalidation,
    /// or Poisoned after disabling native state following an unexpected panic.
    pub fn try_command(&self) -> Result<CommandLease<'_>, AccessError> {
        self.try_lease(None, false)
    }

    /// Acquire only an immediately available close boundary. An idle persistent
    /// response owner may be retired without draining; an actual overlapping
    /// I/O lease still returns Busy. Inspect `has_owned_response` before I/O.
    ///
    /// # Errors
    /// Returns the same closed, poisoned or immediate-overlap failures.
    pub fn try_close_command(&self) -> Result<CommandLease<'_>, AccessError> {
        self.try_lease(None, true)
    }

    /// Reserve one command across host calls without retaining a mutex guard.
    /// Only this owner can resume the wire until its response is synchronized.
    ///
    /// # Errors
    /// Returns the same immediate availability errors as `try_command`.
    pub fn try_owned(self: &Arc<Self>) -> Result<OwnedCommand, AccessError> {
        let mut lease = self.try_command()?;
        self.retain_command(&mut lease)
    }

    /// Retain an already acquired command at its real streaming send handoff,
    /// without releasing/reacquiring the wire or retiring on local preflight.
    ///
    /// # Errors
    /// Rejects a foreign connection lease or an existing response owner.
    pub fn retain_command(
        self: &Arc<Self>,
        lease: &mut CommandLease<'_>,
    ) -> Result<OwnedCommand, AccessError> {
        if !std::ptr::eq(lease.closed, &self.closed) {
            return Err(AccessError::Busy);
        }
        if lease.session()?.owner.is_some() {
            return Err(AccessError::Busy);
        }
        let key = Arc::new(());
        lease.session()?.owner = Some(Arc::clone(&key));
        Ok(OwnedCommand {
            connection: Arc::clone(self),
            key,
            active: AtomicBool::new(true),
        })
    }

    /// Irreversibly retire without waiting or performing network cleanup.
    /// An in-flight lease releases its transport at its next safe exit.
    pub fn retire(&self) {
        self.closed.store(true, Ordering::Release);
        match self.wire.try_lock() {
            Ok(mut wire) => {
                wire.take();
            }
            Err(TryLockError::Poisoned(error)) => {
                error.into_inner().take();
                self.wire.clear_poison();
            }
            Err(TryLockError::WouldBlock) => (),
        }
    }

    fn try_lease(
        &self,
        owner: Option<&Arc<()>>,
        closing: bool,
    ) -> Result<CommandLease<'_>, AccessError> {
        if self.is_closed() {
            return Err(AccessError::Closed);
        }
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
        let actual = wire.as_ref().and_then(|session| session.owner.as_ref());
        let matches = match (owner, actual) {
            (None, _) if closing => true,
            (None, None) => true,
            (Some(expected), Some(actual)) => Arc::ptr_eq(expected, actual),
            _ => false,
        };
        if !matches {
            return Err(AccessError::Busy);
        }
        Ok(CommandLease {
            wire,
            closed: &self.closed,
            pool: &self.result_pool,
        })
    }
}

/// Native identity for an unfinished command. It owns no Python reference and
/// no lock guard, so a later host call may safely resume it on another thread.
/// Dropping an unfinished owner retires the connection locally, without I/O.
pub struct OwnedCommand {
    connection: Arc<Connection>,
    key: Arc<()>,
    active: AtomicBool,
}

impl OwnedCommand {
    /// Obtain temporary exclusive protection for this exact response owner.
    ///
    /// # Errors
    /// Returns Busy during a concurrent operation, or Closed after completion
    /// or connection retirement. No operation waits for another host call.
    pub fn resume(&self) -> Result<CommandLease<'_>, AccessError> {
        if !self.active.load(Ordering::Acquire) {
            return Err(AccessError::Closed);
        }
        self.connection.try_lease(Some(&self.key), false)
    }
}

impl Drop for OwnedCommand {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            self.connection.retire();
        }
    }
}

/// Scoped native execution ownership. Releasing it never performs network I/O.
pub struct CommandLease<'a> {
    wire: MutexGuard<'a, Option<WireSession>>,
    closed: &'a AtomicBool,
    pool: &'a Arc<Pool>,
}

impl CommandLease<'_> {
    /// Whether closing must retire an unread response without COM_QUIT or
    /// further reads. Call only while this exclusive close lease is held.
    #[must_use]
    pub fn has_owned_response(&self) -> bool {
        self.wire
            .as_ref()
            .is_some_and(|session| session.owner.is_some())
    }
    /// Release persistent ownership only after the caller has consumed the
    /// command's complete response, including every result and internal tail.
    ///
    /// # Errors
    /// Returns Closed after retirement, or Busy for a different command owner.
    /// This does not infer synchronization from the presence of buffered rows.
    pub fn finish_owned(&mut self, owner: &OwnedCommand) -> Result<(), AccessError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(AccessError::Closed);
        }
        let session = self.session()?;
        if !session
            .owner
            .as_ref()
            .is_some_and(|key| Arc::ptr_eq(key, &owner.key))
        {
            return Err(AccessError::Busy);
        }
        session.owner = None;
        owner.active.store(false, Ordering::Release);
        Ok(())
    }

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

impl Drop for CommandLease<'_> {
    fn drop(&mut self) {
        if self.closed.load(Ordering::Acquire) {
            self.wire.take();
        }
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
    scope: Option<Arc<Scope>>,
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
        let pool = lease.pool;
        let prepared = (|| -> Result<_, CloseError> {
            let session = lease.session()?;
            session.channel.new_command()?;
            let scope = if session.channel.is_compressed() {
                Some(Scope::new(Arc::clone(pool), usize::MAX).map_err(WireError::from)?)
            } else {
                None
            };
            Ok((MessageWriter::new(1, session.max_send)?, scope))
        })();
        let (sender, scope) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                lease.invalidate();
                return Err(error);
            }
        };
        Ok(Self {
            lease,
            sender,
            scope,
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
        let result = self.io().and_then(|mut io| io.advance(budget));
        if result.is_err() || matches!(result, Ok(Step::Ready(()))) {
            self.lease.invalidate();
            self.finished = true;
        }
        result
    }

    /// Borrow only native transfer fields for an off-GIL IO segment. The
    /// command lease and its non-Send mutex guard remain on the caller thread.
    /// Dropping this borrow does not release the enclosing retirement owner.
    ///
    /// # Errors
    /// Returns Closed after completion or retirement of the actual transport.
    pub fn io(&mut self) -> Result<CloseIo<'_>, CloseError> {
        if self.finished {
            return Err(AccessError::Closed.into());
        }
        Ok(CloseIo {
            channel: &mut self.lease.session()?.channel,
            sender: &mut self.sender,
            scope: self.scope.as_ref(),
        })
    }
}

/// Transferable close IO with no mutex guard or host object. The enclosing
/// Close retains the actual command and always retires the transport on drop.
pub struct CloseIo<'a> {
    channel: &'a mut Channel,
    sender: &'a mut MessageWriter,
    scope: Option<&'a Arc<Scope>>,
}

impl CloseIo<'_> {
    /// Advance the shared COM_QUIT without a response or fresh timeout budget.
    ///
    /// # Errors
    /// Returns the original transport, framing or deadline failure.
    pub fn advance(&mut self, budget: &mut IoBudget) -> Result<Step<()>, CloseError> {
        loop {
            match self
                .channel
                .send_step(self.sender, &[1], budget, self.scope)?
            {
                Step::Wait(direction) => {
                    if self.channel.wait(direction, budget)? == WaitStatus::Control {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::auth::AuthPath;
    use crate::session::initialize::Confirmed;
    use crate::transport::socket::Transport;
    use mysql_common::constants::StatusFlags;
    use std::net::TcpListener;
    use std::sync::Arc;

    #[test]
    fn deferred_statement_ids_are_local_and_require_a_safe_matching_boundary() {
        let connection = connection();
        let other = self::connection();
        assert!(connection.defer_statement(17));
        assert!(connection.defer_statement(29));
        let foreign = other.try_command().unwrap();
        assert_eq!(
            connection.take_deferred_statement(&foreign),
            Err(AccessError::Busy)
        );
        let owner = connection.try_owned().unwrap();
        let owned = owner.resume().unwrap();
        assert_eq!(
            connection.take_deferred_statement(&owned),
            Err(AccessError::Busy)
        );
        drop(owned);
        let mut owned = owner.resume().unwrap();
        owned.finish_owned(&owner).unwrap();
        assert_eq!(connection.take_deferred_statement(&owned), Ok(Some(29)));
        assert_eq!(connection.take_deferred_statement(&owned), Ok(Some(17)));
        assert_eq!(connection.take_deferred_statement(&owned), Ok(None));
        assert_eq!(connection.statement_releases.lock().unwrap().capacity(), 0);
        assert!(!connection.releases_pending.load(Ordering::Acquire));
        drop(owned);
        assert!(!connection.is_closed());
    }

    #[test]
    fn closed_statement_registration_needs_no_retained_resource_record() {
        let connection = connection();
        connection.retire();
        assert!(connection.defer_statement(9));
        assert!(connection.statement_releases.lock().unwrap().is_empty());
    }

    #[test]
    fn idle_unread_close_owns_the_actual_boundary_without_draining() {
        let connection = connection();
        let response = connection.try_owned().unwrap();
        let mut close = connection.try_close_command().unwrap();
        assert!(close.has_owned_response());
        close.invalidate();
        drop(close);
        assert!(connection.is_closed());
        assert!(matches!(response.resume(), Err(AccessError::Closed)));
    }

    #[test]
    fn actual_response_operation_keeps_close_immediately_busy() {
        let connection = connection();
        let response = connection.try_owned().unwrap();
        let mut active = response.resume().unwrap();
        assert!(matches!(
            connection.try_close_command(),
            Err(AccessError::Busy)
        ));
        active.finish_owned(&response).unwrap();
        drop(active);
        let close = connection.try_close_command().unwrap();
        assert!(!close.has_owned_response());
        drop(close);
        assert!(!connection.is_closed());
    }

    #[test]
    fn persistent_handoff_rejects_a_foreign_connection_lease() {
        let first = connection();
        let second = connection();
        let mut lease = first.try_command().unwrap();
        assert!(matches!(
            second.retain_command(&mut lease),
            Err(AccessError::Busy)
        ));
        assert!(!lease.has_owned_response());
        let response = first.retain_command(&mut lease).unwrap();
        lease.finish_owned(&response).unwrap();
    }

    fn connection() -> Arc<Connection> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let channel = Channel::new(
            Transport::tcp(listener.local_addr().unwrap()).unwrap(),
            false,
        );
        Arc::new(Connection {
            wire: Mutex::new(Some(WireSession {
                channel,
                capabilities: CapabilityFlags::empty(),
                max_send: 1024,
                max_receive: 1024,
                owner: None,
            })),
            closed: AtomicBool::new(false),
            metadata: ServerMetadata {
                version: "8.4.0".into(),
                version_tuple: (8, 4, 0),
                connection_id: 1,
                status: StatusFlags::empty(),
                auth_path: AuthPath::Fast,
            },
            reference: Mutex::new(None),
            statement_releases: Mutex::new(Vec::new()),
            releases_pending: AtomicBool::new(false),
            facts: Mutex::new(
                Confirmed {
                    status: StatusFlags::empty(),
                    database: None,
                    time_zone: "+00:00".into(),
                    sql_mode: String::new(),
                    encoding: crate::charset::resolve(None, None).unwrap(),
                    server_timeouts: [1; 4],
                    server_packet_limit: 1024,
                }
                .into(),
            ),
            result_pool: crate::memory::Pool::new(4096),
        })
    }

    #[test]
    fn persistent_owner_blocks_commands_between_calls_and_moves_across_threads() {
        let connection = connection();
        let owner = connection.try_owned().unwrap();
        assert!(matches!(connection.try_command(), Err(AccessError::Busy)));
        {
            let _lease = owner.resume().unwrap();
            assert!(matches!(owner.resume(), Err(AccessError::Busy)));
        }
        assert!(matches!(connection.try_command(), Err(AccessError::Busy)));
        std::thread::scope(|threads| {
            threads
                .spawn(|| {
                    assert_eq!(
                        connection.facts.lock().unwrap().time_zone.as_deref(),
                        Some("+00:00")
                    );
                    let mut lease = owner.resume().unwrap();
                    lease.finish_owned(&owner).unwrap();
                })
                .join()
                .unwrap();
        });
        assert!(matches!(owner.resume(), Err(AccessError::Closed)));
        assert!(connection.try_command().is_ok());
        drop(owner);
        assert!(
            !connection.is_closed(),
            "a completed owner must not retire the wire"
        );
    }

    #[test]
    fn lost_reader_retires_without_network_and_inflight_lease_releases_wire() {
        let connection = connection();
        let owner = connection.try_owned().unwrap();
        drop(owner);
        assert!(connection.is_closed());
        assert!(connection.wire.lock().unwrap().is_none());

        let connection = self::connection();
        let owner = connection.try_owned().unwrap();
        let lease = owner.resume().unwrap();
        // A concurrent Connection.close cannot wait for the live operation or
        // send another command over its incomplete response.
        connection.retire();
        assert!(connection.is_closed());
        drop(lease);
        assert!(connection.wire.lock().unwrap().is_none());
        drop(owner);
    }

    #[test]
    fn auxiliary_reference_is_independent_of_invalid_live_facts_and_close() {
        let connection = connection();
        let original = connection
            .facts
            .lock()
            .unwrap()
            .execution_context(1024)
            .unwrap();
        connection.remember_context(original).unwrap();
        assert!(original.binding.utc_confirmed);
        connection.facts.lock().unwrap().invalidate_unconfirmed();
        assert!(
            connection
                .facts
                .lock()
                .unwrap()
                .execution_context(1024)
                .is_err()
        );
        connection.retire();
        let saved = connection.auxiliary_reference().unwrap().unwrap();
        assert_eq!(saved.binding.codec.name(), original.binding.codec.name());
        assert_eq!(saved.binding.limit, 1024);
        assert!(saved.binding.utc_confirmed);
        assert!(
            connection.facts.lock().unwrap().needs_context(),
            "reading the reference must not restore authority"
        );
    }
}
