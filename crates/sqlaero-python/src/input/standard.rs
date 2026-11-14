//! Approved standard scalar subclasses and the canonical struct_time boundary.

use super::integer_to;
use super::sink::{Destination, emit};
use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::ValueRef;

pub(super) fn struct_time<D: Destination + ?Sized>(
    value: &Bound<'_, PyTuple>,
    result: &mut D,
    context: &Context,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let py = value.py();
    let mut parts: [Vec<u8>; 6] = std::array::from_fn(|_| Vec::new());
    for (index, part) in parts.iter_mut().enumerate() {
        let value = value.get_item(index)?;
        // SAFETY: the checked public integer protocol deliberately rejects
        // float truncation and numeric strings. It returns an owned exact int.
        let converted = super::control::callback(py, budget, || {
            // SAFETY: Python is attached and the actual field owner is live.
            // The public protocol returns a new owned int or its actual error.
            unsafe { Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Index(value.as_ptr())) }
        }).inspect_err(|error| {
            errors::note_format(py, error, format_args!("while reading struct_time component [{index}]; an integer or __index__ result is required"));
        })?;
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
        ValueRef::CalendarComponents(&std::array::from_fn(|index| parts[index].as_slice())),
        context,
    )
}
