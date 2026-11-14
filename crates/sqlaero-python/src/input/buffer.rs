//! Attached public buffer ownership, including qualified base-slot exports.

use crate::errors;
use pyo3::exceptions::PyBufferError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use std::mem::MaybeUninit;

/// Run one bounded acquire/copy/release window. All raw exports must be gone
/// when invoke returns. Known canonical non-callback roots retain their fast
/// path; unknown producer protocols use the original shared Control before and
/// after the complete ownership window, never while an export is being read.
pub(crate) fn read<T>(
    py: Python<'_>,
    budget: &dyn super::control::Checkpoint,
    callback_free: bool,
    invoke: impl FnOnce() -> PyResult<T>,
) -> PyResult<T> {
    if callback_free {
        invoke()
    } else {
        super::control::callback(py, Some(budget), invoke)
    }
}

/// A retained immutable native base and its paired public buffer slots.
pub(super) struct Canonical {
    typ: Py<PyType>,
    get: pyo3::ffi::getbufferproc,
    release: Option<pyo3::ffi::releasebufferproc>,
}

impl Canonical {
    pub(super) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            typ: self.typ.clone_ref(py),
            get: self.get,
            release: self.release,
        }
    }
    pub(super) fn new(typ: Py<PyType>, py: Python<'_>) -> PyResult<Self> {
        // Slot caching is only valid for a native immutable base, never a
        // Python heap type whose public slots can be replaced after preparation.
        // SAFETY: typ is retained and denotes a live actual type.
        let flags = unsafe { pyo3::ffi::PyType_GetFlags(typ.bind(py).as_ptr().cast()) };
        if flags & pyo3::ffi::Py_TPFLAGS_IMMUTABLETYPE == 0 {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "canonical numeric buffer base must be an immutable native type",
            ));
        }
        // SAFETY: this is a live type. Public slot IDs define each returned
        // function's exact signature; the retained type keeps its slots live.
        let (get, release) = unsafe {
            (
                pyo3::ffi::PyType_GetSlot(typ.bind(py).as_ptr().cast(), pyo3::ffi::Py_bf_getbuffer),
                pyo3::ffi::PyType_GetSlot(
                    typ.bind(py).as_ptr().cast(),
                    pyo3::ffi::Py_bf_releasebuffer,
                ),
            )
        };
        if get.is_null() {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "canonical native scalar type must provide the public buffer protocol",
            ));
        }
        // SAFETY: Py_bf_getbuffer and Py_bf_releasebuffer return these public
        // callback types. A null release means the exporter needs no cleanup.
        let get =
            unsafe { std::mem::transmute::<*mut std::ffi::c_void, pyo3::ffi::getbufferproc>(get) };
        let release = if release.is_null() {
            None
        } else {
            // SAFETY: the non-null Py_bf_releasebuffer slot has this exact
            // public callback signature, paired with the retained native base.
            Some(unsafe {
                std::mem::transmute::<*mut std::ffi::c_void, pyo3::ffi::releasebufferproc>(release)
            })
        };
        Ok(Self { typ, get, release })
    }

    pub(super) fn typ<'py>(&self, py: Python<'py>) -> &Bound<'py, PyType> {
        self.typ.bind(py)
    }

    /// Exact sources keep the standard fast path. A real subclass uses only
    /// this native base's public get/release pair, never its Python overrides.
    pub(super) fn acquire<'owner, 'py>(
        &'owner self,
        owner: &'owner Bound<'py, PyAny>,
        flags: i32,
    ) -> PyResult<Export<'owner, 'py>> {
        if owner.get_type().is(self.typ(owner.py())) {
            return Export::acquire(owner, flags);
        }
        if !super::actual_instance(owner, self.typ(owner.py())) {
            return Err(errors::builtin_message::<PyBufferError>(
                owner.py(),
                "native base buffer requires an actual supported subclass",
            ));
        }
        let mut raw = MaybeUninit::uninit();
        // SAFETY: actual subtype qualification is complete and the owner and
        // canonical base remain retained. The public constructor initializes
        // the descriptor on success or leaves the original Python exception.
        if unsafe { (self.get)(owner.as_ptr(), raw.as_mut_ptr(), flags) } < 0 {
            return Err(PyErr::fetch(owner.py()));
        }
        // SAFETY: the successful public buffer slot initialized the descriptor.
        let raw = unsafe { raw.assume_init() };
        // A base can delegate to another exporter. That returned owner then
        // needs its own standard release, not this source's base release.
        let release = if raw.obj == owner.as_ptr() {
            Release::Canonical(self.release)
        } else {
            Release::Standard
        };
        Ok(Export {
            raw,
            _owner: owner,
            release,
        })
    }
}

enum Release {
    Standard,
    Canonical(Option<pyo3::ffi::releasebufferproc>),
}

/// Exactly one successful buffer export; only attached boundary code owns it.
pub(crate) struct Export<'owner, 'py> {
    pub(crate) raw: pyo3::ffi::Py_buffer,
    _owner: &'owner Bound<'py, PyAny>,
    release: Release,
}

impl<'owner, 'py> Export<'owner, 'py> {
    pub(crate) fn acquire(owner: &'owner Bound<'py, PyAny>, flags: i32) -> PyResult<Self> {
        let mut raw = MaybeUninit::uninit();
        // SAFETY: owner stays live and attached for the whole export. A failed
        // public constructor preserves its original Python exception.
        if unsafe { pyo3::ffi::PyObject_GetBuffer(owner.as_ptr(), raw.as_mut_ptr(), flags) } < 0 {
            return Err(PyErr::fetch(owner.py()));
        }
        // SAFETY: success initialized the complete standard descriptor.
        let raw = unsafe { raw.assume_init() };
        Ok(Self {
            raw,
            _owner: owner,
            release: Release::Standard,
        })
    }

    pub(crate) fn contains(&self, pointer: *const u8, length: usize) -> bool {
        if self.raw.len < 0 || (self.raw.buf.is_null() && self.raw.len != 0) {
            return false;
        }
        let lower = self.raw.buf as usize;
        let start = pointer as usize;
        lower
            .checked_add(self.raw.len as usize)
            .zip(start.checked_add(length))
            .is_some_and(|(upper, end)| start >= lower && end <= upper)
    }
}

impl Drop for Export<'_, '_> {
    fn drop(&mut self) {
        // SAFETY: exactly one successful constructor reaches this destructor,
        // while attached. Qualified acquisition must use its matching base
        // release rather than dispatching to an overridden subclass slot.
        unsafe {
            match self.release {
                Release::Standard => pyo3::ffi::PyBuffer_Release(&mut self.raw),
                Release::Canonical(release) if !self.raw.obj.is_null() => {
                    let owner = self.raw.obj;
                    if let Some(release) = release {
                        release(owner, &mut self.raw);
                    }
                    // The buffer contract makes the caller release this owned
                    // reference, not bf_releasebuffer. Mirror PyBuffer_Release.
                    self.raw.obj = std::ptr::null_mut();
                    pyo3::ffi::Py_DECREF(owner);
                }
                Release::Canonical(_) => {}
            }
        }
    }
}
