//! Immutable error snapshots at the Python boundary, without retaining frames.
//!
//! Python GC owns graph edges so genuine shared/cyclic exception chains remain
//! readable and collectible. The short-lived capture map only preserves object
//! identity during capture; it is not a persistent registry or public node ID.

use super::{SQLAeroError, attribute, code_object};
use crate::output;
use num_bigint::BigInt;
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyAttributeError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyList, PyString, PyTuple, PyType};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

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
    pub generation: u64,
}
impl State {
    /// Retire the current representation after recording a real new fact.
    /// The caller must release its guard before dropping the returned reference.
    pub(super) fn invalidate(&mut self) -> Option<Py<ErrorInfo>> {
        self.generation = self.generation.wrapping_add(1);
        self.cached.take()
    }
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
    additional_tuple: OnceLock<Py<PyTuple>>,
    formatted: OnceLock<Option<Py<PyString>>>,
}

/// Own only this capture's new nodes until all relationship writes finish.
struct Construction<'py> {
    py: Python<'py>,
    nodes: Vec<Py<ErrorInfo>>,
    published: bool,
}
impl<'py> Construction<'py> {
    fn new(py: Python<'py>) -> Self {
        Self {
            py,
            nodes: Vec::new(),
            published: false,
        }
    }
    fn object(&mut self, value: ErrorInfo) -> PyResult<Py<ErrorInfo>> {
        self.nodes
            .try_reserve(1)
            .map_err(|_| super::memory_error(self.py))?;
        let value = Py::new(self.py, value)?;
        // SAFETY: this is a new, attached, privately owned GC object. No
        // callback, allocation, detach, blocking operation or safepoint occurs
        // after construction and before hiding it from GC heap enumeration.
        unsafe { pyo3::ffi::PyObject_GC_UnTrack(value.as_ptr().cast()) };
        // Capacity was reserved before construction; these operations only
        // increment and move an owned reference while the node stays private.
        self.nodes.push(value.clone_ref(self.py));
        Ok(value)
    }
    fn publish(&mut self) {
        for value in &self.nodes {
            // SAFETY: every node belongs to this capture, is still untracked,
            // and all graph fields are now complete and will no longer change.
            unsafe { pyo3::ffi::PyObject_GC_Track(value.as_ptr().cast()) };
        }
        self.published = true;
    }
}
impl Drop for Construction<'_> {
    fn drop(&mut self) {
        if !self.published {
            // Keep all new nodes owned and hidden while breaking partial
            // cycles. Each edge targets another owned new node, so clearing it
            // cannot deallocate that node or run a Python callback here.
            // Previously published additional-error graphs are never modified.
            for value in &self.nodes {
                let mut value = value.borrow_mut(self.py);
                value.cause = None;
                value.context = None;
            }
            // PyO3 deallocation accepts untracked objects. Failed partial
            // snapshots never become visible just to make them collectible.
        }
    }
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
    let notes = match attribute(value, c"__notes__") {
        Ok(notes) => notes,
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
            return Ok(PyTuple::empty(py));
        }
        Err(error) => return Err(error),
    };
    let Ok(notes) = notes.cast::<PyList>() else {
        return Ok(PyTuple::empty(py));
    };
    let mut frozen = Vec::new();
    for note in notes.iter() {
        if note.is_instance_of::<PyString>() {
            frozen.push(copied_text(&note)?.into_any().unbind());
        }
    }
    output::tuple_from_ready_vec(py, frozen)
}
fn stack(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    let traceback = attribute(value, c"__traceback__")?;
    if traceback.is_none() {
        return Ok(None);
    }
    let module = super::module(py, c"traceback")?;
    let frames = attribute(module.as_any(), c"walk_tb")?.call1((traceback,))?;
    let keywords = super::dictionary(py)?;
    keywords.set_item(PyString::from_bytes(py, b"capture_locals")?, false)?;
    keywords.set_item(PyString::from_bytes(py, b"lookup_lines")?, false)?;
    // StackSummary consumes walk_tb now. It stores source locations, not the
    // traceback, live frames or their locals; line formatting remains lazy.
    Ok(Some(
        attribute(module.as_any(), c"StackSummary")?
            .call_method(
                PyString::from_bytes(py, b"extract")?,
                (frames,),
                Some(&keywords),
            )?
            .unbind(),
    ))
}
fn fresh(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    stage: Option<&str>,
    include_additional: bool,
    construction: &mut Construction<'_>,
) -> PyResult<Py<ErrorInfo>> {
    let display = copied_text(value.str()?.as_any())?.unbind();
    let (message, code, is_server_error, native, additional, actual_stage) =
        if let Ok(error) = value.extract::<PyRef<'_, SQLAeroError>>() {
            let state = error.diagnostics.lock().map_err(|_| {
                crate::errors::builtin_message::<PyRuntimeError>(
                    py,
                    "error diagnostic ownership is unavailable",
                )
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
        attribute(value, c"errno")?.extract::<Option<i32>>()?
    } else {
        native.as_ref().and_then(|native| native.os_errno)
    };
    let result = construction.object(ErrorInfo {
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
        suppress_context: attribute(value, c"__suppress_context__")?.extract()?,
        stack: stack(py, value)?,
        notes: notes(value)?.unbind(),
        additional,
        additional_tuple: OnceLock::new(),
        formatted: OnceLock::new(),
    })?;
    if let Some(cause) = native.as_ref().and_then(|native| native.cause.as_deref()) {
        let cause = from_native(py, cause, construction)?;
        result.borrow_mut(py).cause = Some(cause);
    }
    Ok(result)
}
fn from_native(
    py: Python<'_>,
    native: &Native,
    construction: &mut Construction<'_>,
) -> PyResult<Py<ErrorInfo>> {
    let message = crate::output::text(
        py,
        native.message.as_bytes(),
        sqlaero_core::charset::output::OutputCodec::for_collation(255),
        true,
        None,
    )?
    .unbind();
    let cause = native
        .cause
        .as_deref()
        .map(|cause| from_native(py, cause, construction))
        .transpose()?;
    construction.object(ErrorInfo {
        stage: None,
        exception_type: None,
        native_type: native.name.map(str::to_owned),
        display: message.clone_ref(py),
        message,
        code: None,
        os_errno: native.os_errno,
        is_server_error: None,
        cause,
        context: None,
        suppress_context: false,
        stack: None,
        notes: PyTuple::empty(py).unbind(),
        additional: Vec::new(),
        additional_tuple: OnceLock::new(),
        formatted: OnceLock::new(),
    })
}

/// Capture actual Python relationships iteratively, before the originals leave
/// their handling boundary. Shared references and cycles keep their real edges.
pub(super) fn capture(
    py: Python<'_>,
    error: &PyErr,
    stage: Option<&str>,
) -> PyResult<Py<ErrorInfo>> {
    let root_source = error.value(py).as_any();
    let mut construction = Construction::new(py);
    let root = fresh(py, root_source, stage, true, &mut construction)?;
    let root_address = root_source.as_ptr() as usize;
    let mut seen = None;
    let mut pending = VecDeque::new();
    // Process the root on the stack. Leaf captures need no graph-work storage.
    let mut current = Some((root_source.clone().unbind(), root.clone_ref(py)));
    while let Some((original, info)) = current {
        for (name, is_cause) in [(c"__cause__", true), (c"__context__", false)] {
            let target = attribute(original.bind(py), name)?;
            if target.is_none() {
                continue;
            }
            // Own the root before the first edge lookup. Every identity key
            // keeps its source alive until capture ends, including temporary
            // exceptions returned by dynamic relationship getters.
            let seen = seen.get_or_insert_with(|| {
                HashMap::from([(
                    root_address,
                    (root_source.clone().unbind(), root.clone_ref(py)),
                )])
            });
            let address = target.as_ptr() as usize;
            let node = if let Some((_, node)) = seen.get(&address) {
                node.clone_ref(py)
            } else {
                let node = fresh(py, &target, None, false, &mut construction)?;
                seen.insert(address, (target.clone().unbind(), node.clone_ref(py)));
                pending.push_back((target.unbind(), node.clone_ref(py)));
                node
            };
            if is_cause {
                info.borrow_mut(py).cause = Some(node);
            } else {
                info.borrow_mut(py).context = Some(node);
            }
        }
        // Release this work item before advancing, as in the previous queue loop.
        drop(info);
        drop(original);
        current = pending.pop_front();
    }
    construction.publish();
    // Source finalizers may call Python. Release the capture-only owners after
    // graph completion, outside Rust locks; snapshots do not retain sources.
    drop(seen);
    Ok(root)
}

fn label(py: Python<'_>, info: &ErrorInfo, qualified: bool) -> PyResult<String> {
    if let Some(typ) = &info.exception_type {
        let typ = typ.bind(py);
        let name = attribute(typ.as_any(), c"__qualname__")?.extract::<String>()?;
        let module = attribute(typ.as_any(), c"__module__")?.extract::<String>()?;
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
    let mut lines: Vec<Py<PyAny>> = Vec::new();
    if cyclic {
        lines.push(
            PyString::from_bytes(py, b"[cycle in captured exception chain]\n")?
                .into_any()
                .unbind(),
        );
    }
    let mut first = true;
    for (info, relation) in chain.into_iter().rev() {
        if !first && !relation.is_empty() {
            lines.push(
                PyString::from_bytes(py, relation.as_bytes())?
                    .into_any()
                    .unbind(),
            );
        }
        first = false;
        let value = info.borrow(py);
        let stack = value.stack.as_ref().map(|stack| stack.clone_ref(py));
        let display = value.display.clone_ref(py);
        let notes = value.notes.clone_ref(py);
        let name = PyString::from_bytes(py, label(py, &value, true)?.as_bytes())?;
        drop(value);
        if let Some(stack) = stack {
            lines.push(
                PyString::from_bytes(py, b"Traceback (most recent call last):\n")?
                    .into_any()
                    .unbind(),
            );
            for line in stack
                .bind(py)
                .call_method0(PyString::from_bytes(py, b"format")?)?
                .try_iter()?
            {
                lines.push(copied_text(&line?)?.into_any().unbind());
            }
        }
        lines.push(
            PyString::from_bytes(py, b"{}: {}\n")?
                .call_method1(PyString::from_bytes(py, b"format")?, (name, display))?
                .cast_into::<PyString>()?
                .into_any()
                .unbind(),
        );
        for note in notes.bind(py).iter() {
            lines.push(
                PyString::from_bytes(py, b"{}\n")?
                    .call_method1(PyString::from_bytes(py, b"format")?, (note,))?
                    .cast_into::<PyString>()?
                    .into_any()
                    .unbind(),
            );
        }
    }
    Ok(Some(
        PyString::from_bytes(py, b"")?
            .call_method1(
                PyString::from_bytes(py, b"join")?,
                (output::tuple_from_ready_vec(py, lines)?,),
            )?
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
    fn code<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyInt>>> {
        self.code
            .as_ref()
            .map(|value| code_object(py, value))
            .transpose()
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
        let info = slf.borrow();
        if let Some(cached) = info.additional_tuple.get() {
            return Ok(cached.clone_ref(py));
        }
        let values: Vec<_> = info
            .additional
            .iter()
            .map(|value| value.clone_ref(py).into_any())
            .collect();
        let value = output::tuple_from_ready_vec(py, values)?.unbind();
        // Construction can call Python. Only the finished reference enters the
        // publication slot, so concurrent readers can reuse the first winner.
        let _ = info.additional_tuple.set(value);
        info.additional_tuple
            .get()
            .map(|value| value.clone_ref(py))
            .ok_or_else(|| {
                crate::errors::builtin_message::<PyRuntimeError>(
                    py,
                    "diagnostic additional-error publication is unavailable",
                )
            })
    }
    /// Captured Python traceback/chain text, formatted once without live frames.
    #[getter]
    fn traceback(slf: &Bound<'_, Self>) -> PyResult<Option<Py<PyString>>> {
        let py = slf.py();
        {
            let info = slf.borrow();
            if let Some(cached) = info.formatted.get() {
                return Ok(cached.as_ref().map(|value| value.clone_ref(py)));
            }
        }
        let formatted = format_traceback(py, slf)?;
        let info = slf.borrow();
        let _ = info.formatted.set(formatted);
        info.formatted
            .get()
            .map(|value| value.as_ref().map(|value| value.clone_ref(py)))
            .ok_or_else(|| {
                crate::errors::builtin_message::<PyRuntimeError>(
                    py,
                    "diagnostic traceback publication is unavailable",
                )
            })
    }
    fn __str__(&self, py: Python<'_>) -> PyResult<Py<PyString>> {
        let escaped = self.message.bind(py).repr()?;
        let length = escaped.len()?;
        // SAFETY: repr produced a live exact Unicode string enclosed by two
        // quotes. CPython copies this in-bounds interior or reports its actual
        // error; no infallible temporary slice or index objects are needed.
        let content = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyUnicode_Substring(escaped.as_ptr(), 1, length as isize - 1),
            )?
        };
        let name = PyString::from_bytes(py, label(py, self, false)?.as_bytes())?;
        let stage = self
            .stage
            .as_deref()
            .map(|value| PyString::from_bytes(py, value.as_bytes()))
            .transpose()?;
        Ok(
            PyString::from_bytes(py, b"{} [stage={}, code={}, is_server_error={}]: {}")?
                .call_method1(
                    PyString::from_bytes(py, b"format")?,
                    (name, stage, self.code(py)?, self.is_server_error, content),
                )?
                .cast_into::<PyString>()?
                .unbind(),
        )
    }
    fn __repr__(&self, py: Python<'_>) -> PyResult<Py<PyString>> {
        let name = if self.exception_type.is_some() {
            label(py, self, true)?
        } else {
            "None".to_owned()
        };
        let exception_type = PyString::from_bytes(py, name.as_bytes())?;
        let stage = self
            .stage
            .as_deref()
            .map(|value| PyString::from_bytes(py, value.as_bytes()))
            .transpose()?;
        let native_type = self
            .native_type
            .as_deref()
            .map(|value| PyString::from_bytes(py, value.as_bytes()))
            .transpose()?;
        let os_errno = self
            .os_errno
            .map(|value| {
                // SAFETY: Python is attached; i32 fits c_long on supported targets.
                // The constructor returns a new integer or its original error.
                unsafe {
                    Bound::<PyAny>::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyLong_FromLong(value.into()),
                    )
                }
            })
            .transpose()?;
        Ok(PyString::from_bytes(py, b"ErrorInfo(stage={!r}, exception_type={}, native_type={!r}, code={}, os_errno={}, is_server_error={}, message={!r})")?
            .call_method1(PyString::from_bytes(py, b"format")?, (stage, exception_type, native_type, self.code(py)?, os_errno, self.is_server_error, self.message.bind(py)))?
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
        if let Some(value) = self.additional_tuple.get() {
            visit.call(value)?;
        }
        if let Some(value) = self.formatted.get() {
            visit.call(value)?;
        }
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
        self.additional_tuple.take();
        self.formatted.take();
    }
}
