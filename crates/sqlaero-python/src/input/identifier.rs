//! Canonical UUID base-slot acquisition, independent of display and target.

use super::{
    control::{Checkpoint, callback},
    integer,
};
use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyType};

pub(super) struct UuidType {
    typ: Py<PyType>,
    getter: std::sync::OnceLock<Py<PyAny>>,
}

impl UuidType {
    pub(super) fn new(py: Python<'_>) -> PyResult<Self> {
        let module = errors::module(py, c"uuid")?;
        let typ = errors::attribute(module.as_any(), c"UUID")?.cast_into::<PyType>()?;
        Ok(Self {
            typ: typ.unbind(),
            getter: std::sync::OnceLock::new(),
        })
    }

    fn prepare_getter(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let slot = errors::attribute(self.typ.bind(py).as_any(), c"int")?;
        // SAFETY: identity comparison with CPython's public immutable member
        // descriptor type only. The getter itself performs the actual owned,
        // free-threaded-safe slot read; no UUID private layout is inspected.
        if unsafe { pyo3::ffi::Py_IS_TYPE(slot.as_ptr(), &raw mut pyo3::ffi::PyMemberDescr_Type) }
            == 0
        {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "UUID's canonical int slot descriptor is unavailable or replaced",
            ));
        }
        if !errors::attribute(&slot, c"__objclass__")?.is(self.typ.bind(py)) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "UUID's int descriptor does not belong to its canonical base",
            ));
        }
        Ok(errors::attribute(&slot, c"__get__")?.unbind())
    }

    pub(super) fn read(
        &self,
        value: &Bound<'_, PyAny>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<u128>> {
        let py = value.py();
        if !super::actual_instance(value, self.typ.bind(py)) {
            return Ok(None);
        }
        if self.getter.get().is_none() {
            let candidate = callback(py, budget, || self.prepare_getter(py))?;
            let _ = self.getter.set(candidate);
            super::checkpoint(py, budget)?;
        }
        let getter = self.getter.get().ok_or_else(|| {
            errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::InternalState,
                "UUID slot preparation did not publish a getter",
            )
        })?;
        let number = callback(py, budget, || {
            getter.bind(py).call1((value, self.typ.bind(py)))
        })?;
        let number = number.cast::<PyInt>().map_err(|_| {
            errors::builtin_message::<PyTypeError>(
                py,
                "UUID's actual int slot must contain real int storage",
            )
        })?;
        let owner = integer::Export::new(number)?;
        let result = match owner.value()? {
            integer::Value::Compact(value) => u128::try_from(value).ok(),
            integer::Value::Digits { negative: true, .. } => None,
            integer::Value::Digits { source, .. } => source
                .unsigned128()
                .map_err(|error| integer::failure(py, error))?,
        };
        result.map(Some).ok_or_else(|| {
            errors::builtin_message::<PyValueError>(
                py,
                "UUID's actual int slot must fit the unsigned 128-bit range",
            )
        })
    }
}
