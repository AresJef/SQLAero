//! Approved standard scalar subclasses and the canonical struct_time boundary.

use super::integer_to;
use super::sink::{Destination, emit};
use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyTuple};
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::ValueRef;

pub(super) const STRUCT_FIELDS: [&str; 11] = [
    "tm_year",
    "tm_mon",
    "tm_mday",
    "tm_hour",
    "tm_min",
    "tm_sec",
    "tm_wday",
    "tm_yday",
    "tm_isdst",
    "tm_zone",
    "tm_gmtoff",
];

/// The source is an exact, non-subclassable standard struct_time. Only its
/// first nine fields are tuple slots; extra facts use its public attributes.
pub(super) fn struct_field<'py>(
    value: &Bound<'py, PyTuple>,
    index: usize,
) -> PyResult<Bound<'py, PyAny>> {
    match index {
        0..9 => value.get_item(index),
        9 => errors::attribute(value.as_any(), c"tm_zone"),
        10 => errors::attribute(value.as_any(), c"tm_gmtoff"),
        _ => Err(errors::client(
            value.py(),
            sqlaero_core::error::ClientErrorCode::InternalState,
            "struct_time field index is outside its fixed source schema",
        )),
    }
}

fn integer<'py>(
    value: &Bound<'py, PyAny>,
    index: usize,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Bound<'py, PyAny>> {
    if value.is_instance_of::<PyInt>() {
        return Ok(value.clone());
    }
    let py = value.py();
    super::control::callback(py, budget, || {
        // SAFETY: attached and owned input; the approved integer protocol
        // returns a new exact integer or preserves its actual exception.
        unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Index(value.as_ptr())) }
    }).inspect_err(|error| {
        errors::note_format(py, error, format_args!(
            "while reading struct_time {} [{index}]; an integer or __index__ result is required",
            STRUCT_FIELDS[index],
        ));
    })
}

pub(super) fn struct_time<D: Destination + ?Sized>(
    value: &Bound<'_, PyTuple>,
    result: &mut D,
    context: &Context,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let py = value.py();
    let offset = struct_field(value, 10)?;
    let offset_seconds = if offset.is_none() {
        None
    } else {
        let offset = integer(&offset, 10, budget)?;
        let mut overflow = 0;
        // SAFETY: integer() acquired actual int storage; overflow is separate
        // from an exception and does not invoke an int subclass replacement.
        let seconds =
            unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(offset.as_ptr(), &mut overflow) };
        if seconds == -1
            && let Some(error) = PyErr::take(py)
        {
            return Err(error);
        }
        if overflow != 0 || !(-86_399..=86_399).contains(&seconds) {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "struct_time tm_gmtoff must be integer seconds strictly within 24 hours",
            ));
        }
        Some(seconds as i32)
    };
    let mut parts: [Vec<u8>; 6] = std::array::from_fn(|_| Vec::new());
    for (index, part) in parts.iter_mut().enumerate() {
        let value = value.get_item(index)?;
        let converted = integer(&value, index, budget)?;
        let mut overflow = 0;
        // SAFETY: `converted` is an owned exact int; range overflow is reported
        // separately, so large positive components retain the integer policy.
        let sign =
            unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(converted.as_ptr(), &mut overflow) };
        // SAFETY: attachment is held and the immediately preceding checked C
        // conversion is the only operation that may have set this error.
        if sign == -1 && unsafe { !pyo3::ffi::PyErr_Occurred().is_null() } {
            return Err(PyErr::fetch(py));
        }
        if overflow < 0 || (overflow == 0 && sign < 0) {
            return Err(errors::builtin_format::<PyValueError>(
                py,
                format_args!(
                    "struct_time component [{index}] must be nonnegative; negative separators cannot be encoded accurately"
                ),
            ));
        }
        integer_to(&converted, part, context)?;
    }
    emit(
        result,
        py,
        ValueRef::CalendarComponents {
            parts: &std::array::from_fn(|index| parts[index].as_slice()),
            offset_seconds,
        },
        context,
    )
}
