//! Exact Python scalar readers; no Python objects cross into the native core.

use crate::errors;
use pyo3::exceptions::{PyTypeError, PyUnicodeEncodeError};
use pyo3::prelude::*;
use pyo3::sync::critical_section::with_critical_section;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDate, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess,
    PyFloat, PyInt, PyString, PyTime, PyTimeAccess, PyTuple, PyType, PyTzInfoAccess,
};
use smallvec::SmallVec;
use sqlaero_core::bind::scalar::{Clock, Date, ScalarError};
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::error::ClientErrorCode;
#[cfg(Py_GIL_DISABLED)]
use sqlaero_core::protocol::framing::WireError;
use sqlaero_core::transport::deadline::IoBudget;

/// Module-owned immutable type references; there is no process-global Python
/// type pointer or per-value import, and a future interpreter can own its state.
#[pyclass(frozen)]
pub(crate) struct ScalarTypes {
    decimal: Py<PyType>,
    utc: Py<PyAny>,
}

impl ScalarTypes {
    /// Describe only the failed input, with bounded text and no arbitrary repr
    /// or tzinfo callback. Failure to obtain this optional detail is ignored by
    /// callers so it cannot replace the real conversion exception.
    pub(crate) fn diagnostic(&self, value: &Bound<'_, PyAny>) -> PyResult<String> {
        let py = value.py();
        let name = value
            .get_type()
            .name()?
            .to_string_lossy()
            .chars()
            .take(64)
            .collect::<String>();
        if value.is_exact_instance_of::<PyString>() {
            let length = value.len()?;
            // SAFETY: the owned exact str remains alive and attached. This
            // bounded substring returns an owned value or its actual error,
            // without infallible temporary Python slice/integer objects.
            let prefix = unsafe {
                Bound::<PyAny>::from_owned_ptr_or_err(
                    py,
                    pyo3::ffi::PyUnicode_Substring(value.as_ptr(), 0, 32),
                )?
            };
            let preview = prefix.repr()?.extract::<String>()?;
            let clipped = preview.chars().count() > 140;
            let preview: String = preview.chars().take(140).collect();
            return Ok(format!(
                "{name} {preview}{} (original length {length})",
                if length > 32 || clipped {
                    " [truncated]"
                } else {
                    ""
                }
            ));
        }
        if value.is_exact_instance_of::<PyBytes>() {
            return Ok(binary_diagnostic(
                &name,
                value.cast::<PyBytes>()?.as_bytes(),
                32,
            ));
        }
        if value.is_exact_instance_of::<PyByteArray>() {
            let array = value.cast::<PyByteArray>()?;
            let (preview, copied, length) = with_critical_section(array, || {
                // SAFETY: the bytearray is protected and stays owned. Only a
                // bounded native copy occurs before the slice is discarded.
                let source = unsafe { array.as_bytes() };
                let mut preview = [0; 32];
                let copied = source.len().min(preview.len());
                preview[..copied].copy_from_slice(&source[..copied]);
                (preview, copied, source.len())
            });
            return Ok(binary_preview(&name, &preview[..copied], length));
        }
        if value.is_exact_instance_of::<PyInt>() {
            let bits = errors::integer_bit_length(value)?.extract::<u64>()?;
            if bits > 512 {
                return Ok(format!("int ({bits} bits; value preview omitted)"));
            }
        }
        if value.is_none()
            || value.is_exact_instance_of::<PyBool>()
            || value.is_exact_instance_of::<PyInt>()
            || value.is_exact_instance_of::<PyFloat>()
            || value.is_exact_instance_of::<PyDate>()
            || value.is_exact_instance_of::<PyDelta>()
        {
            return Ok(format!("{name} {}", value.repr()?.to_string_lossy()));
        }
        if value.is_exact_instance_of::<PyDateTime>() {
            let value = value.cast::<PyDateTime>()?;
            return Ok(format!(
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
                }
            ));
        }
        if value.is_exact_instance_of::<PyTime>() {
            let value = value.cast::<PyTime>()?;
            return Ok(format!(
                "time {:02}:{:02}:{:02}.{:06} (tzinfo not formatted)",
                value.get_hour(),
                value.get_minute(),
                value.get_second(),
                value.get_microsecond()
            ));
        }
        // Decimal errors already describe the native numeric restriction.
        // Avoid formatting an arbitrarily long coefficient merely for notes.
        Ok(format!("{name} (value preview omitted)"))
    }
    pub(crate) fn decimal_type<'py>(&self, py: Python<'py>) -> &Bound<'py, PyType> {
        self.decimal.bind(py)
    }
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            decimal: py
                .import("decimal")?
                .getattr("Decimal")?
                .cast_into()?
                .unbind(),
            utc: py
                .import("datetime")?
                .getattr("timezone")?
                .getattr("utc")?
                .unbind(),
        })
    }

    /// Read one supported exact scalar directly into the native request.
    /// Approved tzinfo code executes only here, while attached to Python.
    pub(crate) fn write(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        let py = value.py();
        let limit = context.limit;
        if value.is_none() {
            return writer::append(output, b"NULL", limit).map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyBool>() {
            return writer::append(
                output,
                if value.extract::<bool>()? { b"1" } else { b"0" },
                limit,
            )
            .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyInt>() {
            return integer(value, output, limit);
        }
        if value.is_exact_instance_of::<PyFloat>() {
            return scalar::float(output, value.extract::<f64>()?, limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyString>() {
            return text(value.cast::<PyString>()?, output, context, true, budget);
        }
        if value.is_exact_instance_of::<PyBytes>() {
            return binary(
                py,
                value.cast::<PyBytes>()?.as_bytes(),
                output,
                limit,
                budget,
            );
        }
        if value.is_exact_instance_of::<PyByteArray>() {
            return binary_bytearray(value.cast::<PyByteArray>()?, output, limit, budget);
        }
        if value.is_exact_instance_of::<PyDateTime>() {
            let datetime = value.cast::<PyDateTime>()?;
            let mut date = date(datetime);
            let mut clock = clock(datetime);
            let offset = match datetime.get_tzinfo() {
                None => None,
                Some(zone) if zone.is(self.utc.bind(py)) => Some(0),
                Some(_) => {
                    // The exact built-in method validates the tzinfo return
                    // type and offset range, preserving its actual exception.
                    let offset = datetime.call_method0("utcoffset")?;
                    if offset.is_none() {
                        None
                    } else {
                        Some(delta(offset.cast::<PyDelta>()?))
                    }
                }
            };
            if let Some(offset) = offset {
                if !context.utc_confirmed {
                    return Err(errors::client(
                        py,
                        ClientErrorCode::SessionState,
                        "aware datetime requires a reliably confirmed UTC MySQL session; time_zone is non-UTC or unknown",
                    ));
                }
                (date, clock) =
                    scalar::utc(date, clock, offset).map_err(|e| scalar_error(py, e))?;
            }
            return scalar::date_time(output, date, Some(clock), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyDate>() {
            return scalar::date_time(output, date(value.cast::<PyDate>()?), None, limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyTime>() {
            return scalar::time(output, clock(value.cast::<PyTime>()?), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyDelta>() {
            return scalar::duration(output, delta(value.cast::<PyDelta>()?), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.get_type().is(self.decimal.bind(py)) {
            let parts = value.call_method0("as_tuple")?;
            let parts = parts.cast::<PyTuple>()?;
            let exponent = parts.get_item(2)?;
            if !exponent.is_exact_instance_of::<PyInt>() {
                return writer::append(output, b"NULL", limit).map_err(|e| errors::wire(py, e));
            }
            let negative = parts.get_item(0)?.extract::<u8>()? != 0;
            let digits = parts.get_item(1)?.extract::<Vec<u8>>()?;
            let exponent = exponent.extract::<i64>()?;
            return scalar::decimal(output, negative, &digits, exponent, limit).map_err(|error| {
                let error = scalar_error(py, error);
                errors::note(py, &error, format!("failed Decimal components: sign={}, digits={:?}{} ({} digits), exponent={exponent}", u8::from(negative), &digits[..digits.len().min(16)], if digits.len() > 16 { " [truncated]" } else { "" }, digits.len()));
                error
            });
        }
        Err(PyTypeError::new_err(format!(
            "unsupported exact escape type: {}",
            value.get_type().name()?
        )))
    }
}

/// Format a bounded raw-byte preview without callbacks or scanning the value.
pub(crate) fn binary_diagnostic(name: &str, value: &[u8], maximum: usize) -> String {
    binary_preview(name, &value[..value.len().min(maximum)], value.len())
}

fn binary_preview(name: &str, preview: &[u8], length: usize) -> String {
    let escaped: String = preview
        .iter()
        .flat_map(|byte| std::ascii::escape_default(*byte))
        .map(char::from)
        .collect();
    format!(
        "{name} b'{escaped}'{} (original length {})",
        if length > preview.len() {
            " [truncated]"
        } else {
            ""
        },
        length
    )
}

fn integer(value: &Bound<'_, PyAny>, output: &mut Vec<u8>, limit: usize) -> PyResult<()> {
    let py = value.py();
    let mut overflow = 0;
    // SAFETY: exact-int dispatch precedes this call; value remains owned and
    // Python stays attached. The public C API reports overflow without a
    // lossy conversion or a Python string. No private integer layout is read.
    let compact = unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(value.as_ptr(), &mut overflow) };
    if overflow == 0 {
        if compact == -1
            && let Some(error) = PyErr::take(py)
        {
            return Err(error);
        }
        return scalar::integer(output, compact, limit).map_err(|e| errors::wire(py, e));
    }
    if overflow > 0 {
        match value.extract::<u64>() {
            Ok(unsigned) => {
                return scalar::integer(output, unsigned, limit).map_err(|e| errors::wire(py, e));
            }
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyOverflowError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    // Exact int's public constant-time size query avoids copying a huge,
    // already out-of-policy integer merely to discover its native bit count.
    if errors::integer_bit_length(value)?.extract::<u64>()? > scalar::MAX_INTEGER_BITS {
        return Err(errors::client(
            py,
            ClientErrorCode::NumericRange,
            "integer exceeds the 65-digit exact SQL range",
        ));
    }
    let value = errors::integer_value(value)?;
    scalar::large_integer(output, &value, limit).map_err(|e| scalar_error(py, e))
}

fn scalar_error(py: Python<'_>, error: ScalarError) -> PyErr {
    match error {
        ScalarError::Wire(error) => errors::wire(py, error),
        ScalarError::Range(message) => errors::client(py, ClientErrorCode::NumericRange, message),
    }
}
fn date(value: &impl PyDateAccess) -> Date {
    Date {
        year: value.get_year(),
        month: value.get_month(),
        day: value.get_day(),
    }
}
fn clock(value: &impl PyTimeAccess) -> Clock {
    Clock {
        hour: value.get_hour(),
        minute: value.get_minute(),
        second: value.get_second(),
        microsecond: value.get_microsecond(),
    }
}
fn delta(value: &impl PyDeltaAccess) -> i128 {
    (i128::from(value.get_days()) * 86_400 + i128::from(value.get_seconds())) * 1_000_000
        + i128::from(value.get_microseconds())
}

fn encode_error(
    source: &Bound<'_, PyString>,
    encoding: &str,
    start: usize,
    end: usize,
    reason: &str,
) -> PyErr {
    match source
        .py()
        .get_type::<PyUnicodeEncodeError>()
        .call1((encoding, source, start, end, reason))
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}
fn codec_error(
    source: &Bound<'_, PyString>,
    text: &str,
    encoding: &str,
    error: CodecError,
) -> PyErr {
    let py = source.py();
    match error {
        CodecError::Encode { offset, point } => {
            let start = text[..offset].chars().count();
            encode_error(
                source,
                encoding,
                start,
                start + 1,
                &format!("character U+{point:04X} is not representable in MySQL {encoding}"),
            )
        }
        CodecError::Wire(error) => errors::wire(py, error),
        error => errors::client(py, ClientErrorCode::UnsupportedCharset, error.to_string()),
    }
}

const WORK_BYTES: usize = 256 * 1024;

/// Copy and consume one mutable scalar without an unprotected Python view.
/// Callers keep mutable inputs stable; this is not an atomic snapshot across
/// the short copy windows. The completed Rust buffer is independently owned.
// Free-threaded object-lock work stays out of the common scalar visitor.
#[inline(never)]
fn binary_bytearray(
    source: &Bound<'_, PyByteArray>,
    output: &mut Vec<u8>,
    limit: usize,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let py = source.py();
    // Only the free-threaded boundary needs the fused lock window. Ordinary
    // GIL builds retain their measured layout; both paths own the same bytes.
    #[cfg(Py_GIL_DISABLED)]
    let mut snapshot = SmallVec::<[u8; 32]>::new();
    #[cfg(Py_GIL_DISABLED)]
    let length = with_critical_section(source, || {
        let length = source.len();
        if length <= snapshot.inline_size() {
            writer::binary_width(output.len(), length, limit)?;
            if length != 0 {
                // SAFETY: source stays owned and protected throughout this
                // bounded native copy. Rechecking the view length proves it
                // fits inline, so extending cannot allocate or call Python.
                let current = unsafe { source.as_bytes() };
                if current.len() == length {
                    snapshot.extend_from_slice(current);
                }
            }
        }
        // Only native state escapes the guard; Python errors stay outside.
        Ok::<usize, WireError>(length)
    })
    .map_err(|error| errors::wire(py, error))?;
    #[cfg(not(Py_GIL_DISABLED))]
    let length = with_critical_section(source, || source.len());
    #[cfg(Py_GIL_DISABLED)]
    if length <= snapshot.inline_size() {
        if snapshot.len() != length {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "bytearray length changed while binding; current request was not sent",
            ));
        }
        return binary(py, &snapshot, output, limit, budget);
    }
    if length > WORK_BYTES {
        checkpoint(py, budget)?;
    }
    writer::binary_width(output.len(), length, limit).map_err(|error| errors::wire(py, error))?;
    // Keep inline storage local until the common binary writer consumes it.
    #[cfg(not(Py_GIL_DISABLED))]
    let mut snapshot = SmallVec::<[u8; 32]>::new();
    snapshot
        .try_reserve_exact(length)
        .map_err(|_| errors::memory_error(py))?;
    for start in (0..length).step_by(WORK_BYTES) {
        if length > WORK_BYTES {
            checkpoint(py, budget)?;
        }
        let end = start + (length - start).min(WORK_BYTES);
        let copied = with_critical_section(source, || {
            // SAFETY: source remains owned and its object lock (or the GIL)
            // protects this freshly acquired slice. No Python call, callback,
            // detach, or reference drop occurs before the native copy ends.
            let current = unsafe { source.as_bytes() };
            if current.len() != length {
                return false;
            }
            let Some(chunk) = current.get(start..end) else {
                return false;
            };
            // Reservation above covers every chunk, so this cannot allocate.
            snapshot.extend_from_slice(chunk);
            true
        });
        if !copied {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "bytearray length changed while binding; current request was not sent",
            ));
        }
    }
    binary(py, &snapshot, output, limit, budget)
}

fn binary(
    py: Python<'_>,
    source: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    if source.len() <= WORK_BYTES {
        return writer::binary(output, source, limit).map_err(|error| errors::wire(py, error));
    }
    checkpoint(py, budget)?;
    writer::binary_start(output, source.len(), limit).map_err(|error| errors::wire(py, error))?;
    for chunk in source.chunks(WORK_BYTES) {
        checkpoint(py, budget)?;
        writer::binary_fragment(output, chunk, limit).map_err(|error| errors::wire(py, error))?;
    }
    writer::binary_end(output, limit).map_err(|error| errors::wire(py, error))
}

/// Check the original whole-operation budget and actual pending Python signal.
// Keep the Python-error path out of the recursive value visitor's stack frame.
// This boundary remains checked at every existing signal/deadline checkpoint.
#[inline(never)]
pub(crate) fn check(py: Python<'_>, budget: &IoBudget) -> PyResult<()> {
    py.check_signals()?;
    budget.check().map_err(|error| errors::timeout(py, error))
}
fn checkpoint(py: Python<'_>, budget: Option<&IoBudget>) -> PyResult<()> {
    match budget {
        Some(budget) => check(py, budget),
        None => py.check_signals(),
    }
}

/// The same strict text conversion serves SQL source and quoted value content.
/// Only the boundary performs signal checks; the codec never calls Python.
pub(crate) fn text(
    source: &Bound<'_, PyString>,
    output: &mut Vec<u8>,
    context: &Context,
    quoted: bool,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let py = source.py();
    let text = source.to_str().map_err(|error| {
        if error.is_instance_of::<PyUnicodeEncodeError>(py) {
            let native = error.value(py);
            let positions = native
                .getattr("start")
                .and_then(|start| start.extract::<usize>())
                .and_then(|start| {
                    native
                        .getattr("end")?
                        .extract::<usize>()
                        .map(|end| (start, end))
                });
            if let Ok((start, end)) = positions {
                return encode_error(
                    source,
                    context.codec.name(),
                    start,
                    end,
                    "surrogates are not valid input characters",
                );
            }
        }
        error
    })?;
    if quoted && text.len() <= WORK_BYTES {
        return context
            .codec
            .literal(text, output, context.no_backslash_escapes, context.limit)
            .map_err(|error| codec_error(source, text, context.codec.name(), error));
    }
    if quoted {
        writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))?;
    }
    let mut start = 0;
    while start < text.len() {
        if text.len() > WORK_BYTES {
            checkpoint(py, budget)?;
        }
        let mut end = (start + WORK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let result = if quoted {
            context.codec.literal_fragment(
                &text[start..end],
                output,
                context.no_backslash_escapes,
                context.limit,
            )
        } else {
            context
                .codec
                .encode(&text[start..end], output, context.limit)
        };
        result.map_err(|error| {
            let error = match error {
                CodecError::Encode { offset, point } => CodecError::Encode {
                    offset: start + offset,
                    point,
                },
                error => error,
            };
            codec_error(source, text, context.codec.name(), error)
        })?;
        start = end;
    }
    if quoted {
        writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))?;
    }
    Ok(())
}
