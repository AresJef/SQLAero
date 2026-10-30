//! One managed transaction scope without holding the wire during user code.

use crate::{
    connection::{Connection, ControlAttempt},
    cursor::Cursor,
    errors, input,
};
use pyo3::prelude::*;
use sqlaero_core::{
    error::ClientErrorCode, protocol::StatusFlags, session::shared::CommandLease,
    transport::deadline::IoBudget,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Internal context manager returned by Connection.transaction(). It owns only
/// its local scope and newly created Cursor, never the user's other Cursors.
#[pyclass(frozen, name = "_Transaction", module = "sqlaero._sqlaero")]
pub(crate) struct Transaction {
    connection: Py<Connection>,
    scope: Arc<AtomicBool>,
    owns: AtomicBool,
    entered: AtomicBool,
    finished: AtomicBool,
    active: AtomicBool,
    cursor: Mutex<Option<Py<Cursor>>>,
}
struct Operation<'a>(&'a AtomicBool);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Transaction {
    pub(crate) fn new(connection: Py<Connection>, scope: Arc<AtomicBool>) -> Self {
        Self {
            connection,
            scope,
            owns: AtomicBool::new(false),
            entered: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            active: AtomicBool::new(false),
            cursor: Mutex::new(None),
        }
    }
    fn operation(&self, py: Python<'_>) -> PyResult<Operation<'_>> {
        if self
            .active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(errors::client(
                py,
                ClientErrorCode::InterfaceBusy,
                "Transaction context is busy entering or exiting another operation",
            ));
        }
        Ok(Operation(&self.active))
    }
    fn release(&self) {
        if self.owns.swap(false, Ordering::AcqRel) {
            self.scope.store(false, Ordering::Release);
        }
    }
    fn note(py: Python<'_>, primary: &PyErr, message: impl Into<String>) {
        errors::note(py, primary, message);
    }
    fn rollback_after_error(
        connection: &Connection,
        py: Python<'_>,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
        primary: &PyErr,
    ) {
        if connection.native.is_closed() {
            Self::note(
                py,
                primary,
                "ROLLBACK was not attempted because the connection is retired; closing does not confirm a rollback.",
            );
            return;
        }
        if budget.check().is_err() {
            Self::note(
                py,
                primary,
                "ROLLBACK was not attempted because the existing operation deadline has expired; the unsafe connection was retired without a new cleanup timeout.",
            );
            lease.invalidate();
            return;
        }
        let mut attempt = ControlAttempt::default();
        let result = connection.control_recorded(
            py,
            lease,
            b"ROLLBACK AND NO CHAIN NO RELEASE",
            budget,
            &mut attempt,
        );
        if attempt.confirmed {
            Self::note(
                py,
                primary,
                "ROLLBACK of the current transaction was confirmed; earlier commits and nontransactional effects are not undone.",
            );
        }
        if let Err(error) = result {
            errors::additional(py, primary, error, "transaction.rollback");
            if !attempt.confirmed {
                lease.invalidate();
            }
        }
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        self.release();
    }
}
#[pymethods]
impl Transaction {
    fn __enter__(&self, py: Python<'_>) -> PyResult<Py<Cursor>> {
        let _operation = self.operation(py)?;
        if self.entered.swap(true, Ordering::AcqRel) {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "This transaction context cannot be entered more than once",
            ));
        }
        let connection = self.connection.borrow(py);
        connection.open(py)?;
        if self
            .scope
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Connection already owns a managed transaction scope; no BEGIN was sent",
            ));
        }
        self.owns.store(true, Ordering::Release);
        let result = (|| {
            let mut budget = connection.runtime(py)?.timeouts.operation();
            let mut lease = connection
                .native
                .try_command()
                .map_err(|error| errors::access(py, error))?;
            let mut status = connection
                .native
                .facts
                .lock()
                .map_err(|_| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "confirmed transaction state is unavailable",
                    )
                })?
                .status;
            if status.is_none() {
                // One necessary read obtains actual protocol status. A constant
                // SELECT does not access a transactional table or replace any
                // user's Cursor result, and shares the original entry budget.
                connection.control_with(py, &mut lease, b"SELECT 1", &mut budget)?;
                status = connection
                    .native
                    .facts
                    .lock()
                    .map_err(|_| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "confirmed transaction state is unavailable",
                        )
                    })?
                    .status;
            }
            let status = status.ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "The actual transaction state is unknown; no BEGIN was sent",
                )
            })?;
            if status.contains(StatusFlags::SERVER_STATUS_IN_TRANS) {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "Connection already has an active server transaction; no BEGIN was sent",
                ));
            }
            let mut begin = ControlAttempt::default();
            let entered = (|| {
                connection.control_recorded(
                    py,
                    &mut lease,
                    b"START TRANSACTION",
                    &mut budget,
                    &mut begin,
                )?;
                let options = connection.runtime(py)?.options;
                let cursor = Py::new(
                    py,
                    Cursor::new(
                        self.connection.clone_ref(py),
                        connection.types.clone_ref(py),
                        options,
                    ),
                )?;
                input::check(py, &budget)?;
                *self.cursor.lock().map_err(|_| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "transaction cursor ownership is unavailable",
                    )
                })? = Some(cursor.clone_ref(py));
                Ok(cursor)
            })();
            if let Err(error) = &entered {
                if begin.confirmed {
                    Self::rollback_after_error(&connection, py, &mut lease, &mut budget, error);
                } else if begin.started && connection.native.is_closed() {
                    Self::note(
                        py,
                        error,
                        "BEGIN entered sending but no complete confirmation was received; transaction start is unknown, and the connection was retired.",
                    );
                }
            }
            entered
        })();
        if result.is_err() {
            self.release();
        }
        result.map_err(|error| errors::at(py, error, "transaction.enter"))
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        _typ: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let _operation = self.operation(py)?;
        if !self.owns.load(Ordering::Acquire) || self.finished.swap(true, Ordering::AcqRel) {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "This transaction context has no active entry to exit",
            ));
        }
        let connection = self.connection.borrow(py);
        let cursor = self
            .cursor
            .lock()
            .map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "transaction cursor ownership is unavailable",
                )
            })?
            .take();
        let business = (!value.is_none()).then(|| PyErr::from_value(value.clone()));
        let mut primary = business.as_ref().map(|error| error.clone_ref(py));
        let mut commit_confirmed = false;
        let mut acquired = false;
        let mut unattempted = None;
        let mut cleanup_detail = String::new();
        let finish = (|| {
            let mut budget = connection.runtime(py)?.timeouts.operation();
            let mut lease = connection
                .native
                .try_command()
                .map_err(|error| errors::access(py, error))?;
            acquired = true;
            if let Some(primary) = &primary {
                Self::rollback_after_error(&connection, py, &mut lease, &mut budget, primary);
            } else {
                let mut commit = ControlAttempt::default();
                let result = connection.control_recorded(
                    py,
                    &mut lease,
                    b"COMMIT AND NO CHAIN NO RELEASE",
                    &mut budget,
                    &mut commit,
                );
                commit_confirmed = commit.confirmed;
                if let Err(error) = result {
                    let error = errors::at(py, error, "transaction.commit");
                    if commit.confirmed {
                        Self::note(
                            py,
                            &error,
                            "COMMIT was fully confirmed; the later local failure does not make its outcome unknown. No rollback or repeated commit was sent.",
                        );
                    } else if !connection.native.is_closed() {
                        Self::rollback_after_error(
                            &connection,
                            py,
                            &mut lease,
                            &mut budget,
                            &error,
                        );
                    } else {
                        Self::note(
                            py,
                            &error,
                            "COMMIT entered sending without complete confirmation; commit outcome is unknown. No replay or rollback was sent on the retired connection.",
                        );
                    }
                    return Err(error);
                }
            }
            Ok(())
        })();
        if let Err(error) = finish {
            if !acquired {
                unattempted = Some(error.to_string());
            }
            if let Some(existing) = &primary {
                if acquired {
                    errors::additional(py, existing, error, "transaction.exit");
                } else {
                    errors::additional_detail(py, existing, error, "transaction.exit", None);
                }
            } else {
                primary = Some(errors::at(py, error, "transaction.exit"));
            }
        }
        if let Some(cursor) = cursor
            && let Err(error) = cursor.borrow(py).close(py)
        {
            // This mandatory cleanup differs from an ordinary busy close:
            // the completed context must not leave its Cursor reusable.
            cursor.borrow(py).retire();
            if unattempted.is_some() {
                cleanup_detail = format!(" Cleanup also failed: {error}.");
            }
            if let Some(existing) = &primary {
                if unattempted.is_some() {
                    errors::additional_detail(py, existing, error, "cursor.close", None);
                } else {
                    errors::additional(py, existing, error, "cursor.close");
                }
            } else {
                if commit_confirmed {
                    Self::note(
                        py,
                        &error,
                        "COMMIT was fully confirmed; subsequent Cursor cleanup failed. No rollback or repeated commit was sent.",
                    );
                }
                primary = Some(error);
            }
        }
        self.release();
        if let (Some(reason), Some(error)) = (unattempted, primary.as_ref()) {
            let action = if business.is_some() {
                "ROLLBACK"
            } else {
                "COMMIT"
            };
            let lifecycle = if connection.native.is_closed() {
                "The connection is locally retired."
            } else {
                "This exit did not close the connection or cancel the in-flight command."
            };
            Self::note(
                py,
                error,
                format!(
                    "SQLAero context:\nTransaction: {action} was not attempted: {reason}. No transaction-control command was sent by this exit; no new confirmation of server transaction state was obtained.\nCleanup: This scope's Cursor was closed or retired and its local scope was released.{cleanup_detail}\nConnection: {lifecycle}"
                ),
            );
        }
        if business.is_some() {
            Ok(false)
        } else {
            match primary {
                Some(error) => Err(error),
                None => Ok(false),
            }
        }
    }
}
