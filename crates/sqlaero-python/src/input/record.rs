//! Optional named-record facts of actual tuple storage, without field getters.

use super::control::{Checkpoint, callback};
use crate::errors;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};

pub(super) struct Names<'py> {
    fields: Bound<'py, PyTuple>,
}

impl<'py> Names<'py> {
    pub(super) fn get(&self, index: usize) -> PyResult<Bound<'py, PyAny>> {
        self.fields.get_item(index)
    }
}

/// Acquire names only when requested. Positional MySQL consumers do not pay
/// this metadata cost. The immutable tuple owner survives every later callback.
pub(super) fn names<'py>(
    value: &Bound<'py, PyAny>,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Option<Names<'py>>> {
    if value.is_exact_instance_of::<PyTuple>() {
        return Ok(None);
    }
    let Some(tuple) = value.cast::<PyTuple>().ok() else {
        return Ok(None);
    };
    let py = value.py();
    callback(py, budget, || {
        let typ = value.get_type();
        // PyO3 owns the pinned ABI's type layout. The critical section covers
        // only its immediate MRO reference acquisition, with no Python calls.
        let mro = pyo3::sync::critical_section::with_critical_section(typ.as_any(), || typ.mro());
        for (index, base) in mro.iter().enumerate() {
            if index.is_multiple_of(256) {
                super::checkpoint(py, budget)?;
            }
            let base = base.cast_into::<PyType>()?;
            // SAFETY: base is a live real type. CPython >=3.12's public read-only
            // namespace API is used while its type is protected; acquire an
            // owned dict reference before leaving that section. No tp_dict
            // layout or metaclass __getattribute__ replacement is involved.
            // https://docs.python.org/3/c-api/type.html#c.PyType_GetDict
            let namespace =
                pyo3::sync::critical_section::with_critical_section(base.as_any(), || unsafe {
                    let raw = pyo3::ffi::PyType_GetDict(base.as_type_ptr());
                    (!raw.is_null()).then(|| Bound::<PyAny>::from_borrowed_ptr(py, raw))
                })
                .ok_or_else(|| {
                    errors::client(
                        py,
                        sqlaero_core::error::ClientErrorCode::InternalState,
                        "a live tuple base has no class namespace",
                    )
                })?
                .cast_into::<PyDict>()?;
            // Namedtuple has no nominal shared Python base beyond tuple. Its
            // own immutable field tuple plus canonical family markers qualify
            // the record schema; unrelated child attributes do not replace it.
            let Some(fields) = namespace.get_item("_fields")? else {
                continue;
            };
            if !(namespace.contains("_make")?
                && namespace.contains("_asdict")?
                && namespace.contains("_replace")?)
            {
                continue;
            }
            let fields = fields.cast_into::<PyTuple>().map_err(|_| {
                errors::builtin_message::<PyTypeError>(
                    py,
                    "namedtuple field metadata must have actual tuple storage",
                )
            })?;
            if fields.len() != tuple.len() {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "namedtuple field metadata does not match its actual tuple width",
                ));
            }
            for field in 0..fields.len() {
                if field.is_multiple_of(256) {
                    super::checkpoint(py, budget)?;
                }
                if !fields.get_item(field)?.is_instance_of::<PyString>() {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "namedtuple field names must have actual str storage",
                    ));
                }
            }
            return Ok(Some(Names { fields }));
        }
        Ok(None)
    })
}
