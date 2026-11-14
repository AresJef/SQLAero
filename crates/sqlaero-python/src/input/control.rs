//! Shared attached input checks and strict thin command inputs.
//! Execution, deadlines and session facts retain their existing authority.

use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyString, PyTuple};
use sqlaero_core::bind::{Context, writer};
use sqlaero_core::charset::codec::{Codec, CodecError};
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use std::cell::Cell;

/// A borrowed input-control boundary, never another clock or state registry.
/// It stays at the attached Python boundary and is not sent to a native worker.
pub(crate) trait Checkpoint {
    fn checkpoint(&self, py: Python<'_>) -> PyResult<()>;

    /// Legacy single-value callers remain safe without an aggregate view.
    fn bytes(&self, py: Python<'_>, _amount: usize) -> PyResult<()> {
        self.checkpoint(py)
    }
}

impl Checkpoint for IoBudget {
    fn checkpoint(&self, py: Python<'_>) -> PyResult<()> {
        py.check_signals()?;
        self.check().map_err(|error| errors::timeout(py, error))
    }
}

/// A view of the original budget and the calling operation's actual owners.
/// The validator only reads existing authority; no cached open-state is kept.
pub(crate) struct Control<'a> {
    parent: Option<&'a dyn Checkpoint>,
    validate: Option<&'a dyn Fn() -> PyResult<()>>,
    bytes: Cell<usize>,
}

impl<'a> Control<'a> {
    pub(crate) fn new(budget: &'a IoBudget, validate: &'a dyn Fn() -> PyResult<()>) -> Self {
        Self {
            parent: Some(budget),
            validate: Some(validate),
            bytes: Cell::new(0),
        }
    }

    /// Local traversal accounting borrows the same authority and never starts
    /// another deadline, including when the parent already validates owners.
    pub(crate) fn local(parent: &'a dyn Checkpoint) -> Self {
        Self {
            parent: Some(parent),
            validate: None,
            bytes: Cell::new(0),
        }
    }

    /// Standalone JSON has signal/work control without a connection or clock.
    pub(crate) fn unlimited() -> Self {
        Self {
            parent: None,
            validate: None,
            bytes: Cell::new(0),
        }
    }
}

impl Checkpoint for Control<'_> {
    fn checkpoint(&self, py: Python<'_>) -> PyResult<()> {
        self.bytes.set(0);
        match self.parent {
            Some(parent) => parent.checkpoint(py)?,
            None => py.check_signals()?,
        }
        if let Some(validate) = self.validate {
            validate()?;
        }
        Ok(())
    }

    fn bytes(&self, py: Python<'_>, amount: usize) -> PyResult<()> {
        let used = self.bytes.get().saturating_add(amount);
        self.bytes.set(used);
        if used >= 16 * 1024 {
            self.checkpoint(py)?;
        }
        Ok(())
    }
}

/// Tiny leaves retain the caller's existing node-work bound. Medium leaves
/// additionally charge bytes, preventing many sub-window values from hiding a
/// large scan/copy interval. Long leaves retain their own fragment checkpoints.
pub(crate) fn bytes(
    py: Python<'_>,
    control: Option<&dyn Checkpoint>,
    amount: usize,
) -> PyResult<()> {
    if amount < 256 {
        return Ok(());
    }
    match control {
        Some(control) => control.bytes(py, amount),
        None => py.check_signals(),
    }
}

/// Check entry, run one approved protocol, then recheck before consuming it.
/// The actual callback exception wins and is propagated unchanged; a success
/// cannot hide a retired owner or an expired original operation deadline.
pub(crate) fn callback<T>(
    py: Python<'_>,
    control: Option<&dyn Checkpoint>,
    invoke: impl FnOnce() -> PyResult<T>,
) -> PyResult<T> {
    // An earlier owner's release can itself run Python finalizers after its
    // last explicit protocol returned. Never enter a later protocol relying on
    // that older success check. Ordinary scalar reads do not use this helper.
    super::checkpoint(py, control)?;
    let value = invoke()?;
    super::checkpoint(py, control)?;
    Ok(value)
}

/// The three explicitly selected native savepoint operations.
#[derive(Clone, Copy)]
pub(crate) enum SavepointAction {
    Create,
    Rollback,
    Release,
}

/// Encode one original name as one safely quoted identifier. This helper does
/// not acquire a command lease, inspect transaction state or send any SQL.
pub(crate) fn savepoint(
    name: &Bound<'_, PyAny>,
    action: SavepointAction,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<Vec<u8>> {
    let py = name.py();
    super::check(py, budget)?;
    let (source, text) = identifier_text(name, "name", context)?;
    let command = match action {
        SavepointAction::Create => b"\x03SAVEPOINT ".as_slice(),
        SavepointAction::Rollback => b"\x03ROLLBACK TO SAVEPOINT ",
        SavepointAction::Release => b"\x03RELEASE SAVEPOINT ",
    };
    let mut output = Vec::new();
    writer::append(&mut output, command, context.limit).map_err(|error| errors::wire(py, error))?;
    identifier_write(source, text, &mut output, context, budget)?;
    Ok(output)
}

/// Append one raw name as one quoted identifier under the current codec.
/// The original Python string remains available only at this attached boundary
/// for exact Unicode error offsets; no Python SQL object is created.
pub(crate) fn identifier(
    name: &Bound<'_, PyAny>,
    name_label: &str,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    super::check(name.py(), budget)?;
    let (source, text) = identifier_text(name, name_label, context)?;
    identifier_write(source, text, output, context, budget)
}

fn identifier_text<'a, 'py>(
    name: &'a Bound<'py, PyAny>,
    name_label: &str,
    context: &Context,
) -> PyResult<(&'a Bound<'py, PyString>, &'a str)> {
    let py = name.py();
    let source = name.cast::<PyString>().map_err(|_| {
        errors::builtin_format::<PyTypeError>(py, format_args!("{name_label} must be str"))
    })?;
    let text = super::unicode(source, context)?;
    if text.is_empty() || text.contains('\0') {
        return Err(errors::builtin_format::<PyValueError>(
            py,
            format_args!("{name_label} must be nonempty and contain no NUL"),
        ));
    }
    Ok((source, text))
}

fn identifier_write(
    source: &Bound<'_, PyString>,
    text: &str,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let py = source.py();
    writer::append(output, b"`", context.limit).map_err(|error| errors::wire(py, error))?;
    let mut start = 0;
    while start < text.len() {
        super::check(py, budget)?;
        let mut end = start.saturating_add(super::WORK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        context
            .codec
            .identifier_fragment(&text[start..end], output, context.limit)
            .map_err(|error| {
                let error = match error {
                    CodecError::Encode { offset, point } => CodecError::Encode {
                        offset: start + offset,
                        point,
                    },
                    error => error,
                };
                super::codec_error(source, text, context.codec.name(), error, Some(budget))
            })?;
        start = end;
    }
    writer::append(output, b"`", context.limit).map_err(|error| errors::wire(py, error))?;
    super::check(py, budget)?;
    Ok(())
}

/// Copy a finite sequence of complete SQL units before connection delivery.
/// No semicolon splitting, user iteration, Python SQL tree or SET whitelist is
/// involved. The caller later encodes each unit under its actual current state.
pub(crate) fn init_sql(
    value: Option<&Bound<'_, PyAny>>,
    budget: &dyn Checkpoint,
) -> PyResult<Vec<String>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Vec::new());
    };
    let py = value.py();
    super::check(py, budget)?;
    if let Ok(source) = value.cast::<PyString>() {
        let mut result = Vec::new();
        result
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        result.push(copy_sql(source, None, budget)?);
        return Ok(result);
    }
    let list = value.cast::<PyList>().ok();
    let tuple = value.cast::<PyTuple>().ok();
    let length = if let Some(list) = list {
        list.len()
    } else if let Some(tuple) = tuple {
        tuple.len()
    } else {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "init_sql must be str, list or tuple of str, or None",
        ));
    };
    let mut result = Vec::new();
    result
        .try_reserve(length)
        .map_err(|_| errors::memory_error(py))?;
    for index in 0..length {
        super::check(py, budget)?;
        let item = if let Some(list) = list {
            list.get_item(index)?
        } else if let Some(tuple) = tuple {
            tuple.get_item(index)?
        } else {
            return Err(errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::InternalState,
                "init_sql sequence was not retained",
            ));
        };
        let source = item.cast::<PyString>().map_err(|_| {
            errors::builtin_message::<PyTypeError>(py, format!("init_sql[{index}] must be str"))
        })?;
        result.push(copy_sql(source, Some(index), budget)?);
    }
    super::check(py, budget)?;
    Ok(result)
}

fn copy_sql(
    source: &Bound<'_, PyString>,
    index: Option<usize>,
    budget: &dyn Checkpoint,
) -> PyResult<String> {
    let py = source.py();
    let text = source.to_str()?;
    if text.is_empty() {
        let name = index.map_or_else(
            || "init_sql".to_owned(),
            |index| format!("init_sql[{index}]"),
        );
        return Err(errors::builtin_message::<PyValueError>(
            py,
            format!("{name} must be nonempty"),
        ));
    }
    let mut result = String::new();
    result
        .try_reserve_exact(text.len())
        .map_err(|_| errors::memory_error(py))?;
    let mut start = 0;
    while start < text.len() {
        super::check(py, budget)?;
        let mut end = start.saturating_add(super::WORK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        result.push_str(&text[start..end]);
        start = end;
    }
    super::check(py, budget)?;
    Ok(result)
}

/// Observe the real name adapter without performing a control command.
#[pyfunction]
#[pyo3(signature = (name, *, action="create", charset=None, limit=67108864, timeout_ns=None))]
pub(crate) fn _savepoint_request<'py>(
    py: Python<'py>,
    name: &Bound<'py, PyAny>,
    action: &str,
    charset: Option<&str>,
    limit: usize,
    timeout_ns: Option<u64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let action = match action {
        "create" => SavepointAction::Create,
        "rollback" => SavepointAction::Rollback,
        "release" => SavepointAction::Release,
        _ => {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "unknown savepoint action",
            ));
        }
    };
    let selected = sqlaero_core::charset::resolve(charset, None)
        .map_err(|error| errors::builtin_message::<PyValueError>(py, error.to_string()))?;
    let context = Context {
        codec: Codec::new(selected.charset),
        no_backslash_escapes: false,
        utc_confirmed: true,
        limit,
    };
    let budget = IoBudget::new(
        TimeoutKind::Operation,
        timeout_ns.map(std::time::Duration::from_nanos),
        None,
        None,
    );
    Ok(PyBytes::new(
        py,
        &savepoint(name, action, &context, &budget)?,
    ))
}

/// Observe entry preparation, not initialization command execution.
#[pyfunction]
#[pyo3(signature = (value=None))]
pub(crate) fn _prepare_init_sql(value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<String>> {
    init_sql(
        value,
        &IoBudget::new(TimeoutKind::Connect, None, None, None),
    )
}

/// Observe native SQL-source encoding independently of command execution.
#[pyfunction]
#[pyo3(signature = (source, *, charset=None, prefix=b"".as_slice(), limit=67108864, timeout_ns=None))]
pub(crate) fn _sql_utf8<'py>(
    py: Python<'py>,
    source: &str,
    charset: Option<&str>,
    prefix: &[u8],
    limit: usize,
    timeout_ns: Option<u64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let selected = sqlaero_core::charset::resolve(charset, None)
        .map_err(|error| errors::builtin_message::<PyValueError>(py, error.to_string()))?;
    let context = Context {
        codec: Codec::new(selected.charset),
        no_backslash_escapes: false,
        utc_confirmed: true,
        limit,
    };
    let mut request = Vec::new();
    writer::append(&mut request, prefix, limit).map_err(|error| errors::wire(py, error))?;
    let budget = IoBudget::new(
        TimeoutKind::Operation,
        timeout_ns.map(std::time::Duration::from_nanos),
        None,
        None,
    );
    super::sql_utf8(py, source, &mut request, &context, &budget)?;
    Ok(PyBytes::new(py, &request))
}
