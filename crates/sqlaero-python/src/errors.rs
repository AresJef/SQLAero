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

/// Preserve a native I/O failure's actual OS number at the Python boundary.
/// PyO3's single-message conversion loses errno; no synthetic errno is supplied
/// for errors which did not originate from an OS failure.
#[cold]
pub(crate) fn io_error(py: Python<'_>, source: &std::io::Error) -> PyErr {
    use pyo3::exceptions::PyOSError;
    if source.kind() == std::io::ErrorKind::OutOfMemory {
        return memory_error(py);
    }
    let create = || -> PyResult<PyErr> {
        let text = message(py, format_args!("{source}"))?;
        let text = PyString::from_bytes(py, text.as_bytes())?;
        let Some(code) = source.raw_os_error() else {
            return Ok(builtin_error(&py.get_type::<PyOSError>(), text.to_str()?));
        };
        let code = code_object(py, &BigInt::from(code))?;
        #[cfg(windows)]
        let value = py
            .get_type::<PyOSError>()
            .call1((0, text, py.None(), code))?;
        #[cfg(not(windows))]
        let value = py.get_type::<PyOSError>().call1((code, text))?;
        Ok(PyErr::from_value(value))
    };
    create().unwrap_or_else(|error| error)
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

/// Format an internal diagnostic through checked Rust capacity growth. Only
/// native scalar/string arguments belong here; this never invokes Python repr.
#[cold]
#[inline(never)]
pub(crate) fn message(py: Python<'_>, arguments: std::fmt::Arguments<'_>) -> PyResult<String> {
    struct Text(String);
    impl std::fmt::Write for Text {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            self.0
                .try_reserve(text.len())
                .map_err(|_| std::fmt::Error)?;
            self.0.push_str(text);
            Ok(())
        }
    }
    let mut text = Text(String::new());
    std::fmt::write(&mut text, arguments).map_err(|_| memory_error(py))?;
    Ok(text.0)
}

/// Keep dynamic built-in diagnostics fallible before lazy FFI restoration.
#[cold]
pub(crate) fn builtin_format<E: pyo3::PyTypeInfo>(
    py: Python<'_>,
    arguments: std::fmt::Arguments<'_>,
) -> PyErr {
    match message(py, arguments) {
        Ok(message) => builtin_message::<E>(py, message),
        Err(error) => error,
    }
}

/// Optional location notes cannot replace the original exception if their
/// formatting or Python allocation fails.
#[cold]
pub(crate) fn note_format(py: Python<'_>, primary: &PyErr, arguments: std::fmt::Arguments<'_>) {
    if let Ok(message) = message(py, arguments) {
        note(py, primary, message);
    }
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
/// Insufficient capacity is rejected before consuming any truncated value;
/// success preserves the whole original signed value, even when the supplied
/// hint understates its width. The caller chooses the maximum native storage.
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
    // reservation proves the full byte count fits both its allocation and
    // Py_ssize_t. Success initializes all requested signed little-endian
    // bytes; errors and truncation leave len zero. The 3.12 call is the same
    // narrow fallback as pinned PyO3, without private layout reads.
    unsafe {
        #[cfg(not(Py_3_13))]
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
        #[cfg(Py_3_13)]
        {
            let capacity = digits * std::mem::size_of::<u32>();
            let required = pyo3::ffi::PyLong_AsNativeBytes(
                value.as_ptr(),
                buffer.as_mut_ptr().cast(),
                capacity as pyo3::ffi::Py_ssize_t,
                pyo3::ffi::Py_ASNATIVEBYTES_LITTLE_ENDIAN,
            );
            if required < 0 {
                return Err(PyErr::fetch(value.py()));
            }
            // Unlike the 3.12 conversion, this public API reports truncation
            // through its returned size without raising an exception.
            if required as usize > capacity {
                return Err(builtin_message::<pyo3::exceptions::PyOverflowError>(
                    value.py(),
                    "integer does not fit its bounded native storage",
                ));
            }
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
pub(crate) fn client(py: Python<'_>, code: ClientErrorCode, message: impl AsRef<str>) -> PyErr {
    let prepared = || -> PyResult<Bound<'_, PyAny>> {
        let keywords = error_keywords(py, code as u32, false)?;
        let message = PyString::from_bytes(py, message.as_ref().as_bytes())?;
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

/// Format a required driver message with the shared fallible native writer.
#[cold]
pub(crate) fn client_format(
    py: Python<'_>,
    code: ClientErrorCode,
    arguments: std::fmt::Arguments<'_>,
) -> PyErr {
    match message(py, arguments) {
        Ok(message) => client(py, code, message),
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
pub(crate) fn wire(py: Python<'_>, error: impl std::borrow::Borrow<WireError>) -> PyErr {
    let error = error.borrow();
    let detail =
        diagnostics::Native::capture(error, Some("sqlaero_core::protocol::framing::WireError"));
    let mapped = match error {
        WireError::Allocation(_) => memory_error(py),
        WireError::Io(error) | WireError::Decode(error)
            if error.kind() == std::io::ErrorKind::OutOfMemory =>
        {
            memory_error(py)
        }
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
    error: impl std::borrow::Borrow<sqlaero_core::transport::channel::ChannelError>,
) -> PyErr {
    use sqlaero_core::transport::{channel::ChannelError, socket::TransportError};
    let error = error.borrow();
    let detail = diagnostics::Native::capture(
        error,
        Some("sqlaero_core::transport::channel::ChannelError"),
    );
    let mapped = match error {
        ChannelError::Wire(error) => wire(py, error),
        ChannelError::Timeout(error) | ChannelError::Transport(TransportError::Timeout(error)) => {
            timeout(py, *error)
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
/// A bounded, frame-free failure boundary owned only by the Python binding.
/// No exception instance, traceback, arbitrary arguments or custom state survive.
pub(crate) struct SavedFailure {
    typ: Option<Py<PyType>>,
    name: String,
    base: SavedBase,
    args: Py<PyTuple>,
    notes: Py<PyTuple>,
    code: Option<BigInt>,
    server: Option<bool>,
    native: Option<diagnostics::Native>,
    stage: Option<&'static str>,
    summary: Option<String>,
    unavailable: Option<&'static str>,
}
#[derive(Clone, Copy)]
enum SavedBase {
    Library,
    Encode,
    Os,
    Exception,
}
impl SavedBase {
    fn typ(self, py: Python<'_>) -> Bound<'_, PyType> {
        match self {
            Self::Library => py.get_type::<SQLAeroError>(),
            Self::Encode => py.get_type::<pyo3::exceptions::PyUnicodeEncodeError>(),
            Self::Os => py.get_type::<pyo3::exceptions::PyOSError>(),
            Self::Exception => py.get_type::<pyo3::exceptions::PyBaseException>(),
        }
    }
}
/// Reuse the earlier retained-message limit for the whole copied text payload.
/// Four bytes per code point conservatively bounds native Unicode storage and
/// permits lone surrogates without creating an unbounded UTF-8 cache first.
const SAVED_FAILURE_TEXT_BYTES: usize = 4096;
fn saved_intrinsic<'py>(
    value: &Bound<'py, PyAny>,
    base: &Bound<'py, PyType>,
    name: &CStr,
) -> PyResult<Bound<'py, PyAny>> {
    let descriptor = attribute(base.as_any(), name)?;
    // SAFETY: the descriptor comes from a fixed trusted builtin base. Calling
    // its descriptor slot bypasses a user's __getattribute__ or field override.
    unsafe {
        let slot = pyo3::ffi::PyType_GetSlot(
            descriptor.get_type().as_type_ptr(),
            pyo3::ffi::Py_tp_descr_get,
        );
        if slot.is_null() {
            return Err(builtin_message::<PyRuntimeError>(
                value.py(),
                "intrinsic exception descriptor is unavailable",
            ));
        }
        let get: pyo3::ffi::descrgetfunc = std::mem::transmute(slot);
        Bound::from_owned_ptr_or_err(
            value.py(),
            get(
                descriptor.as_ptr(),
                value.as_ptr(),
                value.get_type().as_ptr(),
            ),
        )
    }
}
fn saved_summary(value: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    if !value.is_exact_instance_of::<PyString>() {
        return Ok(None);
    }
    let py = value.py();
    // SAFETY: exact Unicode and a bounded slice. The source is only borrowed
    // during copying; only this escaped finite summary is retained.
    let (length, preview) = unsafe {
        let length = pyo3::ffi::PyUnicode_GetLength(value.as_ptr());
        if length < 0 {
            return Err(PyErr::fetch(py));
        }
        let preview = Bound::<PyAny>::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyUnicode_Substring(value.as_ptr(), 0, length.min(160)),
        )?;
        (length, preview)
    };
    let display = preview.repr()?;
    message(
        py,
        format_args!(
            "{}{}",
            display.to_str()?,
            if length > 160 {
                " [truncated original text]"
            } else {
                ""
            }
        ),
    )
    .map(Some)
}
fn saved_text<'py>(
    value: &Bound<'py, PyAny>,
    remaining: &mut usize,
) -> PyResult<Option<Bound<'py, PyString>>> {
    if !value.is_instance_of::<PyString>() {
        return Ok(None);
    }
    // SAFETY: checked Unicode object, attached thread; length does not allocate.
    let length = unsafe { pyo3::ffi::PyUnicode_GetLength(value.as_ptr()) };
    if length < 0 {
        return Err(PyErr::fetch(value.py()));
    }
    let Some(size) = (length as usize)
        .checked_mul(4)
        .and_then(|n| n.checked_add(16))
    else {
        return Ok(None);
    };
    if size > *remaining {
        return Ok(None);
    }
    *remaining -= size;
    // Explicitly copy bounded code points: a full-range Unicode_Substring can
    // return the source itself, which would retain an original input object.
    // SAFETY: Unicode input and checked bounded size. Both public constructors
    // copy their input; the temporary UCS4 allocation is freed on either exit.
    unsafe {
        let points = pyo3::ffi::PyUnicode_AsUCS4Copy(value.as_ptr());
        if points.is_null() {
            return Err(PyErr::fetch(value.py()));
        }
        let pointer = pyo3::ffi::PyUnicode_FromKindAndData(
            pyo3::ffi::PyUnicode_4BYTE_KIND as i32,
            points.cast(),
            length,
        );
        pyo3::ffi::PyMem_Free(points.cast());
        Bound::<PyAny>::from_owned_ptr_or_err(value.py(), pointer)?
    }
    .cast_into()
    .map(Some)
    .map_err(Into::into)
}
fn saved_native_fits(native: &diagnostics::Native, remaining: &mut usize) -> bool {
    let Some(required) = native
        .message
        .len()
        .checked_add(std::mem::size_of::<diagnostics::Native>())
    else {
        return false;
    };
    if required > *remaining {
        return false;
    }
    *remaining -= required;
    native
        .cause
        .as_deref()
        .is_none_or(|cause| saved_native_fits(cause, remaining))
}
impl SavedFailure {
    pub(crate) fn capture(py: Python<'_>, error: &PyErr) -> PyResult<Self> {
        let value = error.value(py).as_any();
        let typ = value.get_type();
        let mut remaining = SAVED_FAILURE_TEXT_BYTES;
        let name = typ.name()?;
        let name = if let Some(name) = saved_text(name.as_any(), &mut remaining)? {
            message(py, format_args!("{}", name.to_str()?))?
        } else {
            message(
                py,
                format_args!("exception whose type name exceeds the retained diagnostic limit"),
            )?
        };
        let library = value.extract::<PyRef<'_, SQLAeroError>>().ok();
        let base = if library.is_some() {
            SavedBase::Library
        } else if error.is_instance_of::<pyo3::exceptions::PyUnicodeEncodeError>(py) {
            SavedBase::Encode
        } else if error.is_instance_of::<pyo3::exceptions::PyOSError>(py) {
            SavedBase::Os
        } else {
            SavedBase::Exception
        };
        let mut result = Self {
            typ: Some(typ.clone().unbind()),
            name,
            base,
            args: PyTuple::empty(py).unbind(),
            notes: PyTuple::empty(py).unbind(),
            code: None,
            server: None,
            native: None,
            stage: None,
            summary: None,
            unavailable: None,
        };
        let summary = if let Some(error) = &library {
            saved_summary(error.message.bind(py).as_any())?
        } else if matches!(base, SavedBase::Encode) {
            let base = base.typ(py);
            let encoding = saved_summary(&saved_intrinsic(value, &base, c"encoding")?)?;
            let reason = saved_summary(&saved_intrinsic(value, &base, c"reason")?)?;
            let start: isize = saved_intrinsic(value, &base, c"start")?.extract()?;
            let end: isize = saved_intrinsic(value, &base, c"end")?.extract()?;
            Some(message(
                py,
                format_args!(
                    "encoding={}, start={start}, end={end}, reason={}",
                    encoding.as_deref().unwrap_or("not safely available"),
                    reason.as_deref().unwrap_or("not safely available")
                ),
            )?)
        } else {
            let args = saved_intrinsic(
                value,
                &py.get_type::<pyo3::exceptions::PyBaseException>(),
                c"args",
            )?
            .cast_into::<PyTuple>()?;
            let mut summary = None;
            for (index, argument) in args.iter().take(SAVED_FAILURE_TEXT_BYTES).enumerate() {
                if let Some(text) = saved_summary(&argument)? {
                    summary = Some(if args.len() == 1 {
                        text
                    } else {
                        message(py, format_args!("text argument[{index}]={text}"))?
                    });
                    break;
                }
            }
            summary
        };
        if let Some(summary) = summary
            && summary.len() <= remaining
        {
            remaining -= summary.len();
            result.summary = Some(summary);
        }
        // Dynamic exception state is not replayable data. The ordinary custom
        // subclass with inherited display and no extra instance state is safe.
        // SAFETY: a live BaseException owns CPython's intrinsic dictionary;
        // generic dictionary access bypasses user attribute overrides.
        let dictionary = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )?
        }
        .cast_into::<PyDict>()?;
        for (key, _) in dictionary.iter() {
            if key.extract::<&str>().ok() != Some("__notes__") {
                result.unavailable =
                    Some("custom exception attributes cannot be safely reconstructed");
                break;
            }
        }
        let safe_type = base.typ(py);
        // SAFETY: both attached type objects remain owned. Fixed layout sizes
        // are immutable; extra C-extension/builtin fields cannot be initialized
        // safely by this selected base allocator and are never reconstructed.
        let same_layout = unsafe {
            (*typ.as_type_ptr()).tp_basicsize == (*safe_type.as_type_ptr()).tp_basicsize
                && (*typ.as_type_ptr()).tp_itemsize == (*safe_type.as_type_ptr()).tp_itemsize
        };
        if !same_layout {
            result.unavailable = Some(
                "additional native exception layout cannot be safely initialized by the retained base constructor",
            );
        }
        if !attribute(typ.as_any(), c"__str__")?.is(attribute(safe_type.as_any(), c"__str__")?) {
            result.unavailable =
                Some("custom exception display requires state that was not retained");
        }
        if !attribute(typ.as_any(), c"__getattribute__")?
            .is(attribute(safe_type.as_any(), c"__getattribute__")?)
        {
            result.unavailable =
                Some("custom exception attribute access cannot be safely reconstructed");
        }
        match attribute(typ.as_any(), c"__del__") {
            Ok(_) => {
                result.unavailable =
                    Some("custom exception finalization requires state that was not retained")
            }
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) => (),
            Err(error) => return Err(error),
        }
        if matches!(base, SavedBase::Encode) {
            for field in [c"encoding", c"object", c"start", c"end", c"reason"] {
                if !attribute(typ.as_any(), field)?.is(attribute(safe_type.as_any(), field)?) {
                    result.unavailable =
                        Some("custom Unicode diagnostic descriptors cannot be safely copied");
                    break;
                }
            }
        }
        for ancestor in typ.mro().iter() {
            if ancestor.is(&safe_type) {
                break;
            }
            let dict = attribute(&ancestor, c"__dict__")?;
            let slots = PyString::from_bytes(py, b"__slots__")?;
            match dict.get_item(slots) {
                Ok(_) => {
                    result.unavailable =
                        Some("custom exception slots cannot be safely reconstructed");
                    break;
                }
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyKeyError>(py) => (),
                Err(error) => return Err(error),
            }
        }
        if matches!(base, SavedBase::Os)
            && (!saved_intrinsic(value, &safe_type, c"errno")?.is_none()
                || !saved_intrinsic(value, &safe_type, c"filename")?.is_none()
                || !saved_intrinsic(value, &safe_type, c"filename2")?.is_none())
        {
            result.unavailable =
                Some("structured OS error fields require data beyond the retained scalar payload");
        }
        if matches!(base, SavedBase::Exception) {
            let args = saved_intrinsic(
                value,
                &py.get_type::<pyo3::exceptions::PyBaseException>(),
                c"args",
            )?
            .cast_into::<PyTuple>()?;
            if args.len() > 1
                || (args.len() == 1 && !args.get_item(0)?.is_instance_of::<PyString>())
            {
                result.unavailable = Some(
                    "non-text exception arguments cannot be safely reconstructed without retaining original parameters",
                );
            }
        }
        if !saved_intrinsic(
            value,
            &py.get_type::<pyo3::exceptions::PyBaseException>(),
            c"__cause__",
        )?
        .is_none()
            || !saved_intrinsic(
                value,
                &py.get_type::<pyo3::exceptions::PyBaseException>(),
                c"__context__",
            )?
            .is_none()
        {
            result.unavailable = Some(
                "external exception chains cannot be safely retained as bounded scalar failure facts",
            );
        }
        if let Some(library) = &library {
            if let Some(code) = &library.code {
                let bytes = code.bits().div_ceil(8);
                if bytes > remaining as u64 {
                    result.unavailable =
                        Some("original error code exceeds the bounded retained failure facts");
                } else {
                    remaining -= bytes as usize;
                    result.code = Some(code.clone());
                }
            }
            result.server = library.is_server_error;
            let state = library.diagnostics.lock().map_err(|_| {
                builtin_message::<PyRuntimeError>(py, "error diagnostic ownership is unavailable")
            })?;
            result.stage = state.stage;
            if !state.additional.is_empty() {
                result.unavailable = Some(
                    "additional exception graphs are outside the bounded retained failure facts",
                );
            }
            if let Some(native) = &state.native {
                if saved_native_fits(native, &mut remaining) {
                    result.native = Some(native.clone());
                } else {
                    result.unavailable =
                        Some("native cause text exceeds the retained diagnostic limit");
                }
            }
        }
        if result.unavailable.is_none() {
            let mut arguments = Vec::new();
            arguments
                .try_reserve_exact(if matches!(base, SavedBase::Encode) {
                    5
                } else {
                    1
                })
                .map_err(|_| memory_error(py))?;
            if matches!(base, SavedBase::Encode) {
                for key in [c"encoding", c"object"] {
                    match saved_text(&attribute(value, key)?, &mut remaining)? {
                        Some(text) => arguments.push(text.into_any().unbind()),
                        None => {
                            result.unavailable = Some(
                                "Unicode diagnostic text exceeds the retained limit or is not safely copyable",
                            );
                            break;
                        }
                    }
                }
                if result.unavailable.is_none() {
                    for key in [c"start", c"end"] {
                        let number: isize = attribute(value, key)?.extract()?;
                        // SAFETY: attached scalar constructor; NULL preserves
                        // the actual allocation error instead of panicking.
                        let number = unsafe {
                            Bound::<PyAny>::from_owned_ptr_or_err(
                                py,
                                pyo3::ffi::PyLong_FromSsize_t(number),
                            )?
                        };
                        arguments.push(number.unbind());
                    }
                    match saved_text(&attribute(value, c"reason")?, &mut remaining)? {
                        Some(text) => arguments.push(text.into_any().unbind()),
                        None => {
                            result.unavailable =
                                Some("Unicode diagnostic reason exceeds the retained limit")
                        }
                    }
                }
            } else if matches!(base, SavedBase::Library)
                || !saved_intrinsic(
                    value,
                    &py.get_type::<pyo3::exceptions::PyBaseException>(),
                    c"args",
                )?
                .cast_into::<PyTuple>()?
                .is_empty()
            {
                let text = match &library {
                    Some(error) => error.message.bind(py).clone(),
                    None => value.str()?,
                };
                match saved_text(text.as_any(), &mut remaining)? {
                    Some(text) => arguments.push(text.into_any().unbind()),
                    None => {
                        result.unavailable =
                            Some("original exception text exceeds the retained diagnostic limit")
                    }
                }
            }
            if result.unavailable.is_none() {
                result.args = output::tuple_from_ready_vec(py, arguments)?.unbind();
            }
        }
        let (notes, complete) = diagnostics::retained_notes(value, &mut remaining)?;
        result.notes = notes.unbind();
        if !complete {
            result
                .unavailable
                .get_or_insert("not all original notes fit the bounded retained failure facts");
        }
        if result.unavailable.is_some() {
            // An unreconstructible boundary needs its copied type name only.
            // Do not keep arbitrary class dictionaries/closures alive for it.
            result.typ = None;
        }
        Ok(result)
    }
    pub(crate) fn traverse(
        &self,
        visit: &pyo3::class::PyVisit<'_>,
    ) -> Result<(), pyo3::class::PyTraverseError> {
        visit.call(&self.typ)?;
        visit.call(&self.args)?;
        visit.call(&self.notes)
    }
    pub(crate) fn restore(&self, py: Python<'_>) -> PyErr {
        if let Some(reason) = self.unavailable {
            let error = client_format(
                py,
                ClientErrorCode::SessionState,
                format_args!(
                    "The original {} failure cannot be reconstructed safely at this retained boundary: {reason}. Original reason summary: {}; original code={:?}, is_server_error={:?}. The immediate original exception was preserved in full; this is a local diagnostic limitation, not a new database failure.",
                    self.name,
                    self.summary
                        .as_deref()
                        .unwrap_or("not safely available from intrinsic text diagnostics"),
                    self.code.as_ref().and_then(|code| u32::try_from(code).ok()),
                    self.server
                ),
            );
            return match self.restore_notes(py, error.value(py).as_any()) {
                Ok(()) => error,
                Err(resource) => resource,
            };
        }
        match self.rebuild(py) {
            Ok(value) => PyErr::from_value(value),
            Err(error) => error,
        }
    }
    fn restore_notes(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let source = self.notes.bind(py);
        // SAFETY: exact new list with a bounded tuple length. NULL is checked;
        // each successful SetItem steals only its own newly owned reference.
        let notes = unsafe {
            let list = Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyList_New(source.len() as isize),
            )?;
            for (index, note) in source.iter().enumerate() {
                if pyo3::ffi::PyList_SetItem(list.as_ptr(), index as isize, note.into_ptr()) < 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            list
        };
        // Bypass a custom __setattr__/__notes__ descriptor. Only the exception's
        // intrinsic dictionary is changed; arbitrary old attributes are absent.
        // SAFETY: this newly allocated BaseException owns the intrinsic dict;
        // the returned dictionary is a new reference or the real Python error.
        let dict = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )?
        }
        .cast_into::<PyDict>()?;
        dict.set_item(PyString::from_bytes(py, b"__notes__")?, notes)
    }
    fn rebuild<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let Some(typ) = &self.typ else {
            return Err(builtin_message::<PyRuntimeError>(
                py,
                "retained failure type is unavailable",
            ));
        };
        let base = self.base.typ(py);
        let args = self.args.bind(py);
        let keywords = dictionary(py)?;
        if matches!(self.base, SavedBase::Library) {
            if let Some(code) = &self.code {
                keywords.set_item(PyString::from_bytes(py, b"code")?, code_object(py, code)?)?;
            }
            if let Some(server) = self.server {
                keywords.set_item(PyString::from_bytes(py, b"is_server_error")?, server)?;
            }
        }
        // SAFETY: these slots belong to the fixed trusted built-in/library base,
        // never the retained user's type. Both accept a true subtype and return
        // its fully allocated layout. No user __new__ or __init__ is dispatched.
        let value = unsafe {
            let slot = pyo3::ffi::PyType_GetSlot(base.as_type_ptr(), pyo3::ffi::Py_tp_new);
            if slot.is_null() {
                return Err(builtin_message::<PyRuntimeError>(
                    py,
                    "safe exception allocator is unavailable",
                ));
            }
            let create: pyo3::ffi::newfunc = std::mem::transmute(slot);
            let value = Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                create(typ.bind(py).as_type_ptr(), args.as_ptr(), keywords.as_ptr()),
            )?;
            if !matches!(self.base, SavedBase::Library) {
                let slot = pyo3::ffi::PyType_GetSlot(base.as_type_ptr(), pyo3::ffi::Py_tp_init);
                if !slot.is_null() {
                    let initialize: pyo3::ffi::initproc = std::mem::transmute(slot);
                    if initialize(value.as_ptr(), args.as_ptr(), keywords.as_ptr()) < 0 {
                        return Err(PyErr::fetch(py));
                    }
                }
            }
            value
        };
        self.restore_notes(py, &value)?;
        if let Ok(library) = value.extract::<PyRef<'_, SQLAeroError>>() {
            let mut state = library.diagnostics.lock().map_err(|_| {
                builtin_message::<PyRuntimeError>(
                    py,
                    "restored diagnostic ownership is unavailable",
                )
            })?;
            state.native = self.native.clone();
            state.stage = self.stage;
        }
        Ok(value)
    }
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
    error: impl std::borrow::Borrow<sqlaero_core::session::query::QueryError>,
    codec: Option<sqlaero_core::charset::output::OutputCodec>,
) -> PyErr {
    use sqlaero_core::{result::text::ResultError, session::query::QueryError};
    match error.borrow() {
        QueryError::Local(error) => {
            use sqlaero_core::protocol::local_infile::LocalError;
            let mapped = match error {
                LocalError::Wire(error) => wire(py, error),
                LocalError::Channel(error) => channel(py, error),
                LocalError::Source(error) => io_error(py, error),
            };
            native(
                py,
                &mapped,
                diagnostics::Native::capture(
                    error,
                    Some("sqlaero_core::protocol::local_infile::LocalError"),
                ),
            );
            mapped
        }
        QueryError::Wire(error) | QueryError::Result(ResultError::Wire(error)) => wire(py, error),
        QueryError::Channel(error) => channel(py, error),
        QueryError::Result(ResultError::Memory(error)) | QueryError::Paused(error) => {
            crate::output::memory(py, error)
        }
        QueryError::Result(ResultError::Server(error)) => server_with_codec(
            py,
            error.error_code(),
            error.sql_state_ref().map(|state| state.as_bytes()),
            error.message_ref(),
            codec,
        ),
        error @ QueryError::Result(ResultError::Unsupported(_)) => client(
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
        ConnectError::TlsHandshake {
            minimum,
            maximum,
            source,
        } => {
            let primary = channel(py, source);
            note_format(
                py,
                &primary,
                format_args!(
                    "allowed TLS range: {}..{}",
                    minimum.as_str(),
                    maximum.map_or(
                        "None (no extra maximum)",
                        sqlaero_core::transport::tls::Version::as_str
                    )
                ),
            );
            primary
        }
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
pub(crate) fn note(py: Python<'_>, primary: &PyErr, message: impl AsRef<str>) {
    {
        let Ok(message) = PyString::from_bytes(py, message.as_ref().as_bytes()) else {
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

/// Preserve one certificate-property failure through the shared native-source
/// diagnostics, without per-property exception types or cached Python errors.
pub(crate) fn tls_certificate<E: std::error::Error + 'static>(
    py: Python<'_>,
    certificate_index: usize,
    property: &'static str,
    source: &E,
) -> PyErr {
    let primary = client_format(
        py,
        ClientErrorCode::TlsCertificateContent,
        format_args!("Cannot read peer_certificates[{certificate_index}].{property}: {source}"),
    );
    native(
        py,
        &primary,
        diagnostics::Native::capture(source, Some(std::any::type_name::<E>())),
    );
    at(py, primary, "tls_certificate_details")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn wire_out_of_memory_preserves_python_memory_error() {
        Python::initialize();
        Python::attach(|py| {
            // Controlled error kinds verify mapping; no global/native OOM is
            // induced, and compression's typed classification is tested there.
            for source in [
                WireError::Io(io::ErrorKind::OutOfMemory.into()),
                WireError::Decode(io::ErrorKind::OutOfMemory.into()),
            ] {
                let error = wire(py, source);
                assert!(error.is_instance_of::<pyo3::exceptions::PyMemoryError>(py));
            }
        });
    }

    #[test]
    fn ordinary_wire_errors_keep_registered_client_diagnostics() {
        Python::initialize();
        Python::attach(|py| {
            for (source, expected) in [
                (
                    WireError::Io(io::ErrorKind::ConnectionReset.into()),
                    ClientErrorCode::Transport,
                ),
                (
                    WireError::Decode(io::ErrorKind::InvalidData.into()),
                    ClientErrorCode::Protocol,
                ),
            ] {
                let error = wire(py, source);
                assert!(!error.is_instance_of::<pyo3::exceptions::PyMemoryError>(py));
                assert_eq!(
                    error
                        .value(py)
                        .getattr("code")
                        .unwrap()
                        .extract::<u32>()
                        .unwrap(),
                    expected as u32
                );
                assert!(
                    !error
                        .value(py)
                        .getattr("is_server_error")
                        .unwrap()
                        .extract::<bool>()
                        .unwrap()
                );
            }
        });
    }
}
