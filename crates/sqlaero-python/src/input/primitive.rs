//! One primitive recognition boundary shared by all output targets.

use pyo3::PyTypeCheck;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyFloatMethods, PyInt, PyString};
use sqlaero_transcode::value::ScalarRef;

/// A current primitive source. Python owners stay at this attached boundary;
/// native facts are borrowed only for immediate consumption.
pub(crate) enum Source<'a, 'py> {
    Native(ScalarRef<'a>),
    Text(&'a Bound<'py, PyString>),
    WideInteger {
        value: &'a Bound<'py, PyInt>,
        overflow: i32,
    },
}

/// Inspect actual base storage, never a replacement conversion protocol.
/// Wider integers retain their owner for the target's checked native transfer.
#[inline(always)]
pub(crate) fn read<'a, 'py>(value: &'a Bound<'py, PyAny>) -> PyResult<Option<Source<'a, 'py>>> {
    let scalar = if value.is_none() {
        ScalarRef::Null
    } else if value.is_exact_instance_of::<PyBool>() {
        ScalarRef::Boolean(value.extract()?)
    } else if let Some(value) = borrow_if::<PyInt>(value) {
        let mut overflow = 0;
        // SAFETY: this is a true int, its owner remains live and Python stays
        // attached. The public conversion reads base storage without invoking
        // an overridden __int__ or __index__; overflow is reported separately.
        let compact =
            unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(value.as_ptr(), &mut overflow) };
        if overflow != 0 {
            return Ok(Some(Source::WideInteger { value, overflow }));
        }
        if compact == -1
            && let Some(error) = PyErr::take(value.py())
        {
            return Err(error);
        }
        ScalarRef::Signed(compact)
    } else if value.is_exact_instance_of::<PyFloat>() {
        // SAFETY: the exact native float check proves the borrowed type.
        let value = unsafe { value.cast_unchecked::<PyFloat>() };
        ScalarRef::Float(value.value())
    } else if let Some(value) = borrow_if::<PyString>(value) {
        // The Unicode type flag avoids a float subtype lookup for text.
        return Ok(Some(Source::Text(value)));
    } else if let Some(value) = borrow_if::<PyFloat>(value) {
        ScalarRef::Float(value.value())
    } else if let Ok(value) = value.cast::<PyBytes>() {
        ScalarRef::Bytes(value.as_bytes())
    } else {
        return Ok(None);
    };
    Ok(Some(Source::Native(scalar)))
}

/// Probe a primitive type without building diagnostics for an expected miss.
#[inline]
fn borrow_if<'a, 'py, T: PyTypeCheck>(value: &'a Bound<'py, PyAny>) -> Option<&'a Bound<'py, T>> {
    if !value.is_instance_of::<T>() {
        return None;
    }
    // SAFETY: PyTypeCheck guarantees the same type proof used by Bound::cast.
    // This borrow uses the original live owner without crossing a callback.
    Some(unsafe { value.cast_unchecked::<T>() })
}
