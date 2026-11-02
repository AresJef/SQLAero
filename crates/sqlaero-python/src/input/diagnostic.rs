//! Bounded Escape previews formatted only through the shared fallible writer.

use std::fmt::{self, Write};

use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyBytes, PyDate, PyDateAccess, PyDateTime, PyDelta, PyFloat, PyInt, PyString, PyTime,
    PyTimeAccess, PyTzInfoAccess,
};

use crate::errors;

struct Prefix<'a>(&'a str, usize);

impl fmt::Display for Prefix<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for character in self.0.chars().take(self.1) {
            formatter.write_char(character)?;
        }
        Ok(())
    }
}

/// Borrow the bounded raw preview; formatting itself creates no intermediate
/// String and never calls Python or scans the complete field.
pub(crate) struct BinaryPreview<'a> {
    name: &'a str,
    value: &'a [u8],
    maximum: usize,
}

impl fmt::Display for BinaryPreview<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} b'", self.name)?;
        for byte in self.value.iter().take(self.maximum) {
            for ascii in std::ascii::escape_default(*byte) {
                formatter.write_char(char::from(ascii))?;
            }
        }
        write!(
            formatter,
            "'{} (original length {})",
            if self.value.len() > self.maximum {
                " [truncated]"
            } else {
                ""
            },
            self.value.len()
        )
    }
}

pub(crate) fn binary_diagnostic<'a>(
    name: &'a str,
    value: &'a [u8],
    maximum: usize,
) -> BinaryPreview<'a> {
    BinaryPreview {
        name,
        value,
        maximum,
    }
}

pub(super) fn value(value: &Bound<'_, PyAny>) -> PyResult<String> {
    let py = value.py();
    let type_name = value.get_type().name()?;
    let name = Prefix(type_name.to_str()?, 64);
    if value.is_exact_instance_of::<PyString>() {
        let length = value.len()?;
        // SAFETY: the owned exact str is live and this thread is attached.
        // Only its bounded prefix is materialized, with the real failure kept.
        let prefix = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyUnicode_Substring(value.as_ptr(), 0, 32),
            )?
        };
        let preview = prefix.repr()?;
        let preview = preview.to_str()?;
        let clipped = preview.chars().count() > 140;
        return errors::message(
            py,
            format_args!(
                "{name} {}{} (original length {length})",
                Prefix(preview, 140),
                if length > 32 || clipped {
                    " [truncated]"
                } else {
                    ""
                }
            ),
        );
    }
    if value.is_exact_instance_of::<PyBytes>() {
        let preview =
            binary_diagnostic(type_name.to_str()?, value.cast::<PyBytes>()?.as_bytes(), 32);
        return errors::message(py, format_args!("{preview}"));
    }
    if value.is_exact_instance_of::<PyInt>() {
        let bits = errors::integer_bit_length(value)?.extract::<u64>()?;
        if bits > 512 {
            return errors::message(py, format_args!("int ({bits} bits; value preview omitted)"));
        }
    }
    if value.is_none()
        || value.is_exact_instance_of::<PyBool>()
        || value.is_exact_instance_of::<PyInt>()
        || value.is_exact_instance_of::<PyFloat>()
        || value.is_exact_instance_of::<PyDate>()
        || value.is_exact_instance_of::<PyDelta>()
    {
        let preview = value.repr()?;
        return errors::message(py, format_args!("{name} {}", preview.to_str()?));
    }
    if value.is_exact_instance_of::<PyDateTime>() {
        let value = value.cast::<PyDateTime>()?;
        return errors::message(
            py,
            format_args!(
                "datetime {:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06} (tzinfo {})",
                value.get_year(),
                value.get_month(),
                value.get_day(),
                value.get_hour(),
                value.get_minute(),
                value.get_second(),
                value.get_microsecond(),
                if value.get_tzinfo().is_some() {
                    "present; not formatted"
                } else {
                    "None"
                },
            ),
        );
    }
    if value.is_exact_instance_of::<PyTime>() {
        let value = value.cast::<PyTime>()?;
        return errors::message(
            py,
            format_args!(
                "time {:02}:{:02}:{:02}.{:06} (tzinfo not formatted)",
                value.get_hour(),
                value.get_minute(),
                value.get_second(),
                value.get_microsecond(),
            ),
        );
    }
    errors::message(py, format_args!("{name} (value preview omitted)"))
}
