//! Exact Python scalar readers; no Python objects cross into the native core.

use crate::errors;
use pyo3::exceptions::{PyTypeError, PyUnicodeEncodeError};
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyBytes, PyDate, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess, PyFloat, PyInt,
    PyString, PyTime, PyTimeAccess, PyType, PyTzInfoAccess,
};
use sqlaero_core::bind::scalar::{Clock, Date, ScalarError};
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;

pub(crate) mod arrow;
mod decimal;
pub(crate) mod numpy;
pub(crate) mod pandas;
pub(crate) mod polars;
pub(crate) mod sequence;
pub(crate) mod source;
pub(crate) mod standard;
pub(crate) mod temporal;

/// Allocate one boundary-owned strategy fallibly, using the same global
/// allocation/layout that Box will later release. No upstream storage is read.
pub(crate) fn boxed<T>(py: Python<'_>, value: T) -> PyResult<Box<T>> {
    let layout = std::alloc::Layout::new::<T>();
    let pointer = if layout.size() == 0 {
        std::ptr::NonNull::<T>::dangling()
    } else {
        // SAFETY: Layout::new is valid for T; alloc returns suitably aligned
        // storage or NULL. NULL is checked before any write or ownership move.
        let raw = unsafe { std::alloc::alloc(layout) }.cast::<T>();
        std::ptr::NonNull::new(raw).ok_or_else(|| errors::memory_error(py))?
    };
    // SAFETY: this is the unique suitably aligned allocation for exactly T
    // (or its valid ZST dangling pointer). Initialize before giving Box the
    // matching global-allocator ownership; no fallible work follows the write.
    unsafe {
        pointer.as_ptr().write(value);
        Ok(Box::from_raw(pointer.as_ptr()))
    }
}

/// Build a public Python index fallibly, shared by provider boundary adapters.
/// No intermediate Python value or implicit infallible tuple conversion is used.
pub(crate) fn python_index<'py>(py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: CPython returns a new integer reference or its actual allocation
    // exception. `usize` is exactly the argument domain of PyLong_FromSize_t.
    unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromSize_t(index)) }
}

/// Test the actual type hierarchy without invoking an object's __class__ hook.
pub(crate) fn actual_instance(value: &Bound<'_, PyAny>, base: &Bound<'_, PyType>) -> bool {
    // SAFETY: both references denote live actual type objects. The public
    // subtype predicate consults their MRO and does not call Python code.
    unsafe {
        pyo3::ffi::PyType_IsSubtype(value.get_type().as_ptr().cast(), base.as_ptr().cast()) != 0
    }
}

/// Module-owned immutable type references; there is no process-global Python
/// type pointer or per-value import, and a future interpreter can own its state.
#[pyclass(frozen)]
pub(crate) struct ScalarTypes {
    decimal: Py<PyType>,
    native_decimal: bool,
    utc: Py<PyAny>,
    struct_time: Py<PyType>,
    numpy: std::sync::OnceLock<numpy::NumpyTypes>,
    pandas: std::sync::OnceLock<pandas::PandasTypes>,
    polars: std::sync::OnceLock<polars::PolarsTypes>,
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
        // PyO3's exact datetime type checks expect an initialized C API. Its
        // public constructor initializes that API fallibly; do this once at
        // module preparation, rather than allowing the first value check to
        // panic on a capsule-import failure. No temporary escapes this call.
        drop(PyDate::new(py, 1, 1, 1)?);
        let decimal = errors::module(py, c"decimal")?;
        let datetime = errors::module(py, c"datetime")?;
        let timezone = errors::attribute(datetime.as_any(), c"timezone")?;
        let decimal_type =
            errors::attribute(decimal.as_any(), c"Decimal")?.cast_into::<PyType>()?;
        let native_decimal = decimal::native_type(&decimal_type)?;
        Ok(Self {
            decimal: decimal_type.unbind(),
            native_decimal,
            utc: errors::attribute(&timezone, c"utc")?.unbind(),
            struct_time: errors::attribute(errors::module(py, c"time")?.as_any(), c"struct_time")?
                .cast_into::<PyType>()?
                .unbind(),
            numpy: std::sync::OnceLock::new(),
            pandas: std::sync::OnceLock::new(),
            polars: std::sync::OnceLock::new(),
        })
    }

    /// Retain the scalar-only probe contract without treating a recognized
    /// provider failure as an unsupported-type sentinel.
    pub(crate) fn write(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        if self.try_write(value, output, context, budget)? {
            Ok(())
        } else {
            Err(unsupported(value))
        }
    }

    /// Read one supported exact scalar directly into the native request.
    /// Approved tzinfo code executes only here, while attached to Python.
    pub(crate) fn try_write(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<bool> {
        let py = value.py();
        let limit = context.limit;
        if value.is_none() {
            return writer::append(output, b"NULL", limit)
                .map_err(|e| errors::wire(py, e))
                .map(|()| true);
        }
        if value.is_exact_instance_of::<PyBool>() {
            return writer::append(
                output,
                if value.extract::<bool>()? { b"1" } else { b"0" },
                limit,
            )
            .map_err(|e| errors::wire(py, e))
            .map(|()| true);
        }
        if value.is_exact_instance_of::<PyInt>() {
            return integer(value, output, limit).map(|()| true);
        }
        if value.is_exact_instance_of::<PyFloat>() {
            return scalar::float(output, value.extract::<f64>()?, limit)
                .map_err(|e| errors::wire(py, e))
                .map(|()| true);
        }
        if value.is_exact_instance_of::<PyString>() {
            return text(value.cast::<PyString>()?, output, context, true, budget).map(|()| true);
        }
        if value.is_exact_instance_of::<PyBytes>() {
            return binary(
                py,
                value.cast::<PyBytes>()?.as_bytes(),
                output,
                limit,
                budget,
            )
            .map(|()| true);
        }
        self.write_extended(value, output, context, budget)
    }

    /// Isolate callback-rich temporal and Decimal work from primitive frames.
    /// The exact-type checks, conversion order and original errors are unchanged.
    #[inline(never)]
    fn write_extended(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<bool> {
        let py = value.py();
        let limit = context.limit;
        if value.is_exact_instance_of::<PyDateTime>() {
            return self
                .write_datetime(value.cast::<PyDateTime>()?, output, context)
                .map(|()| true);
        }
        if value.is_exact_instance_of::<PyDate>() {
            return scalar::date_time(output, date(value.cast::<PyDate>()?), None, limit)
                .map_err(|e| errors::wire(py, e))
                .map(|()| true);
        }
        if value.is_exact_instance_of::<PyTime>() {
            return scalar::time(output, clock(value.cast::<PyTime>()?), limit)
                .map_err(|e| errors::wire(py, e))
                .map(|()| true);
        }
        if value.is_exact_instance_of::<PyDelta>() {
            return scalar::duration(output, delta(value.cast::<PyDelta>()?), limit)
                .map_err(|e| errors::wire(py, e))
                .map(|()| true);
        }
        if value.get_type().is(self.decimal.bind(py)) {
            return decimal::write(value, self.native_decimal, output, context, budget)
                .map(|()| true);
        }
        if self.write_pandas(value, output, context, budget)? {
            return Ok(true);
        }
        if self.write_numpy(value, output, context, budget)? {
            return Ok(true);
        }
        standard::write(self, value, output, context, budget)
    }

    fn write_datetime(
        &self,
        datetime: &Bound<'_, PyDateTime>,
        output: &mut Vec<u8>,
        context: &Context,
    ) -> PyResult<()> {
        let py = datetime.py();
        let limit = context.limit;
        let mut date = date(datetime);
        let mut clock = clock(datetime);
        let offset = match datetime.get_tzinfo() {
            None => None,
            Some(zone) if zone.is(self.utc.bind(py)) => Some(0),
            Some(_) => {
                // The exact built-in method validates the tzinfo return
                // type and offset range, preserving its actual exception.
                let offset = datetime.call_method0(PyString::from_bytes(py, b"utcoffset")?)?;
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
            (date, clock) = scalar::utc(date, clock, offset).map_err(|e| scalar_error(py, e))?;
        }
        scalar::date_time(output, date, Some(clock), limit).map_err(|e| errors::wire(py, e))
    }

    /// Discover an already-used optional provider, outside every primitive path.
    /// Full candidates are prepared without a cache lock and losers drop after
    /// OnceLock has returned; Python callbacks never run inside a publish lock.
    pub(crate) fn numpy_types(&self, py: Python<'_>) -> PyResult<Option<&numpy::NumpyTypes>> {
        if let Some(types) = self.numpy.get() {
            return Ok(Some(types));
        }
        if !provider_loaded(py, b"numpy")? {
            return Ok(None);
        }
        let candidate = numpy::NumpyTypes::new(py)?;
        let _ = self.numpy.set(candidate);
        Ok(self.numpy.get())
    }

    /// Discover Pandas only after the caller imported it, and publish a fully
    /// prepared interpreter-owned cache without holding a lock during Python.
    pub(crate) fn pandas_types(&self, py: Python<'_>) -> PyResult<Option<&pandas::PandasTypes>> {
        if let Some(types) = self.pandas.get() {
            return Ok(Some(types));
        }
        if !provider_loaded(py, b"pandas")? {
            return Ok(None);
        }
        let candidate = pandas::PandasTypes::new(py)?;
        let _ = self.pandas.set(candidate);
        Ok(self.pandas.get())
    }

    /// Retain complete provider identities without caching absence or holding
    /// a publish lock during Python preparation and losing-candidate drops.
    pub(crate) fn polars_types(
        &self,
        py: Python<'_>,
        budget: &IoBudget,
    ) -> PyResult<Option<&polars::PolarsTypes>> {
        if let Some(types) = self.polars.get() {
            return Ok(Some(types));
        }
        check(py, budget)?;
        let Some(candidate) = polars::PolarsTypes::loaded(py)? else {
            return Ok(None);
        };
        let _ = self.polars.set(candidate);
        Ok(self.polars.get())
    }

    fn write_pandas(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<bool> {
        let py = value.py();
        let Some(types) = self.pandas_types(py)? else {
            return Ok(false);
        };
        let Some(numpy) = self.numpy_types(py)? else {
            return Ok(false);
        };
        let Some(scalar) = types.read_scalar(value, numpy)? else {
            return Ok(false);
        };
        match scalar {
            pandas::PandasScalar::Null => writer::append(output, b"NULL", context.limit)
                .map_err(|error| errors::wire(py, error))?,
            pandas::PandasScalar::Temporal {
                ticks,
                unit,
                multiplier,
                kind,
                aware,
            } => {
                if aware && !context.utc_confirmed {
                    return Err(errors::client(
                        py,
                        ClientErrorCode::SessionState,
                        "timezone-aware Pandas input requires a confirmed UTC session",
                    ));
                }
                temporal::write(py, ticks, unit, multiplier, kind, output, context, budget)?;
            }
        }
        Ok(true)
    }

    fn write_numpy(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<bool> {
        let py = value.py();
        let Some(types) = self.numpy_types(py)? else {
            return Ok(false);
        };
        use numpy::NumpyScalar;
        let result = match types.read(value)? {
            Some(NumpyScalar::Signed(value)) => scalar::integer(output, value, context.limit)
                .map_err(|error| errors::wire(py, error)),
            Some(NumpyScalar::Unsigned(value)) => scalar::integer(output, value, context.limit)
                .map_err(|error| errors::wire(py, error)),
            Some(NumpyScalar::Float(value)) => {
                scalar::float(output, value, context.limit).map_err(|error| errors::wire(py, error))
            }
            Some(NumpyScalar::Bool(value)) => {
                writer::append(output, if value { b"1" } else { b"0" }, context.limit)
                    .map_err(|error| errors::wire(py, error))
            }
            Some(NumpyScalar::Temporal {
                ticks,
                unit,
                multiplier,
                kind,
            }) => temporal::write(py, ticks, unit, multiplier, kind, output, context, budget),
            None if types.is_text(value) => text(value.cast()?, output, context, true, budget),
            None if types.is_binary(value) => binary(
                py,
                value.cast::<PyBytes>()?.as_bytes(),
                output,
                context.limit,
                budget,
            ),
            None => return Ok(false),
        };
        result.map(|()| true)
    }
}

/// Consult an existing provider import without freezing a negative result.
fn provider_loaded(py: Python<'_>, name: &[u8]) -> PyResult<bool> {
    // SAFETY: CPython owns sys.modules for this interpreter; acquire an owned
    // reference while attached and use PyO3's owned dictionary lookup API.
    let modules =
        unsafe { Bound::<PyAny>::from_borrowed_ptr(py, pyo3::ffi::PyImport_GetModuleDict()) }
            .cast_into::<pyo3::types::PyDict>()?;
    Ok(modules.get_item(PyString::from_bytes(py, name)?)?.is_some())
}

/// Keep fallible error formatting outside the common scalar visitor's frame.
#[cold]
#[inline(never)]
pub(crate) fn unsupported(value: &Bound<'_, PyAny>) -> PyErr {
    match value.get_type().name() {
        Ok(name) => errors::builtin_message::<PyTypeError>(
            value.py(),
            format!("unsupported exact escape type: {name}"),
        ),
        Err(error) => error,
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

// The sampled primitive hot path should not pay an extra reader call.
// Wide allocation/conversion remains in the separate non-inlined helper.
#[inline(always)]
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
    integer_wide(value, output, limit, overflow)
}

/// Keep owned big-integer storage and error construction off the compact
/// integer writer's frame. The public conversion and range policy are shared.
#[inline(never)]
fn integer_wide(
    value: &Bound<'_, PyAny>,
    output: &mut Vec<u8>,
    limit: usize,
    overflow: i32,
) -> PyResult<()> {
    let py = value.py();
    if overflow > 0 {
        match value.extract::<u64>() {
            Ok(unsigned) => {
                return scalar::integer(output, unsigned, limit).map_err(|e| errors::wire(py, e));
            }
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyOverflowError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    let value = match errors::integer_value_with_bits(value.cast::<PyInt>()?, 255) {
        // A fixed 32-byte transfer avoids Python bit_length and the generic
        // extractor's separate size query. The core still checks the actual
        // 216-bit and 65-decimal-digit policy before formatting or appending.
        Ok(value) => value,
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyOverflowError>(py) => {
            return Err(errors::client(
                py,
                ClientErrorCode::NumericRange,
                "integer exceeds the 65-digit exact SQL range",
            ));
        }
        Err(error) => return Err(error),
    };
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

#[inline(never)]
pub(crate) fn binary(
    py: Python<'_>,
    source: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    if source.len() <= WORK_BYTES {
        return writer::binary(output, source, limit).map_err(|error| errors::wire(py, error));
    }
    binary_long(py, source, output, limit, budget)
}

#[inline(never)]
fn binary_long(
    py: Python<'_>,
    source: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
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

/// Encode native UTF-8 content through the same codec and checked work windows.
/// A Python string is constructed only if the codec needs its error payload.
pub(crate) fn text_utf8(
    py: Python<'_>,
    source: &str,
    output: &mut Vec<u8>,
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let convert_error = |error: CodecError, start: usize| -> PyErr {
        match error {
            CodecError::Encode { offset, point } => {
                // SAFETY: source is valid UTF-8 and remains borrowed only for
                // this synchronous call. Construction is checked and attached.
                let original = unsafe {
                    Bound::<PyAny>::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyUnicode_FromStringAndSize(
                            source.as_ptr().cast(),
                            source.len() as pyo3::ffi::Py_ssize_t,
                        ),
                    )
                }
                .and_then(|value| value.cast_into::<PyString>().map_err(Into::into));
                match original {
                    Ok(original) => codec_error(
                        &original,
                        source,
                        context.codec.name(),
                        CodecError::Encode {
                            offset: start + offset,
                            point,
                        },
                    ),
                    Err(error) => error,
                }
            }
            CodecError::Wire(error) => errors::wire(py, error),
            error => errors::client(py, ClientErrorCode::UnsupportedCharset, error.to_string()),
        }
    };
    if source.len() <= WORK_BYTES {
        return context
            .codec
            .literal(source, output, context.no_backslash_escapes, context.limit)
            .map_err(|error| convert_error(error, 0));
    }
    checkpoint(py, budget)?;
    writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))?;
    let mut start = 0;
    while start < source.len() {
        checkpoint(py, budget)?;
        let mut end = (start + WORK_BYTES).min(source.len());
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        context
            .codec
            .literal_fragment(
                &source[start..end],
                output,
                context.no_backslash_escapes,
                context.limit,
            )
            .map_err(|error| convert_error(error, start))?;
        start = end;
    }
    writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))
}

/// Write a bounded Arrow/Polars two's-complement coefficient without Python.
pub(crate) fn decimal_bytes(
    py: Python<'_>,
    signed_le: &[u8],
    scale: i8,
    output: &mut Vec<u8>,
    context: &Context,
) -> PyResult<()> {
    if signed_le.is_empty() || signed_le.len() > 32 {
        return Err(errors::client(
            py,
            ClientErrorCode::NumericRange,
            "Decimal coefficient must contain between 1 and 32 bytes",
        ));
    }
    // Wider physical storage may contain only redundant i128 sign extension.
    // The retained low half's sign, not merely uniform high bytes, proves it.
    let signed_le = if signed_le.len() > 16 {
        let fill = if signed_le[15] & 0x80 == 0 { 0 } else { 0xff };
        if signed_le[16..].iter().all(|byte| *byte == fill) {
            &signed_le[..16]
        } else {
            signed_le
        }
    } else {
        signed_le
    };
    if signed_le.len() <= 16 {
        // The source is signed little-endian two's complement. Extending its
        // actual sign preserves every width, including the i128 minimum.
        let fill = if signed_le[signed_le.len() - 1] & 0x80 == 0 {
            0
        } else {
            0xff
        };
        let mut bytes = [fill; 16];
        bytes[..signed_le.len()].copy_from_slice(signed_le);
        let coefficient = i128::from_le_bytes(bytes);
        let mut buffer = itoa::Buffer::new();
        let text = match i64::try_from(coefficient) {
            Ok(value) => buffer.format(value),
            Err(_) => buffer.format(coefficient),
        };
        let negative = coefficient < 0;
        let ascii = if negative {
            &text.as_bytes()[1..]
        } else {
            text.as_bytes()
        };
        // i128 has at most 39 magnitude digits; format the signed value
        // directly so i128::MIN never needs an overflowing abs conversion.
        let mut digits = [0u8; 39];
        for (digit, ascii) in digits.iter_mut().zip(ascii) {
            *digit = *ascii - b'0';
        }
        return scalar::decimal(
            output,
            negative,
            &digits[..ascii.len()],
            -i64::from(scale),
            context.limit,
        )
        .map_err(|error| scalar_error(py, error));
    }
    let coefficient = num_bigint::BigInt::from_signed_bytes_le(signed_le);
    let (sign, digits) = coefficient.to_radix_be(10);
    scalar::decimal(
        output,
        sign == num_bigint::Sign::Minus,
        &digits,
        -i64::from(scale),
        context.limit,
    )
    .map_err(|error| scalar_error(py, error))
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
