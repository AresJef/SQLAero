//! Approved standard scalar subclasses and the canonical struct_time boundary.

use super::integer_wide;
use super::sink::{Destination, emit};
use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyString, PyTuple};
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::ValueRef;
use sqlaero_core::protocol::framing::WireError;
use sqlaero_transcode::number::IntegerBuffer;
use std::borrow::Cow;

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

fn integer<'a, 'py>(
    value: &'a Bound<'py, PyAny>,
    index: usize,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Cow<'a, Bound<'py, PyAny>>> {
    if value.is_instance_of::<PyInt>() {
        return Ok(Cow::Borrowed(value));
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
    .map(Cow::Owned)
}

fn compact_integer(value: &Bound<'_, PyAny>) -> PyResult<(i64, i32)> {
    let mut overflow = 0;
    // SAFETY: integer() obtained a real int from the immutable tuple slot or
    // an owned __index__ result. The base-storage API does not call overrides.
    let number = unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(value.as_ptr(), &mut overflow) };
    if number == -1
        && let Some(error) = PyErr::take(value.py())
    {
        return Err(error);
    }
    Ok((number, overflow))
}

#[inline(never)]
pub(super) fn struct_time<D: Destination + ?Sized>(
    value: &Bound<'_, PyTuple>,
    offset_name: &Bound<'_, PyString>,
    result: &mut D,
    context: &Context,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let py = value.py();
    let offset = value.as_any().getattr(offset_name)?;
    let offset_seconds = if offset.is_none() {
        None
    } else {
        let offset = integer(&offset, 10, budget)?;
        let (seconds, overflow) = compact_integer(&offset)?;
        if overflow != 0 || !(-86_399..=86_399).contains(&seconds) {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "struct_time tm_gmtoff must be integer seconds strictly within 24 hours",
            ));
        }
        Some(seconds as i32)
    };
    // Only a component outside the compact int range needs heap storage.
    // Keep its checked formatting and numeric-range policy on the wide path.
    let mut compact = std::array::from_fn::<_, 6, _>(|_| IntegerBuffer::new());
    let mut wide: [Vec<u8>; 6] = std::array::from_fn(|_| Vec::new());
    let mut parts = [&[][..]; 6];
    for (index, ((part, compact), wide)) in parts
        .iter_mut()
        .zip(&mut compact)
        .zip(&mut wide)
        .enumerate()
    {
        let source = value.get_borrowed_item(index)?;
        let converted = integer(&source, index, budget)?;
        let (number, overflow) = compact_integer(&converted)?;
        if overflow < 0 || (overflow == 0 && number < 0) {
            return Err(errors::builtin_format::<PyValueError>(
                py,
                format_args!(
                    "struct_time component [{index}] must be nonnegative; negative separators cannot be encoded accurately"
                ),
            ));
        }
        *part = if overflow > 0 {
            integer_wide(&converted, wide, context, overflow)?;
            wide.as_slice()
        } else {
            let digits = compact.format(number).as_bytes();
            // Preserve the original per-component failure before reading the
            // next slot, whose integer protocol may call back into Python.
            if digits.len() > context.limit {
                return Err(errors::wire(
                    py,
                    WireError::Limit {
                        required: digits.len(),
                        limit: context.limit,
                    },
                ));
            }
            digits
        };
    }
    emit(
        result,
        py,
        ValueRef::CalendarComponents {
            parts: &parts,
            offset_seconds,
        },
        context,
    )
}
