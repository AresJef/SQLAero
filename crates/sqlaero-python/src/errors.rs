//! Native exception identity and immutable public constructor values.

use num_bigint::BigInt;
use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyException, PyTypeError, PyWarning};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyInt, PyTuple};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::protocol::framing::WireError;

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
pub(crate) fn client<T: PyTypeInfo>(
    py: Python<'_>,
    code: ClientErrorCode,
    message: impl Into<String>,
) -> PyErr {
    let keywords = PyDict::new(py);
    if let Err(error) = keywords
        .set_item("code", code as u32)
        .and_then(|()| keywords.set_item("is_server_error", false))
    {
        return error;
    }
    match py.get_type::<T>().call((message.into(),), Some(&keywords)) {
        Ok(value) => match value.cast_into::<PyException>() {
            Ok(value) => PyErr::from_value(value.into_any()),
            Err(error) => error.into(),
        },
        Err(error) => error,
    }
}

/// Preserve an actual allocation failure; distinguish active resource bounds
/// from malformed wire data without parsing a human error string.
pub(crate) fn wire(py: Python<'_>, error: WireError) -> PyErr {
    match error {
        WireError::Allocation(error) => pyo3::exceptions::PyMemoryError::new_err(error.to_string()),
        error @ (WireError::Limit { .. } | WireError::Memory(_)) => {
            client::<OperationalError>(py, ClientErrorCode::ResourceLimit, error.to_string())
        }
        error => client::<OperationalError>(py, ClientErrorCode::Protocol, error.to_string()),
    }
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
    message: String,
    code: Option<BigInt>,
    is_server_error: Option<bool>,
    args: Py<PyTuple>,
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
        let message = message
            .map(|value| value.extract::<String>())
            .transpose()?
            .unwrap_or_default();
        // Normalize a str subclass to plain text: no user-owned objects are
        // retained in this immutable tuple, so it cannot introduce a GC cycle.
        let args = if provided {
            PyTuple::new(py, [message.as_str()])?
        } else {
            PyTuple::empty(py)
        }
        .unbind();
        Ok(Self {
            message,
            code,
            is_server_error: source,
            args,
        })
    }

    // BaseException.__init__ would reject our keyword-only metadata or rewrite
    // args. Validation and initialization are performed once by __new__.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(&self, _args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) {}

    /// Return the original message, without a display prefix or server request.
    #[getter]
    fn message(&self) -> &str {
        &self.message
    }

    /// Return the declared integer code, or None when manually unspecified.
    #[getter]
    fn code(&self) -> Option<&BigInt> {
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

    fn __str__(&self) -> String {
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
            return self.message.clone();
        }
        if !self.message.is_empty() {
            prefix.push_str(": ");
            prefix.push_str(&self.message);
        }
        prefix
    }
}

/// Register the native root. Detailed categories and diagnostics are added at
/// their implementation step; this is not a completed public errors module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SQLAeroError>()?;
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
    module.add("SQLAeroWarning", py.get_type::<SQLAeroWarning>())
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
    client::<OperationalTimeoutError>(py, code, error.to_string())
}
pub(crate) fn access(py: Python<'_>, error: sqlaero_core::session::shared::AccessError) -> PyErr {
    use sqlaero_core::session::shared::AccessError;
    match error {
        AccessError::Busy => {
            client::<InterfaceBusyError>(py, ClientErrorCode::InterfaceBusy, error.to_string())
        }
        AccessError::Closed => {
            client::<InterfaceError>(py, ClientErrorCode::SessionState, error.to_string())
        }
        AccessError::Poisoned => {
            client::<InternalError>(py, ClientErrorCode::InternalState, error.to_string())
        }
    }
}
pub(crate) fn channel(
    py: Python<'_>,
    error: sqlaero_core::transport::channel::ChannelError,
) -> PyErr {
    use sqlaero_core::transport::{channel::ChannelError, socket::TransportError};
    match error {
        ChannelError::Wire(error) => wire(py, error),
        ChannelError::Timeout(error) | ChannelError::Transport(TransportError::Timeout(error)) => {
            timeout(py, error)
        }
        error @ ChannelError::Transport(_) => {
            client::<OperationalError>(py, ClientErrorCode::Transport, error.to_string())
        }
        error @ ChannelError::Tls(_) => {
            client::<OperationalError>(py, ClientErrorCode::Tls, error.to_string())
        }
    }
}
/// Build a server exception from actual ERR fields. The complete maintained
/// code catalog is integrated in P05.2; these SQLSTATE classes are its fallback.
pub(crate) fn server(py: Python<'_>, code: u16, state: Option<[u8; 5]>, message: &[u8]) -> PyErr {
    let state_class = state.map(|state| [state[0], state[1]]);
    let typ = match (code, state_class) {
        (1062 | 1451 | 1452, _) | (_, Some([b'2', b'3'])) => py.get_type::<IntegrityError>(),
        (_, Some([b'2', b'2'])) => py.get_type::<DataError>(),
        (_, Some([b'4', b'2'])) => py.get_type::<ProgrammingError>(),
        (_, Some([b'0', b'A'])) => py.get_type::<NotSupportedError>(),
        (_, Some([b'0', b'8'] | [b'4', b'0'])) => py.get_type::<OperationalError>(),
        _ => py.get_type::<OperationalError>(),
    };
    let keywords = PyDict::new(py);
    if let Err(error) = keywords
        .set_item("code", code)
        .and_then(|()| keywords.set_item("is_server_error", true))
    {
        return error;
    }
    let message = String::from_utf8_lossy(message);
    match typ.call((message.as_ref(),), Some(&keywords)) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}
pub(crate) fn query(py: Python<'_>, error: sqlaero_core::session::query::QueryError) -> PyErr {
    use sqlaero_core::{result::text::ResultError, session::query::QueryError};
    match error {
        QueryError::Wire(error) | QueryError::Result(ResultError::Wire(error)) => wire(py, error),
        QueryError::Channel(error) => channel(py, error),
        QueryError::Result(ResultError::Memory(error)) => crate::output::memory(py, error),
        QueryError::Result(ResultError::Server(error)) => server(
            py,
            error.error_code(),
            error.sql_state_ref().map(|state| state.as_bytes()),
            error.message_ref(),
        ),
        error @ (QueryError::MultipleResults { .. }
        | QueryError::Result(ResultError::Unsupported(_))) => client::<NotSupportedError>(
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
    match error {
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
        | ConnectError::Auth(AuthError::UnsupportedPlugin)) => client::<NotSupportedError>(
            py,
            ClientErrorCode::UnsupportedCapability,
            error.to_string(),
        ),
        error @ ConnectError::Auth(_) => {
            client::<OperationalError>(py, ClientErrorCode::Authentication, error.to_string())
        }
        error => client::<OperationalError>(py, ClientErrorCode::Transport, error.to_string()),
    }
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
        error => client::<InterfaceError>(py, ClientErrorCode::SessionState, error.to_string()),
    }
}

/// Preserve the same primary exception while recording a real independent
/// cleanup failure. Diagnostic snapshot publication is implemented in P05.3.
pub(crate) fn additional(py: Python<'_>, primary: &PyErr, secondary: PyErr, stage: &str) {
    let _ = primary.add_note(py, format!("Additional failure at {stage}: {secondary}"));
}
