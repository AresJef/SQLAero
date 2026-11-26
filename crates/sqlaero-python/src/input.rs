//! Exact Python scalar readers; no Python objects cross into the native core.

use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::{PyTypeError, PyUnicodeEncodeError};
use pyo3::prelude::*;
use pyo3::types::{
    PyDate, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess, PyInt, PyString, PyTime,
    PyTimeAccess, PyType, PyTzInfoAccess,
};
use sink::{Destination, emit};
use sqlaero_core::bind::scalar::{Clock, Date, ScalarError};
use sqlaero_core::bind::value::{ValueError, ValueRef};
use sqlaero_core::bind::{Context, writer};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_transcode::temporal;

pub(crate) mod arrow;
mod attribute;
mod buffer;
pub(crate) mod cell;
pub(crate) mod control;
mod decimal;
mod diagnostic;
mod identifier;
pub(crate) mod integer;
pub(crate) mod json;
pub(crate) mod sink;
pub(crate) use diagnostic::binary_diagnostic;
pub(crate) mod numpy;
pub(crate) mod pandas;
pub(crate) mod polars;
pub(crate) mod primitive;
pub(crate) mod probe;
mod record;
mod scalar_source;
pub(crate) mod sequence;
pub(crate) mod source;
pub(crate) mod sql_func;
pub(crate) mod sql_intv;
pub(crate) mod standard;
pub(crate) mod transparent;
pub(crate) mod unicode;

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
    actual_type_is_subtype(&value.get_type(), base)
}

/// Compare an already retained actual type against a canonical base.
pub(crate) fn actual_type_is_subtype(
    candidate: &Bound<'_, PyType>,
    base: &Bound<'_, PyType>,
) -> bool {
    // SAFETY: both references denote live actual type objects. The public
    // subtype predicate consults their MRO and does not call Python code.
    unsafe { pyo3::ffi::PyType_IsSubtype(candidate.as_ptr().cast(), base.as_ptr().cast()) != 0 }
}

/// Module-owned immutable type references; there is no process-global Python
/// type pointer or per-value import, and a future interpreter can own its state.
#[pyclass(frozen)]
pub(crate) struct ScalarTypes {
    decimal: decimal::Api,
    utc: Py<PyAny>,
    datetime_offset: Py<PyAny>,
    time_offset: Py<PyAny>,
    struct_time: Py<PyType>,
    sequences: sequence::Types,
    uuid: std::sync::OnceLock<identifier::UuidType>,
    enum_type: std::sync::OnceLock<Py<PyType>>,
    numpy: std::sync::OnceLock<numpy::NumpyTypes>,
    pandas: std::sync::OnceLock<pandas::PandasTypes>,
    polars: std::sync::OnceLock<polars::PolarsTypes>,
}

impl ScalarTypes {
    fn enum_type<'py>(
        &self,
        py: Python<'py>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<&Bound<'py, PyType>>> {
        if self.enum_type.get().is_none() {
            if !provider_loaded(py, b"enum")? {
                return Ok(None);
            }
            let candidate = control::callback(py, budget, || transparent::prepare_type(py))?;
            let _ = self.enum_type.set(candidate);
            checkpoint(py, budget)?;
        }
        Ok(self.enum_type.get().map(|typ| typ.bind(py)))
    }

    pub(crate) fn transparent<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<transparent::Value<'py>>> {
        transparent::read(value, self, budget)
    }

    /// Describe only the failed input, with bounded text and no arbitrary repr
    /// or tzinfo callback. Failure to obtain this optional detail is ignored by
    /// callers so it cannot replace the real conversion exception.
    pub(crate) fn diagnostic(&self, value: &Bound<'_, PyAny>) -> PyResult<String> {
        diagnostic::value(value)
    }
    pub(crate) fn decimal_type<'py>(&self, py: Python<'py>) -> &Bound<'py, PyType> {
        self.decimal.typ(py)
    }

    pub(super) fn uuid_value(
        &self,
        value: &Bound<'_, PyAny>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<u128>> {
        let py = value.py();
        if self.uuid.get().is_none() {
            if !provider_loaded(py, b"uuid")? {
                return Ok(None);
            }
            let candidate = control::callback(py, budget, || identifier::UuidType::new(py))?;
            let _ = self.uuid.set(candidate);
            // A losing candidate is dropped after OnceLock has released its
            // publish lock, before actual source facts are consumed.
            checkpoint(py, budget)?;
        }
        match self.uuid.get() {
            Some(types) => types.read(value, budget),
            None => Ok(None),
        }
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
        let decimal_api = decimal::Api::new(&decimal_type)?;
        Ok(Self {
            decimal: decimal_api,
            sequences: sequence::Types::new(py)?,
            uuid: std::sync::OnceLock::new(),
            enum_type: std::sync::OnceLock::new(),
            utc: errors::attribute(&timezone, c"utc")?.unbind(),
            datetime_offset: errors::attribute(py.get_type::<PyDateTime>().as_any(), c"utcoffset")?
                .unbind(),
            time_offset: errors::attribute(py.get_type::<PyTime>().as_any(), c"utcoffset")?
                .unbind(),
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
        budget: Option<&dyn Checkpoint>,
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
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<bool> {
        self.try_write_to(value, output, context, budget)
    }

    /// Consume one scalar through the common recognition and protocol boundary.
    pub(crate) fn write_native<S: sink::ParameterSink>(
        &self,
        value: &Bound<'_, PyAny>,
        sink: &mut S,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        if self.try_write_to(value, &mut sink::Native(sink), context, budget)? {
            Ok(())
        } else {
            Err(unsupported(value))
        }
    }

    #[inline(always)]
    pub(crate) fn try_write_to<D: Destination + ?Sized>(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut D,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<bool> {
        let py = value.py();
        if let Some(source) = primitive::read(value)? {
            match source {
                primitive::Source::Native(ValueRef::Bytes(bytes)) => {
                    output.binary(py, bytes, context, budget)?;
                }
                primitive::Source::Native(value) => emit(output, py, value, context)?,
                primitive::Source::Text(value) => output.text(value, context, budget)?,
                primitive::Source::WideInteger { value, overflow } => {
                    integer_wide(value.as_any(), output, context, overflow)?;
                }
            }
            return Ok(true);
        }
        self.write_extended(value, output, context, budget)
    }

    /// Read through the single extended registry, then consume source facts.
    #[inline(never)]
    fn write_extended<D: Destination + ?Sized>(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut D,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<bool> {
        let py = value.py();
        let Some(source) = scalar_source::read(self, value, budget)? else {
            return Ok(false);
        };
        match source {
            scalar_source::Source::Native(value) => emit(output, py, value, context)?,
            scalar_source::Source::Datetime(value) => {
                emit(output, py, self.read_datetime(value, budget)?, context)?;
            }
            scalar_source::Source::Clock(value) => {
                emit(output, py, scalar_source::clock_fact(value), context)?
            }
            scalar_source::Source::Decimal(value) => {
                self.decimal.read(value, budget, |source| {
                    let (header, digits) = (source.header, source.digits);
                    decimal::consume(
                        py,
                        source,
                        &mut sink::Adapter(output),
                        context,
                        budget,
                        |error| {
                            let error = sink::failure(py, error);
                            decimal::note(py, &error, header, digits);
                            error
                        },
                    )
                })?;
            }
            scalar_source::Source::StructTime(value) => {
                standard::struct_time(value, output, context, budget)?
            }
            scalar_source::Source::Json(value) => {
                json::mysql(value, self, output, context, budget)?
            }
            scalar_source::Source::Pandas { facts, .. } => {
                self.write_pandas(py, facts, output, context)?
            }
            scalar_source::Source::Numpy(facts) => self.write_numpy(py, facts, output, context)?,
        }
        Ok(true)
    }

    /// Acquire original local fields and the actual base datetime offset.
    /// UTC admission and normalization are policies of the MySQL target.
    pub(super) fn read_datetime(
        &self,
        datetime: &Bound<'_, PyDateTime>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<ValueRef<'static>> {
        let offset_micros = self.read_datetime_offset(datetime, budget)?;
        Ok(ValueRef::Datetime {
            date: date(datetime),
            clock: clock(datetime),
            offset_micros,
        })
    }

    /// Acquire only the required original offset, without narrowing provider
    /// calendar fields or discarding its higher-resolution UTC epoch ticks.
    pub(super) fn read_datetime_offset(
        &self,
        datetime: &Bound<'_, PyDateTime>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<i128>> {
        let py = datetime.py();
        Ok(match datetime.get_tzinfo() {
            None => None,
            Some(zone) if zone.is(self.utc.bind(py)) => Some(0),
            Some(_) => {
                let offset = control::callback(py, budget, || {
                    self.datetime_offset.bind(py).call1((datetime,))
                })?;
                if offset.is_none() {
                    None
                } else {
                    Some(delta(offset.cast::<PyDelta>()?))
                }
            }
        })
    }

    /// Only a target requiring a clock offset calls this source protocol.
    pub(super) fn read_time(
        &self,
        time: &Bound<'_, PyTime>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<ValueRef<'static>> {
        let py = time.py();
        let offset_micros = match time.get_tzinfo() {
            None => None,
            Some(zone) if zone.is(self.utc.bind(py)) => Some(0),
            Some(_) => {
                let offset =
                    control::callback(py, budget, || self.time_offset.bind(py).call1((time,)))?;
                if offset.is_none() {
                    None
                } else {
                    Some(delta(offset.cast::<PyDelta>()?))
                }
            }
        };
        Ok(ValueRef::Time {
            clock: clock(time),
            offset_micros,
        })
    }

    /// Discover an already-used optional provider, outside every primitive path.
    /// Full candidates are prepared without a cache lock and losers drop after
    /// OnceLock has returned; Python callbacks never run inside a publish lock.
    pub(crate) fn numpy_types(
        &self,
        py: Python<'_>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<&numpy::NumpyTypes>> {
        if let Some(types) = self.numpy.get() {
            return Ok(Some(types));
        }
        if !provider_loaded(py, b"numpy")? {
            return Ok(None);
        }
        let candidate = control::callback(py, budget, || numpy::NumpyTypes::new(py, budget))?;
        let _ = self.numpy.set(candidate);
        checkpoint(py, budget)?;
        Ok(self.numpy.get())
    }

    /// Discover Pandas only after the caller imported it, and publish a fully
    /// prepared interpreter-owned cache without holding a lock during Python.
    pub(crate) fn pandas_types(
        &self,
        py: Python<'_>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<&pandas::PandasTypes>> {
        if let Some(types) = self.pandas.get() {
            return Ok(Some(types));
        }
        if !provider_loaded(py, b"pandas")? {
            return Ok(None);
        }
        let candidate = control::callback(py, budget, || pandas::PandasTypes::new(py))?;
        let _ = self.pandas.set(candidate);
        checkpoint(py, budget)?;
        Ok(self.pandas.get())
    }

    /// Retain complete provider identities without caching absence or holding
    /// a publish lock during Python preparation and losing-candidate drops.
    pub(crate) fn polars_types(
        &self,
        py: Python<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<&polars::PolarsTypes>> {
        if let Some(types) = self.polars.get() {
            return Ok(Some(types));
        }
        check(py, budget)?;
        let Some(candidate) =
            control::callback(py, Some(budget), || polars::PolarsTypes::loaded(py))?
        else {
            return Ok(None);
        };
        let _ = self.polars.set(candidate);
        checkpoint(py, Some(budget))?;
        Ok(self.polars.get())
    }

    fn write_pandas<D: Destination + ?Sized>(
        &self,
        py: Python<'_>,
        scalar: pandas::PandasTemporal,
        output: &mut D,
        context: &Context,
    ) -> PyResult<()> {
        let pandas::PandasTemporal {
            ticks,
            unit,
            multiplier,
            kind,
            aware,
        } = scalar;
        let value = if ticks == i64::MIN {
            ValueRef::Null
        } else if aware {
            ValueRef::UtcTicks {
                value: ticks,
                unit,
                multiplier,
                offset_micros: None,
            }
        } else {
            ValueRef::Ticks {
                value: ticks,
                unit,
                multiplier,
                kind,
            }
        };
        emit(output, py, value, context)
    }

    fn write_numpy<D: Destination + ?Sized>(
        &self,
        py: Python<'_>,
        scalar: numpy::NumpyScalar,
        output: &mut D,
        context: &Context,
    ) -> PyResult<()> {
        use numpy::NumpyScalar;
        match scalar {
            NumpyScalar::Signed(value) => emit(output, py, ValueRef::Signed(value), context),
            NumpyScalar::Unsigned(value) => emit(output, py, ValueRef::Unsigned(value), context),
            NumpyScalar::Float(value) => emit(output, py, ValueRef::Float(value), context),
            NumpyScalar::Bool(value) => emit(output, py, ValueRef::Boolean(value), context),
            NumpyScalar::Temporal {
                ticks,
                unit,
                multiplier,
                kind,
            } => temporal_to(output, py, ticks, unit, multiplier, kind, context),
        }
    }
}

fn temporal_to(
    output: &mut (impl Destination + ?Sized),
    py: Python<'_>,
    ticks: i64,
    unit: temporal::TimeUnit,
    multiplier: u64,
    kind: temporal::TimeKind,
    context: &Context,
) -> PyResult<()> {
    if ticks == i64::MIN {
        return emit(output, py, ValueRef::Null, context);
    }
    if multiplier == 0 {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "NumPy datetime unit multiplier must be positive",
        ));
    }
    emit(
        output,
        py,
        ValueRef::Ticks {
            value: ticks,
            unit,
            multiplier,
            kind,
        },
        context,
    )
}

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
        Ok(name) => errors::builtin_format::<PyTypeError>(
            value.py(),
            format_args!("unsupported exact escape type: {name}"),
        ),
        Err(error) => error,
    }
}

/// Keep owned big-integer storage and error construction off the compact
/// integer writer's frame. The public conversion and range policy are shared.
#[inline(never)]
fn integer_wide(
    value: &Bound<'_, PyAny>,
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    overflow: i32,
) -> PyResult<()> {
    let py = value.py();
    if overflow > 0 {
        match value.extract::<u64>() {
            Ok(unsigned) => {
                return emit(output, py, ValueRef::Unsigned(unsigned), context);
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
    emit(output, py, ValueRef::BigInteger(&value), context)
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
    let py = source.py();
    let make = || -> PyResult<Bound<'_, PyAny>> {
        let arguments = crate::output::tuple_from_ready(
            py,
            [
                PyString::from_bytes(py, encoding.as_bytes())?.into_any(),
                source.as_any().clone(),
                python_index(py, start)?,
                python_index(py, end)?,
                PyString::from_bytes(py, reason.as_bytes())?.into_any(),
            ],
        )?;
        py.get_type::<PyUnicodeEncodeError>().call1(arguments)
    };
    match make() {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}
fn codec_error(
    source: &Bound<'_, PyString>,
    text: &str,
    encoding: &str,
    error: CodecError,
    budget: Option<&dyn Checkpoint>,
) -> PyErr {
    let py = source.py();
    match error {
        CodecError::Encode { offset, point } => {
            let Some(prefix) = text.get(..offset) else {
                return errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "text codec returned an invalid source byte position",
                );
            };
            let mut start = 0usize;
            if let Err(error) = unicode::native_fragments(py, prefix, budget, |_, fragment| {
                start += fragment.chars().count();
                Ok(())
            }) {
                return error;
            }
            match errors::message(
                py,
                format_args!("character U+{point:04X} is not representable in MySQL {encoding}"),
            ) {
                Ok(reason) => encode_error(source, encoding, start, start + 1, &reason),
                Err(error) => error,
            }
        }
        CodecError::Wire(error) => errors::wire(py, error),
        error => errors::client_format(
            py,
            ClientErrorCode::UnsupportedCharset,
            format_args!("{error}"),
        ),
    }
}

const WORK_BYTES: usize = 256 * 1024;

#[inline(never)]
pub(crate) fn binary(
    py: Python<'_>,
    source: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    if source.len() <= WORK_BYTES {
        control::bytes(py, budget, source.len().saturating_mul(2))?;
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
    budget: Option<&dyn Checkpoint>,
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
pub(crate) fn check(py: Python<'_>, budget: &dyn Checkpoint) -> PyResult<()> {
    budget.checkpoint(py)
}
pub(crate) fn checkpoint(py: Python<'_>, budget: Option<&dyn Checkpoint>) -> PyResult<()> {
    match budget {
        Some(budget) => check(py, budget),
        None => py.check_signals(),
    }
}

/// Encode unquoted native SQL source into the existing request, without a
/// command byte or a Python string on the successful path. The caller owns
/// execution and supplies one unchanged deadline for all initialization units.
pub(crate) fn sql_utf8(
    py: Python<'_>,
    source: &str,
    request: &mut Vec<u8>,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    check(py, budget)?;
    let mut start = 0;
    while start < source.len() {
        let mut end = start.saturating_add(WORK_BYTES).min(source.len());
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        context
            .codec
            .encode(&source[start..end], request, context.limit)
            .map_err(|error| native_codec_error(py, source, context, error, start, Some(budget)))?;
        check(py, budget)?;
        start = end;
    }
    Ok(())
}

/// Retain full original character coordinates for native UTF-8 sources. The
/// exception's source object is allocated only when encoding actually fails.
fn native_codec_error(
    py: Python<'_>,
    source: &str,
    context: &Context,
    error: CodecError,
    base: usize,
    budget: Option<&dyn Checkpoint>,
) -> PyErr {
    let unlimited = control::Control::unlimited();
    let source_control = budget.unwrap_or(&unlimited);
    match error {
        CodecError::Encode { offset, point } => {
            match unicode::from_utf8(py, source, source_control) {
                Ok(original) => codec_error(
                    &original,
                    source,
                    context.codec.name(),
                    CodecError::Encode {
                        offset: base + offset,
                        point,
                    },
                    Some(source_control),
                ),
                Err(error) => error,
            }
        }
        CodecError::Wire(error) => errors::wire(py, error),
        error => errors::client_format(
            py,
            ClientErrorCode::UnsupportedCharset,
            format_args!("{error}"),
        ),
    }
}

/// Encode native UTF-8 content through the same codec and checked work windows.
/// A Python string is constructed only if the codec needs its error payload.
pub(crate) fn text_utf8(
    py: Python<'_>,
    source: &str,
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let convert_error = |error: ValueError, start: usize| -> PyErr {
        match error {
            ValueError::Encoding(error) => {
                native_codec_error(py, source, context, error, start, budget)
            }
            error => sink::failure(py, error),
        }
    };
    if source.len() <= WORK_BYTES {
        control::bytes(py, budget, source.len())?;
        return output
            .write(ValueRef::Text(source), context)
            .map_err(|error| convert_error(error, 0));
    }
    checkpoint(py, budget)?;
    output
        .text_start(Some(source.len()), context)
        .map_err(|error| convert_error(error, 0))?;
    unicode::native_fragments(py, source, budget, |start, text| {
        output
            .text_fragment(text, context)
            .map_err(|error| convert_error(error, start))
    })?;
    output
        .text_end(context)
        .map_err(|error| convert_error(error, 0))
}

/// The same strict text conversion serves SQL source and quoted value content.
/// Only the boundary performs signal checks; the codec never calls Python.
pub(crate) fn text(
    source: &Bound<'_, PyString>,
    output: &mut Vec<u8>,
    context: &Context,
    quoted: bool,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let py = source.py();
    let text = unicode(source, context)?;
    if quoted && text.len() <= WORK_BYTES {
        return context
            .codec
            .literal(text, output, context.no_backslash_escapes, context.limit)
            .map_err(|error| codec_error(source, text, context.codec.name(), error, budget));
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
            codec_error(source, text, context.codec.name(), error, budget)
        })?;
        start = end;
    }
    if quoted {
        writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))?;
    }
    Ok(())
}

/// Borrow immutable UTF-8 directly; translate exceptional input off this path.
#[inline]
fn unicode<'a>(source: &'a Bound<'_, PyString>, context: &Context) -> PyResult<&'a str> {
    source
        .to_str()
        .map_err(|error| unicode_error(source, context, error))
}

#[cold]
fn unicode_error(source: &Bound<'_, PyString>, context: &Context, error: PyErr) -> PyErr {
    let py = source.py();
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
}
