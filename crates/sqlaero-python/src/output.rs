//! Final Python values from prepared native fields; no Python decoder loops.

use crate::{errors, input::ScalarTypes, json};
use pyo3::exceptions::PyUnicodeDecodeError;
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyString, PyTime, PyTuple};
use sqlaero_core::charset::{codec::CodecError, output::OutputCodec};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, MemoryError, Scope};
use sqlaero_core::result::decode::{self, ContentError, Integer, Kind, Plan};
use std::sync::Arc;

pub(crate) fn memory(py: Python<'_>, error: MemoryError) -> PyErr {
    match error {
        MemoryError::Allocation(error) => {
            pyo3::exceptions::PyMemoryError::new_err(error.to_string())
        }
        error => errors::client(py, ClientErrorCode::ResourceLimit, error.to_string()),
    }
}
fn content(py: Python<'_>, error: ContentError) -> PyErr {
    errors::client(py, ClientErrorCode::DecodeContent, error.to_string())
}
fn unicode(py: Python<'_>, data: &[u8], codec: OutputCodec, start: usize, end: usize) -> PyErr {
    let original = match bytes(py, data) {
        Ok(value) => value,
        Err(error) => return error,
    };
    match py.get_type::<PyUnicodeDecodeError>().call1((
        codec.name(),
        original,
        start,
        end,
        "invalid or incomplete byte sequence",
    )) {
        Ok(error) => PyErr::from_value(error),
        Err(error) => error,
    }
}

/// Copy a final independent bytes value with normal Python MemoryError behavior.
pub(crate) fn bytes<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: this initialized Rust slice remains alive for the copying C call;
    // its object-backed length fits Py_ssize_t, and Python is attached.
    unsafe {
        Bound::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyBytes_FromStringAndSize(data.as_ptr().cast(), data.len() as isize),
        )
    }
}
pub(crate) fn codepoints<'py>(py: Python<'py>, data: &[u32]) -> PyResult<Bound<'py, PyString>> {
    // SAFETY: every u32 is a Unicode code point (surrogates are intentional
    // only under surrogateescape); CPython copies the initialized slice.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyUnicode_FromKindAndData(
                pyo3::ffi::PyUnicode_4BYTE_KIND as i32,
                data.as_ptr().cast(),
                data.len() as isize,
            ),
        )?
        .cast_into::<PyString>()
        .map_err(Into::into)
    }
}

pub(crate) fn points(
    py: Python<'_>,
    data: &[u8],
    codec: OutputCodec,
    strict: bool,
    scope: Option<&Arc<Scope>>,
) -> PyResult<Buffer<u32>> {
    if let OutputCodec::Unknown(id) = codec {
        return Err(errors::client(
            py,
            ClientErrorCode::UnsupportedCharset,
            format!("cannot decode text with unknown MySQL collation ID {id}"),
        ));
    }
    let mut values = scope.map_or_else(Buffer::new, Buffer::tracked);
    let mut position = 0;
    let mut checkpoint = 64 * 1024;
    while position < data.len() {
        match codec.point(data, position) {
            Ok((point, end)) => {
                values
                    .push(u32::from(point))
                    .map_err(|error| memory(py, error))?;
                position = end;
            }
            Err(CodecError::Decode { start, .. }) if !strict && data[start] >= 128 => {
                values
                    .push(0xdc00 + u32::from(data[start]))
                    .map_err(|error| memory(py, error))?;
                // The recovery retains this raw byte. Reconsider the following
                // byte in its real encoding rather than discarding an interval.
                position = start + 1;
            }
            Err(CodecError::Decode { start, end }) => {
                return Err(unicode(py, data, codec, start, end));
            }
            Err(CodecError::Wire(error)) => return Err(errors::wire(py, error)),
            Err(_) => {
                return Err(errors::client(
                    py,
                    ClientErrorCode::UnsupportedCharset,
                    "output charset conversion is unavailable",
                ));
            }
        }
        if position >= checkpoint {
            py.check_signals()?;
            checkpoint = position.saturating_add(64 * 1024);
        }
    }
    Ok(values)
}

/// Materialize one final str. UTF-8/ASCII/byte-valued text uses CPython's
/// direct native constructors. Other MySQL mappings retain a charged bounded
/// scalar buffer, never an intermediate Python str/bytes or replacement codec.
pub(crate) fn text<'py>(
    py: Python<'py>,
    data: &[u8],
    codec: OutputCodec,
    strict: bool,
    scope: Option<&Arc<Scope>>,
) -> PyResult<Bound<'py, PyString>> {
    let handler = if strict {
        c"strict".as_ptr()
    } else {
        c"surrogateescape".as_ptr()
    };
    let pointer = match codec {
        OutputCodec::Utf8(_) => {
            // SAFETY: attached copying decoder; the initialized slice remains
            // valid and the static error-handler C string is NUL-terminated.
            unsafe {
                pyo3::ffi::PyUnicode_DecodeUTF8(data.as_ptr().cast(), data.len() as isize, handler)
            }
        }
        OutputCodec::Ascii(_) => {
            // SAFETY: same lifetime and attached-decoder conditions as above.
            unsafe {
                pyo3::ffi::PyUnicode_DecodeASCII(data.as_ptr().cast(), data.len() as isize, handler)
            }
        }
        OutputCodec::ByteValues(_) => {
            // SAFETY: every input byte has one Latin-1-valued Unicode scalar.
            unsafe {
                pyo3::ffi::PyUnicode_DecodeLatin1(
                    data.as_ptr().cast(),
                    data.len() as isize,
                    handler,
                )
            }
        }
        _ => return codepoints(py, &points(py, data, codec, strict, scope)?),
    };
    // SAFETY: each branch above returns a new owned Python Unicode reference
    // or NULL with the original Python allocation/content exception set.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(py, pointer)?
            .cast_into::<PyString>()
            .map_err(Into::into)
    }
}

fn json_value<'py>(
    py: Python<'py>,
    data: &[u8],
    codec: OutputCodec,
    scope: Option<&Arc<Scope>>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut converted = scope.map_or_else(Buffer::new, Buffer::tracked);
    let input = if matches!(codec, OutputCodec::Utf8(_) | OutputCodec::Ascii(_)) {
        data
    } else {
        let mut position = 0;
        let mut checkpoint = 64 * 1024;
        while position < data.len() {
            let (point, end) = codec.point(data, position).map_err(|_| {
                content(py, ContentError("JSON field contains invalid encoded text"))
            })?;
            let mut storage = [0; 4];
            converted
                .extend_from_slice(point.encode_utf8(&mut storage).as_bytes())
                .map_err(|error| memory(py, error))?;
            position = end;
            if position >= checkpoint {
                py.check_signals()?;
                checkpoint = position.saturating_add(64 * 1024);
            }
        }
        converted.as_slice()
    };
    match json::decode(py, input) {
        Err(json::DecodeFailure::Invalid(error)) => {
            let main = errors::client(
                py,
                ClientErrorCode::DecodeContent,
                format!("invalid JSON result: {error}"),
            );
            main.set_cause(py, Some(error));
            Err(main)
        }
        Err(json::DecodeFailure::Python(error)) => Err(error),
        Ok(result) => Ok(result),
    }
}

/// Materialize one non-NULL field using its already prepared column plan.
/// All normal failures propagate before the caller commits a fetch position.
pub(crate) fn field<'py>(
    py: Python<'py>,
    data: &[u8],
    plan: Plan,
    types: &ScalarTypes,
    scope: Option<&Arc<Scope>>,
) -> PyResult<Bound<'py, PyAny>> {
    if matches!(plan.kind, Kind::Text | Kind::Set | Kind::Json)
        && let OutputCodec::Unknown(id) = plan.text
    {
        return Err(errors::client(
            py,
            ClientErrorCode::UnsupportedCharset,
            format!("cannot decode text with unknown MySQL collation ID {id}"),
        ));
    }
    match plan.kind {
        Kind::Integer { unsigned, bits } => {
            let value =
                decode::integer(data, unsigned, bits).map_err(|error| content(py, error))?;
            // SAFETY: attached native constructors return a new exact int or
            // NULL/MemoryError; there is no intermediate Python str/float.
            unsafe {
                Bound::from_owned_ptr_or_err(
                    py,
                    match value {
                        Integer::Signed(value) => pyo3::ffi::PyLong_FromLongLong(value),
                        Integer::Unsigned(value) => pyo3::ffi::PyLong_FromUnsignedLongLong(value),
                    },
                )
            }
        }
        Kind::Float => {
            let value = decode::float(data).map_err(|error| content(py, error))?;
            // SAFETY: attached native constructor, preserving its allocation error.
            unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyFloat_FromDouble(value)) }
        }
        Kind::Year => {
            let Integer::Unsigned(value) =
                decode::integer(data, true, 16).map_err(|error| content(py, error))?
            else {
                return Err(content(py, ContentError("invalid YEAR integer kind")));
            };
            if value != 0 && !(1901..=2155).contains(&value) {
                return Err(content(
                    py,
                    ContentError("YEAR result exceeds the MySQL YEAR range"),
                ));
            }
            // SAFETY: attached final integer constructor with a bounded year.
            unsafe {
                Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromUnsignedLongLong(value))
            }
        }
        Kind::Decimal { exact } => {
            decode::decimal(data).map_err(|error| content(py, error))?;
            if !exact {
                let value = decode::float(data).map_err(|error| content(py, error))?;
                // SAFETY: attached native constructor with a validated finite value.
                return unsafe {
                    Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyFloat_FromDouble(value))
                };
            }
            let value = text(py, data, plan.text, true, scope)?;
            types.decimal_type(py).call1((value,))
        }
        Kind::Bytes => bytes(py, data),
        Kind::Bit { width, integer } => {
            let value = decode::bit(data, width).map_err(|error| content(py, error))?;
            if !integer {
                return bytes(py, data);
            }
            // SAFETY: attached native constructor; the full u64 is preserved.
            unsafe {
                Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromUnsignedLongLong(value))
            }
        }
        Kind::Text => text(py, data, plan.text, plan.strict_text, scope).map(Bound::into_any),
        Kind::Json => json_value(py, data, plan.text, scope),
        Kind::Set => {
            if data.is_empty() {
                return Ok(PyTuple::empty(py).into_any());
            }
            let values = points(py, data, plan.text, plan.strict_text, scope)?;
            let mut members = Vec::new();
            let mut start = 0;
            for end in 0..=values.len() {
                if end == values.len() || values[end] == u32::from(b',') {
                    members.try_reserve(1).map_err(|error| {
                        pyo3::exceptions::PyMemoryError::new_err(error.to_string())
                    })?;
                    members.push(codepoints(py, &values[start..end])?.unbind());
                    start = end + 1;
                }
            }
            Ok(PyTuple::new(py, members)?.into_any())
        }
        Kind::Date { strict } => match decode::date(data) {
            Ok(value) => PyDate::new(py, value.year, value.month, value.day).map(Bound::into_any),
            Err(error) if strict => Err(content(py, error)),
            Err(_) => text(py, data, plan.text, plan.strict_text, scope).map(Bound::into_any),
        },
        Kind::Datetime { strict } => match decode::datetime(data) {
            Ok((date, clock)) => PyDateTime::new(
                py,
                date.year,
                date.month,
                date.day,
                clock.hour,
                clock.minute,
                clock.second,
                clock.microsecond,
                None,
            )
            .map(Bound::into_any),
            Err(error) if strict => Err(content(py, error)),
            Err(_) => text(py, data, plan.text, plan.strict_text, scope).map(Bound::into_any),
        },
        Kind::Time { duration } => {
            let value = decode::time(data).map_err(|error| content(py, error))?;
            if duration {
                let days = value.div_euclid(86_400_000_000);
                let rest = value.rem_euclid(86_400_000_000);
                PyDelta::new(
                    py,
                    days as i32,
                    (rest / 1_000_000) as i32,
                    (rest % 1_000_000) as i32,
                    false,
                )
                .map(Bound::into_any)
            } else {
                if !(0..86_400_000_000).contains(&value) {
                    return Err(content(
                        py,
                        ContentError("TIME cannot be represented as a nonnegative clock time"),
                    ));
                }
                PyTime::new(
                    py,
                    (value / 3_600_000_000) as u8,
                    (value / 60_000_000 % 60) as u8,
                    (value / 1_000_000 % 60) as u8,
                    (value % 1_000_000) as u32,
                    None,
                )
                .map(Bound::into_any)
            }
        }
        Kind::Null => Err(content(
            py,
            ContentError("NULL column contains a non-NULL field"),
        )),
    }
}
