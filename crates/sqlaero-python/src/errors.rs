//! Native exception identity and immutable public constructor values.

use num_bigint::BigInt;
use pyo3::exceptions::{PyException, PyTypeError, PyWarning};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyInt, PyString, PyTuple, PyType};
use sqlaero_core::error::{ClientErrorCode, ExceptionClass};
use sqlaero_core::protocol::framing::WireError;
use std::sync::Mutex;

mod diagnostics;
pub(crate) use diagnostics::ErrorInfo;

pyo3::create_exception!(
    sqlaero.errors,
    InterfaceError,
    SQLAeroError,
    "Invalid client interface or session state."
);
pyo3::create_exception!(
    sqlaero.errors,
    InterfaceBusyError,
    InterfaceError,
    "A conflicting operation already owns this connection or cursor."
);
pyo3::create_exception!(
    sqlaero.errors,
    DatabaseError,
    SQLAeroError,
    "Base class for database and data-processing failures."
);
pyo3::create_exception!(
    sqlaero.errors,
    DataError,
    DatabaseError,
    "A supported value cannot be represented under the data contract."
);
pyo3::create_exception!(
    sqlaero.errors,
    OperationalError,
    DatabaseError,
    "A network, server availability or operational failure."
);
pyo3::create_exception!(
    sqlaero.errors,
    OperationalTimeoutError,
    OperationalError,
    "An actual connection, operation or network deadline expired."
);
pyo3::create_exception!(
    sqlaero.errors,
    IntegrityError,
    DatabaseError,
    "A database integrity constraint was violated."
);
pyo3::create_exception!(
    sqlaero.errors,
    InternalError,
    DatabaseError,
    "An internal protocol or driver invariant failed."
);
pyo3::create_exception!(
    sqlaero.errors,
    ProgrammingError,
    DatabaseError,
    "An invalid SQL command or programming contract was rejected."
);
pyo3::create_exception!(
    sqlaero.errors,
    NotSupportedError,
    DatabaseError,
    "An explicitly unsupported capability was requested."
);
pyo3::create_exception!(
    sqlaero.errors,
    SQLAeroWarning,
    PyWarning,
    "A SQLAero warning category, separate from SQLAeroError."
);

/// Construct a real driver exception with complete source metadata. User-made
/// exceptions retain the independent, optional constructor semantics below.
pub(crate) fn client(py: Python<'_>, code: ClientErrorCode, message: impl Into<String>) -> PyErr {
    let keywords = PyDict::new(py);
    if let Err(error) = keywords
        .set_item("code", code as u32)
        .and_then(|()| keywords.set_item("is_server_error", false))
    {
        return error;
    }
    match exception_type(py, code.entry().class).call((message.into(),), Some(&keywords)) {
        Ok(value) => match value.cast_into::<PyException>() {
            Ok(value) => PyErr::from_value(value.into_any()),
            Err(error) => error.into(),
        },
        Err(error) => error,
    }
}

fn exception_type(py: Python<'_>, class: ExceptionClass) -> Bound<'_, PyType> {
    match class {
        ExceptionClass::Interface => py.get_type::<InterfaceError>(),
        ExceptionClass::InterfaceBusy => py.get_type::<InterfaceBusyError>(),
        ExceptionClass::Database => py.get_type::<DatabaseError>(),
        ExceptionClass::Data => py.get_type::<DataError>(),
        ExceptionClass::Operational => py.get_type::<OperationalError>(),
        ExceptionClass::OperationalTimeout => py.get_type::<OperationalTimeoutError>(),
        ExceptionClass::Integrity => py.get_type::<IntegrityError>(),
        ExceptionClass::Internal => py.get_type::<InternalError>(),
        ExceptionClass::Programming => py.get_type::<ProgrammingError>(),
        ExceptionClass::NotSupported => py.get_type::<NotSupportedError>(),
    }
}

/// Preserve an actual allocation failure; distinguish active resource bounds
/// from malformed wire data without parsing a human error string.
pub(crate) fn wire(py: Python<'_>, error: WireError) -> PyErr {
    let detail =
        diagnostics::Native::capture(&error, Some("sqlaero_core::protocol::framing::WireError"));
    let mapped = match error {
        WireError::Allocation(error) => pyo3::exceptions::PyMemoryError::new_err(error.to_string()),
        error @ WireError::Io(_) => client(py, ClientErrorCode::Transport, error.to_string()),
        error @ (WireError::Limit { .. } | WireError::Memory(_)) => {
            client(py, ClientErrorCode::ResourceLimit, error.to_string())
        }
        error => client(py, ClientErrorCode::Protocol, error.to_string()),
    };
    native(py, &mapped, detail);
    mapped
}

/// Base exception for SQLAero client and server failures.
///
/// `message` is optional text. `code` is an optional arbitrary-size integer;
/// booleans are not codes. `is_server_error` is an optional exact bool. Manual
/// construction does not infer or validate a relationship between these values.
/// The values and `args` are read-only; Python's native exception chaining and
/// traceback mechanisms remain available.
#[pyclass(extends = PyException, subclass, module = "sqlaero.errors")]
pub(crate) struct SQLAeroError {
    message: Py<PyString>,
    code: Option<BigInt>,
    is_server_error: Option<bool>,
    args: Py<PyTuple>,
    diagnostics: Mutex<diagnostics::State>,
}

#[pymethods]
impl SQLAeroError {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    #[pyo3(text_signature = "(message='', *, code=None, is_server_error=None)")]
    fn new(
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        if args.len() > 1 {
            return Err(PyTypeError::new_err(
                "expected at most one positional message",
            ));
        }
        let mut message = if args.is_empty() {
            None
        } else {
            Some(args.get_item(0)?)
        };
        let mut code = None;
        let mut source = None;
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                match key.extract::<&str>()? {
                    "message" => {
                        if message.is_some() {
                            return Err(PyTypeError::new_err("message was provided twice"));
                        }
                        message = Some(value);
                    }
                    "code" => {
                        if !value.is_none() {
                            if !value.is_instance_of::<PyInt>() || value.is_instance_of::<PyBool>()
                            {
                                return Err(PyTypeError::new_err(
                                    "code must be int or None, not bool",
                                ));
                            }
                            code = Some(value.extract::<BigInt>()?);
                        }
                    }
                    "is_server_error" => {
                        if !value.is_none() {
                            if !value.is_exact_instance_of::<PyBool>() {
                                return Err(PyTypeError::new_err(
                                    "is_server_error must be bool or None",
                                ));
                            }
                            source = Some(value.extract::<bool>()?);
                        }
                    }
                    key => {
                        return Err(PyTypeError::new_err(format!(
                            "unexpected keyword argument {key:?}"
                        )));
                    }
                }
            }
        }
        let provided = message.is_some();
        let message = match message {
            Some(value) => {
                value
                    .cast::<PyString>()
                    .map_err(|_| PyTypeError::new_err("message must be str"))?;
                // SAFETY: the attached copying/base-normalizing Unicode API
                // receives a live str. It preserves lone surrogates without
                // calling a str subclass's conversion hooks or retaining it.
                unsafe {
                    Bound::<PyAny>::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyUnicode_FromObject(value.as_ptr()),
                    )?
                }
                .cast_into::<PyString>()?
            }
            None => PyString::new(py, ""),
        };
        // Normalize a str subclass to plain text: no user-owned objects are
        // retained in this immutable tuple, so it cannot introduce a GC cycle.
        let args = if provided {
            PyTuple::new(py, [message.clone()])?
        } else {
            PyTuple::empty(py)
        }
        .unbind();
        Ok(Self {
            message: message.unbind(),
            code,
            is_server_error: source,
            args,
            diagnostics: Mutex::new(diagnostics::State::default()),
        })
    }

    // BaseException.__init__ would reject our keyword-only metadata or rewrite
    // args. Validation and initialization are performed once by __new__.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(&self, _args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) {}

    /// Return the original message, without a display prefix or server request.
    #[getter]
    fn message(&self, py: Python<'_>) -> Py<PyString> {
        self.message.clone_ref(py)
    }

    /// Return the declared integer code, or None when manually unspecified.
    #[getter]
    pub(crate) fn code(&self) -> Option<&BigInt> {
        self.code.as_ref()
    }

    /// Return True for a direct server error, False for a client error, or None.
    #[getter]
    fn is_server_error(&self) -> Option<bool> {
        self.is_server_error
    }

    /// Return the immutable message-only exception arguments.
    #[getter]
    fn args(&self, py: Python<'_>) -> Py<PyTuple> {
        self.args.clone_ref(py)
    }

    /// Return an immutable diagnostic snapshot. Repeated reads reuse the cached
    /// snapshot while SQLAero has recorded no new diagnostic information. If a
    /// later handling stage records additional errors or relevant facts, the
    /// next read returns an updated snapshot. Previously returned snapshots
    /// remain unchanged. Reading this property sends no request to the server.
    #[getter]
    fn error_info(slf: &Bound<'_, Self>) -> PyResult<Py<ErrorInfo>> {
        let py = slf.py();
        {
            let value = slf.borrow();
            let state = value.diagnostics.lock().map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "error diagnostic ownership is unavailable",
                )
            })?;
            if let Some(cached) = &state.cached {
                return Ok(cached.clone_ref(py));
            }
        }
        let snapshot = diagnostics::capture(py, &PyErr::from_value(slf.clone().into_any()), None)?;
        slf.borrow()
            .diagnostics
            .lock()
            .map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "error diagnostic ownership is unavailable",
                )
            })?
            .cached = Some(snapshot.clone_ref(py));
        Ok(snapshot)
    }

    fn __traverse__(
        &self,
        visit: pyo3::class::PyVisit<'_>,
    ) -> Result<(), pyo3::class::PyTraverseError> {
        visit.call(&self.message)?;
        visit.call(&self.args)?;
        let state = self
            .diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.traverse(&visit)
    }
    fn __clear__(&mut self) {
        self.diagnostics
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        let mut prefix = match self.is_server_error {
            Some(true) => "Server Error".to_owned(),
            Some(false) => "Client Error".to_owned(),
            None if self.code.is_some() => "Error".to_owned(),
            None => String::new(),
        };
        if let Some(code) = &self.code {
            prefix.push_str(&format!(" [{code}]"));
        }
        if prefix.is_empty() {
            return Ok(self.message.clone_ref(py).into_bound(py));
        }
        let message = self.message.bind(py);
        if !message.is_empty()? {
            prefix.push_str(": ");
        }
        let prefix = crate::output::text(
            py,
            prefix.as_bytes(),
            sqlaero_core::charset::output::OutputCodec::for_collation(255),
            true,
            None,
        )?;
        // SAFETY: both attached operands are owned exact Unicode values. The
        // concatenation returns a new reference or the actual allocation error.
        unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyUnicode_Concat(prefix.as_ptr(), message.as_ptr()),
            )?
        }
        .cast_into()
        .map_err(Into::into)
    }
}

/// Register the native root. Detailed categories and diagnostics are added at
/// their implementation step; this is not a completed public errors module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SQLAeroError>()?;
    module.add_class::<ErrorInfo>()?;
    let py = module.py();
    module.add("InterfaceError", py.get_type::<InterfaceError>())?;
    module.add("InterfaceBusyError", py.get_type::<InterfaceBusyError>())?;
    module.add("DatabaseError", py.get_type::<DatabaseError>())?;
    module.add("DataError", py.get_type::<DataError>())?;
    module.add("OperationalError", py.get_type::<OperationalError>())?;
    module.add(
        "OperationalTimeoutError",
        py.get_type::<OperationalTimeoutError>(),
    )?;
    module.add("IntegrityError", py.get_type::<IntegrityError>())?;
    module.add("InternalError", py.get_type::<InternalError>())?;
    module.add("ProgrammingError", py.get_type::<ProgrammingError>())?;
    module.add("NotSupportedError", py.get_type::<NotSupportedError>())?;
    module.add("SQLAeroWarning", py.get_type::<SQLAeroWarning>())?;
    register_codes(module)
}

/// Map the actual native timeout category without parsing its display text.
pub(crate) fn timeout(
    py: Python<'_>,
    error: sqlaero_core::transport::deadline::TimeoutKind,
) -> PyErr {
    use sqlaero_core::transport::deadline::TimeoutKind;
    let code = match error {
        TimeoutKind::Connect => ClientErrorCode::ConnectDeadline,
        TimeoutKind::Operation => ClientErrorCode::OperationDeadline,
        TimeoutKind::Read => ClientErrorCode::ReadDeadline,
        TimeoutKind::Write => ClientErrorCode::WriteDeadline,
    };
    client(py, code, error.to_string())
}
pub(crate) fn access(py: Python<'_>, error: sqlaero_core::session::shared::AccessError) -> PyErr {
    use sqlaero_core::session::shared::AccessError;
    match error {
        AccessError::Busy => client(py, ClientErrorCode::InterfaceBusy, error.to_string()),
        AccessError::Closed => client(py, ClientErrorCode::SessionState, error.to_string()),
        AccessError::Poisoned => client(py, ClientErrorCode::InternalState, error.to_string()),
    }
}
pub(crate) fn channel(
    py: Python<'_>,
    error: sqlaero_core::transport::channel::ChannelError,
) -> PyErr {
    use sqlaero_core::transport::{channel::ChannelError, socket::TransportError};
    let detail = diagnostics::Native::capture(
        &error,
        Some("sqlaero_core::transport::channel::ChannelError"),
    );
    let mapped = match error {
        ChannelError::Wire(error) => wire(py, error),
        ChannelError::Timeout(error) | ChannelError::Transport(TransportError::Timeout(error)) => {
            timeout(py, error)
        }
        error @ ChannelError::Transport(_) => {
            client(py, ClientErrorCode::Transport, error.to_string())
        }
        error @ ChannelError::Tls(_) => client(py, ClientErrorCode::Tls, error.to_string()),
    };
    native(py, &mapped, detail);
    mapped
}
/// Preserve actual server code/state and decode the final Python error text
/// using the response's real encoding. Unknown encoding uses lossless byte
/// escapes rather than claiming UTF-8 or silently replacing data.
pub(crate) fn server(py: Python<'_>, code: u16, state: Option<[u8; 5]>, message: &[u8]) -> PyErr {
    server_with_codec(
        py,
        code,
        state,
        message,
        Some(sqlaero_core::charset::output::OutputCodec::for_collation(
            255,
        )),
    )
}
fn server_with_codec(
    py: Python<'_>,
    code: u16,
    state: Option<[u8; 5]>,
    message: &[u8],
    codec: Option<sqlaero_core::charset::output::OutputCodec>,
) -> PyErr {
    let typ = exception_type(py, sqlaero_core::error::catalog::classify(code, state));
    let keywords = PyDict::new(py);
    if let Err(error) = keywords
        .set_item("code", code)
        .and_then(|()| keywords.set_item("is_server_error", true))
    {
        return error;
    }
    let mut raw_text = false;
    let message = match codec {
        Some(codec) => match crate::output::text(py, message, codec, false, None) {
            Ok(value) => value,
            // The server can truncate its own converted ERR text at a NUL
            // byte. Invalid diagnostic bytes must not replace its real error
            // class/code with an unrelated UnicodeDecodeError. Resource and
            // control exceptions still propagate unchanged.
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyUnicodeDecodeError>(py) => {
                raw_text = true;
                match escaped_error_text(py, message) {
                    Ok(value) => value,
                    Err(error) => return error,
                }
            }
            Err(error) => return error,
        },
        None => {
            raw_text = true;
            // ASCII bytes are already a reversible representation; preserve
            // normal server punctuation instead of needlessly escaping it.
            if message.is_ascii() {
                match crate::output::text(
                    py,
                    message,
                    sqlaero_core::charset::output::OutputCodec::for_collation(255),
                    true,
                    None,
                ) {
                    Ok(value) => value,
                    Err(error) => return error,
                }
            } else {
                match escaped_error_text(py, message) {
                    Ok(value) => value,
                    Err(error) => return error,
                }
            }
        }
    };
    match typ.call((message,), Some(&keywords)) {
        Ok(value) => {
            let error = PyErr::from_value(value);
            if raw_text {
                let _ = error.add_note(py, "MySQL ERR text has an unconfirmed encoding or invalid/incomplete byte sequence; message preserves the received bytes as ASCII with lossless byte escapes where needed.");
            }
            if let Some(state) = state {
                let state: String = state
                    .iter()
                    .flat_map(|byte| std::ascii::escape_default(*byte))
                    .map(char::from)
                    .collect();
                let _ = error.add_note(py, format!("MySQL SQLSTATE: {state}"));
            }
            error
        }
        Err(error) => error,
    }
}

fn escaped_error_text<'py>(py: Python<'py>, message: &[u8]) -> PyResult<Bound<'py, PyString>> {
    let escaped: String = message
        .iter()
        .flat_map(|byte| std::ascii::escape_default(*byte))
        .map(char::from)
        .collect();
    crate::output::text(
        py,
        escaped.as_bytes(),
        sqlaero_core::charset::output::OutputCodec::for_collation(255),
        true,
        None,
    )
}
pub(crate) fn query(py: Python<'_>, error: sqlaero_core::session::query::QueryError) -> PyErr {
    query_with_codec(
        py,
        error,
        Some(sqlaero_core::charset::output::OutputCodec::for_collation(
            255,
        )),
    )
}
pub(crate) fn query_with_codec(
    py: Python<'_>,
    error: sqlaero_core::session::query::QueryError,
    codec: Option<sqlaero_core::charset::output::OutputCodec>,
) -> PyErr {
    use sqlaero_core::{result::text::ResultError, session::query::QueryError};
    match error {
        QueryError::Wire(error) | QueryError::Result(ResultError::Wire(error)) => wire(py, error),
        QueryError::Channel(error) => channel(py, error),
        QueryError::Result(ResultError::Memory(error)) => crate::output::memory(py, error),
        QueryError::Result(ResultError::Server(error)) => server_with_codec(
            py,
            error.error_code(),
            error.sql_state_ref().map(|state| state.as_bytes()),
            error.message_ref(),
            codec,
        ),
        error @ (QueryError::MultipleResults { .. }
        | QueryError::Result(ResultError::Unsupported(_))) => client(
            py,
            ClientErrorCode::UnsupportedCapability,
            error.to_string(),
        ),
    }
}
pub(crate) fn establish(
    py: Python<'_>,
    error: sqlaero_core::session::connect::ConnectError,
) -> PyErr {
    use sqlaero_core::{protocol::auth::AuthError, session::connect::ConnectError};
    let detail =
        diagnostics::Native::capture(&error, Some("sqlaero_core::session::connect::ConnectError"));
    let mapped = match error {
        ConnectError::Wire(error) | ConnectError::Auth(AuthError::Wire(error)) => wire(py, error),
        ConnectError::Channel(error) => channel(py, error),
        ConnectError::Timeout(error) => timeout(py, error),
        ConnectError::Auth(AuthError::Server(error)) => server(
            py,
            error.error_code(),
            error.sql_state_ref().map(|state| state.as_bytes()),
            error.message_ref(),
        ),
        error @ (ConnectError::Unsupported(_)
        | ConnectError::Auth(AuthError::UnsupportedPlugin(_))) => client(
            py,
            ClientErrorCode::UnsupportedCapability,
            error.to_string(),
        ),
        error @ ConnectError::Auth(_) => {
            client(py, ClientErrorCode::Authentication, error.to_string())
        }
        error => client(py, ClientErrorCode::Transport, error.to_string()),
    };
    native(py, &mapped, detail);
    mapped
}
pub(crate) fn initialize(
    py: Python<'_>,
    error: sqlaero_core::session::initialize::InitializeError,
) -> PyErr {
    use sqlaero_core::session::initialize::InitializeError;
    match error {
        InitializeError::Connect(error) => establish(py, error),
        InitializeError::Query(error) => query(py, error),
        InitializeError::Wire(error) => wire(py, error),
        error => client(py, ClientErrorCode::SessionState, error.to_string()),
    }
}

/// Preserve the same primary exception while recording a real independent
/// cleanup failure. Diagnostic snapshot publication is implemented in P05.3.
pub(crate) fn additional(py: Python<'_>, primary: &PyErr, secondary: PyErr, stage: &str) {
    let captured = diagnostics::capture(py, &secondary, Some(stage));
    note(
        py,
        primary,
        format!("Additional failure at {stage}: {secondary}"),
    );
    if let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
        && let Ok(captured) = captured
        && state.additional.try_reserve(1).is_ok()
    {
        state.additional.push(captured);
        state.cached = None;
    }
}

/// Record a real new fact without changing the primary object or older views.
pub(crate) fn note(py: Python<'_>, primary: &PyErr, message: impl Into<String>) {
    if primary.add_note(py, message.into()).is_ok()
        && let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
    {
        state.cached = None;
    }
}

/// Publish the operation boundary that actually owns this failure. This is
/// recorded only on failure and never parses a human message to infer state.
pub(crate) fn at(py: Python<'_>, primary: PyErr, stage: &'static str) -> PyErr {
    if let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
        && state.stage != Some(stage)
    {
        state.stage = Some(stage);
        state.cached = None;
    }
    primary
}

/// Attach actual native source facts before consuming the native error value.
fn native(py: Python<'_>, primary: &PyErr, captured: diagnostics::Native) {
    if let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
    {
        state.native = Some(captured);
        state.cached = None;
    }
}

/// Materialize standard IntEnum metadata once, from the same native registry
/// used for dispatch. No per-command enumeration or duplicate mapping occurs.
fn register_codes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    use sqlaero_core::error::{CLIENT_CODES, catalog::SERVER_CODES};
    let py = module.py();
    let factory = py.import("enum")?.getattr("IntEnum")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("module", "sqlaero.errors")?;
    let client = PyDict::new(py);
    for entry in CLIENT_CODES {
        client.set_item(entry.name, entry.code as u32)?;
    }
    let client = factory.call(("SQLAeroErrorCode", client), Some(&kwargs))?;
    client.setattr("__doc__","Registered SQLAero client-origin codes. Each member documents its exact exception class and meaning.")?;
    for entry in CLIENT_CODES {
        client.getattr(entry.name)?.setattr("__doc__", entry.doc)?;
    }
    module.add("SQLAeroErrorCode", client)?;
    let server = PyDict::new(py);
    for entry in SERVER_CODES {
        for name in entry.names {
            server.set_item(*name, entry.code)?;
        }
    }
    let server = factory.call(("MySQLErrorCode", server), Some(&kwargs))?;
    server.setattr("__doc__","Known MySQL protocol ERR numbers. Warning/Note responses are not automatically raised; version aliases share their actual numeric identity.")?;
    for entry in SERVER_CODES {
        server
            .getattr(entry.names[0])?
            .setattr("__doc__", entry.doc)?;
    }
    module.add("MySQLErrorCode", server)
}
