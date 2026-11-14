//! Owned, iterative source transparency; targets never inspect Enum classes.

use super::{
    ScalarTypes,
    control::{Checkpoint, callback},
};
use crate::errors;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};
use std::collections::HashSet;

/// One value position with retained transparent owners, not a parameter row.
/// Owners prevent identity reuse while later payload protocols are evaluated.
pub(crate) struct Value<'py> {
    owners: Vec<Bound<'py, PyAny>>,
    identities: HashSet<usize>,
    payload: Bound<'py, PyAny>,
    cycle: bool,
}

impl<'py> Value<'py> {
    pub(crate) fn payload(&self) -> &Bound<'py, PyAny> {
        &self.payload
    }

    pub(crate) fn len(&self) -> usize {
        self.owners.len()
    }

    pub(crate) fn identities(&self) -> impl Iterator<Item = usize> + '_ {
        self.owners.iter().map(|owner| owner.as_ptr() as usize)
    }

    pub(crate) fn intersects(
        &self,
        active: &[usize],
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<bool> {
        if self.cycle {
            return Ok(true);
        }
        for chunk in active.chunks(256) {
            super::checkpoint(self.payload.py(), budget)?;
            if chunk
                .iter()
                .any(|identity| self.identities.contains(identity))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Shallow container checks remain direct; transparent chains can be much
/// longer and must not hide an unbounded identity scan between checkpoints.
pub(crate) fn contains(
    py: Python<'_>,
    active: &[usize],
    identity: usize,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<bool> {
    if active.len() <= 256 {
        return Ok(active.contains(&identity));
    }
    for chunk in active.chunks(256) {
        super::checkpoint(py, budget)?;
        if chunk.contains(&identity) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn read<'py>(
    root: &Bound<'py, PyAny>,
    types: &ScalarTypes,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Option<Value<'py>>> {
    let py = root.py();
    let Some(base) = types.enum_type(py, budget)? else {
        return Ok(None);
    };
    if !super::actual_instance(root, base) {
        return Ok(None);
    }
    let mut value = Value {
        owners: Vec::new(),
        identities: HashSet::new(),
        payload: root.clone(),
        cycle: false,
    };
    loop {
        // Integer/text/float mixins obey the already approved actual base
        // storage priority, even if their member metadata was replaced.
        if super::primitive::read(&value.payload)?.is_some()
            || !super::actual_instance(&value.payload, base)
        {
            break;
        }
        if value.owners.len().is_multiple_of(256) {
            super::checkpoint(py, budget)?;
        }
        let identity = value.payload.as_ptr() as usize;
        value
            .identities
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        if !value.identities.insert(identity) {
            value.cycle = true;
            break;
        }
        value
            .owners
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        let next = callback(py, budget, || {
            // SAFETY: the public generic-dictionary getter reads actual base
            // storage, bypassing __getattribute__ and replacement .value.
            // Its new owned dictionary reference survives the owned lookup;
            // public APIs supply the free-threaded dictionary protection.
            let dictionary = unsafe {
                Bound::<PyAny>::from_owned_ptr_or_err(
                    py,
                    pyo3::ffi::PyObject_GenericGetDict(
                        value.payload.as_ptr(),
                        std::ptr::null_mut(),
                    ),
                )
            }?
            .cast_into::<PyDict>()?;
            dictionary.get_item("_value_")?.ok_or_else(|| {
                errors::builtin_message::<pyo3::exceptions::PyTypeError>(
                    py,
                    "Enum's actual member dictionary has no _value_ storage",
                )
            })
        })?;
        value
            .owners
            .push(std::mem::replace(&mut value.payload, next));
    }
    Ok(Some(value))
}

pub(super) fn prepare_type(py: Python<'_>) -> PyResult<Py<PyType>> {
    Ok(
        errors::attribute(errors::module(py, c"enum")?.as_any(), c"Enum")?
            .cast_into::<PyType>()?
            .unbind(),
    )
}
