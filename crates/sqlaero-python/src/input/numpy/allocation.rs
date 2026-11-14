//! Resolve and retain the physical buffer owner independently of logical shape.

pub(super) use crate::input::buffer::Export;
use crate::input::control::Checkpoint;
use crate::{errors, output};
use pyo3::exceptions::{PyAttributeError, PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyMemoryView, PyType};

/// Keep an owning buffer or a native producer's valid contiguous interval,
/// plus every inspected identity. A strided view's logical nbytes alone never
/// establishes its allocation bounds.
pub(super) struct Allocation<'py> {
    root: Bound<'py, PyAny>,
    origin: usize,
    dtype: Option<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
    zero_only: bool,
    _chain: Vec<Bound<'py, PyAny>>,
    _resize_guards: Vec<Bound<'py, PyAny>>,
}

impl<'py> Allocation<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        array_type: &Py<PyType>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let py = value.py();
        let base_descriptor = errors::attribute(array_type.bind(py).as_any(), c"base")?;
        let base_get = errors::attribute(&base_descriptor, c"__get__")?;
        let weakref = errors::module(py, c"weakref")?;
        let reference = errors::attribute(weakref.as_any(), c"ref")?;
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
                    array_type.bind(py).as_ptr().cast(),
                ) != 0
            };
            let next = if is_array {
                guards
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                guards.push(reference.call1(output::tuple_from_ready(py, [current.clone()])?)?);
                // Use NumPy's canonical descriptor even for an allocation
                // owner's subclass; no user-overridden base property can
                // disguise an as_strided/DummyArray chain as owning storage.
                Some(base_get.call1(output::tuple_from_ready(py, [current.clone()])?)?)
            } else if current.is_exact_instance_of::<PyMemoryView>() {
                Some(errors::attribute(&current, c"obj")?)
            } else {
                match errors::attribute(&current, c"base") {
                    Ok(base) => Some(base),
                    Err(error) if error.is_instance_of::<PyAttributeError>(py) => None,
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
                let descriptor = errors::attribute(array_type.bind(py).as_any(), c"flags")?;
                let getter = errors::attribute(&descriptor, c"__get__")?;
                let flags = getter.call1(output::tuple_from_ready(py, [current.clone()])?)?;
                if !native_producer && !errors::attribute(&flags, c"owndata")?.extract::<bool>()? {
                    return Err(errors::builtin_message::<PyBufferError>(
                        py,
                        "NumPy allocation bounds cannot be proved: terminal ndarray does not own its storage",
                    ));
                }
                let descriptor = errors::attribute(array_type.bind(py).as_any(), c"dtype")?;
                let getter = errors::attribute(&descriptor, c"__get__")?;
                let prepared = getter.call1(output::tuple_from_ready(py, [current.clone()])?)?;
                dtype = Some((getter, prepared));
            }
            break;
        }
        let mut zero_only = false;
        // Only an address survives this scope; its export is already released
        // before root selection can run Python or replace `current`.
        let origin = match contiguous_origin(&current) {
            Ok(origin) => origin,
            Err(original) if original.is_instance_of::<PyTypeError>(py) => {
                // Take only native address/length facts, then release the
                // source export before a producer's Python protocol is called.
                let source = Export::acquire(value, pyo3::ffi::PyBUF_STRIDES)?;
                let address = source.raw.buf as usize;
                let empty = source.raw.len == 0;
                drop(source);
                if let Some(buffer) =
                    crate::input::arrow::allocation_buffer(&current, address, budget)?
                {
                    current = buffer;
                    contiguous_origin(&current)?
                } else if empty {
                    // No source byte may be read from this fallback. Future
                    // exports must remain zero length, not merely same origin.
                    current = value.clone();
                    zero_only = true;
                    contiguous_origin(&current)?
                } else {
                    return Err(original);
                }
            }
            Err(error) => return Err(error),
        };
        Ok(Self {
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
            let current = getter.call1(output::tuple_from_ready(py, [self.root.clone()])?)?;
            if !current.is(prepared) {
                return Err(bounds_error(py));
            }
        }
        let export = Export::acquire(&self.root, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?;
        if export.raw.buf as usize != self.origin || self.zero_only && export.raw.len != 0 {
            drop(export);
            return Err(bounds_error(py));
        }
        Ok(export)
    }

    /// Records are one contiguous logical value. Check their entire stored
    /// region before public field access, without reading private field offsets.
    pub(super) fn validate_record(&self, value: &Bound<'py, PyAny>) -> PyResult<()> {
        let py = value.py();
        let root = self.export()?;
        let source = match Export::acquire(value, pyo3::ffi::PyBUF_SIMPLE) {
            Ok(source) => source,
            Err(error) => {
                drop(root);
                return Err(error);
            }
        };
        let valid =
            source.raw.len >= 0 && root.contains(source.raw.buf.cast(), source.raw.len as usize);
        drop(source);
        drop(root);
        if valid { Ok(()) } else { Err(bounds_error(py)) }
    }
}

fn contiguous_origin(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    let export = Export::acquire(value, pyo3::ffi::PyBUF_ANY_CONTIGUOUS)?;
    let origin = export.raw.buf as usize;
    drop(export);
    Ok(origin)
}

pub(super) fn bounds_error(py: Python<'_>) -> PyErr {
    errors::builtin_message::<PyBufferError>(
        py,
        "NumPy logical element lies outside its current physical allocation; keep input storage and metadata stable",
    )
}
