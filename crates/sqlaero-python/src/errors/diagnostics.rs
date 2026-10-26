//! Immutable error snapshots at the Python boundary, without retaining frames.
//!
//! Python GC owns graph edges so genuine shared/cyclic exception chains remain
//! readable and collectible. The short-lived capture map only preserves object
//! identity during capture; it is not a persistent registry or public node ID.

use super::SQLAeroError;
use num_bigint::BigInt;
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple, PyType};
use std::collections::{HashMap, HashSet, VecDeque};

/// Native facts copied only on a real failure, before its Rust source is dropped.
#[derive(Clone)]
pub(super) struct Native {
    pub name: Option<&'static str>,
    pub message: String,
    pub os_errno: Option<i32>,
    pub cause: Option<Box<Self>>,
}
impl Native {
    pub(super) fn capture(
        error: &(dyn std::error::Error + 'static),
        name: Option<&'static str>,
    ) -> Self {
        let io = error.downcast_ref::<std::io::Error>();
        // Transparent native wrappers can delegate Error::source past their
        // immediate stored error. Preserve that actual owned edge explicitly.
        let wrapped = error
            .downcast_ref::<sqlaero_core::protocol::framing::WireError>()
            .and_then(|error| match error {
                sqlaero_core::protocol::framing::WireError::Io(io)
                | sqlaero_core::protocol::framing::WireError::Decode(io) => {
                    Some(Self::capture(io, Some("std::io::Error")))
                }
                _ => None,
            })
            .or_else(|| {
                error
                    .downcast_ref::<sqlaero_core::transport::channel::ChannelError>()
                    .map(|error| match error {
                        sqlaero_core::transport::channel::ChannelError::Transport(error) => {
                            Self::capture(
                                error,
                                Some("sqlaero_core::transport::socket::TransportError"),
                            )
                        }
                        sqlaero_core::transport::channel::ChannelError::Wire(error) => {
                            Self::capture(error, Some("sqlaero_core::protocol::framing::WireError"))
                        }
                        sqlaero_core::transport::channel::ChannelError::Tls(error) => {
                            Self::capture(error, Some("rustls::Error"))
                        }
                        sqlaero_core::transport::channel::ChannelError::Timeout(error) => {
                            Self::capture(
                                error,
                                Some("sqlaero_core::transport::deadline::TimeoutKind"),
                            )
                        }
                    })
            })
            .or_else(|| {
                error
                    .downcast_ref::<sqlaero_core::transport::socket::TransportError>()
                    .map(|error| match error {
                        sqlaero_core::transport::socket::TransportError::Io(error) => {
                            Self::capture(error, Some("std::io::Error"))
                        }
                        sqlaero_core::transport::socket::TransportError::Timeout(error) => {
                            Self::capture(
                                error,
                                Some("sqlaero_core::transport::deadline::TimeoutKind"),
                            )
                        }
                    })
            });
        Self {
            name: io.map(|_| "std::io::Error").or(name),
            message: error.to_string(),
            os_errno: io.and_then(std::io::Error::raw_os_error),
            cause: wrapped.map(Box::new).or_else(|| {
                error
                    .source()
                    .map(|source| Box::new(Self::capture(source, None)))
            }),
        }
    }
}

/// An exception's current library diagnostics, separate from returned snapshots.
#[derive(Default)]
pub(super) struct State {
    pub stage: Option<&'static str>,
    pub native: Option<Native>,
    pub additional: Vec<Py<ErrorInfo>>,
    pub cached: Option<Py<ErrorInfo>>,
}
impl State {
    pub(super) fn traverse(&self, visit: &PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.cached)?;
        for value in &self.additional {
            visit.call(value)?;
        }
        Ok(())
    }
    pub(super) fn clear(&mut self) {
        self.cached = None;
        self.additional.clear();
    }
}

/// An immutable diagnostic snapshot, not an exception or a replayable operation.
///
/// Cause and context preserve their actual independent relationships. Python
/// frames, exception instances and local variables are not kept by this object.
/// Traceback text and the additional-errors tuple are formatted lazily and reused.
#[pyclass(module = "sqlaero.errors", weakref)]
pub(crate) struct ErrorInfo {
    stage: Option<String>,
    exception_type: Option<Py<PyType>>,
    native_type: Option<String>,
    message: Py<PyString>,
    display: Py<PyString>,
    code: Option<BigInt>,
    os_errno: Option<i32>,
    is_server_error: Option<bool>,
    cause: Option<Py<Self>>,
    context: Option<Py<Self>>,
    suppress_context: bool,
    stack: Option<Py<PyAny>>,
    notes: Py<PyTuple>,
    additional: Vec<Py<Self>>,
    additional_tuple: Option<Py<PyTuple>>,
    formatted: Option<Py<PyString>>,
    formatted_ready: bool,
}

fn copied_text<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyString>> {
    // SAFETY: callers pass a checked str. This normalizes a subclass without
    // retaining it or losing lone surrogates through a Rust UTF-8 conversion.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(
            value.py(),
            pyo3::ffi::PyUnicode_FromObject(value.as_ptr()),
        )?
    }
    .cast_into()
    .map_err(Into::into)
}
fn notes<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyTuple>> {
    let py = value.py();
    let Ok(notes) = value.getattr("__notes__") else {
        return Ok(PyTuple::empty(py));
    };
    let Ok(notes) = notes.cast::<PyList>() else {
        return Ok(PyTuple::empty(py));
    };
    let mut frozen = Vec::new();
    for note in notes.iter() {
        if note.is_instance_of::<PyString>() {
            frozen.push(copied_text(&note)?.unbind());
        }
    }
    PyTuple::new(py, frozen)
}
fn stack(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    let traceback = value.getattr("__traceback__")?;
    if traceback.is_none() {
        return Ok(None);
    }
    let module = py.import("traceback")?;
    let frames = module.getattr("walk_tb")?.call1((traceback,))?;
    let keywords = pyo3::types::PyDict::new(py);
    keywords.set_item("capture_locals", false)?;
    keywords.set_item("lookup_lines", false)?;
    // StackSummary consumes walk_tb now. It stores source locations, not the
    // traceback, live frames or their locals; line formatting remains lazy.
    Ok(Some(
        module
            .getattr("StackSummary")?
            .call_method("extract", (frames,), Some(&keywords))?
            .unbind(),
    ))
}
fn fresh(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    stage: Option<&str>,
    include_additional: bool,
) -> PyResult<Py<ErrorInfo>> {
    let display = copied_text(value.str()?.as_any())?.unbind();
    let (message, code, is_server_error, native, additional, actual_stage) = if let Ok(error) =
        value.extract::<PyRef<'_, SQLAeroError>>()
    {
        let state = error.diagnostics.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("error diagnostic ownership is unavailable")
        })?;
        (
            error.message.clone_ref(py),
            error.code.clone(),
            error.is_server_error,
            state.native.clone(),
            if include_additional {
                state
                    .additional
                    .iter()
                    .map(|info| info.clone_ref(py))
                    .collect()
            } else {
                Vec::new()
            },
            stage
                .map(str::to_owned)
                .or_else(|| state.stage.map(str::to_owned)),
        )
    } else {
        (
            display.clone_ref(py),
            None,
            None,
            None,
            Vec::new(),
            stage.map(str::to_owned),
        )
    };
    let os_errno = if value.is_instance_of::<pyo3::exceptions::PyOSError>() {
        value.getattr("errno")?.extract::<Option<i32>>()?
    } else {
        native.as_ref().and_then(|native| native.os_errno)
    };
    let result = Py::new(
        py,
        ErrorInfo {
            stage: actual_stage,
            exception_type: Some(value.get_type().unbind()),
            native_type: native
                .as_ref()
                .and_then(|native| native.name.map(str::to_owned)),
            message,
            display,
            code,
            os_errno,
            is_server_error,
            cause: None,
            context: None,
            suppress_context: value.getattr("__suppress_context__")?.extract()?,
            stack: stack(py, value)?,
            notes: notes(value)?.unbind(),
            additional,
            additional_tuple: None,
            formatted: None,
            formatted_ready: false,
        },
    )?;
    if let Some(cause) = native.as_ref().and_then(|native| native.cause.as_deref()) {
        result.borrow_mut(py).cause = Some(from_native(py, cause)?);
    }
    Ok(result)
}
fn from_native(py: Python<'_>, native: &Native) -> PyResult<Py<ErrorInfo>> {
    let message = crate::output::text(
        py,
        native.message.as_bytes(),
        sqlaero_core::charset::output::OutputCodec::for_collation(255),
        true,
        None,
    )?
    .unbind();
    Py::new(
        py,
        ErrorInfo {
            stage: None,
            exception_type: None,
            native_type: native.name.map(str::to_owned),
            display: message.clone_ref(py),
            message,
            code: None,
            os_errno: native.os_errno,
            is_server_error: None,
            cause: native
                .cause
                .as_deref()
                .map(|cause| from_native(py, cause))
                .transpose()?,
            context: None,
            suppress_context: false,
            stack: None,
            notes: PyTuple::empty(py).unbind(),
            additional: Vec::new(),
            additional_tuple: None,
            formatted: None,
            formatted_ready: false,
        },
    )
}

/// Capture actual Python relationships iteratively, before the originals leave
/// their handling boundary. Shared references and cycles keep their real edges.
pub(super) fn capture(
    py: Python<'_>,
    error: &PyErr,
    stage: Option<&str>,
) -> PyResult<Py<ErrorInfo>> {
    let original = error.value(py).as_any();
    let root = fresh(py, original, stage, true)?;
    let mut seen = HashMap::from([(original.as_ptr() as usize, root.clone_ref(py))]);
    let mut pending = VecDeque::from([(original.clone().unbind(), root.clone_ref(py))]);
    while let Some((original, info)) = pending.pop_front() {
        for (attribute, is_cause) in [("__cause__", true), ("__context__", false)] {
            let target = original.bind(py).getattr(attribute)?;
            if target.is_none() {
                continue;
            }
            let address = target.as_ptr() as usize;
            let node = if let Some(node) = seen.get(&address) {
                node.clone_ref(py)
            } else {
                let node = fresh(py, &target, None, false)?;
                seen.insert(address, node.clone_ref(py));
                pending.push_back((target.unbind(), node.clone_ref(py)));
                node
            };
            if is_cause {
                info.borrow_mut(py).cause = Some(node);
            } else {
                info.borrow_mut(py).context = Some(node);
            }
        }
    }
    Ok(root)
}

fn label(py: Python<'_>, info: &ErrorInfo, qualified: bool) -> PyResult<String> {
    if let Some(typ) = &info.exception_type {
        let typ = typ.bind(py);
        let name = typ.getattr("__qualname__")?.extract::<String>()?;
        let module = typ.getattr("__module__")?.extract::<String>()?;
        return Ok(if qualified && module != "builtins" {
            format!("{module}.{name}")
        } else {
            name
        });
    }
    Ok(info
        .native_type
        .clone()
        .unwrap_or_else(|| "NativeError".to_owned()))
}
fn format_traceback(py: Python<'_>, root: &Bound<'_, ErrorInfo>) -> PyResult<Option<Py<PyString>>> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(root.clone().unbind());
    let mut cyclic = false;
    let mut has_frames = false;
    while let Some(info) = current {
        if !seen.insert(info.as_ptr() as usize) {
            cyclic = true;
            break;
        }
        let value = info.borrow(py);
        let (next, relation) = if let Some(cause) = &value.cause {
            (
                Some(cause.clone_ref(py)),
                "\nThe above exception was the direct cause of the following exception:\n\n",
            )
        } else if !value.suppress_context && value.context.is_some() {
            (
                value.context.as_ref().map(|context| context.clone_ref(py)),
                "\nDuring handling of the above exception, another exception occurred:\n\n",
            )
        } else {
            (None, "")
        };
        has_frames |= value.stack.is_some();
        drop(value);
        chain.push((info, relation));
        current = next;
    }
    if !has_frames {
        return Ok(None);
    }
    let mut lines: Vec<Py<PyString>> = Vec::new();
    if cyclic {
        lines.push(PyString::new(py, "[cycle in captured exception chain]\n").unbind());
    }
    let mut first = true;
    for (info, relation) in chain.into_iter().rev() {
        if !first && !relation.is_empty() {
            lines.push(PyString::new(py, relation).unbind());
        }
        first = false;
        let value = info.borrow(py);
        let stack = value.stack.as_ref().map(|stack| stack.clone_ref(py));
        let display = value.display.clone_ref(py);
        let notes = value.notes.clone_ref(py);
        let name = label(py, &value, true)?;
        drop(value);
        if let Some(stack) = stack {
            lines.push(PyString::new(py, "Traceback (most recent call last):\n").unbind());
            for line in stack.bind(py).call_method0("format")?.try_iter()? {
                lines.push(copied_text(&line?)?.unbind());
            }
        }
        lines.push(
            PyString::new(py, "{}: {}\n")
                .call_method1("format", (name, display))?
                .cast_into::<PyString>()?
                .unbind(),
        );
        for note in notes.bind(py).iter() {
            lines.push(
                PyString::new(py, "{}\n")
                    .call_method1("format", (note,))?
                    .cast_into::<PyString>()?
                    .unbind(),
            );
        }
    }
    Ok(Some(
        PyString::new(py, "")
            .call_method1("join", (PyList::new(py, lines)?,))?
            .cast_into::<PyString>()?
            .unbind(),
    ))
}

#[pymethods]
impl ErrorInfo {
    /// Reliable handling stage, or None when no stage was recorded.
    #[getter]
    fn stage(&self) -> Option<&str> {
        self.stage.as_deref()
    }
    /// Actual Python exception class; pure Rust source nodes return None.
    #[getter]
    fn exception_type(&self, py: Python<'_>) -> Option<Py<PyType>> {
        self.exception_type
            .as_ref()
            .map(|value| value.clone_ref(py))
    }
    /// Reliable native type name, when known; not a stable matching key.
    #[getter]
    fn native_type(&self) -> Option<&str> {
        self.native_type.as_deref()
    }
    /// Original node message captured without changing the primary arguments.
    #[getter]
    fn message(&self, py: Python<'_>) -> Py<PyString> {
        self.message.clone_ref(py)
    }
    /// This node's actual library code, or None for an unnumbered node.
    #[getter]
    fn code(&self) -> Option<&BigInt> {
        self.code.as_ref()
    }
    /// Actual OS error number of this node alone, when one was recorded.
    #[getter]
    fn os_errno(&self) -> Option<i32> {
        self.os_errno
    }
    /// This library node's declared source, or None when not declared.
    #[getter]
    fn is_server_error(&self) -> Option<bool> {
        self.is_server_error
    }
    /// Independent explicit cause; shared and cyclic relationships are retained.
    #[getter]
    fn cause(&self, py: Python<'_>) -> Option<Py<Self>> {
        self.cause.as_ref().map(|value| value.clone_ref(py))
    }
    /// Actual implicit Python context, even when suppressed from display.
    #[getter]
    fn context(&self, py: Python<'_>) -> Option<Py<Self>> {
        self.context.as_ref().map(|value| value.clone_ref(py))
    }
    /// Original Python context-display suppression flag.
    #[getter]
    fn suppress_context(&self) -> bool {
        self.suppress_context
    }
    /// Frozen notes in original order, independent of later exception changes.
    #[getter]
    fn notes(&self, py: Python<'_>) -> Py<PyTuple> {
        self.notes.clone_ref(py)
    }
    /// Captured independent later failures, in actual order; cached as a tuple.
    #[getter]
    fn additional_errors(slf: &Bound<'_, Self>) -> PyResult<Py<PyTuple>> {
        let py = slf.py();
        if let Some(cached) = &slf.borrow().additional_tuple {
            return Ok(cached.clone_ref(py));
        }
        let values: Vec<_> = slf
            .borrow()
            .additional
            .iter()
            .map(|value| value.clone_ref(py))
            .collect();
        let value = PyTuple::new(py, values)?.unbind();
        slf.borrow_mut().additional_tuple = Some(value.clone_ref(py));
        Ok(value)
    }
    /// Captured Python traceback/chain text, formatted once without live frames.
    #[getter]
    fn traceback(slf: &Bound<'_, Self>) -> PyResult<Option<Py<PyString>>> {
        let py = slf.py();
        if slf.borrow().formatted_ready {
            return Ok(slf
                .borrow()
                .formatted
                .as_ref()
                .map(|value| value.clone_ref(py)));
        }
        let formatted = format_traceback(py, slf)?;
        let mut value = slf.borrow_mut();
        value.formatted_ready = true;
        value.formatted = formatted.as_ref().map(|value| value.clone_ref(py));
        Ok(formatted)
    }
    fn __str__(&self, py: Python<'_>) -> PyResult<Py<PyString>> {
        let escaped = self.message.bind(py).repr()?;
        let content = escaped.get_item(pyo3::types::PySlice::new(py, 1, -1, 1))?;
        Ok(
            PyString::new(py, "{} [stage={}, code={}, is_server_error={}]: {}")
                .call_method1(
                    "format",
                    (
                        label(py, self, false)?,
                        self.stage.as_deref(),
                        self.code.as_ref(),
                        self.is_server_error,
                        content,
                    ),
                )?
                .cast_into::<PyString>()?
                .unbind(),
        )
    }
    fn __repr__(&self, py: Python<'_>) -> PyResult<Py<PyString>> {
        let exception_type = if self.exception_type.is_some() {
            label(py, self, true)?
        } else {
            "None".to_owned()
        };
        Ok(PyString::new(py, "ErrorInfo(stage={!r}, exception_type={}, native_type={!r}, code={}, os_errno={}, is_server_error={}, message={!r})")
            .call_method1("format", (self.stage.as_deref(), exception_type, self.native_type.as_deref(), self.code.as_ref(), self.os_errno, self.is_server_error, self.message.bind(py)))?
            .cast_into::<PyString>()?.unbind())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.exception_type)?;
        visit.call(&self.message)?;
        visit.call(&self.display)?;
        visit.call(&self.cause)?;
        visit.call(&self.context)?;
        visit.call(&self.stack)?;
        visit.call(&self.notes)?;
        visit.call(&self.additional_tuple)?;
        visit.call(&self.formatted)?;
        for error in &self.additional {
            visit.call(error)?;
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.exception_type = None;
        self.cause = None;
        self.context = None;
        self.stack = None;
        self.additional.clear();
        self.additional_tuple = None;
        self.formatted = None;
    }
}
