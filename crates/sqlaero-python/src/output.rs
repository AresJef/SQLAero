//! Final Python values from prepared native fields; no Python decoder loops.

use crate::{errors, input::ScalarTypes, json};
use pyo3::exceptions::{PyOverflowError, PyUnicodeDecodeError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyString, PyTime, PyTuple};
use sqlaero_core::charset::{codec::CodecError, output::OutputCodec};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, MemoryError, Scope};
use sqlaero_core::result::decode::{self, ContentError, Integer, Kind, Plan};
use std::sync::Arc;

/// Materialize an already interpreted native binary field under its saved plan.
/// The byte view is borrowed only for this attached call; final values own their
/// Python storage before the caller may release or advance the row owner.
pub(crate) fn value<'py>(
    py: Python<'py>,
    value: sqlaero_core::result::value::FieldValue<'_>,
    plan: Plan,
    types: &ScalarTypes,
    scope: Option<&Arc<Scope>>,
) -> PyResult<Bound<'py, PyAny>> {
    use sqlaero_core::result::value::FieldValue;
    match value {
        FieldValue::Null => Ok(py.None().into_bound(py)),
        FieldValue::Bytes(data) => field(py, data, plan, types, scope),
        FieldValue::Signed(number) => binary_integer(py, Integer::Signed(number), plan.kind),
        FieldValue::Unsigned(number) => binary_integer(py, Integer::Unsigned(number), plan.kind),
        FieldValue::Float(number) if matches!(plan.kind, Kind::Float) => {
            if !number.is_finite() {
                return Err(content(
                    py,
                    ContentError("non-finite MySQL floating result"),
                ));
            }
            // SAFETY: this is the actual interpreted IEEE value. The attached
            // final constructor copies it or returns its original exception.
            unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyFloat_FromDouble(number)) }
        }
        FieldValue::Duration(microseconds) => match plan.kind {
            Kind::Time { duration } => time_value(py, microseconds, duration),
            _ => Err(inconsistent_binary(py)),
        },
        FieldValue::Date { year, month, day } => match plan.kind {
            Kind::Date { strict } => match decode::date_components(i32::from(year), month, day) {
                Ok(date) => PyDate::new(py, date.year, date.month, date.day).map(Bound::into_any),
                Err(error) if strict => Err(content(py, error)),
                Err(_) => calendar_fallback(py, value, plan.fractional_seconds_precision),
            },
            _ => Err(inconsistent_binary(py)),
        },
        FieldValue::Datetime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond,
        } => match plan.kind {
            Kind::Datetime { strict } => match decode::datetime_components(
                i32::from(year),
                month,
                day,
                hour,
                minute,
                second,
                microsecond,
            ) {
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
                Err(_) => calendar_fallback(py, value, plan.fractional_seconds_precision),
            },
            _ => Err(inconsistent_binary(py)),
        },
        _ => Err(inconsistent_binary(py)),
    }
}

fn calendar_fallback<'py>(
    py: Python<'py>,
    value: sqlaero_core::result::value::FieldValue<'_>,
    precision: Option<u8>,
) -> PyResult<Bound<'py, PyAny>> {
    use sqlaero_core::result::value::FieldValue;
    let (year, month, day, clock) = match value {
        FieldValue::Date { year, month, day } => (year, month, day, None),
        FieldValue::Datetime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond,
        } => (year, month, day, Some((hour, minute, second, microsecond))),
        _ => return Err(inconsistent_binary(py)),
    };
    // Invalid calendars retain received components, but canonical MySQL display
    // cannot silently truncate an over-wide field or invent a different shape.
    if year > 9999
        || month > 99
        || day > 99
        || clock.is_some_and(|(hour, minute, second, microsecond)| {
            hour > 99 || minute > 99 || second > 99 || microsecond > 999_999
        })
    {
        return Err(errors::client_format(
            py,
            ClientErrorCode::DecodeContent,
            format_args!(
                "binary calendar components do not fit canonical MySQL display: {value:?}"
            ),
        ));
    }
    let rendered = match clock {
        None => errors::message(py, format_args!("{year:04}-{month:02}-{day:02}"))?,
        Some((hour, minute, second, microsecond)) => {
            let precision = precision.filter(|precision| *precision <= 6).ok_or_else(||
                errors::client_format(py, ClientErrorCode::DecodeContent,
                    format_args!("binary datetime fallback requires trustworthy fractional-second precision: {value:?}")))?;
            let divisor = 10_u32.pow(u32::from(6 - precision));
            if !microsecond.is_multiple_of(divisor) {
                return Err(errors::client_format(
                    py,
                    ClientErrorCode::DecodeContent,
                    format_args!(
                        "binary datetime fraction {microsecond} cannot be represented losslessly with declared precision {precision}"
                    ),
                ));
            }
            if precision == 0 {
                errors::message(
                    py,
                    format_args!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"),
                )?
            } else {
                let fraction = microsecond / divisor;
                let width = usize::from(precision);
                errors::message(
                    py,
                    format_args!(
                        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{fraction:0width$}"
                    ),
                )?
            }
        }
    };
    PyString::from_bytes(py, rendered.as_bytes()).map(Bound::into_any)
}

fn inconsistent_binary(py: Python<'_>) -> PyErr {
    errors::client(
        py,
        ClientErrorCode::InternalState,
        "native binary value disagrees with the prepared column plan",
    )
}

fn integer_object<'py>(py: Python<'py>, value: Integer) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: attached constructors return a final exact int or its original
    // exception, without a Python str, float or narrowing of the u64 domain.
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

fn binary_integer<'py>(py: Python<'py>, value: Integer, kind: Kind) -> PyResult<Bound<'py, PyAny>> {
    match kind {
        Kind::Year => {
            let year = match value {
                Integer::Signed(value) => u64::try_from(value).map_err(|_| {
                    content(py, ContentError("YEAR result exceeds the MySQL YEAR range"))
                })?,
                Integer::Unsigned(value) => value,
            };
            if year != 0 && !(1901..=2155).contains(&year) {
                return Err(content(
                    py,
                    ContentError("YEAR result exceeds the MySQL YEAR range"),
                ));
            }
            return integer_object(py, Integer::Unsigned(year));
        }
        Kind::Integer { bits, .. } if bits == 0 || bits > 64 => {
            return Err(inconsistent_binary(py));
        }
        Kind::Integer {
            unsigned: false,
            bits,
        } => match value {
            Integer::Signed(number) => {
                let bound = 1_i128 << (bits - 1);
                if !(-bound..bound).contains(&i128::from(number)) {
                    return Err(content(
                        py,
                        ContentError("signed MySQL integer exceeds its declared range"),
                    ));
                }
            }
            _ => return Err(inconsistent_binary(py)),
        },
        Kind::Integer {
            unsigned: true,
            bits,
        } => match value {
            Integer::Unsigned(number) => {
                if bits < 64 && number >= 1_u64 << bits {
                    return Err(content(
                        py,
                        ContentError("unsigned MySQL integer exceeds its declared range"),
                    ));
                }
            }
            _ => return Err(inconsistent_binary(py)),
        },
        _ => return Err(inconsistent_binary(py)),
    }
    integer_object(py, value)
}

fn time_value<'py>(py: Python<'py>, value: i128, duration: bool) -> PyResult<Bound<'py, PyAny>> {
    const DAY: i128 = 86_400_000_000;
    const MAX: i128 = ((838 * 60 + 59) * 60 + 59) * 1_000_000 + 999_999;
    if !(-MAX..=MAX).contains(&value) {
        return Err(content(
            py,
            ContentError("invalid or out-of-range MySQL TIME value"),
        ));
    }
    if duration {
        let days = value.div_euclid(DAY);
        let rest = value.rem_euclid(DAY);
        // The validated MySQL TIME range proves all constructor widths.
        PyDelta::new(
            py,
            days as i32,
            (rest / 1_000_000) as i32,
            (rest % 1_000_000) as i32,
            false,
        )
        .map(Bound::into_any)
    } else {
        if !(0..DAY).contains(&value) {
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

/// Exercise the same binary interpreter and final output boundary used by rows.
/// The synthetic column is parsed by the production metadata receiver; this
/// private probe adds neither a public row API nor another protocol decoder.
#[pyfunction]
#[pyo3(pass_module, signature = (type_code, data, *, collation_id=255, flags=0, length=0, decimals=0, decode_json=false, decode_set=false, decode_bit=false, use_decimal=true, use_timedelta=true, decode_text_strict=true, decode_date_strict=true))]
#[expect(
    clippy::too_many_arguments,
    reason = "private probe mirrors actual binary metadata and the seven frozen policies"
)]
pub(crate) fn _decode_binary_field<'py>(
    module: &Bound<'py, PyModule>,
    type_code: u8,
    data: Option<&Bound<'py, pyo3::types::PyBytes>>,
    collation_id: u16,
    flags: u16,
    length: u32,
    decimals: u8,
    decode_json: bool,
    decode_set: bool,
    decode_bit: bool,
    use_decimal: bool,
    use_timedelta: bool,
    decode_text_strict: bool,
    decode_date_strict: bool,
) -> PyResult<Bound<'py, PyAny>> {
    use sqlaero_core::{
        protocol::CapabilityFlags,
        result::text::{Receiver, Response},
        session::query::QueryError,
    };
    let py = module.py();
    let Some(data) = data else {
        return Ok(py.None().into_bound(py));
    };
    // Six labels (catalog def and five empty labels), then the ordinary
    // twelve-byte fixed ColumnDefinition payload. All storage is initialized.
    let mut metadata = [
        3, b'd', b'e', b'f', 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, type_code, 0, 0, decimals, 0, 0,
    ];
    metadata[10..12].copy_from_slice(&collation_id.to_le_bytes());
    metadata[12..16].copy_from_slice(&length.to_le_bytes());
    metadata[17..19].copy_from_slice(&flags.to_le_bytes());
    let failure = |error| errors::query(py, QueryError::Result(error));
    let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF;
    let mut receiver = Receiver::isolated(capabilities, 1_000_000).map_err(failure)?;
    receiver.push(&[1]).map_err(failure)?;
    receiver.push(&metadata).map_err(failure)?;
    let Some(Response::Rows(rows)) = receiver.push(&[0xfe, 0, 0, 2, 0, 0, 0]).map_err(failure)?
    else {
        return Err(errors::client(
            py,
            ClientErrorCode::InternalState,
            "private binary metadata fixture did not produce a column",
        ));
    };
    let native = sqlaero_core::protocol::binary::value(&rows.columns[0], data.as_bytes())
        .map_err(|error| errors::wire(py, error))?;
    let owner = errors::attribute(module.as_any(), c"_scalar_types")?;
    let types = owner.extract::<PyRef<'_, ScalarTypes>>()?;
    let plan = Plan::new(
        type_code,
        flags,
        collation_id,
        length,
        decode::Options {
            decode_json,
            decode_set,
            decode_bit,
            use_decimal,
            use_timedelta,
            decode_text_strict,
            decode_date_strict,
        },
    )
    .with_decimals(decimals);
    value(py, native, plan, &types, None)
}

/// Construct a final tuple from already decoded stack values. Array iteration
/// only transfers owned references, so no callback can see unfinished slots.
pub(crate) fn tuple_from_ready<'py, const N: usize>(
    py: Python<'py>,
    values: [Bound<'py, PyAny>; N],
) -> PyResult<Bound<'py, PyTuple>> {
    let length = isize::try_from(N).map_err(|_| {
        crate::errors::builtin_message::<PyOverflowError>(py, "tuple length exceeds Py_ssize_t")
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
        crate::errors::builtin_message::<PyOverflowError>(py, "tuple length exceeds Py_ssize_t")
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
        crate::errors::builtin_message::<PyOverflowError>(py, "tuple length exceeds Py_ssize_t")
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

pub(crate) fn memory(py: Python<'_>, error: impl std::borrow::Borrow<MemoryError>) -> PyErr {
    match error.borrow() {
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
            integer_object(py, value)
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
            time_value(py, i128::from(value), duration)
        }
        Kind::Null => Err(content(
            py,
            ContentError("NULL column contains a non-NULL field"),
        )),
    }
}
