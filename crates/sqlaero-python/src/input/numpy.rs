//! Exact NumPy scalar acquisition through the public, fallible buffer API.
//!
//! These readers only consume immutable canonical scalars. Array buffers have
//! different ownership and concurrent-mutation requirements and are deliberately
//! not accepted by the fixed-width reader.

use super::buffer::{Canonical, Export};
use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyModule, PyString, PyTuple, PyType};
use std::ffi::CStr;

use sqlaero_transcode::temporal::{TimeKind, TimeUnit};

mod allocation;
mod array;
pub(crate) use array::{NumpySource, RowValue};
mod record;
pub(crate) use record::RecordSource;
mod mask;
pub(crate) use mask::MaskedSource;
pub(crate) use mask::Reader as MaskReader;
pub(crate) use mask::State as MaskState;

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

#[derive(Clone, Copy)]
enum ScalarExport<'a> {
    Exact,
    Qualified(&'a Canonical),
}

/// Canonical type and function references owned by one Python module state.
pub(crate) struct NumpyTypes {
    numeric: Vec<(Canonical, Kind)>,
    exact_export: bool,
    numeric_base: Option<Py<PyType>>,
    datetime_data: Py<PyAny>,
    datetime_dtype_get: Py<PyAny>,
    duration_dtype_get: Py<PyAny>,
    arrays: NumpyArrayApi,
    void: Py<PyType>,
}

/// The two owned public API references needed by bounded column re-preparation.
/// Scalar caches are not borrowed across the lifetime of a prepared source.
pub(crate) struct NumpyArrayApi {
    array: Canonical,
    dtype: super::attribute::GetSet,
    base: super::attribute::GetSet,
    flags: super::attribute::GetSet,
    schemas: Py<PyTuple>,
    masked: Py<PyType>,
    datetime_data: Py<PyAny>,
    item: pyo3::ffi::binaryfunc,
    record: Canonical,
    record_dtype: super::attribute::GetSet,
    record_base: super::attribute::GetSet,
    record_array: Py<PyAny>,
    array_view: Py<PyAny>,
    boolean: Canonical,
}

impl NumpyArrayApi {
    fn new(
        module: &Bound<'_, PyModule>,
        numeric: &[(Canonical, Kind)],
        datetime_data: &Bound<'_, PyAny>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Self> {
        let py = module.py();
        let array = exact_type(module, c"ndarray", budget)?;
        let buffer = Canonical::new(array.clone_ref(py), py)?;
        let dtype = super::attribute::GetSet::new(array.bind(py), c"dtype", budget)?;
        let base = super::attribute::GetSet::new(array.bind(py), c"base", budget)?;
        let flags = super::attribute::GetSet::new(array.bind(py), c"flags", budget)?;
        let record = exact_type(module, c"void", budget)?;
        let record_dtype = super::attribute::GetSet::new(record.bind(py), c"dtype", budget)?;
        let record_base = super::attribute::GetSet::new(record.bind(py), c"base", budget)?;
        let record_array = super::control::callback(py, budget, || {
            errors::attribute(record.bind(py).as_any(), c"__array__")
        })?
        .unbind();
        let array_view = super::control::callback(py, budget, || {
            errors::attribute(array.bind(py).as_any(), c"view")
        })?
        .unbind();
        let record = Canonical::new(record, py)?;
        let boolean = numeric
            .iter()
            .find(|(_, kind)| matches!(kind, Kind::Bool))
            .map(|(typ, _)| typ.clone_ref(py))
            .ok_or_else(|| {
                errors::builtin_message::<PyTypeError>(py, "NumPy bool base is missing")
            })?;
        let ma =
            super::control::callback(py, budget, || errors::attribute(module.as_any(), c"ma"))?;
        let masked =
            super::control::callback(py, budget, || errors::attribute(&ma, c"MaskedArray"))?
                .cast_into::<PyType>()?
                .unbind();
        let constructor =
            super::control::callback(py, budget, || errors::attribute(module.as_any(), c"dtype"))?;
        let mut schemas = Vec::new();
        schemas
            .try_reserve_exact(numeric.len() + 4)
            .map_err(|_| errors::memory_error(py))?;
        for (canonical, _) in numeric {
            let args = crate::output::tuple_from_ready(py, [canonical.typ(py).clone().into_any()])?;
            let value = super::control::callback(py, budget, || constructor.call1(args))?;
            schemas.push(value.get_type().into_any().unbind());
        }
        for name in [c"str_", c"bytes_", c"object_", c"void"] {
            let typ = exact_type(module, name, budget)?;
            let args = crate::output::tuple_from_ready(py, [typ.into_bound(py).into_any()])?;
            let value = super::control::callback(py, budget, || constructor.call1(args))?;
            schemas.push(value.get_type().into_any().unbind());
        }
        // SAFETY: this retained immutable native ndarray type owns the public
        // mapping subscript slot. Its exact signature is binaryfunc.
        let item = unsafe {
            pyo3::ffi::PyType_GetSlot(array.bind(py).as_ptr().cast(), pyo3::ffi::Py_mp_subscript)
        };
        if item.is_null() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical ndarray must have a public subscript slot",
            ));
        }
        // SAFETY: Py_mp_subscript has this documented non-null callback type.
        let item =
            unsafe { std::mem::transmute::<*mut std::ffi::c_void, pyo3::ffi::binaryfunc>(item) };
        Ok(Self {
            array: buffer,
            dtype,
            base,
            flags,
            schemas: crate::output::tuple_from_ready_vec(py, schemas)?.unbind(),
            masked,
            datetime_data: datetime_data.clone().unbind(),
            item,
            record,
            record_dtype,
            record_base,
            record_array,
            array_view,
            boolean,
        })
    }

    fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            array: self.array.clone_ref(py),
            dtype: self.dtype.clone_ref(py),
            base: self.base.clone_ref(py),
            flags: self.flags.clone_ref(py),
            schemas: self.schemas.clone_ref(py),
            masked: self.masked.clone_ref(py),
            datetime_data: self.datetime_data.clone_ref(py),
            item: self.item,
            record: self.record.clone_ref(py),
            record_dtype: self.record_dtype.clone_ref(py),
            record_base: self.record_base.clone_ref(py),
            record_array: self.record_array.clone_ref(py),
            array_view: self.array_view.clone_ref(py),
            boolean: self.boolean.clone_ref(py),
        }
    }

    /// Recognize qualified base storage without invoking replacement methods.
    pub(crate) fn accepts(&self, value: &Bound<'_, PyAny>) -> bool {
        super::actual_instance(value, self.array.typ(value.py()))
            && !super::actual_instance(value, self.masked.bind(value.py()))
    }

    /// Reuse admitted physical dtype facts without acquiring a value array.
    pub(crate) fn dtype_schema(
        &self,
        dtype: &Bound<'_, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<super::cell::Schema>> {
        if !self.supports_dtype(dtype) {
            return Err(errors::builtin_message::<PyTypeError>(
                dtype.py(),
                "unsupported NumPy dtype schema",
            ));
        }
        array::dtype_schema(dtype, budget)
    }

    /// Prepare through one canonical checked reader; mask never falls through.
    pub(crate) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<NumpySource<'py>>> {
        if !super::actual_instance(value, self.array.typ(value.py())) {
            return Ok(None);
        }
        if super::actual_instance(value, self.masked.bind(value.py())) {
            return Err(errors::builtin_message::<PyTypeError>(
                value.py(),
                "MaskedArray requires the dedicated mask-aware source path",
            ));
        }
        super::control::callback(value.py(), Some(budget), || {
            NumpySource::new(value, self, budget)
        })
        .map(Some)
    }

    fn supports_dtype(&self, dtype: &Bound<'_, PyAny>) -> bool {
        let actual = dtype.get_type();
        self.schemas
            .bind(dtype.py())
            .iter()
            .any(|typ| typ.is(&actual))
    }
}

impl NumpyTypes {
    pub(crate) fn prepare_masked<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<MaskedSource<'py>>> {
        if !super::actual_instance(value, self.arrays.masked.bind(value.py())) {
            return Ok(None);
        }
        super::control::callback(value.py(), Some(budget), || {
            MaskedSource::new(value, &self.arrays, budget)
        })
        .map(Some)
    }

    /// Prepare NumPy only on the actual optional-input path.
    pub(crate) fn new(py: Python<'_>, budget: Option<&dyn Checkpoint>) -> PyResult<Self> {
        let module = super::control::callback(py, budget, || errors::module(py, c"numpy"))?;
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
            numeric.push((
                Canonical::new(exact_type(&module, name, budget)?, py)?,
                kind,
            ));
        }
        let module_type = py.get_type::<PyModule>();
        let exact_export = numeric.iter().all(|(registered, _)| {
            // Canonical::new established immutable types. CPython allows
            // class reassignment between module subtypes despite that flag.
            !super::actual_type_is_subtype(registered.typ(py), &module_type)
        });
        let numeric_base = super::control::callback(py, budget, || {
            module
                .dict()
                .get_item(PyString::from_bytes(py, b"generic")?)
        })?
        .and_then(|value| value.cast_into::<PyType>().ok())
        .filter(|base| {
            // Static CPython types can only have immutable static bases. Only
            // use this rejection shortcut while that common ancestry is proven.
            numeric.iter().all(|(registered, _)| {
                let candidate = registered.typ(py);
                // SAFETY: the registered canonical type is retained and live.
                let flags = unsafe { pyo3::ffi::PyType_GetFlags(candidate.as_type_ptr()) };
                flags & pyo3::ffi::Py_TPFLAGS_HEAPTYPE == 0
                    && flags & pyo3::ffi::Py_TPFLAGS_IMMUTABLETYPE != 0
                    && super::actual_type_is_subtype(candidate, base)
            })
        })
        .map(Bound::unbind);
        let datetime_data = super::control::callback(py, budget, || {
            errors::attribute(module.as_any(), c"datetime_data")
        })?;
        if !datetime_data.is_callable() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NumPy datetime_data API must be callable before preparing input",
            ));
        }
        let arrays = NumpyArrayApi::new(&module, &numeric, &datetime_data, budget)?;
        Ok(Self {
            numeric,
            exact_export,
            numeric_base,
            datetime_data: datetime_data.unbind(),
            datetime_dtype_get: dtype_get(&module, c"datetime64", budget)?,
            duration_dtype_get: dtype_get(&module, c"timedelta64", budget)?,
            arrays,
            void: exact_type(&module, c"void", budget)?,
        })
    }

    /// Prepare only an exact ndarray; logical shape is independent of SQL.
    pub(crate) fn prepare_array<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<NumpySource<'py>>> {
        self.arrays.prepare(value, budget)
    }

    /// Own only the array preparation API used by a provider's bounded blocks.
    pub(crate) fn array_api(&self, py: Python<'_>) -> NumpyArrayApi {
        self.arrays.clone_ref(py)
    }

    /// A canonical record is one logical row, not a general structured array.
    pub(crate) fn prepare_record<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<RecordSource<'py>>> {
        let is_void = super::actual_instance(value, self.void.bind(value.py()));
        let is_array = super::actual_instance(value, self.arrays.array.typ(value.py()));
        if !is_void && !is_array {
            return Ok(None);
        }
        super::check(value.py(), budget)?;
        let source = RecordSource::new(value, &self.arrays, budget, is_void)?;
        super::check(value.py(), budget)?;
        Ok(source)
    }

    /// Read a recognized immutable numeric scalar into a native value.
    pub(crate) fn read(
        &self,
        value: &Bound<'_, PyAny>,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<NumpyScalar>> {
        let Some((export, kind)) = self.recognize(value) else {
            return Ok(None);
        };
        self.read_classified(value, export, kind, budget).map(Some)
    }

    /// Ask for actual integer facts without reading unrelated numeric/temporal
    /// payloads or executing a protocol that the consuming target cannot use.
    pub(crate) fn read_integer(&self, value: &Bound<'_, PyAny>) -> PyResult<Option<NumpyScalar>> {
        let Some((export, kind)) = self.recognize(value) else {
            return Ok(None);
        };
        if !matches!(
            kind,
            Kind::I8
                | Kind::I16
                | Kind::I32
                | Kind::I64
                | Kind::U8
                | Kind::U16
                | Kind::U32
                | Kind::U64
        ) {
            return Ok(None);
        }
        self.read_classified(value, export, kind, None).map(Some)
    }

    fn recognize<'a>(&'a self, value: &Bound<'_, PyAny>) -> Option<(ScalarExport<'a>, Kind)> {
        let py = value.py();
        // Only compare the source's type pointer to retained canonical types.
        // A non-match uses a live type reference for subclass inspection.
        let pointer = value.get_type_ptr().cast();
        if let Some((registered, kind)) = self
            .numeric
            .iter()
            .find(|(registered, _)| pointer == registered.typ(py).as_ptr())
        {
            let export = if self.exact_export {
                ScalarExport::Exact
            } else {
                ScalarExport::Qualified(registered)
            };
            return Some((export, *kind));
        }
        let typ = value.get_type();
        if let Some(base) = &self.numeric_base
            && !super::actual_type_is_subtype(&typ, base.bind(py))
        {
            return None;
        }
        self.numeric
            .iter()
            .find(|(registered, _)| super::actual_type_is_subtype(&typ, registered.typ(py)))
            .map(|(registered, kind)| (ScalarExport::Qualified(registered), *kind))
    }

    fn read_classified(
        &self,
        value: &Bound<'_, PyAny>,
        export: ScalarExport<'_>,
        kind: Kind,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<NumpyScalar> {
        let scalar = match kind {
            Kind::I8 => NumpyScalar::Signed(i64::from(i8::from_ne_bytes([fixed_bytes(
                value, export, 1,
            )?[0]]))),
            Kind::I16 => {
                let b = fixed_bytes(value, export, 2)?;
                NumpyScalar::Signed(i64::from(i16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::I32 => {
                let b = fixed_bytes(value, export, 4)?;
                NumpyScalar::Signed(i64::from(i32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::I64 => NumpyScalar::Signed(i64::from_ne_bytes(fixed_bytes(value, export, 8)?)),
            Kind::U8 => NumpyScalar::Unsigned(u64::from(fixed_bytes(value, export, 1)?[0])),
            Kind::U16 => {
                let b = fixed_bytes(value, export, 2)?;
                NumpyScalar::Unsigned(u64::from(u16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::U32 => {
                let b = fixed_bytes(value, export, 4)?;
                NumpyScalar::Unsigned(u64::from(u32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::U64 => NumpyScalar::Unsigned(u64::from_ne_bytes(fixed_bytes(value, export, 8)?)),
            Kind::F16 => {
                let b = fixed_bytes(value, export, 2)?;
                NumpyScalar::Float(half_to_f64(u16::from_ne_bytes([b[0], b[1]])))
            }
            Kind::F32 => {
                let b = fixed_bytes(value, export, 4)?;
                NumpyScalar::Float(f64::from(f32::from_ne_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::F64 => NumpyScalar::Float(f64::from_ne_bytes(fixed_bytes(value, export, 8)?)),
            Kind::Bool => NumpyScalar::Bool(fixed_bytes(value, export, 1)?[0] != 0),
            Kind::DateTime | Kind::Duration => {
                let ticks = i64::from_ne_bytes(fixed_bytes(value, export, 8)?);
                let kind = match kind {
                    Kind::DateTime => TimeKind::DateTime,
                    _ => TimeKind::Duration,
                };
                // The source sentinel is sufficient for NaT. Do not ask a
                // generic or unsupported unit to interpret a missing value.
                let (unit, multiplier) = if ticks == i64::MIN {
                    (TimeUnit::Generic, 1)
                } else {
                    self.temporal_metadata(value, kind, budget)?
                };
                NumpyScalar::Temporal {
                    ticks,
                    unit,
                    multiplier,
                    kind,
                }
            }
        };
        Ok(scalar)
    }

    fn temporal_metadata(
        &self,
        value: &Bound<'_, PyAny>,
        kind: TimeKind,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<(TimeUnit, u64)> {
        let get = match kind {
            TimeKind::DateTime => &self.datetime_dtype_get,
            TimeKind::Duration => &self.duration_dtype_get,
        };
        let args = crate::output::tuple_from_ready(value.py(), [value.clone()])?;
        let dtype =
            super::control::callback(value.py(), budget, || get.bind(value.py()).call1(args))?;
        temporal_dtype_metadata(&dtype, &self.datetime_data, budget)
    }
}

fn temporal_dtype_metadata(
    dtype: &Bound<'_, PyAny>,
    datetime_data: &Py<PyAny>,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<(TimeUnit, u64)> {
    let py = dtype.py();
    let args = crate::output::tuple_from_ready(py, [dtype.clone()])?;
    let metadata = super::control::callback(py, budget, || datetime_data.bind(py).call1(args))?;
    let metadata = metadata.cast::<PyTuple>()?;
    if metadata.len() != 2 {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "NumPy datetime_data must return exactly (unit, multiplier)",
        ));
    }
    let name = metadata.get_item(0)?;
    let name = name.cast::<PyString>()?;
    // SAFETY: the actual Unicode base was checked above. The public length API
    // reads its base storage without dispatching a subclass __len__ override.
    let length = unsafe { pyo3::ffi::PyUnicode_GetLength(name.as_ptr()) };
    if length < 0 {
        return Err(PyErr::fetch(py));
    }
    if length > 7 {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "NumPy returned an unrecognized datetime unit",
        ));
    }
    let name = name.to_str()?;
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
    let multiplier = metadata.get_item(1)?.cast::<PyInt>()?.extract::<u64>()?;
    // A dtype can carry zero units for a source NaT. Retain the metadata until
    // the common boundary has checked its ticks; finite zero multipliers are
    // rejected there, after the source-specific NaT -> NULL rule.
    Ok((unit, multiplier))
}

fn dtype_get(
    module: &Bound<'_, PyModule>,
    name: &CStr,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Py<PyAny>> {
    let typ = exact_type(module, name, budget)?;
    let descriptor = errors::attribute(typ.bind(module.py()).as_any(), c"dtype")?;
    Ok(errors::attribute(&descriptor, c"__get__")?.unbind())
}

fn exact_type(
    module: &Bound<'_, PyModule>,
    name: &CStr,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<Py<PyType>> {
    Ok(super::control::callback(module.py(), budget, || {
        errors::attribute(module.as_any(), name)
    })?
    .cast_into::<PyType>()?
    .unbind())
}

/// Copy at most eight bytes from an immutable canonical scalar export.
fn fixed_bytes(
    value: &Bound<'_, PyAny>,
    source: ScalarExport<'_>,
    width: usize,
) -> PyResult<[u8; 8]> {
    let py = value.py();
    let export = match source {
        ScalarExport::Exact => Export::acquire(value, pyo3::ffi::PyBUF_SIMPLE)?,
        ScalarExport::Qualified(base) => base.acquire(value, pyo3::ffi::PyBUF_SIMPLE)?,
    };
    let raw = &export.raw;
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
    drop(export);
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
