//! Exact Decimal acquisition through a qualified native string protocol.
//!
//! The native conversion may allocate and is not internally interruptible by
//! this crate. It already underlies the previous native `as_tuple` conversion;
//! checks surround that call and bound our own parsing work. Scientific text
//! is never sent to MySQL: the existing exact native formatter owns SQL output.

use crate::errors;
use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyString, PyTuple, PyType};
use sqlaero_core::bind::scalar::ScalarError;
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::transport::deadline::IoBudget;
use std::ffi::CStr;

const COEFFICIENT_CAPACITY: usize = 65;

/// Select the immutable native protocol without retaining any new Python
/// objects. Mutable and pure-Python providers retain their original hooks.
pub(super) fn native_type(typ: &Bound<'_, PyType>) -> PyResult<bool> {
    let py = typ.py();
    if !typ.get_type().is(py.get_type::<PyType>()) {
        return Ok(false);
    }
    let flags: std::ffi::c_ulong = errors::attribute(typ.as_any(), c"__flags__")?.extract()?;
    if flags & pyo3::ffi::Py_TPFLAGS_IMMUTABLETYPE == 0 {
        return Ok(false);
    }
    let native_str = errors::attribute(py.get_type::<PyAny>().as_any(), c"__str__")?;
    let native_method = errors::attribute(py.get_type::<PyDict>().as_any(), c"copy")?;
    for (name, reference) in [(c"__str__", native_str), (c"as_tuple", native_method)] {
        let Some(descriptor) = optional_attribute(typ.as_any(), name)? else {
            return Ok(false);
        };
        if !descriptor.get_type().is(reference.get_type()) {
            return Ok(false);
        }
        let Some(owner) = optional_attribute(&descriptor, c"__objclass__")? else {
            return Ok(false);
        };
        if !owner.is(typ) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn optional_attribute<'py>(
    value: &Bound<'py, PyAny>,
    name: &CStr,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    match errors::attribute(value, name) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyAttributeError>(value.py()) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Acquire one exact Decimal and write only through the original SQL numeric
/// formatter. No binary float, normalization or context arithmetic is used.
pub(super) fn write(
    value: &Bound<'_, PyAny>,
    native: bool,
    output: &mut Vec<u8>,
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let py = value.py();
    if native {
        super::checkpoint(py, budget)?;
        let text = value.str()?;
        super::checkpoint(py, budget)?;
        if let Some(parsed) = parse(py, text.to_str()?.as_bytes(), budget)? {
            return match parsed {
                Parsed::Null => writer::append(output, b"NULL", context.limit)
                    .map_err(|error| errors::wire(py, error)),
                Parsed::Finite(parts) => parts.write(py, output, context.limit),
            };
        }
        // An unexpected future native representation does not redefine the
        // public numeric contract. Only grammar mismatch falls back; Python
        // allocation, signal and deadline exceptions above are never caught.
    }
    original(value, output, context.limit)
}

fn original(value: &Bound<'_, PyAny>, output: &mut Vec<u8>, limit: usize) -> PyResult<()> {
    let py = value.py();
    let parts = value.call_method0(PyString::from_bytes(py, b"as_tuple")?)?;
    let parts = parts.cast::<PyTuple>()?;
    let exponent = parts.get_item(2)?;
    if !exponent.is_exact_instance_of::<PyInt>() {
        return writer::append(output, b"NULL", limit).map_err(|error| errors::wire(py, error));
    }
    let negative = parts.get_item(0)?.extract::<u8>()? != 0;
    let digits = parts.get_item(1)?.extract::<Vec<u8>>()?;
    let exponent = exponent.extract::<i64>()?;
    scalar::decimal(output, negative, &digits, exponent, limit).map_err(|error| {
        failure(
            py,
            error,
            negative,
            &digits[..digits.len().min(16)],
            digits.len(),
            exponent,
        )
    })
}

enum Parsed {
    Null,
    Finite(Parts),
}

struct Parts {
    negative: bool,
    digits: [u8; COEFFICIENT_CAPACITY],
    necessary: usize,
    preview: [u8; 16],
    original_digits: usize,
    original_exponent: i64,
    trailing: usize,
}

impl Parts {
    fn write(self, py: Python<'_>, output: &mut Vec<u8>, limit: usize) -> PyResult<()> {
        // Preserve the core's exponent-overflow precedence before its exact
        // width rejection. Zero bypasses exponent expansion in the core.
        let result = match i64::try_from(self.trailing)
            .ok()
            .and_then(|trailing| self.original_exponent.checked_add(trailing))
        {
            None => Err(ScalarError::Range(
                "Decimal exponent exceeds the exact SQL range",
            )),
            Some(_) if self.necessary > self.digits.len() => Err(ScalarError::Range(
                "Decimal exceeds 65 necessary decimal places",
            )),
            Some(exponent) => scalar::decimal(
                output,
                self.negative,
                &self.digits[..self.necessary],
                exponent,
                limit,
            ),
        };
        result.map_err(|error| {
            failure(
                py,
                error,
                self.negative,
                &self.preview[..self.original_digits.min(self.preview.len())],
                self.original_digits,
                self.original_exponent,
            )
        })
    }
}

fn failure(
    py: Python<'_>,
    error: ScalarError,
    negative: bool,
    preview: &[u8],
    digits: usize,
    exponent: i64,
) -> PyErr {
    let error = super::scalar_error(py, error);
    errors::note_format(
        py,
        &error,
        format_args!(
            "failed Decimal components: sign={}, digits={:?}{} ({} digits), exponent={exponent}",
            u8::from(negative),
            preview,
            if digits > 16 { " [truncated]" } else { "" },
            digits,
        ),
    );
    error
}

/// Parse only the native ASCII grammar. All storage is fixed size even when
/// an input has many insignificant zeros or an enormous compact exponent.
fn parse(py: Python<'_>, text: &[u8], budget: Option<&IoBudget>) -> PyResult<Option<Parsed>> {
    let mut at = 0;
    let negative = text.first() == Some(&b'-');
    if negative || text.first() == Some(&b'+') {
        at += 1;
    }
    let unsigned = &text[at..];
    if unsigned == b"Infinity" {
        return Ok(Some(Parsed::Null));
    }
    let payload = unsigned
        .strip_prefix(b"NaN")
        .or_else(|| unsigned.strip_prefix(b"sNaN"));
    if let Some(payload) = payload {
        for (index, byte) in payload.iter().enumerate() {
            if index != 0 && index.is_multiple_of(4096) {
                super::checkpoint(py, budget)?;
            }
            if !byte.is_ascii_digit() {
                return Ok(None);
            }
        }
        return Ok(Some(Parsed::Null));
    }
    let mut parts = Parts {
        negative,
        digits: [0; COEFFICIENT_CAPACITY],
        necessary: 0,
        preview: [0; 16],
        original_digits: 0,
        original_exponent: 0,
        trailing: 0,
    };
    let mut dot = false;
    let mut any = false;
    let mut fractional = 0usize;
    while let Some(&byte) = text.get(at) {
        if at != 0 && at.is_multiple_of(4096) {
            super::checkpoint(py, budget)?;
        }
        if byte == b'e' || byte == b'E' {
            break;
        }
        at += 1;
        if byte == b'.' && !dot {
            dot = true;
            continue;
        }
        if !byte.is_ascii_digit() {
            return Ok(None);
        }
        any = true;
        fractional += usize::from(dot);
        let digit = byte - b'0';
        if digit != 0 || parts.original_digits != 0 {
            let index = parts.original_digits;
            if let Some(place) = parts.digits.get_mut(index) {
                *place = digit;
            }
            if let Some(place) = parts.preview.get_mut(index) {
                *place = digit;
            }
            parts.original_digits += 1;
            parts.trailing = if digit == 0 { parts.trailing + 1 } else { 0 };
        }
    }
    if !any {
        return Ok(None);
    }
    let mut exponent = 0i128;
    if at < text.len() {
        at += 1;
        let exp_negative = text.get(at) == Some(&b'-');
        if exp_negative || text.get(at) == Some(&b'+') {
            at += 1;
        }
        let start = at;
        while let Some(&byte) = text.get(at) {
            if at != 0 && at.is_multiple_of(4096) {
                super::checkpoint(py, budget)?;
            }
            if !byte.is_ascii_digit() {
                return Ok(None);
            }
            let Some(next) = exponent
                .checked_mul(10)
                .and_then(|value| value.checked_add(i128::from(byte - b'0')))
            else {
                // Preserve the old exponent conversion's actual exception.
                return Ok(None);
            };
            exponent = next;
            at += 1;
        }
        if at == start {
            return Ok(None);
        }
        if exp_negative {
            exponent = -exponent;
        }
    }
    let Some(exponent) = exponent
        .checked_sub(fractional as i128)
        .and_then(|exponent| i64::try_from(exponent).ok())
    else {
        return Ok(None);
    };
    parts.original_exponent = exponent;
    if parts.original_digits == 0 {
        // Native as_tuple represents every zero with exactly one zero digit;
        // its possibly extreme exponent is immaterial to the SQL zero value.
        parts.original_digits = 1;
        parts.necessary = 1;
    } else {
        parts.necessary = parts.original_digits - parts.trailing;
    }
    Ok(Some(Parsed::Finite(parts)))
}
