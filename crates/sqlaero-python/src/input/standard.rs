//! Approved standard scalar subclasses and the canonical struct_time boundary.

use super::{ScalarTypes, binary, clock, date, delta, integer, scalar_error, text};
use crate::{errors, output};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBytes, PyDate, PyDateTime, PyDelta, PyFloat, PyInt, PyString, PyTime, PyTuple,
};
use sqlaero_core::bind::{Context, scalar, temporal};
use sqlaero_core::transport::deadline::IoBudget;

/// Invoke only protocols belonging to a confirmed actual base-type family.
/// Conversion results are written once and never interpreted as SQL source.
pub(super) fn write(
    types: &ScalarTypes,
    value: &Bound<'_, PyAny>,
    result: &mut Vec<u8>,
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<bool> {
    let py = value.py();
    if value.get_type().is(types.struct_time.bind(py)) {
        struct_time(value.cast::<PyTuple>()?, result, context)?;
        return Ok(true);
    }
    if value.is_instance_of::<PyString>() {
        text(&value.str()?, result, context, true, budget)?;
    } else if value.is_instance_of::<PyInt>() {
        // SAFETY: the public protocol returns a new exact int or its original
        // exception. No source slot or foreign view is held across the call.
        let converted = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Long(value.as_ptr()))?
        };
        integer(&converted, result, context.limit)?;
    } else if value.is_instance_of::<PyFloat>() {
        // SAFETY: formal float conversion preserves the subclass __float__
        // behavior, unlike directly reading its base PyFloat storage.
        let converted = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Float(value.as_ptr()))?
        };
        scalar::float(result, converted.extract::<f64>()?, context.limit)
            .map_err(|error| errors::wire(py, error))?;
    } else if value.is_instance_of::<PyBytes>() {
        // SAFETY: the bytes protocol is used only for a true bytes subclass;
        // bytearray and memoryview cannot enter this branch.
        let converted = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyObject_Bytes(value.as_ptr()))?
        };
        binary(
            py,
            converted.cast::<PyBytes>()?.as_bytes(),
            result,
            context.limit,
            budget,
        )?;
    } else if value.is_instance_of::<PyDateTime>() {
        types.write_datetime(value.cast::<PyDateTime>()?, result, context)?;
    } else if value.is_instance_of::<PyDate>() {
        scalar::date_time(result, date(value.cast::<PyDate>()?), None, context.limit)
            .map_err(|error| errors::wire(py, error))?;
    } else if value.is_instance_of::<PyTime>() {
        scalar::time(result, clock(value.cast::<PyTime>()?), context.limit)
            .map_err(|error| errors::wire(py, error))?;
    } else if value.is_instance_of::<PyDelta>() {
        scalar::duration(result, delta(value.cast::<PyDelta>()?), context.limit)
            .map_err(|error| errors::wire(py, error))?;
    } else if super::actual_instance(value, types.decimal_type(py)) {
        let converted = types
            .decimal_type(py)
            .call1(output::tuple_from_ready(py, [value.clone()])?)?;
        types.write(&converted, result, context, budget)?;
    } else {
        return Ok(false);
    }
    Ok(true)
}

fn struct_time(
    value: &Bound<'_, PyTuple>,
    result: &mut Vec<u8>,
    context: &Context,
) -> PyResult<()> {
    let py = value.py();
    let mut parts: [Vec<u8>; 6] = std::array::from_fn(|_| Vec::new());
    for (index, part) in parts.iter_mut().enumerate() {
        let value = value.get_item(index)?;
        // SAFETY: the checked public integer protocol deliberately rejects
        // float truncation and numeric strings. It returns an owned exact int.
        let converted = unsafe { Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Index(value.as_ptr())) }.inspect_err(|error| {
            errors::note(py, error, format!("while reading struct_time component [{index}]; an integer or __index__ result is required"));
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
            return Err(errors::builtin_message::<PyValueError>(
                py,
                format!(
                    "struct_time component [{index}] must be nonnegative; negative separators cannot be encoded accurately"
                ),
            ));
        }
        integer(&converted, part, context.limit)?;
    }
    temporal::write_components(
        result,
        std::array::from_fn(|index| parts[index].as_slice()),
        context.limit,
    )
    .map_err(|error| scalar_error(py, error))
}
