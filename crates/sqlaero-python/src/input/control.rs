//! Strict thin-control inputs; execution and session facts remain with Connection.

use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyString, PyTuple};
use sqlaero_core::bind::{Context, writer};
use sqlaero_core::charset::codec::{Codec, CodecError};
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};

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
    budget: &IoBudget,
) -> PyResult<Vec<u8>> {
    let py = name.py();
    super::check(py, budget)?;
    let source = name
        .cast::<PyString>()
        .map_err(|_| errors::builtin_message::<PyTypeError>(py, "name must be str"))?;
    let text = super::unicode(source, context)?;
    if text.is_empty() || text.contains('\0') {
        return Err(errors::builtin_message::<PyValueError>(
            py,
            "name must be nonempty and contain no NUL",
        ));
    }
    let command = match action {
        SavepointAction::Create => b"\x03SAVEPOINT ".as_slice(),
        SavepointAction::Rollback => b"\x03ROLLBACK TO SAVEPOINT ",
        SavepointAction::Release => b"\x03RELEASE SAVEPOINT ",
    };
    let mut output = Vec::new();
    writer::append(&mut output, command, context.limit).map_err(|error| errors::wire(py, error))?;
    writer::append(&mut output, b"`", context.limit).map_err(|error| errors::wire(py, error))?;
    let mut start = 0;
    while start < text.len() {
        super::check(py, budget)?;
        let mut end = start.saturating_add(super::WORK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        context
            .codec
            .identifier_fragment(&text[start..end], &mut output, context.limit)
            .map_err(|error| {
                let error = match error {
                    CodecError::Encode { offset, point } => CodecError::Encode {
                        offset: start + offset,
                        point,
                    },
                    error => error,
                };
                super::codec_error(source, text, context.codec.name(), error)
            })?;
        start = end;
    }
    writer::append(&mut output, b"`", context.limit).map_err(|error| errors::wire(py, error))?;
    super::check(py, budget)?;
    Ok(output)
}

/// Copy a finite sequence of complete SQL units before connection delivery.
/// No semicolon splitting, user iteration, Python SQL tree or SET whitelist is
/// involved. The caller later encodes each unit under its actual current state.
pub(crate) fn init_sql(
    value: Option<&Bound<'_, PyAny>>,
    budget: &IoBudget,
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
    budget: &IoBudget,
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
