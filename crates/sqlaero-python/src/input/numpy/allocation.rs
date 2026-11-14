//! Resolve and retain the physical buffer owner independently of logical shape.

use super::NumpyArrayApi;
use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::buffer::Canonical;
pub(super) use crate::input::buffer::Export;
use crate::input::control::Checkpoint;
use pyo3::exceptions::{PyAttributeError, PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyCapsule, PyMemoryView};

/// Keep an owning buffer or a native producer's valid contiguous interval,
/// plus every inspected identity. A strided view's logical nbytes alone never
/// establishes its allocation bounds.
pub(super) struct Allocation<'py> {
    root: Bound<'py, PyAny>,
    origin: usize,
    dtype: Option<(GetSet, Bound<'py, PyAny>)>,
    array: Canonical,
    record: Canonical,
    native_root: bool,
    zero_only: bool,
    _chain: Vec<Bound<'py, PyAny>>,
    _resize_guards: Vec<Bound<'py, PyAny>>,
}

impl<'py> Allocation<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let py = value.py();
        let mut current = value.clone();
        let mut chain = Vec::new();
        let mut guards = Vec::new();
        let mut dtype = None;
        loop {
            crate::input::check(py, budget)?;
            if chain
                .iter()
                .any(|owner: &Bound<'_, PyAny>| owner.is(&current))
            {
                return Err(errors::builtin_message::<PyBufferError>(
                    py,
                    "NumPy allocation owner chain is cyclic",
                ));
            }
            chain.try_reserve(1).map_err(|_| errors::memory_error(py))?;
            chain.push(current.clone());
            // SAFETY: both operands are live actual PyType objects. This
            // checks their real MRO, not an overridable instance __class__.
            let is_array = unsafe {
                pyo3::ffi::PyType_IsSubtype(
                    current.get_type().as_ptr().cast(),
                    api.array.typ(py).as_ptr().cast(),
                ) != 0
            };
            let is_record = crate::input::actual_instance(&current, api.record.typ(py));
            let next = if is_array {
                guards
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                // SAFETY: the live source supports weak references. The public
                // constructor either owns a callback-free weakref or preserves
                // its original allocation/type exception; no Python hook runs.
                let guard = unsafe {
                    Bound::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyWeakref_NewRef(current.as_ptr(), std::ptr::null_mut()),
                    )
                }?;
                guards.push(guard);
                // Use NumPy's canonical descriptor even for an allocation
                // owner's subclass; no user-overridden base property can
                // disguise an as_strided/DummyArray chain as owning storage.
                Some(api.base.get(&current)?)
            } else if is_record {
                Some(api.record_base.get(&current)?)
            } else if current.is_exact_instance_of::<PyMemoryView>() {
                Some(crate::input::control::callback(py, Some(budget), || {
                    errors::attribute(&current, c"obj")
                })?)
            } else {
                match crate::input::control::callback(py, Some(budget), || {
                    errors::attribute(&current, c"base")
                }) {
                    Ok(base) => Some(base),
                    Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                        crate::input::check(py, budget)?;
                        None
                    }
                    Err(error) => return Err(error),
                }
            };
            let native_producer = is_array
                && next
                    .as_ref()
                    .is_some_and(|next| next.is_exact_instance_of::<PyCapsule>());
            if let Some(next) = next.filter(|next| !next.is_none()) {
                if native_producer {
                    // NumPy's native base descriptor directly retains this
                    // producer owner. Its public contiguous export is the
                    // valid region promised by that native producer, just as
                    // for Arrow C Data. No capsule payload is interpreted.
                    // An arbitrary Python object's base cannot use this path.
                    chain.try_reserve(1).map_err(|_| errors::memory_error(py))?;
                    chain.push(next);
                } else {
                    current = next;
                    continue;
                }
            }
            if is_array {
                let flags = api.flags.get(&current)?;
                if !native_producer && !errors::attribute(&flags, c"owndata")?.extract::<bool>()? {
                    return Err(errors::builtin_message::<PyBufferError>(
                        py,
                        "NumPy allocation bounds cannot be proved: terminal ndarray does not own its storage",
                    ));
                }
                let prepared = api.dtype.get(&current)?;
                dtype = Some((api.dtype.clone_ref(py), prepared));
            } else if is_record {
                let prepared = api.record_dtype.get(&current)?;
                dtype = Some((api.record_dtype.clone_ref(py), prepared));
            }
            break;
        }
        let mut zero_only = false;
        // Only an address survives this scope; its export is already released
        // before root selection can run Python or replace `current`.
        let origin = match contiguous_origin(&current, api, budget) {
            Ok(origin) => origin,
            Err(original) if original.is_instance_of::<PyTypeError>(py) => {
                crate::input::check(py, budget)?;
                // Take only native address/length facts, then release the
                // source export before a producer's Python protocol is called.
                let source = if crate::input::actual_instance(value, api.array.typ(py)) {
                    api.array.acquire(value, pyo3::ffi::PyBUF_STRIDES)?
                } else {
                    Export::acquire(value, pyo3::ffi::PyBUF_STRIDES)?
                };
                let address = source.raw.buf as usize;
                let empty = source.raw.len == 0;
                drop(source);
                if let Some(buffer) =
                    crate::input::arrow::allocation_buffer(&current, address, budget)?
                {
                    current = buffer;
                    contiguous_origin(&current, api, budget)?
                } else if empty {
                    // No source byte may be read from this fallback. Future
                    // exports must remain zero length, not merely same origin.
                    current = value.clone();
                    zero_only = true;
                    contiguous_origin(&current, api, budget)?
                } else {
                    return Err(original);
                }
            }
            Err(error) => return Err(error),
        };
        let native_root = super::super::actual_instance(&current, api.array.typ(py))
            || super::super::actual_instance(&current, api.record.typ(py))
            || current.is_exact_instance_of::<PyBytes>()
            || current.is_exact_instance_of::<PyByteArray>();
        Ok(Self {
            array: api.array.clone_ref(py),
            record: api.record.clone_ref(py),
            native_root,
            root: current,
            origin,
            dtype,
            zero_only,
            _chain: chain,
            _resize_guards: guards,
        })
    }

    /// A contiguous public root export supplies a physical interval. Producers
    /// remain responsible for their buffer protocol and concurrent-write
    /// contract; this is not a lock against forbidden external native mutation.
    pub(super) fn export(&self) -> PyResult<Export<'_, 'py>> {
        let py = self.root.py();
        if let Some((getter, prepared)) = &self.dtype {
            let current = getter.get(&self.root)?;
            if !current.is(prepared) {
                return Err(bounds_error(py));
            }
        }
        let export = if crate::input::actual_instance(&self.root, self.array.typ(py)) {
            self.array
                .acquire(&self.root, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        } else if crate::input::actual_instance(&self.root, self.record.typ(py)) {
            self.record
                .acquire(&self.root, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        } else {
            Export::acquire(&self.root, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        };
        if export.raw.buf as usize != self.origin || self.zero_only && export.raw.len != 0 {
            drop(export);
            return Err(bounds_error(py));
        }
        Ok(export)
    }

    pub(super) fn native_root(&self) -> bool {
        self.native_root
    }
}

fn contiguous_origin(
    value: &Bound<'_, PyAny>,
    api: &NumpyArrayApi,
    budget: &dyn Checkpoint,
) -> PyResult<usize> {
    crate::input::control::callback(value.py(), Some(budget), || {
        let export = if crate::input::actual_instance(value, api.array.typ(value.py())) {
            api.array.acquire(value, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        } else if crate::input::actual_instance(value, api.record.typ(value.py())) {
            api.record.acquire(value, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        } else {
            Export::acquire(value, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?
        };
        let origin = export.raw.buf as usize;
        drop(export);
        Ok(origin)
    })
}

pub(super) fn bounds_error(py: Python<'_>) -> PyErr {
    errors::builtin_message::<PyBufferError>(
        py,
        "NumPy logical element lies outside its current physical allocation; keep input storage and metadata stable",
    )
}
