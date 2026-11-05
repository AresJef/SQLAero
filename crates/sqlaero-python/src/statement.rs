//! Prepared control ownership over the existing native channel and deadline.
//!
//! PREPARE is never target-SQL execution. CLOSE has no server acknowledgement.
//! Dropping a synchronized handle records only its native ID for a later safe
//! command boundary; dropping an unread response still retires its connection.

use crate::{connection::Connection, errors, input};
use pyo3::prelude::*;
use sqlaero_core::{
    error::ClientErrorCode,
    protocol::prepared::{Close, Prepare, Statement},
    session::{
        query::RequestProgress,
        shared::{CommandLease, Connection as NativeConnection},
    },
    transport::{channel::Step, deadline::IoBudget},
};
use std::sync::Arc;

/// Server preparation preserves these actual session semantics. Transaction
/// boundaries, result encoding, UTC value policy and limits are not a substitute
/// for schema/SQL-mode/collation confirmation and do not define SQL identity.
#[derive(PartialEq, Eq)]
pub(crate) struct Context {
    database: Option<Vec<u8>>,
    sql_mode: String,
    client_charset: String,
    connection_charset: String,
    collation: String,
}

impl Context {
    pub(crate) fn capture(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<Self> {
        let unknown = {
            let facts = connection.native.facts.lock().map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "Prepared context facts are unavailable",
                )
            })?;
            facts.needs_context() || facts.database.is_none()
        };
        if unknown {
            connection.confirm_session_state(py, lease, budget)?;
        }
        let facts = connection.native.facts.lock().map_err(|_| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Prepared context facts are unavailable",
            )
        })?;
        let missing = || {
            errors::client(
                py,
                ClientErrorCode::SessionState,
                "The current prepared statement's schema or lexical context is unconfirmed; no earlier context was substituted",
            )
        };
        Ok(Self {
            database: facts.database.as_ref().ok_or_else(missing)?.clone(),
            sql_mode: facts.sql_mode.as_ref().ok_or_else(missing)?.clone(),
            client_charset: facts.client_charset.as_ref().ok_or_else(missing)?.clone(),
            connection_charset: facts
                .connection_charset
                .as_ref()
                .ok_or_else(missing)?
                .clone(),
            collation: facts
                .connection_collation
                .as_ref()
                .ok_or_else(missing)?
                .clone(),
        })
    }
}

pub(crate) struct Handle {
    pub(crate) info: Statement,
    connection: Arc<NativeConnection>,
    armed: bool,
}

pub(crate) struct Cached {
    template: crate::binding::PreparedTemplate,
    context: Context,
    handle: Handle,
}

/// These actual server errors explicitly reject the current prepared handle
/// or its reprepare. Ordinary SQL errors do not erase a synchronized handle.
pub(crate) fn invalidates(error: &sqlaero_core::session::query::QueryError) -> bool {
    matches!(error, sqlaero_core::session::query::QueryError::Result(
        sqlaero_core::result::text::ResultError::Server(server))
        if matches!(server.error_code(), 1243 | 1615))
}

/// Resolve one Cursor's single current slot after complete local row encoding.
/// Replacement releases the old handle before preparing the new SQL. No retry
/// or text fallback occurs, including for unknown/invalidated server handles.
pub(crate) fn resolve(
    py: Python<'_>,
    connection: &Connection,
    lease: &mut CommandLease<'_>,
    template: crate::binding::PreparedTemplate,
    budget: &mut IoBudget,
    slot: &mut Option<Cached>,
) -> PyResult<u32> {
    let context = Context::capture(py, connection, lease, budget)?;
    if let Some(cached) = slot.as_ref()
        && cached.template.key == template.key
        && cached.template.parameters == template.parameters
        && (Arc::ptr_eq(&cached.template.request, &template.request)
            || cached.template.request.as_slice() == template.request.as_slice())
        && cached.context == context
    {
        return Ok(cached.handle.info.id);
    }
    // Complete fallible local request ownership before retiring the old slot.
    let mut request = Vec::new();
    request
        .try_reserve_exact(template.request.len())
        .map_err(|error| {
            errors::wire(
                py,
                sqlaero_core::protocol::framing::WireError::Allocation(error),
            )
        })?;
    request.extend_from_slice(&template.request);
    if let Some(old) = slot.take() {
        old.handle.close(py, connection, lease, budget)?;
    }
    let handle = prepare(py, connection, lease, request, budget)?;
    if handle.info.parameters != template.parameters {
        let error = errors::client(
            py,
            ClientErrorCode::ParameterCount,
            format!(
                "MySQL prepared {} parameter positions, but the safely bound template supplies {}; the target SQL was not executed",
                handle.info.parameters, template.parameters
            ),
        );
        if let Err(cleanup) = handle.close(py, connection, lease, budget) {
            errors::additional(py, &error, cleanup, "cursor.statement_close");
        }
        return Err(error);
    }
    let id = handle.info.id;
    *slot = Some(Cached {
        template,
        context,
        handle,
    });
    Ok(id)
}

impl Handle {
    pub(crate) fn close(
        mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        if !Arc::ptr_eq(&self.connection, &connection.native) {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "A prepared statement cannot be released on a different physical Connection",
            ));
        }
        let result = release_id(py, connection, lease, self.info.id, budget);
        // release_id retires on failure. No ID remains live in a reusable
        // connection, and Drop must not schedule a duplicate CLOSE.
        self.armed = false;
        result
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.armed {
            let _recorded_or_retired = self.connection.defer_statement(self.info.id);
        }
    }
}

pub(crate) fn prepare(
    py: Python<'_>,
    connection: &Connection,
    lease: &mut CommandLease<'_>,
    request: Vec<u8>,
    budget: &mut IoBudget,
) -> PyResult<Handle> {
    let result = (|| {
        input::check(py, budget)?;
        let limit = connection.result_limit;
        let session = lease.session().map_err(|error| errors::access(py, error))?;
        let mut job = Prepare::new(
            request,
            session.capabilities,
            session.max_send,
            session.max_receive,
            limit,
            Arc::clone(&connection.native.result_pool),
        )
        .map_err(|error| errors::query(py, error))?;
        loop {
            let session = lease.session().map_err(|error| errors::access(py, error))?;
            let step = py.detach(|| job.advance(&mut session.channel, budget));
            let error = match step {
                Ok(Step::Ready(info)) => {
                    let handle = Handle {
                        info,
                        connection: Arc::clone(&connection.native),
                        armed: true,
                    };
                    // The native handle exists before a fallible host checkpoint.
                    // If it fails, Drop records safe deferred release, not replay.
                    input::check(py, budget)?;
                    return Ok(handle);
                }
                Ok(Step::Control) => match input::check(py, budget) {
                    Ok(()) => continue,
                    Err(error) => error,
                },
                Ok(Step::Wait(_)) => errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The native PREPARE returned an undriven I/O wait",
                ),
                Err(error) => errors::query(py, error),
            };
            if job.received_server_err() {
                if let Some(id) = job.statement_id() {
                    let _recorded_or_retired = connection.native.defer_statement(id);
                }
            } else if job.progress() != RequestProgress::NotSent {
                lease.invalidate();
            }
            return Err(error);
        }
    })();
    result.map_err(|error| {
        errors::note(py,&error,
            "The target SQL was not executed: PREPARE or its local handoff failed before EXECUTE. No EXECUTE or replay was sent. Any newly allocated handle is safely scheduled for release or released with its retired physical session.");
        errors::at(py,error,"cursor.prepare")
    })
}

pub(crate) fn flush(
    py: Python<'_>,
    connection: &Connection,
    lease: &mut CommandLease<'_>,
    budget: &mut IoBudget,
) -> PyResult<()> {
    while let Some(id) = connection
        .native
        .take_deferred_statement(lease)
        .map_err(|error| errors::access(py, error))?
    {
        release_id(py, connection, lease, id, budget)?;
    }
    Ok(())
}

fn release_id(
    py: Python<'_>,
    connection: &Connection,
    lease: &mut CommandLease<'_>,
    id: u32,
    budget: &mut IoBudget,
) -> PyResult<()> {
    let result = (|| {
        input::check(py, budget)?;
        let session = lease.session().map_err(|error| errors::access(py, error))?;
        let scope = if session.channel.compression_options().is_some() {
            Some(
                sqlaero_core::memory::Scope::new(
                    Arc::clone(&connection.native.result_pool),
                    usize::MAX,
                )
                .map_err(|error| crate::output::memory(py, error))?,
            )
        } else {
            None
        };
        let mut job =
            Close::new(id, session.max_send, scope).map_err(|error| errors::wire(py, error))?;
        loop {
            let session = lease.session().map_err(|error| errors::access(py, error))?;
            match py.detach(|| job.advance(&mut session.channel, budget)) {
                Ok(Step::Ready(())) => return Ok(()),
                Ok(Step::Control) => input::check(py, budget)?,
                Ok(Step::Wait(_)) => {
                    return Err(errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "The native statement CLOSE returned an undriven I/O wait",
                    ));
                }
                Err(error) => return Err(errors::query(py, error)),
            }
        }
    })();
    if let Err(error) = result {
        lease.invalidate();
        errors::note(
            py,
            &error,
            "Releasing a prepared statement could not complete safely within the existing operation. The physical Connection was retired; the target SQL was not replayed. CLOSE has no server acknowledgement and is not a transaction commit.",
        );
        return Err(errors::at(py, error, "cursor.statement_close"));
    }
    if connection.native.is_closed() {
        return Err(errors::client(
            py,
            ClientErrorCode::SessionState,
            "The physical Connection was retired while releasing a prepared statement",
        ));
    }
    Ok(())
}
