//! Exact NumPy scalar acquisition through the public, fallible buffer API.
//!
//! These readers only consume immutable canonical scalars. Array buffers have
//! different ownership and concurrent-mutation requirements and are deliberately
//! not accepted by the fixed-width reader.

use crate::errors;
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyString, PyTuple, PyType};
use sqlaero_core::transport::deadline::IoBudget;
use std::ffi::CStr;
use std::mem::MaybeUninit;

use super::temporal::{TimeKind, TimeUnit};

mod allocation;
mod array;
pub(crate) use array::NumpySource;
mod record;
pub(crate) use record::RecordSource;

/// A native value obtained without an intermediate Python numeric object.
pub(crate) enum NumpyScalar {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    Temporal {
        ticks: i64,
        unit: TimeUnit,
        multiplier: u64,
        kind: TimeKind,
    },
}

#[derive(Clone, Copy)]
enum Kind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F16,
    F32,
    F64,
    Bool,
    DateTime,
    Duration,
}

/// Canonical type and function references owned by one Python module state.
pub(crate) struct NumpyTypes {
    numeric: Vec<(Py<PyType>, Kind)>,
    text: Py<PyType>,
    binary: Py<PyType>,
    datetime_data: Py<PyAny>,
    array: Py<PyType>,
    record: Py<PyType>,
}

/// The two owned public API references needed by bounded column re-preparation.
/// Scalar caches are not borrowed across the lifetime of a prepared source.
pub(crate) struct NumpyArrayApi {
    array: Py<PyType>,
    datetime_data: Py<PyAny>,
}

impl NumpyArrayApi {
    /// Recognize an exact ndarray without preparing or reading it twice.
    pub(crate) fn accepts(&self, value: &Bound<'_, PyAny>) -> bool {
        value.get_type().is(self.array.bind(value.py()))
    }

    /// Prepare an exact ndarray through the same checked primitive reader.
    pub(crate) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &IoBudget,
    ) -> PyResult<Option<NumpySource<'py>>> {
        prepare_array(value, &self.array, &self.datetime_data, budget)
    }
}

impl NumpyTypes {
    /// Prepare NumPy only on the actual optional-input path.
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        let module = errors::module(py, c"numpy")?;
        let definitions: &[(&CStr, Kind)] = &[
            (c"int8", Kind::I8),
            (c"int16", Kind::I16),
            (c"int32", Kind::I32),
            (c"int64", Kind::I64),
            (c"uint8", Kind::U8),
            (c"uint16", Kind::U16),
            (c"uint32", Kind::U32),
            (c"uint64", Kind::U64),
            // LongLong can be a distinct scalar class even when its logical
            // width equals Long. Never use C long for this conversion.
            (c"longlong", Kind::I64),
            (c"ulonglong", Kind::U64),
            (c"float16", Kind::F16),
            (c"float32", Kind::F32),
            (c"float64", Kind::F64),
            (c"bool_", Kind::Bool),
            (c"datetime64", Kind::DateTime),
            (c"timedelta64", Kind::Duration),
        ];
        let mut numeric = Vec::new();
        numeric
            .try_reserve_exact(definitions.len())
            .map_err(|_| errors::memory_error(py))?;
        for &(name, kind) in definitions {
            numeric.push((exact_type(&module, name)?, kind));
        }
        let datetime_data = errors::attribute(module.as_any(), c"datetime_data")?;
        if !datetime_data.is_callable() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NumPy datetime_data API must be callable before preparing input",
            ));
        }
        Ok(Self {
            numeric,
            text: exact_type(&module, c"str_")?,
            binary: exact_type(&module, c"bytes_")?,
            datetime_data: datetime_data.unbind(),
            array: exact_type(&module, c"ndarray")?,
            record: exact_type(&module, c"record")?,
        })
    }

    /// Identify canonical NumPy Unicode without invoking its string protocol.
    pub(crate) fn is_text(&self, value: &Bound<'_, PyAny>) -> bool {
        value.get_type().is(self.text.bind(value.py()))
    }

    /// Identify canonical NumPy bytes without a temporary Python bytes object.
    pub(crate) fn is_binary(&self, value: &Bound<'_, PyAny>) -> bool {
        value.get_type().is(self.binary.bind(value.py()))
    }

    /// Prepare only an exact ndarray; logical shape is independent of SQL.
    pub(crate) fn prepare_array<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &IoBudget,
    ) -> PyResult<Option<NumpySource<'py>>> {
        prepare_array(value, &self.array, &self.datetime_data, budget)
    }

    /// Own only the array preparation API used by a provider's bounded blocks.
    pub(crate) fn array_api(&self, py: Python<'_>) -> NumpyArrayApi {
        NumpyArrayApi {
            array: self.array.clone_ref(py),
            datetime_data: self.datetime_data.clone_ref(py),
        }
    }

    /// A canonical record is one logical row, not a general structured array.
    pub(crate) fn prepare_record<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &IoBudget,
    ) -> PyResult<Option<RecordSource<'py>>> {
        if !value.get_type().is(self.record.bind(value.py())) {
            return Ok(None);
        }
        super::check(value.py(), budget)?;
        let source = RecordSource::new(value, &self.array, budget)?;
        super::check(value.py(), budget)?;
        Ok(Some(source))
    }

    /// Read a recognized immutable numeric scalar into a native value.
    pub(crate) fn read(&self, value: &Bound<'_, PyAny>) -> PyResult<Option<NumpyScalar>> {
        let py = value.py();
        let typ = value.get_type();
        let Some(kind) = self
            .numeric
            .iter()
            .find_map(|(registered, kind)| typ.is(registered.bind(py)).then_some(*kind))
        else {
            return Ok(None);
        };
        let scalar = match kind {
            Kind::I8 => {
                NumpyScalar::Signed(i64::from(i8::from_ne_bytes([fixed_bytes(value, 1)?[0]])))
            }
            Kind::I16 => {
                let b = fixed_bytes(value, 2)?;
                NumpyScalar::Signed(i64::from(i16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::I32 => {
                let b = fixed_bytes(value, 4)?;
                NumpyScalar::Signed(i64::from(i32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::I64 => NumpyScalar::Signed(i64::from_ne_bytes(fixed_bytes(value, 8)?)),
            Kind::U8 => NumpyScalar::Unsigned(u64::from(fixed_bytes(value, 1)?[0])),
            Kind::U16 => {
                let b = fixed_bytes(value, 2)?;
                NumpyScalar::Unsigned(u64::from(u16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::U32 => {
                let b = fixed_bytes(value, 4)?;
                NumpyScalar::Unsigned(u64::from(u32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::U64 => NumpyScalar::Unsigned(u64::from_ne_bytes(fixed_bytes(value, 8)?)),
            Kind::F16 => {
                let b = fixed_bytes(value, 2)?;
                NumpyScalar::Float(half_to_f64(u16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::F32 => {
                let b = fixed_bytes(value, 4)?;
                NumpyScalar::Float(f64::from(f32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::F64 => NumpyScalar::Float(f64::from_ne_bytes(fixed_bytes(value, 8)?)),
            Kind::Bool => NumpyScalar::Bool(fixed_bytes(value, 1)?[0] != 0),
            Kind::DateTime | Kind::Duration => {
                let ticks = i64::from_ne_bytes(fixed_bytes(value, 8)?);
                let kind = match kind {
                    Kind::DateTime => TimeKind::DateTime,
                    _ => TimeKind::Duration,
                };
                // The source sentinel is sufficient for NaT. Do not ask a
                // generic or unsupported unit to interpret a missing value.
                let (unit, multiplier) = if ticks == i64::MIN {
                    (TimeUnit::Generic, 1)
                } else {
                    self.temporal_metadata(value)?
                };
                NumpyScalar::Temporal {
                    ticks,
                    unit,
                    multiplier,
                    kind,
                }
            }
        };
        Ok(Some(scalar))
    }

    fn temporal_metadata(&self, value: &Bound<'_, PyAny>) -> PyResult<(TimeUnit, u64)> {
        temporal_metadata(value, &self.datetime_data)
    }
}

fn prepare_array<'py>(
    value: &Bound<'py, PyAny>,
    array: &Py<PyType>,
    datetime_data: &Py<PyAny>,
    budget: &IoBudget,
) -> PyResult<Option<NumpySource<'py>>> {
    if !value.get_type().is(array.bind(value.py())) {
        return Ok(None);
    }
    super::check(value.py(), budget)?;
    let source = NumpySource::new(value, array, datetime_data, budget)?;
    super::check(value.py(), budget)?;
    Ok(Some(source))
}

fn temporal_metadata(
    value: &Bound<'_, PyAny>,
    datetime_data: &Py<PyAny>,
) -> PyResult<(TimeUnit, u64)> {
    let py = value.py();
    let dtype = errors::attribute(value, c"dtype")?;
    let args = crate::output::tuple_from_ready(py, [dtype])?;
    let metadata = datetime_data.bind(py).call1(args)?;
    let metadata = metadata.cast::<PyTuple>()?;
    if metadata.len() != 2 {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "NumPy datetime_data must return exactly (unit, multiplier)",
        ));
    }
    let name = metadata.get_item(0)?;
    let name = name.cast::<PyString>()?.to_str()?;
    let unit = match name {
        "Y" => TimeUnit::Year,
        "M" => TimeUnit::Month,
        "W" => TimeUnit::Week,
        "D" => TimeUnit::Day,
        "h" => TimeUnit::Hour,
        "m" => TimeUnit::Minute,
        "s" => TimeUnit::Second,
        "ms" => TimeUnit::Millisecond,
        "us" => TimeUnit::Microsecond,
        "ns" => TimeUnit::Nanosecond,
        "ps" => TimeUnit::Picosecond,
        "fs" => TimeUnit::Femtosecond,
        "as" => TimeUnit::Attosecond,
        "generic" => TimeUnit::Generic,
        _ => {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NumPy returned an unrecognized datetime unit",
            ));
        }
    };
    let multiplier = metadata.get_item(1)?.extract::<u64>()?;
    // A dtype can carry zero units for a source NaT. Retain the metadata until
    // the common boundary has checked its ticks; finite zero multipliers are
    // rejected there, after the source-specific NaT -> NULL rule.
    Ok((unit, multiplier))
}

fn exact_type(module: &Bound<'_, PyModule>, name: &CStr) -> PyResult<Py<PyType>> {
    Ok(errors::attribute(module.as_any(), name)?
        .cast_into::<PyType>()?
        .unbind())
}

/// Copy at most eight bytes from an immutable canonical scalar export.
fn fixed_bytes(value: &Bound<'_, PyAny>, width: usize) -> PyResult<[u8; 8]> {
    let py = value.py();
    let mut raw = MaybeUninit::<pyo3::ffi::Py_buffer>::uninit();
    // SAFETY: Python is attached and the owned input remains live. The public
    // constructor either initializes the stack descriptor or reports PyErr.
    let result = unsafe {
        pyo3::ffi::PyObject_GetBuffer(value.as_ptr(), raw.as_mut_ptr(), pyo3::ffi::PyBUF_SIMPLE)
    };
    if result < 0 {
        return Err(PyErr::fetch(py));
    }
    // SAFETY: PyObject_GetBuffer succeeded, so every descriptor field is live.
    let mut raw = unsafe { raw.assume_init() };
    let valid = raw.len == width as isize && raw.readonly != 0 && !raw.buf.is_null();
    let mut bytes = [0; 8];
    if valid {
        // SAFETY: only registered immutable canonical scalar classes reach
        // this function. Their native contiguous export has exactly `width`
        // bytes, is owned until release, and width is fixed to 1, 2, 4 or 8.
        // No Python call, allocation or safepoint occurs during the copy.
        unsafe {
            std::ptr::copy_nonoverlapping(raw.buf.cast::<u8>(), bytes.as_mut_ptr(), width);
        }
    }
    // SAFETY: release the one successful export exactly once while attached;
    // only copied native bytes survive this potentially callback-capable step.
    unsafe { pyo3::ffi::PyBuffer_Release(&mut raw) };
    if valid {
        Ok(bytes)
    } else {
        Err(errors::builtin_message::<PyBufferError>(
            py,
            "canonical NumPy scalar must export its exact immutable native width",
        ))
    }
}

/// Promote every IEEE binary16 bit pattern exactly into binary64.
fn half_to_f64(bits: u16) -> f64 {
    let sign = u64::from(bits & 0x8000) << 48;
    let exponent = u64::from((bits >> 10) & 0x1f);
    let fraction = u64::from(bits & 0x03ff);
    let native = match (exponent, fraction) {
        (0, 0) => sign,
        (0, fraction) => {
            let top = 63 - fraction.leading_zeros();
            let native_exponent = u64::from(top) + 999;
            sign | (native_exponent << 52) | ((fraction - (1 << top)) << (52 - top))
        }
        (31, fraction) => sign | (0x7ff << 52) | (fraction << 42),
        (exponent, fraction) => sign | ((exponent + 1008) << 52) | (fraction << 42),
    };
    f64::from_bits(native)
}
