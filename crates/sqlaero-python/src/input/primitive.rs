//! One primitive recognition boundary shared by all output targets.

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
    } else if let Ok(value) = value.cast::<PyInt>() {
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
    } else if let Ok(value) = value.cast::<PyFloat>() {
        ScalarRef::Float(value.value())
    } else if let Ok(value) = value.cast::<PyString>() {
        return Ok(Some(Source::Text(value)));
    } else if let Ok(value) = value.cast::<PyBytes>() {
        ScalarRef::Bytes(value.as_bytes())
    } else {
        return Ok(None);
    };
    Ok(Some(Source::Native(scalar)))
}
