//! Public column preparation and row-wise writes, without Python row objects.

use crate::errors;
use crate::input::source::{CellWriter, PreparedSource, Shape};
use crate::input::temporal::{self, TimeKind, TimeUnit};
use crate::input::{
    arrow,
    numpy::{NumpyArrayApi, NumpySource, NumpyTypes},
};
use pyo3::exceptions::{PyIndexError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple, PyType};
use smallvec::SmallVec;
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;
use std::ffi::CStr;
use std::fmt::Write;

/// Fixed public dtype families. Unsupported extension schemas never disappear
/// merely because an array is empty or every value is missing.
#[derive(Clone, Copy)]
enum Family {
    Nullable,
    PythonString,
    Arrow,
    Categorical,
    AwareTime,
    Numpy,
}

pub(super) struct SourceTypes {
    series: Py<PyType>,
    index: Py<PyType>,
    frame: Py<PyType>,
    range: Py<PyType>,
    multi: Py<PyType>,
    native_indices: Vec<Py<PyType>>,
    arrays: Vec<Py<PyType>>,
    dtypes: Vec<(Py<PyType>, Family)>,
    string_dtype: Py<PyType>,
    numpy_dtype: Py<PyType>,
    numpy_extension: Py<PyType>,
}

impl SourceTypes {
    pub(super) fn new(module: &Bound<'_, PyModule>) -> PyResult<Self> {
        let py = module.py();
        let mut native_indices = Vec::new();
        native_indices
            .try_reserve_exact(5)
            .map_err(|_| errors::memory_error(py))?;
        for name in [
            c"Index" as &CStr,
            c"RangeIndex",
            c"DatetimeIndex",
            c"TimedeltaIndex",
            c"CategoricalIndex",
        ] {
            native_indices.push(super::exact_type(module, name)?);
        }
        let array_module = errors::attribute(module.as_any(), c"arrays")?;
        let mut arrays = Vec::new();
        arrays
            .try_reserve_exact(7)
            .map_err(|_| errors::memory_error(py))?;
        arrays.push(super::exact_type(module, c"Categorical")?);
        for name in [
            c"IntegerArray" as &CStr,
            c"FloatingArray",
            c"BooleanArray",
            c"StringArray",
            c"ArrowStringArray",
            c"ArrowExtensionArray",
        ] {
            arrays.push(
                errors::attribute(&array_module, name)?
                    .cast_into::<PyType>()?
                    .unbind(),
            );
        }
        let definitions: &[(&CStr, Family)] = &[
            (c"Int8Dtype", Family::Nullable),
            (c"Int16Dtype", Family::Nullable),
            (c"Int32Dtype", Family::Nullable),
            (c"Int64Dtype", Family::Nullable),
            (c"UInt8Dtype", Family::Nullable),
            (c"UInt16Dtype", Family::Nullable),
            (c"UInt32Dtype", Family::Nullable),
            (c"UInt64Dtype", Family::Nullable),
            (c"Float32Dtype", Family::Nullable),
            (c"Float64Dtype", Family::Nullable),
            (c"BooleanDtype", Family::Nullable),
            (c"ArrowDtype", Family::Arrow),
            (c"CategoricalDtype", Family::Categorical),
            (c"DatetimeTZDtype", Family::AwareTime),
        ];
        let mut dtypes = Vec::new();
        dtypes
            .try_reserve_exact(definitions.len())
            .map_err(|_| errors::memory_error(py))?;
        for &(name, family) in definitions {
            dtypes.push((super::exact_type(module, name)?, family));
        }
        let numpy = errors::module(py, c"numpy")?;
        Ok(Self {
            series: super::exact_type(module, c"Series")?,
            index: super::exact_type(module, c"Index")?,
            frame: super::exact_type(module, c"DataFrame")?,
            range: super::exact_type(module, c"RangeIndex")?,
            multi: super::exact_type(module, c"MultiIndex")?,
            native_indices,
            arrays,
            dtypes,
            string_dtype: super::exact_type(module, c"StringDtype")?,
            numpy_dtype: super::exact_type(&numpy, c"dtype")?,
            numpy_extension: errors::attribute(&array_module, c"NumpyExtensionArray")?
                .cast_into::<PyType>()?
                .unbind(),
        })
    }

    pub(super) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        numpy: &NumpyTypes,
        budget: &IoBudget,
    ) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
        let py = value.py();
        let typ = value.get_type();
        let is_frame = typ.is(self.frame.bind(py));
        let is_series = inherits(value, self.series.bind(py));
        let is_index = inherits(value, self.index.bind(py));
        let is_array = self.arrays.iter().any(|array| typ.is(array.bind(py)));
        if !is_frame && !is_series && !is_index && !is_array {
            return Ok(None);
        }
        crate::input::check(py, budget)?;
        let api = numpy.array_api(py);
        let (shape, columns) = if is_frame {
            self.frame(value, &api, budget)?
        } else {
            // In particular, native PeriodIndex/IntervalIndex and a Sparse
            // Series must not enter the subclass values path and hide their
            // unsupported declared schema behind an empty object ndarray.
            self.family(&errors::attribute(value, c"dtype")?)?;
            let length = value.len()?;
            let native_index = self
                .native_indices
                .iter()
                .any(|index| typ.is(index.bind(py)));
            let column = if typ.is(self.range.bind(py)) {
                let start = errors::attribute(value, c"start")?;
                let step = errors::attribute(value, c"step")?;
                // The exact RangeIndex's public coordinates are Python ints.
                // Only values too wide for i128 use its public scalar getter.
                let native = match native_integer(&start)? {
                    Some(start) => native_integer(&step)?.map(|step| (start, step)),
                    None => None,
                };
                Column::Range {
                    owner: value.clone(),
                    length,
                    native,
                }
            } else if typ.is(self.multi.bind(py)) {
                let levels = errors::attribute(value, c"levels")?;
                let mut candidates = SmallVec::<[Bound<'py, PyAny>; 8]>::new();
                let mut eligible = length >= 32;
                for level in levels.try_iter()? {
                    crate::input::check(py, budget)?;
                    let level = level?;
                    self.family(&errors::attribute(&level, c"dtype")?)?;
                    if eligible && candidates.len() < 8 {
                        candidates.push(level);
                    } else {
                        eligible = false;
                    }
                }
                if eligible
                    && let Some(levels) =
                        self.multi_column(value, &candidates, length, &api, budget)?
                {
                    let source = crate::input::boxed(
                        py,
                        Multi {
                            owner: value.clone(),
                            length,
                            levels,
                            numpy: api,
                            numpy_extension: self.numpy_extension.bind(py).clone(),
                        },
                    )?;
                    crate::input::check(py, budget)?;
                    return Ok(Some(source));
                } else {
                    Column::Python {
                        owner: value.clone(),
                        length,
                    }
                }
            } else if is_series && !typ.is(self.series.bind(py)) || is_index && !native_index {
                // Exactly one observable values access for real subclasses.
                let values = errors::attribute(value, c"values")?;
                let accepted = api.accepts(&values)
                    || self
                        .arrays
                        .iter()
                        .any(|array| values.get_type().is(array.bind(py)))
                    || native_index;
                if !accepted {
                    return Err(invalid(
                        py,
                        "Pandas subclass values must return a supported one-dimensional storage array",
                    ));
                }
                self.column(&values, &api, budget)?
            } else if is_series || is_index {
                let storage = errors::attribute(value, c"array")?;
                self.column(&storage, &api, budget)?
            } else {
                self.column(value, &api, budget)?
            };
            if column.length()? != length {
                return Err(invalid(
                    py,
                    "Pandas storage length differs from its outer logical source",
                ));
            }
            let mut columns = Vec::new();
            columns
                .try_reserve_exact(1)
                .map_err(|_| errors::memory_error(py))?;
            columns.push(column);
            (Shape::Vector { length }, columns)
        };
        crate::input::check(py, budget)?;
        Ok(Some(crate::input::boxed(
            py,
            PandasSource {
                owner: value.clone(),
                shape,
                columns,
                numpy: api,
            },
        )?))
    }

    fn multi_column<'py>(
        &self,
        owner: &Bound<'py, PyAny>,
        levels: &[Bound<'py, PyAny>],
        length: usize,
        api: &NumpyArrayApi,
        budget: &IoBudget,
    ) -> PyResult<Option<Vec<MultiLevel<'py>>>> {
        let py = owner.py();
        if levels.is_empty() {
            return Ok(None);
        }
        let mut signed = SmallVec::<[Option<Bound<'py, PyAny>>; 8]>::new();
        for level in levels {
            crate::input::check(py, budget)?;
            if !level.get_type().is(self.index.bind(py)) {
                return Ok(None);
            }
            let storage = errors::attribute(level, c"array")?;
            if !storage.get_type().is(self.numpy_extension.bind(py)) {
                return Ok(None);
            }
            let dtype = errors::attribute(level, c"dtype")?;
            let kind = errors::attribute(&dtype, c"kind")?;
            let kind = kind.extract::<&str>()?;
            // Object labels still use the public getter and original visitor.
            // Inferring their values here would scan unreferenced labels.
            if !matches!(kind, "i" | "u" | "f" | "b" | "O") {
                return Ok(None);
            }
            signed.push(
                if kind == "i"
                    && matches!(
                        errors::attribute(&dtype, c"itemsize")?.extract::<usize>()?,
                        1 | 2 | 4 | 8
                    )
                {
                    Some(storage)
                } else {
                    None
                },
            );
        }
        let codes = errors::attribute(owner, c"codes")?;
        if codes.len()? != levels.len() {
            return Ok(None);
        }
        let mut prepared = Vec::new();
        prepared
            .try_reserve_exact(levels.len())
            .map_err(|_| errors::memory_error(py))?;
        for (position, code) in codes.try_iter()?.enumerate() {
            crate::input::check(py, budget)?;
            if position >= levels.len() {
                return Ok(None);
            }
            let code = code?;
            if !api.accepts(&code)
                || errors::attribute(&code, c"ndim")?.extract::<usize>()? != 1
                || code.len()? != length
            {
                return Ok(None);
            }
            let dtype = errors::attribute(&code, c"dtype")?;
            if errors::attribute(&dtype, c"kind")?.extract::<&str>()? != "i"
                || !matches!(
                    errors::attribute(&dtype, c"itemsize")?.extract::<usize>()?,
                    1 | 2 | 4 | 8
                )
            {
                return Ok(None);
            }
            prepared.push(MultiLevel {
                owner: levels[position].clone(),
                length: levels[position].len()?,
                codes: numpy_source(api, &code, budget, length)?,
                prepare_signed: signed[position].take(),
                signed: None,
            });
        }
        if prepared.len() != levels.len() {
            return Ok(None);
        }
        Ok(Some(prepared))
    }

    fn frame<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &IoBudget,
    ) -> PyResult<(Shape, Vec<Column<'py>>)> {
        let py = value.py();
        let shape = errors::attribute(value, c"shape")?.cast_into::<PyTuple>()?;
        if shape.len() != 2 {
            return Err(invalid(
                py,
                "Pandas DataFrame shape must have two dimensions",
            ));
        }
        let rows = shape.get_item(0)?.extract::<usize>()?;
        let count = shape.get_item(1)?.extract::<usize>()?;
        let mut columns = Vec::new();
        columns
            .try_reserve_exact(count)
            .map_err(|_| errors::memory_error(py))?;
        let iloc = errors::attribute(value, c"iloc")?;
        let all = full_slice(py)?;
        for index in 0..count {
            crate::input::check(py, budget)?;
            let position = crate::input::python_index(py, index)?;
            let key = crate::output::tuple_from_ready(py, [all.clone(), position])?;
            let series = iloc.get_item(key)?;
            let storage = errors::attribute(&series, c"array")?;
            let column = self
                .column(&storage, api, budget)
                .map_err(|error| column_error(py, error, index))?;
            if column.length()? != rows {
                return Err(invalid(
                    py,
                    "Pandas DataFrame column length differs from its prepared row count",
                ));
            }
            columns.push(column);
        }
        Ok((
            Shape::Table {
                rows,
                columns: count,
            },
            columns,
        ))
    }

    fn family(&self, dtype: &Bound<'_, PyAny>) -> PyResult<Family> {
        let py = dtype.py();
        let typ = dtype.get_type();
        if typ.is(self.string_dtype.bind(py)) {
            let storage = errors::attribute(dtype, c"storage")?;
            return match storage.extract::<&str>()? {
                "python" => Ok(Family::PythonString),
                "pyarrow" => Ok(Family::Arrow),
                _ => Err(invalid(py, "Pandas StringDtype storage is not supported")),
            };
        }
        if let Some(family) = self
            .dtypes
            .iter()
            .find_map(|(typ, family)| dtype.get_type().is(typ.bind(py)).then_some(*family))
        {
            return Ok(family);
        }
        if inherits(dtype, self.numpy_dtype.bind(py)) {
            return Ok(Family::Numpy);
        }
        Err(invalid(
            py,
            "unsupported declared Pandas extension dtype; Period, Interval, Sparse and custom extension schemas are excluded",
        ))
    }

    fn column<'py>(
        &self,
        storage: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &IoBudget,
    ) -> PyResult<Column<'py>> {
        let py = storage.py();
        crate::input::check(py, budget)?;
        if let Some(array) = api.prepare(storage, budget)? {
            require_vector(&array)?;
            return Ok(Column::Numpy(array));
        }
        let length = storage.len()?;
        if storage.get_type().is(self.numpy_extension.bind(py)) {
            // The public canonical wrapper has a NumpyEADtype facade. Its
            // to_numpy result, not that facade's class/name, establishes the
            // actual dtype and ownership. This internal bridge does not make
            // standalone NumpyExtensionArray a new accepted root input.
            return converted_numpy_column(storage, api, budget, length);
        }
        let dtype = errors::attribute(storage, c"dtype")?;
        let family = self.family(&dtype)?;
        match family {
            Family::Numpy => converted_numpy_column(storage, api, budget, length),
            Family::Nullable | Family::PythonString => {
                let python_string = matches!(family, Family::PythonString);
                let numpy_dtype = if python_string {
                    py.get_type::<PyAny>().into_any()
                } else {
                    errors::attribute(&dtype, c"numpy_dtype")?
                };
                Ok(Column::Masked(Masked {
                    owner: storage.clone(),
                    length,
                    dtype: numpy_dtype,
                    python_string,
                    standard_array: (if python_string {
                        self.arrays
                            .get(4)
                            .is_some_and(|typ| storage.get_type().is(typ.bind(py)))
                    } else {
                        length >= 32
                            && self
                                .arrays
                                .iter()
                                .skip(1)
                                .take(3)
                                .any(|typ| storage.get_type().is(typ.bind(py)))
                    })
                    .then(|| storage.get_type()),
                    current: None,
                }))
            }
            Family::Arrow => {
                let exported = errors::attribute(storage, c"__arrow_array__")?.call0()?;
                crate::input::check(py, budget)?;
                let source =
                    arrow::prepare_exported(storage, &exported, Shape::Vector { length }, budget)?;
                Ok(Column::Arrow(source))
            }
            Family::Categorical => {
                let codes = errors::attribute(storage, c"codes")?;
                let categories = errors::attribute(storage, c"categories")?;
                let dtype = errors::attribute(&categories, c"dtype")?;
                let family = self.family(&dtype)?;
                let labels = categories.len()?;
                let codes = numpy_source(api, &codes, budget, length)?;
                // Standard Index values may use the already validated native
                // storage contract. Object labels and real Index subclasses
                // retain their actual scalar getter and conversion protocols.
                let native = if length >= 32
                    && self
                        .arrays
                        .first()
                        .is_some_and(|typ| storage.get_type().is(typ.bind(py)))
                    && categories.get_type().is(self.index.bind(py))
                {
                    match family {
                        Family::Numpy
                            if matches!(
                                errors::attribute(&dtype, c"kind")?.extract::<&str>()?,
                                "i" | "u" | "f" | "b"
                            ) =>
                        {
                            Some((self.numpy_extension.bind(py).clone(), family))
                        }
                        // Python strings still prepare mask/Object blocks;
                        // short columns cannot amortize that fixed work.
                        Family::PythonString if length >= 512 => {
                            self.arrays.get(4).map(|typ| (typ.bind(py).clone(), family))
                        }
                        Family::Arrow if dtype.get_type().is(self.string_dtype.bind(py)) => {
                            self.arrays.get(5).map(|typ| (typ.bind(py).clone(), family))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(Column::Categorical(crate::input::boxed(
                    py,
                    Category {
                        codes,
                        categories,
                        labels,
                        length,
                        prepare_native: native,
                        native: None,
                    },
                )?))
            }
            Family::AwareTime => {
                let unit = errors::attribute(&dtype, c"unit")?;
                let unit = match unit.extract::<&str>()? {
                    "s" => TimeUnit::Second,
                    "ms" => TimeUnit::Millisecond,
                    "us" => TimeUnit::Microsecond,
                    "ns" => TimeUnit::Nanosecond,
                    _ => {
                        return Err(invalid(
                            py,
                            "Pandas DatetimeTZDtype unit must be s, ms, us or ns",
                        ));
                    }
                };
                let ticks = errors::attribute(storage, c"asi8")?;
                let ticks = numpy_source(api, &ticks, budget, length)?;
                Ok(Column::Time {
                    ticks,
                    unit,
                    length,
                })
            }
        }
    }
}

pub(crate) struct PandasSource<'py> {
    owner: Bound<'py, PyAny>,
    shape: Shape,
    columns: Vec<Column<'py>>,
    numpy: NumpyArrayApi,
}

impl<'py> PreparedSource<'py> for PandasSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }
    fn shape(&self) -> Shape {
        self.shape
    }
    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        let length = match self.shape {
            Shape::Vector { length } => length,
            Shape::Table { rows, .. } => rows,
            Shape::Record { .. } => {
                return Err(invalid(py, "Pandas source cannot have record shape"));
            }
        };
        if row >= length {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "Pandas row is outside its prepared shape",
            ));
        }
        let source = self.columns.get_mut(column).ok_or_else(|| {
            errors::builtin_message::<PyIndexError>(
                py,
                "Pandas column is outside its prepared shape",
            )
        })?;
        visitor.checkpoint(py, 1)?;
        source.write(row, output, context, budget, visitor, &self.numpy)
    }
}

enum Column<'py> {
    Numpy(NumpySource<'py>),
    Arrow(arrow::Prepared<'py>),
    Masked(Masked<'py>),
    Range {
        owner: Bound<'py, PyAny>,
        length: usize,
        native: Option<(i128, i128)>,
    },
    Python {
        owner: Bound<'py, PyAny>,
        length: usize,
    },
    Categorical(Box<Category<'py>>),
    Time {
        ticks: NumpySource<'py>,
        unit: TimeUnit,
        length: usize,
    },
}

impl Column<'_> {
    fn length(&self) -> PyResult<usize> {
        match self {
            Self::Numpy(source) => require_vector(source),
            Self::Arrow(source) => match source.shape() {
                Shape::Vector { length } => Ok(length),
                _ => Err(invalid(
                    source.owner().py(),
                    "Pandas Arrow column must be one-dimensional",
                )),
            },
            Self::Masked(source) => Ok(source.length),
            Self::Categorical(source) => Ok(source.length),
            Self::Range { length, .. }
            | Self::Python { length, .. }
            | Self::Time { length, .. } => Ok(*length),
        }
    }

    fn write(
        &mut self,
        row: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        match self {
            Self::Numpy(source) => source.write_cell(row, 0, output, context, budget, visitor),
            Self::Arrow(source) => source.write_cell(row, 0, output, context, budget, visitor),
            Self::Masked(source) => source.write(row, output, context, budget, visitor, api),
            Self::Range { owner, native, .. } => {
                if let Some(value) = native
                    .and_then(|(start, step)| (row as i128).checked_mul(step)?.checked_add(start))
                {
                    let result = match i64::try_from(value) {
                        Ok(value) => scalar::integer(output, value, context.limit),
                        Err(_) => scalar::integer(output, value, context.limit),
                    };
                    return result.map_err(|error| errors::wire(owner.py(), error));
                }
                let value = owner.get_item(crate::input::python_index(owner.py(), row)?)?;
                visitor.python(&value, output, None)
            }
            Self::Python { owner, .. } => {
                let value = owner.get_item(crate::input::python_index(owner.py(), row)?)?;
                visitor.python(&value, output, None)
            }
            Self::Categorical(source) => source.write(row, output, context, budget, visitor, api),
            Self::Time { ticks, unit, .. } => {
                let py = ticks.owner().py();
                let value = ticks.signed_at(row, budget)?;
                if value != i64::MIN && !context.utc_confirmed {
                    return Err(errors::client(
                        py,
                        ClientErrorCode::SessionState,
                        "aware Pandas datetime requires a reliably confirmed UTC MySQL session; time_zone is non-UTC or unknown",
                    ));
                }
                temporal::write(
                    py,
                    value,
                    *unit,
                    1,
                    TimeKind::DateTime,
                    output,
                    context,
                    Some(budget),
                )
            }
        }
    }
}

/// One category column owns its codes, label owner and optional standard
/// storage reader. No encoded label answers or raw Object slots are cached.
struct Category<'py> {
    codes: NumpySource<'py>,
    categories: Bound<'py, PyAny>,
    labels: usize,
    length: usize,
    prepare_native: Option<(Bound<'py, PyType>, Family)>,
    native: Option<Box<Column<'py>>>,
}

impl<'py> Category<'py> {
    fn write(
        &mut self,
        row: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        let py = self.categories.py();
        let code = self.codes.signed_at(row, budget)?;
        if code == -1 {
            return writer::append(output, b"NULL", context.limit)
                .map_err(|error| errors::wire(py, error));
        }
        let code = usize::try_from(code)
            .ok()
            .filter(|code| *code < self.labels)
            .ok_or_else(|| {
                errors::builtin_message::<PyValueError>(
                    py,
                    "Pandas categorical code must be -1 or a valid categories position",
                )
            })?;
        // Resolve only after an actual valid reference. An all-missing input
        // or bad first code must not export/read otherwise unused labels.
        if let Some((typ, family)) = self.prepare_native.take() {
            let storage = errors::attribute(&self.categories, c"array")?;
            crate::input::check(py, budget)?;
            if storage.get_type().is(&typ) {
                if storage.len()? != self.labels {
                    return Err(invalid(
                        py,
                        "Pandas categorical label storage length changed during encoding",
                    ));
                }
                let source = match family {
                    Family::Numpy => {
                        let keywords = errors::dictionary(py)?;
                        keywords
                            .set_item(pyo3::types::PyString::from_bytes(py, b"copy")?, false)?;
                        let converted = call_keywords(&storage, c"to_numpy", &keywords)?;
                        crate::input::check(py, budget)?;
                        let native = if api.accepts(&converted) {
                            let dtype = errors::attribute(&converted, c"dtype")?;
                            let kind = errors::attribute(&dtype, c"kind")?;
                            let width =
                                errors::attribute(&dtype, c"itemsize")?.extract::<usize>()?;
                            matches!(
                                (kind.extract::<&str>()?, width),
                                ("i" | "u", 1 | 2 | 4 | 8) | ("f", 2 | 4 | 8) | ("b", 1)
                            )
                        } else {
                            false
                        };
                        if !native {
                            let label = self
                                .categories
                                .get_item(crate::input::python_index(py, code)?)?;
                            return visitor.python(&label, output, None);
                        }
                        Column::Numpy(numpy_source(api, &converted, budget, self.labels)?)
                    }
                    Family::PythonString => Column::Masked(Masked {
                        owner: storage,
                        length: self.labels,
                        dtype: py.get_type::<PyAny>().into_any(),
                        python_string: true,
                        standard_array: None,
                        current: None,
                    }),
                    Family::Arrow => {
                        let exported = errors::attribute(&storage, c"__arrow_array__")?.call0()?;
                        crate::input::check(py, budget)?;
                        // Category codes are random access. A forward-only
                        // stream cannot stand in for a label table; select a
                        // single public array, or leave this column generic.
                        let exported = if exported.hasattr(c"__arrow_c_array__")? {
                            Some(exported)
                        } else if exported.hasattr(c"num_chunks")?
                            && errors::attribute(&exported, c"num_chunks")?.extract::<usize>()? == 1
                        {
                            let chunk = exported.call_method1(c"chunk", (0,))?;
                            if chunk.hasattr(c"__arrow_c_array__")? {
                                Some(chunk)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let Some(exported) = exported else {
                            let label = self
                                .categories
                                .get_item(crate::input::python_index(py, code)?)?;
                            return visitor.python(&label, output, None);
                        };
                        Column::Arrow(arrow::prepare_exported(
                            &storage,
                            &exported,
                            Shape::Vector {
                                length: self.labels,
                            },
                            budget,
                        )?)
                    }
                    _ => {
                        return Err(invalid(py, "Pandas categorical label strategy is invalid"));
                    }
                };
                self.native = Some(crate::input::boxed(py, source)?);
            }
        }
        if let Some(source) = &mut self.native {
            return source.write(code, output, context, budget, visitor, api);
        }
        let label = self
            .categories
            .get_item(crate::input::python_index(py, code)?)?;
        visitor.python(&label, output, None)
    }
}

struct MultiLevel<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    codes: NumpySource<'py>,
    prepare_signed: Option<Bound<'py, PyAny>>,
    signed: Option<Box<NumpySource<'py>>>,
}

impl<'py> MultiLevel<'py> {
    fn value(
        &mut self,
        position: usize,
        api: &NumpyArrayApi,
        numpy_extension: &Bound<'py, PyType>,
        budget: &IoBudget,
    ) -> PyResult<OwnedLeaf<'py>> {
        let py = self.owner.py();
        // The caller has checked every code in this actual row. Preparing a
        // signed label reader now cannot mask the original whole-row fallback
        // for a missing, negative or out-of-range code.
        if let Some(storage) = self.prepare_signed.take()
            && storage.get_type().is(numpy_extension)
        {
            crate::input::check(py, budget)?;
            let keywords = errors::dictionary(py)?;
            keywords.set_item(pyo3::types::PyString::from_bytes(py, b"copy")?, false)?;
            let converted = call_keywords(&storage, c"to_numpy", &keywords)?;
            crate::input::check(py, budget)?;
            if api.accepts(&converted) {
                let dtype = errors::attribute(&converted, c"dtype")?;
                if errors::attribute(&dtype, c"kind")?.extract::<&str>()? == "i"
                    && matches!(
                        errors::attribute(&dtype, c"itemsize")?.extract::<usize>()?,
                        1 | 2 | 4 | 8
                    )
                {
                    self.signed = Some(crate::input::boxed(
                        py,
                        numpy_source(api, &converted, budget, self.length)?,
                    )?);
                }
            }
        }
        if let Some(source) = &mut self.signed {
            return Ok(OwnedLeaf::Signed(source.signed_at(position, budget)?));
        }
        self.owner
            .get_item(crate::input::python_index(py, position)?)
            .map(OwnedLeaf::Python)
    }
}

/// At most eight current-row leaves, all acquired before any tuple output or
/// conversion. This is neither a recursive value tree nor a SQL/value cache.
enum OwnedLeaf<'py> {
    Signed(i64),
    Python(Bound<'py, PyAny>),
}

impl OwnedLeaf<'_> {
    fn write(
        &self,
        py: Python<'_>,
        index: usize,
        output: &mut Vec<u8>,
        context: &Context,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        match self {
            Self::Python(value) => visitor.python(value, output, Some(index)),
            Self::Signed(value) => {
                visitor.push_index(py, index)?;
                let result = visitor.checkpoint(py, 1).and_then(|()| {
                    scalar::integer(output, *value, context.limit)
                        .map_err(|error| errors::wire(py, error))
                });
                if result.is_ok() {
                    visitor.pop_index();
                }
                result
            }
        }
    }
}

struct Multi<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    levels: Vec<MultiLevel<'py>>,
    numpy: NumpyArrayApi,
    numpy_extension: Bound<'py, PyType>,
}

impl<'py> PreparedSource<'py> for Multi<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        Shape::Vector {
            length: self.length,
        }
    }

    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        if row >= self.length {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "Pandas row is outside its prepared shape",
            ));
        }
        if column != 0 {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "Pandas column is outside its prepared shape",
            ));
        }
        // Match the original PandasSource boundary checkpoint; the tuple
        // checkpoint below remains separate after all labels are acquired.
        visitor.checkpoint(py, 1)?;
        // Both inline buffers are bounded by the qualified eight-level plan.
        // Read this actual row only, before any label getter or SQL write.
        let mut positions = SmallVec::<[usize; 8]>::new();
        for level in &mut self.levels {
            let code = level.codes.signed_at(row, budget)?;
            let Some(position) = usize::try_from(code)
                .ok()
                .filter(|position| *position < level.length)
            else {
                // Includes -1: preserve the original np.nan leaf and all
                // diagnostics, as well as unusual negative-code semantics.
                let value = self.owner.get_item(crate::input::python_index(py, row)?)?;
                return visitor.python(&value, output, None);
            };
            positions.push(position);
        }
        let mut labels = SmallVec::<[OwnedLeaf<'_>; 8]>::new();
        for (level, position) in self.levels.iter_mut().zip(positions) {
            labels.push(level.value(position, &self.numpy, &self.numpy_extension, budget)?);
        }
        // The original MultiIndex getter obtains every label before its tuple
        // reaches Visit. Keep that order, without constructing a Python tuple.
        visitor.checkpoint(py, 1)?;
        visitor.enter_native(py)?;
        let result = (|| {
            writer::append(output, b"(", context.limit).map_err(|error| errors::wire(py, error))?;
            for (index, label) in labels.iter().enumerate() {
                if index != 0 {
                    writer::append(output, b",", context.limit)
                        .map_err(|error| errors::wire(py, error))?;
                }
                label.write(py, index, output, context, visitor)?;
            }
            writer::append(output, b")", context.limit).map_err(|error| errors::wire(py, error))
        })();
        visitor.leave_native();
        result
    }
}

struct Masked<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    dtype: Bound<'py, PyAny>,
    python_string: bool,
    standard_array: Option<Bound<'py, PyType>>,
    current: Option<Block<'py>>,
}
struct Block<'py> {
    start: usize,
    length: usize,
    values: BlockValues<'py>,
}
enum BlockValues<'py> {
    Numpy(Box<NumpyBlock<'py>>),
    Arrow(Box<arrow::Prepared<'py>>),
}
struct NumpyBlock<'py> {
    data: NumpySource<'py>,
    mask: NumpySource<'py>,
}

impl<'py> Masked<'py> {
    fn write(
        &mut self,
        row: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        let py = self.owner.py();
        if !self
            .current
            .as_ref()
            .is_some_and(|block| row >= block.start && row - block.start < block.length)
        {
            crate::input::check(py, budget)?;
            let start = row / 1024 * 1024;
            let length = (self.length - start).min(1024);
            let slice = bounded_slice(py, start, start + length)?;
            let piece = self.owner.get_item(slice)?;
            let standard = self
                .standard_array
                .as_ref()
                .is_some_and(|typ| piece.get_type().is(typ));
            // Exact standard nullable arrays may export this bounded piece.
            // Do not import an optional provider to discover whether it exists.
            // The current block owns its export; Context is read only at write.
            let values = if !self.python_string && standard && loaded_arrow(py)? {
                let exported = errors::attribute(&piece, c"__arrow_array__")?.call0()?;
                crate::input::check(py, budget)?;
                let source =
                    arrow::prepare_exported(&piece, &exported, Shape::Vector { length }, budget)?;
                crate::input::check(py, budget)?;
                BlockValues::Arrow(crate::input::boxed(py, source)?)
            } else {
                let mask = errors::attribute(&piece, c"isna")?.call0()?;
                let mask = numpy_source(api, &mask, budget, length)?;
                let keywords = errors::dictionary(py)?;
                keywords.set_item(
                    pyo3::types::PyString::from_bytes(py, b"dtype")?,
                    &self.dtype,
                )?;
                keywords.set_item(pyo3::types::PyString::from_bytes(py, b"copy")?, false)?;
                // The authoritative mask already handles missing strings.
                // Request their existing object storage without normalizing
                // NULLs again. Exact standard sources stay stable by contract;
                // real EA subclasses retain their original conversion hooks.
                if !self.python_string || !standard {
                    let missing = if self.python_string {
                        py.None().into_bound(py)
                    } else {
                        crate::input::python_index(py, 0)?
                    };
                    keywords
                        .set_item(pyo3::types::PyString::from_bytes(py, b"na_value")?, missing)?;
                }
                let data = call_keywords(&piece, c"to_numpy", &keywords)?;
                let data = numpy_source(api, &data, budget, length)?;
                crate::input::check(py, budget)?;
                BlockValues::Numpy(crate::input::boxed(py, NumpyBlock { data, mask })?)
            };
            // The small range header stays local; only the chosen payload is
            // boxed. Each route retains one fallible state allocation.
            self.current = Some(Block {
                start,
                length,
                values,
            });
        }
        let block = self
            .current
            .as_mut()
            .ok_or_else(|| invalid(py, "Pandas nullable block preparation failed"))?;
        let local = row - block.start;
        match &mut block.values {
            BlockValues::Arrow(source) => {
                source.write_cell(local, 0, output, context, budget, visitor)
            }
            BlockValues::Numpy(source) => {
                if source.mask.bool_at(local, budget)? {
                    writer::append(output, b"NULL", context.limit)
                        .map_err(|error| errors::wire(py, error))
                } else {
                    source
                        .data
                        .write_cell(local, 0, output, context, budget, visitor)
                }
            }
        }
    }
}

/// A real, already loaded optional module, not an installed distribution probe.
/// Absence and the conventional None import blocker retain the NumPy route.
/// Recheck on block preparation; a prior absence is not a permanent decision.
fn loaded_arrow(py: Python<'_>) -> PyResult<bool> {
    let sys = errors::module(py, c"sys")?;
    let modules = errors::attribute(sys.as_any(), c"modules")?.cast_into::<PyDict>()?;
    Ok(modules
        .get_item(pyo3::types::PyString::from_bytes(py, b"pyarrow")?)?
        .is_some_and(|module| module.is_instance_of::<PyModule>()))
}

fn require_vector(source: &NumpySource<'_>) -> PyResult<usize> {
    match source.shape() {
        Shape::Vector { length } => Ok(length),
        _ => Err(invalid(
            source.owner().py(),
            "Pandas storage must be one-dimensional",
        )),
    }
}
fn numpy_source<'py>(
    api: &NumpyArrayApi,
    value: &Bound<'py, PyAny>,
    budget: &IoBudget,
    expected: usize,
) -> PyResult<NumpySource<'py>> {
    let source = api.prepare(value, budget)?.ok_or_else(|| {
        invalid(
            value.py(),
            "Pandas public storage conversion must return an exact NumPy ndarray",
        )
    })?;
    if require_vector(&source)? != expected {
        return Err(invalid(
            value.py(),
            "Pandas converted storage length differs from its prepared source",
        ));
    }
    Ok(source)
}

fn converted_numpy_column<'py>(
    storage: &Bound<'py, PyAny>,
    api: &NumpyArrayApi,
    budget: &IoBudget,
    length: usize,
) -> PyResult<Column<'py>> {
    let py = storage.py();
    let keywords = errors::dictionary(py)?;
    keywords.set_item(pyo3::types::PyString::from_bytes(py, b"copy")?, false)?;
    let output = call_keywords(storage, c"to_numpy", &keywords)?;
    let source = numpy_source(api, &output, budget, length)?;
    Ok(Column::Numpy(source))
}
fn inherits(value: &Bound<'_, PyAny>, base: &Bound<'_, PyType>) -> bool {
    // SAFETY: both are live actual type objects; inspect the real MRO.
    unsafe {
        pyo3::ffi::PyType_IsSubtype(value.get_type().as_ptr().cast(), base.as_ptr().cast()) != 0
    }
}

fn native_integer(value: &Bound<'_, PyAny>) -> PyResult<Option<i128>> {
    match value.extract::<i128>() {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyOverflowError>(value.py()) => Ok(None),
        Err(error) => Err(error),
    }
}

fn invalid(py: Python<'_>, message: &str) -> PyErr {
    errors::builtin_message::<PyTypeError>(py, message)
}
fn call_keywords<'py>(
    value: &Bound<'py, PyAny>,
    method: &CStr,
    keywords: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    errors::attribute(value, method)?.call(
        crate::output::tuple_from_ready(value.py(), [])?,
        Some(keywords),
    )
}
fn full_slice(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    // SAFETY: the attached constructor retains None and reports actual NULL.
    unsafe {
        Bound::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PySlice_New(
                pyo3::ffi::Py_None(),
                pyo3::ffi::Py_None(),
                pyo3::ffi::Py_None(),
            ),
        )
    }
}
fn bounded_slice(py: Python<'_>, start: usize, end: usize) -> PyResult<Bound<'_, PyAny>> {
    let start = crate::input::python_index(py, start)?;
    let end = crate::input::python_index(py, end)?;
    // SAFETY: both endpoints are owned exact ints before the checked call.
    unsafe {
        Bound::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PySlice_New(start.as_ptr(), end.as_ptr(), pyo3::ffi::Py_None()),
        )
    }
}
fn column_error(py: Python<'_>, error: PyErr, column: usize) -> PyErr {
    let mut message = String::new();
    if message.try_reserve(96).is_ok()
        && write!(
            &mut message,
            "DataFrame column[{column}] preparation failed"
        )
        .is_ok()
    {
        errors::note(py, &error, message);
    }
    error
}
