//! Retained public native descriptors, without tuple calls or private layouts.

use super::control::{self, Checkpoint};
use crate::errors;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use std::ffi::CStr;

/// Read one actual Python instance-state entry at the boundary, never through
/// a source's replacement attribute hooks or a private provider C layout.
pub(crate) fn instance<'py>(
    value: &Bound<'py, PyAny>,
    name: &str,
    budget: &dyn Checkpoint,
) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    control::callback(py, Some(budget), || {
        // SAFETY: the documented API obtains an owning dictionary reference;
        // NULL is the required context. A dict lookup may invoke equality, so
        // the common callback boundary owns lifecycle checks and no raw view.
        let dictionary = unsafe {
            Bound::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )
        }?
        .cast_into::<pyo3::types::PyDict>()?;
        dictionary.get_item(name)?.ok_or_else(|| {
            errors::builtin_format::<PyTypeError>(
                py,
                format_args!("supported source has no actual instance storage {name}"),
            )
        })
    })
}

/// One canonical native get/set descriptor and its qualified receiver base.
/// A caller holding raw exports must separately establish that its specific
/// registered getter cannot call Python; descriptor identity alone is not that
/// guarantee. Ordinary protocol callers still use the shared Control wrapper.
pub(crate) struct GetSet {
    owner: Py<PyType>,
    descriptor: Py<PyAny>,
    get: pyo3::ffi::descrgetfunc,
}

impl GetSet {
    pub(crate) fn new(
        owner: &Bound<'_, PyType>,
        name: &CStr,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Self> {
        let py = owner.py();
        let descriptor = control::callback(py, budget, || errors::attribute(owner.as_any(), name))?;
        let typ = descriptor.get_type();
        // SAFETY: only addresses are compared. This public built-in type is
        // interpreter-provided and is neither mutated nor privately inspected.
        let canonical = std::ptr::addr_of_mut!(pyo3::ffi::PyGetSetDescr_Type);
        if typ.as_ptr().cast::<pyo3::ffi::PyTypeObject>() != canonical {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical source attribute must be a native get/set descriptor",
            ));
        }
        // SAFETY: this retained live descriptor type exposes the documented
        // tp_descr_get slot, whose public signature is descrgetfunc.
        let slot =
            unsafe { pyo3::ffi::PyType_GetSlot(typ.as_ptr().cast(), pyo3::ffi::Py_tp_descr_get) };
        if slot.is_null() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical native descriptor has no public getter slot",
            ));
        }
        // SAFETY: the non-null public tp_descr_get slot has this exact callback
        // signature. Descriptor and receiver base are both retained below.
        let get =
            unsafe { std::mem::transmute::<*mut std::ffi::c_void, pyo3::ffi::descrgetfunc>(slot) };
        Ok(Self {
            owner: owner.clone().unbind(),
            descriptor: descriptor.unbind(),
            get,
        })
    }

    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            owner: self.owner.clone_ref(py),
            descriptor: self.descriptor.clone_ref(py),
            get: self.get,
        }
    }

    pub(crate) fn get<'py>(&self, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = value.py();
        if !value.get_type().is(self.owner.bind(py))
            && !super::actual_instance(value, self.owner.bind(py))
        {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "native descriptor requires an actual supported receiver base",
            ));
        }
        // SAFETY: both descriptor and actual receiver are qualified and live.
        // The standard descriptor supplies a new reference or its original
        // exception. No source __getattribute__ or replacement property runs.
        unsafe {
            Bound::from_owned_ptr_or_err(
                py,
                (self.get)(
                    self.descriptor.bind(py).as_ptr(),
                    value.as_ptr(),
                    self.owner.bind(py).as_ptr(),
                ),
            )
        }
    }
}

/// A short canonical receiver header shares the actual Python instance state.
/// Native-backed providers never enter this object.__new__ route.
pub(crate) fn receiver_view<'py>(
    value: &Bound<'py, PyAny>,
    base: &Bound<'py, PyType>,
    object_new: &Bound<'py, PyAny>,
    budget: &dyn Checkpoint,
) -> PyResult<Bound<'py, PyAny>> {
    if value.get_type().is(base) {
        return Ok(value.clone());
    }
    let py = value.py();
    control::callback(py, Some(budget), || {
        // object.__new__ refuses native bases requiring additional C
        // state. This route is only used for qualified Python-state provider
        // bases; native-backed arrays use retained public getters below.
        let view = object_new.call1((base,))?;
        // SAFETY: documented object dictionary APIs return/retain owning
        // references and bypass child attribute hooks. The exact base
        // receiver shares the original state, not a copied value array or
        // alternative value. No provider-private memory layout is read.
        let dictionary = unsafe {
            Bound::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )
        }?;
        // SAFETY: view is a fresh exact Python-state base receiver and
        // dictionary is an owning actual dict from the public getter above.
        let status = unsafe {
            pyo3::ffi::PyObject_GenericSetDict(
                view.as_ptr(),
                dictionary.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(PyErr::fetch(py));
        }
        Ok(view)
    })
}
