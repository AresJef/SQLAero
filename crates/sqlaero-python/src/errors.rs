//! Native exception identity and immutable public constructor values.

use crate::output;
use num_bigint::BigInt;
use pyo3::exceptions::{PyException, PyRuntimeError, PyTypeError, PyWarning};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyInt, PyString, PyTuple, PyType};
use sqlaero_core::error::{ClientErrorCode, ExceptionClass};
use sqlaero_core::protocol::framing::WireError;
use std::ffi::CStr;
use std::sync::Mutex;

mod diagnostics;
pub(crate) use diagnostics::ErrorInfo;

/// Read a named Python attribute without an infallible temporary name object.
pub(crate) fn attribute<'py>(
    value: &Bound<'py, PyAny>,
    name: &CStr,
) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: the attached call receives a live object and a terminated name.
    // CPython returns a new reference or its original attribute/allocation error.
    unsafe {
        Bound::from_owned_ptr_or_err(
            value.py(),
            pyo3::ffi::PyObject_GetAttrString(value.as_ptr(), name.as_ptr()),
        )
    }
}

/// Record an allocation failure without preparing a diagnostic message.
#[cold]
#[inline(never)]
pub(crate) fn memory_error(py: Python<'_>) -> PyErr {
    // SAFETY: the Python token proves this thread is attached. CPython records
    // its native MemoryError, which fetch consumes without String conversion.
    unsafe { pyo3::ffi::PyErr_NoMemory() };
    PyErr::fetch(py)
}

/// Prepare a built-in error message fallibly before returning across FFI.
/// The caller supplies a built-in exception type, never a user-defined class.
/// Native error restoration handles exception allocation with the prepared text.
pub(crate) fn builtin_error(typ: &Bound<'_, PyType>, message: &str) -> PyErr {
    match PyString::from_bytes(typ.py(), message.as_bytes()) {
        // An owned Python string needs no infallible Rust-string conversion in
        // lazy restoration; CPython retains its actual normalization failures.
        Ok(message) => PyErr::from_type(typ.clone(), message.unbind()),
        Err(error) => error,
    }
}

/// Prepare a built-in validation error before lazy FFI error restoration.
/// Reuse the caller's attachment token rather than attaching inside a native
/// state guard. E must be a built-in exception type.
#[cold]
#[inline(never)]
pub(crate) fn builtin_message<E: pyo3::PyTypeInfo>(
    py: Python<'_>,
    message: impl AsRef<str>,
) -> PyErr {
    builtin_error(&py.get_type::<E>(), message.as_ref())
}

/// Allocate a keyword dictionary with its actual Python failure preserved.
pub(crate) fn dictionary(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    // SAFETY: this attached C constructor returns a new exact dict or NULL
    // with its original error. No unchecked ownership conversion is used.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyDict_New())?
            .cast_into::<PyDict>()
            .map_err(Into::into)
    }
}

/// Import through CPython's fallible C boundary, without a Rust-name conversion.
pub(crate) fn module<'py>(py: Python<'py>, name: &CStr) -> PyResult<Bound<'py, PyModule>> {
    // SAFETY: the name is terminated and Python is attached. CPython owns the
    // import protocol and reports both import and allocation errors normally.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyImport_ImportModule(name.as_ptr()))?
            .cast_into::<PyModule>()
            .map_err(Into::into)
    }
}

/// Prepare the shared driver-error keywords without unchecked conversions.
fn error_keywords(py: Python<'_>, code: u32, server: bool) -> PyResult<Bound<'_, PyDict>> {
    let keywords = dictionary(py)?;
    // SAFETY: u32 fits u64 on every supported target. Python is attached
    // and this normal constructor returns an owned integer or its actual error.
    let code = unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyLong_FromUnsignedLongLong(u64::from(code)),
        )?
    };
    keywords.set_item(PyString::from_bytes(py, b"code")?, code)?;
    keywords.set_item(PyString::from_bytes(py, b"is_server_error")?, server)?;
    Ok(keywords)
}

/// Call the existing integer method without an infallible temporary name.
#[inline]
pub(crate) fn integer_bit_length<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    value.call_method0(PyString::from_bytes(value.py(), b"bit_length")?)
}

/// Read a caller-validated int while preserving its actual conversion errors.
#[inline]
pub(crate) fn integer_value(value: &Bound<'_, PyAny>) -> PyResult<BigInt> {
    #[cfg(Py_3_13)]
    {
        value.extract::<BigInt>()
    }
    #[cfg(not(Py_3_13))]
    {
        let value = value.cast::<PyInt>()?;
        // The pinned PyO3's 3.12 extraction interns this method name through
        // an infallible constructor. Keep its method/subclass semantics with
        // a fallible name, then use the same signed native-byte conversion.
        let reported = integer_bit_length(value.as_any())?.extract::<usize>()?;
        let bits = if value.is_exact_instance_of::<PyInt>() {
            reported
        } else {
            // Keep the existing subclass callback and its errors above, but
            // never trust its width as the value. A small report only selects
            // a bounded signed native attempt (at most 32 bytes). The existing
            // C conversion must prove that the whole value fits; it cannot
            // truncate successfully.
            if reported <= 255 {
                match integer_value_with_bits(value, reported.max(63)) {
                    Ok(actual) => return Ok(actual),
                    Err(error)
                        if error
                            .is_instance_of::<pyo3::exceptions::PyOverflowError>(value.py()) => {}
                    Err(error) => return Err(error),
                }
            }
            // A large report or an insufficient native attempt uses int's
            // actual base width. Do not reserve arbitrary storage from an
            // untrusted override or mask a real allocation/conversion error.
            let method = attribute(value.py().get_type::<PyInt>().as_any(), c"bit_length")?;
            // SAFETY: both owned references stay live while attached. This
            // calls int's descriptor, not a second subclass override.
            unsafe {
                Bound::<PyAny>::from_owned_ptr_or_err(
                    value.py(),
                    pyo3::ffi::PyObject_CallOneArg(method.as_ptr(), value.as_ptr()),
                )?
            }
            .extract::<usize>()?
        };
        integer_value_with_bits(value, bits)
    }
}

/// Read an int into checked signed storage derived from a bit-capacity hint.
/// The C conversion rejects insufficient capacity; success preserves the whole
/// original signed value, even when the supplied hint understates its width.
#[cfg(not(Py_3_13))]
#[inline]
pub(crate) fn integer_value_with_bits(value: &Bound<'_, PyInt>, bits: usize) -> PyResult<BigInt> {
    if bits == 0 {
        return Ok(BigInt::from(0u8));
    }
    let digits = bits / 32 + 1;
    let mut buffer = Vec::<u32>::new();
    buffer
        .try_reserve_exact(digits)
        .map_err(|_| memory_error(value.py()))?;
    // SAFETY: value is a live int and Python remains attached. The native
    // reservation proves the full u32 byte count fits its allocation
    // layout. On success CPython writes every byte in signed little-endian
    // form; on error len stays zero.
    // This is the pinned PyO3's 3.12 fallback, without private layout reads.
    unsafe {
        if pyo3::ffi::_PyLong_AsByteArray(
            value.as_ptr().cast(),
            buffer.as_mut_ptr().cast(),
            digits * std::mem::size_of::<u32>(),
            1,
            1,
        ) == -1
        {
            return Err(PyErr::fetch(value.py()));
        }
        buffer.set_len(digits);
    }
    for word in &mut buffer {
        *word = u32::from_le(*word);
    }
    let sign = if buffer.last().is_some_and(|word| word >> 31 != 0) {
        let mut words = buffer.iter_mut();
        for word in words.by_ref() {
            *word = (!*word).wrapping_add(1);
            if *word != 0 {
                break;
            }
        }
        for word in words {
            *word = !*word;
        }
        num_bigint::Sign::Minus
    } else {
        num_bigint::Sign::Plus
    };
    Ok(BigInt::new(sign, buffer))
}

/// Materialize an exact code with fallible CPython constructors at every size.
pub(crate) fn code_object<'py>(py: Python<'py>, value: &BigInt) -> PyResult<Bound<'py, PyInt>> {
    // SAFETY: every constructor copies its native input and returns an owned
    // exact int or NULL with its original error. Common codes use direct scalar
    // constructors; only arbitrary-precision values need a signed byte buffer.
    unsafe {
        let pointer = if let Ok(value) = i64::try_from(value) {
            pyo3::ffi::PyLong_FromLongLong(value)
        } else if let Ok(value) = u64::try_from(value) {
            pyo3::ffi::PyLong_FromUnsignedLongLong(value)
        } else {
            let bytes = value.to_signed_bytes_le();
            #[cfg(Py_3_13)]
            let pointer = pyo3::ffi::PyLong_FromNativeBytes(
                bytes.as_ptr().cast(),
                bytes.len(),
                pyo3::ffi::Py_ASNATIVEBYTES_LITTLE_ENDIAN,
            );
            // CPython 3.12 lacks the public native-byte constructor. This is the
            // same narrow fallback used by the pinned PyO3, with NULL checked.
            #[cfg(not(Py_3_13))]
            let pointer = pyo3::ffi::_PyLong_FromByteArray(bytes.as_ptr(), bytes.len(), 1, 1);
            pointer
        };
        let value = Bound::<PyAny>::from_owned_ptr_or_err(py, pointer)?;
        Ok(value.cast_into_unchecked())
    }
}

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
    "A reliably identified internal driver or server condition failed."
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
    let prepared = || -> PyResult<Bound<'_, PyAny>> {
        let keywords = error_keywords(py, code as u32, false)?;
        let message = message.into();
        let message = PyString::from_bytes(py, message.as_bytes())?;
        exception_type(py, code.entry().class).call((message,), Some(&keywords))
    };
    match prepared() {
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
        WireError::Allocation(_) => memory_error(py),
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
    // Ordinary access is attached and guards contain only native work and
    // increfs. Traversal takes this same lock without attaching, so never
    // detach or release a final Python reference while holding its guard.
    diagnostics: Mutex<diagnostics::State>,
}
impl SQLAeroError {
    pub(crate) fn code(&self) -> Option<&BigInt> {
        self.code.as_ref()
    }
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
            return Err(crate::errors::builtin_message::<PyTypeError>(
                py,
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
                            return Err(crate::errors::builtin_message::<PyTypeError>(
                                py,
                                "message was provided twice",
                            ));
                        }
                        message = Some(value);
                    }
                    "code" => {
                        if !value.is_none() {
                            if !value.is_instance_of::<PyInt>() || value.is_instance_of::<PyBool>()
                            {
                                return Err(crate::errors::builtin_message::<PyTypeError>(
                                    py,
                                    "code must be int or None, not bool",
                                ));
                            }
                            code = Some(integer_value(&value)?);
                        }
                    }
                    "is_server_error" => {
                        if !value.is_none() {
                            if !value.is_exact_instance_of::<PyBool>() {
                                return Err(crate::errors::builtin_message::<PyTypeError>(
                                    py,
                                    "is_server_error must be bool or None",
                                ));
                            }
                            source = Some(value.extract::<bool>()?);
                        }
                    }
                    key => {
                        return Err(crate::errors::builtin_message::<PyTypeError>(
                            py,
                            format!("unexpected keyword argument {key:?}"),
                        ));
                    }
                }
            }
        }
        let provided = message.is_some();
        let message = match message {
            Some(value) => {
                value.cast::<PyString>().map_err(|_| {
                    crate::errors::builtin_message::<PyTypeError>(py, "message must be str")
                })?;
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
            None => PyString::from_bytes(py, b"")?,
        };
        // Normalize a str subclass to plain text: no user-owned objects are
        // retained in this immutable tuple, so it cannot introduce a GC cycle.
        let args = if provided {
            output::tuple_from_ready(py, [message.clone().into_any()])?
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
    #[getter(code)]
    fn python_code<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyInt>>> {
        self.code
            .as_ref()
            .map(|value| code_object(py, value))
            .transpose()
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
        let generation = {
            let value = slf.borrow();
            let state = value.diagnostics.lock().map_err(|_| {
                crate::errors::builtin_message::<PyRuntimeError>(
                    py,
                    "error diagnostic ownership is unavailable",
                )
            })?;
            if let Some(cached) = &state.cached {
                return Ok(cached.clone_ref(py));
            }
            state.generation
        };
        let snapshot = diagnostics::capture(py, &PyErr::from_value(slf.clone().into_any()), None)?;
        let published = {
            let value = slf.borrow();
            let mut state = value.diagnostics.lock().map_err(|_| {
                crate::errors::builtin_message::<PyRuntimeError>(
                    py,
                    "error diagnostic ownership is unavailable",
                )
            })?;
            if let Some(cached) = &state.cached {
                Some(cached.clone_ref(py))
            } else {
                if state.generation == generation {
                    state.cached = Some(snapshot.clone_ref(py));
                }
                None
            }
        };
        // An overlapping update may leave this completed snapshot uncached.
        // Never restore its older generation after that update retired it.
        Ok(published.unwrap_or(snapshot))
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

/// Register all public native exceptions, immutable diagnostics and code enums.
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
    let keywords = match error_keywords(py, u32::from(code), true) {
        Ok(value) => value,
        Err(error) => return error,
    };
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
    let message = format!("Additional failure at {stage}: {secondary}");
    additional_detail(py, primary, secondary, stage, Some(message));
}

/// Record an independent failure while letting its owner compose one fact block.
/// A deferred note must be supplied by that boundary after its cleanup finishes.
pub(crate) fn additional_detail(
    py: Python<'_>,
    primary: &PyErr,
    secondary: PyErr,
    stage: &str,
    message: Option<String>,
) {
    let captured = diagnostics::capture(py, &secondary, Some(stage));
    if let Some(message) = message {
        note(py, primary, message);
    }
    let Ok(captured) = captured else { return };
    let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>() else {
        return;
    };
    let retired = if let Ok(mut state) = error.diagnostics.lock() {
        if state.additional.try_reserve(1).is_ok() {
            state.additional.push(captured);
            state.invalidate()
        } else {
            None
        }
    } else {
        None
    };
    drop(retired);
}

/// Record a real new fact without changing the primary object or older views.
pub(crate) fn note(py: Python<'_>, primary: &PyErr, message: impl Into<String>) {
    {
        let message = message.into();
        let Ok(message) = PyString::from_bytes(py, message.as_bytes()) else {
            return;
        };
        let Ok(method) = attribute(primary.value(py), c"add_note") else {
            return;
        };
        // SAFETY: Python is attached and both callable and argument stay owned
        // through the public call. Its result is a new reference or NULL with
        // the actual error; secondary note failures remain best effort.
        let result = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_CallOneArg(method.as_ptr(), message.as_ptr()),
            )
        };
        if result.is_err() {
            return;
        }
        // Drop the return value, method and message outside diagnostics locks;
        // an overridden method's returned object may run Python on final drop.
    }
    let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>() else {
        return;
    };
    let retired = error
        .diagnostics
        .lock()
        .ok()
        .and_then(|mut state| state.invalidate());
    drop(retired);
}

/// Publish the operation boundary that actually owns this failure. This is
/// recorded only on failure and never parses a human message to infer state.
pub(crate) fn at(py: Python<'_>, primary: PyErr, stage: &'static str) -> PyErr {
    let retired = if let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
        && state.stage != Some(stage)
    {
        state.stage = Some(stage);
        state.invalidate()
    } else {
        None
    };
    drop(retired);
    primary
}

/// Attach actual native source facts before consuming the native error value.
fn native(py: Python<'_>, primary: &PyErr, captured: diagnostics::Native) {
    let retired = if let Ok(error) = primary.value(py).extract::<PyRef<'_, SQLAeroError>>()
        && let Ok(mut state) = error.diagnostics.lock()
    {
        state.native = Some(captured);
        state.invalidate()
    } else {
        None
    };
    drop(retired);
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
