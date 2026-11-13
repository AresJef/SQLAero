//! Attached ownership for CPython's immutable, zero-copy integer export.

use crate::errors;
use pyo3::prelude::*;
use pyo3::types::PyInt;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_transcode::integer::{Digits, Error};
use std::ffi::c_void;

#[repr(C)]
struct RawView {
    handle: *mut c_void,
    digits: *const u8,
    count: usize,
    compact: i64,
    bits: u8,
    width: u8,
    order: i8,
    endian: i8,
    negative: u8,
}

unsafe extern "C" {
    fn sqlaero_long_open(value: *mut pyo3::ffi::PyObject, view: *mut RawView) -> i32;
    fn sqlaero_long_close(handle: *mut c_void);
}

/// Retains both the real Python owner and the API's export token. No raw view
/// can outlive this guard, detach from Python or cross a thread boundary.
pub(crate) struct Export<'a, 'py> {
    source: &'a Bound<'py, PyInt>,
    raw: RawView,
}

pub(crate) enum Value<'a> {
    Compact(i64),
    Digits { negative: bool, source: Digits<'a> },
}

impl<'a, 'py> Export<'a, 'py> {
    pub(crate) fn new(source: &'a Bound<'py, PyInt>) -> PyResult<Self> {
        let mut raw = std::mem::MaybeUninit::uninit();
        // SAFETY: source is an owned actual int or subclass and Python stays
        // attached. The C shim initializes every RawView field on success and
        // preserves the actual Python allocation/export exception on failure.
        let result = unsafe { sqlaero_long_open(source.as_ptr(), raw.as_mut_ptr()) };
        if result < 0 {
            return Err(PyErr::fetch(source.py()));
        }
        // SAFETY: the successful C function initialized exactly our repr(C)
        // metadata; ownership of its export token now belongs to this guard.
        let raw = unsafe { raw.assume_init() };
        Ok(Self { source, raw })
    }

    pub(crate) fn value(&self) -> PyResult<Value<'_>> {
        if self.raw.digits.is_null() {
            return Ok(Value::Compact(self.raw.compact));
        }
        let py = self.source.py();
        let length = self
            .raw
            .count
            .checked_mul(usize::from(self.raw.width))
            .filter(|length| *length <= isize::MAX as usize)
            .ok_or_else(|| failure(py, Error::Representation))?;
        if !matches!(self.raw.order, -1 | 1)
            || !matches!(self.raw.endian, -1 | 1)
            || self.raw.negative > 1
        {
            return Err(failure(py, Error::Representation));
        }
        // SAFETY: the documented export owns a read-only array of count native
        // digits until FreeExport. Its exact length was checked above; source
        // and export remain live through all bounded steps and signal checks.
        // CPython integers are immutable, including on free-threaded builds.
        let bytes = unsafe { std::slice::from_raw_parts(self.raw.digits, length) };
        let source = Digits::new(
            bytes,
            self.raw.bits,
            self.raw.width,
            self.raw.order == -1,
            self.raw.endian == -1,
        )
        .map_err(|error| failure(py, error))?;
        Ok(Value::Digits {
            negative: self.raw.negative != 0,
            source,
        })
    }
}

impl Drop for Export<'_, '_> {
    fn drop(&mut self) {
        // SAFETY: only this attached guard owns handle. The retained source
        // prevents a last-owner destructor while the export token is released.
        unsafe { sqlaero_long_close(self.raw.handle) };
    }
}

pub(crate) fn failure(py: Python<'_>, error: Error) -> PyErr {
    match error {
        Error::Allocation(_) => errors::memory_error(py),
        Error::Representation => errors::client(
            py,
            ClientErrorCode::InternalState,
            "CPython integer export has an invalid native digit layout",
        ),
    }
}
