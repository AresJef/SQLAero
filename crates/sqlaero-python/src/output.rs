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

/// Construct a final tuple from already decoded stack values. Array iteration
/// only transfers owned references, so no callback can see unfinished slots.
pub(crate) fn tuple_from_ready<'py, const N: usize>(
    py: Python<'py>,
    values: [Bound<'py, PyAny>; N],
) -> PyResult<Bound<'py, PyTuple>> {
    let length = isize::try_from(N).map_err(|_| {
        pyo3::exceptions::PyOverflowError::new_err("tuple length exceeds Py_ssize_t")
    })?;
    // SAFETY: the attached constructor returns a new exact tuple or its original
    // error. The array already owns every final value; filling only moves those
    // references into distinct in-bounds slots. There is no callback, allocation,
    // detach or blocking operation after allocation and before all slots are
    // valid. In free-threaded CPython, GC heap inspection stops attached threads
    // at safepoints; it cannot discover this private tuple inside that interval.
    unsafe {
        let tuple = Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyTuple_New(length))?
            .cast_into_unchecked::<PyTuple>();
        for (index, value) in values.into_iter().enumerate() {
            pyo3::ffi::PyTuple_SET_ITEM(tuple.as_ptr(), index as isize, value.into_ptr());
        }
        Ok(tuple)
    }
}

/// Transfer an existing buffer of final owned values into a fallible tuple.
/// No element conversion or reference cloning runs while its slots are empty.
pub(crate) fn tuple_from_ready_vec<'py>(
    py: Python<'py>,
    values: Vec<Py<PyAny>>,
) -> PyResult<Bound<'py, PyTuple>> {
    let length = isize::try_from(values.len()).map_err(|_| {
        pyo3::exceptions::PyOverflowError::new_err("tuple length exceeds Py_ssize_t")
    })?;
    // SAFETY: the attached constructor returns an owned exact tuple or its
    // original error. Vec iteration and into_ptr only transfer existing owned
    // references into distinct in-bounds slots. No callback, allocation, detach
    // or blocking operation can expose unfinished slots to another thread or GC.
    unsafe {
        let tuple = Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyTuple_New(length))?
            .cast_into_unchecked::<PyTuple>();
        for (index, value) in values.into_iter().enumerate() {
            pyo3::ffi::PyTuple_SET_ITEM(tuple.as_ptr(), index as isize, value.into_ptr());
        }
        Ok(tuple)
    }
}

/// Fill one final tuple without exposing its unfinished slots to Python GC.
/// The iterator may decode values or check signals. Keep the tuple untracked
/// until all slots are valid; already completed children remain normal values.
/// The sole owned reference also cleans up initialized slots on any exception.
/// Allocation through immediate untracking stays attached without callbacks,
/// detach or blocking operations. Free-threaded GC cannot inspect an attached
/// thread's private heap until it reaches a safepoint. Once untracked, this
/// private tuple stays invisible to GC while decoding or signal handlers run.
pub(crate) fn tuple_from_values<'py>(
    py: Python<'py>,
    mut values: impl ExactSizeIterator<Item = PyResult<Bound<'py, PyAny>>>,
) -> PyResult<Bound<'py, PyTuple>> {
    let length = isize::try_from(values.len()).map_err(|_| {
        pyo3::exceptions::PyOverflowError::new_err("tuple length exceeds Py_ssize_t")
    })?;
    // SAFETY: attached CPython returns an owned exact tuple or NULL with its
    // original allocation error. The only owner is local, and no callback,
    // detach, blocking operation or safepoint intervenes before untracking.
    // CPython's free-threaded GC inspections stop the world, and object
    // enumeration excludes untracked values. The empty tuple is shared and
    // must not be re-tracked.
    let tuple = unsafe {
        let tuple = Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyTuple_New(length))?
            .cast_into_unchecked::<PyTuple>();
        if length != 0 {
            pyo3::ffi::PyObject_GC_UnTrack(tuple.as_ptr().cast());
        }
        tuple
    };
    for index in 0..length {
        let value = values.next().ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "native tuple iterator ended early",
            )
        })??;
        // SAFETY: this private tuple has one owner, is hidden from GC, and
        // index is within its allocated length. Each new slot steals exactly
        // one owned final value; no initialized slot is overwritten.
        unsafe { pyo3::ffi::PyTuple_SET_ITEM(tuple.as_ptr(), index, value.into_ptr()) };
    }
    if let Some(extra) = values.next() {
        extra?;
        return Err(errors::client(
            py,
            ClientErrorCode::InternalState,
            "native tuple iterator exceeded its declared length",
        ));
    }
    if length != 0 {
        // SAFETY: every slot is initialized and the tuple is still untracked.
        // It may now safely become visible to ordinary GC and user callbacks.
        unsafe { pyo3::ffi::PyObject_GC_Track(tuple.as_ptr().cast()) };
    }
    Ok(tuple)
}

pub(crate) fn memory(py: Python<'_>, error: MemoryError) -> PyErr {
    match error {
        MemoryError::Allocation(_) => errors::memory_error(py),
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
    // SAFETY: each documented Unicode decoder above returns a new Unicode
    // reference or NULL with the original exception set. The same constructor
    // contract used by codepoints makes a second dynamic type check redundant.
    unsafe {
        Ok(Bound::<PyAny>::from_owned_ptr_or_err(py, pointer)?.cast_into_unchecked::<PyString>())
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
                    members
                        .try_reserve(1)
                        .map_err(|_| errors::memory_error(py))?;
                    members.push(codepoints(py, &values[start..end])?.into_any().unbind());
                    start = end + 1;
                }
            }
            Ok(tuple_from_ready_vec(py, members)?.into_any())
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
