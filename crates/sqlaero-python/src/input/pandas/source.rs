//! Public column preparation and row-wise writes, without Python row objects.

use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::cell::Target;
use crate::input::control::{self, Checkpoint};
use crate::input::sink::{self, Destination};
use crate::input::source::{CellWriter, PreparedSource, Shape};
use crate::input::temporal::TimeUnit;
use crate::input::{
    arrow,
    numpy::{NumpyArrayApi, NumpySource, NumpyTypes},
};
use pyo3::exceptions::{PyIndexError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple, PyType};
use smallvec::SmallVec;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::ValueRef;
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
    object_new: Py<PyAny>,
    native_arrays: Vec<NativeArray>,
    name_api: Py<PyTuple>,
    offset_api: Py<PyTuple>,
}

/// Retained, provider-exposed native getters. No Cython structure offsets or
/// borrowed provider pointers are inspected by this adapter.
struct NativeArray {
    typ: Py<PyType>,
    data: GetSet,
    dtype: GetSet,
    family: Family,
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
            .try_reserve_exact(10)
            .map_err(|_| errors::memory_error(py))?;
        arrays.push(super::exact_type(module, c"Categorical")?);
        for name in [
            c"IntegerArray" as &CStr,
            c"FloatingArray",
            c"BooleanArray",
            c"StringArray",
            c"ArrowStringArray",
            c"ArrowExtensionArray",
            c"DatetimeArray",
            c"TimedeltaArray",
            c"NumpyExtensionArray",
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
        let object_new = errors::attribute(py.get_type::<PyAny>().as_any(), c"__new__")?;
        let mut name_api = Vec::new();
        name_api
            .try_reserve_exact(7)
            .map_err(|_| errors::memory_error(py))?;
        name_api.push(object_new.clone().unbind());
        name_api.push(super::exact_type(module, c"MultiIndex")?.into_any());
        for base in native_indices.iter().rev() {
            name_api.push(base.clone_ref(py).into_any());
        }
        let name_api = crate::output::tuple_from_ready_vec(py, name_api)?.unbind();
        let datetime = errors::module(py, c"datetime")?;
        let offset_api = crate::output::tuple_from_ready(
            py,
            [
                super::exact_type(module, c"Timestamp")?
                    .into_bound(py)
                    .into_any(),
                errors::attribute(
                    py.get_type::<pyo3::types::PyDateTime>().as_any(),
                    c"utcoffset",
                )?,
                errors::attribute(datetime.as_any(), c"timezone")?,
            ],
        )?
        .unbind();
        let mut native_arrays = Vec::new();
        native_arrays
            .try_reserve_exact(5)
            .map_err(|_| errors::memory_error(py))?;
        for (index, family) in [
            (0, Family::Categorical),
            (4, Family::PythonString),
            (7, Family::Numpy),
            (8, Family::Numpy),
            (9, Family::Numpy),
        ] {
            let typ = arrays[index].bind(py);
            native_arrays.push(NativeArray {
                typ: typ.clone().unbind(),
                data: GetSet::new(typ, c"_ndarray", None)?,
                dtype: GetSet::new(typ, c"_dtype", None)?,
                family,
            });
        }
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
            object_new: object_new.unbind(),
            native_arrays,
            name_api,
            offset_api,
        })
    }

    fn receiver<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        base: &Bound<'py, PyType>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyAny>> {
        receiver_view(value, base, self.object_new.bind(value.py()), budget)
    }

    pub(super) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        numpy: &NumpyTypes,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
        let py = value.py();
        let original = value.clone();
        let is_frame = inherits(value, self.frame.bind(py));
        let is_series = inherits(value, self.series.bind(py));
        let is_index = inherits(value, self.index.bind(py));
        let is_array = self
            .arrays
            .iter()
            .any(|array| inherits(value, array.bind(py)));
        if !is_frame && !is_series && !is_index && !is_array {
            return Ok(None);
        }
        crate::input::check(py, budget)?;
        let api = numpy.array_api(py);
        if is_array {
            let column = self.column(value, &api, budget)?;
            let length = column.length()?;
            let mut columns = Vec::new();
            columns
                .try_reserve_exact(1)
                .map_err(|_| errors::memory_error(py))?;
            columns.push(column);
            return Ok(Some(crate::input::boxed(
                py,
                PandasSource {
                    owner: original,
                    receiver: value.clone(),
                    names: None,
                    shape: Shape::Vector { length },
                    columns,
                    numpy: api,
                    name_api: None,
                },
            )?));
        }
        let base = if is_frame {
            self.frame.bind(py)
        } else if is_series {
            self.series.bind(py)
        } else if inherits(value, self.multi.bind(py)) {
            self.multi.bind(py)
        } else {
            self.native_indices
                .iter()
                .rev()
                .find(|base| inherits(value, base.bind(py)))
                .ok_or_else(|| invalid(py, "Pandas index has no supported base storage"))?
                .bind(py)
        };
        let receiver = self.receiver(value, base, budget)?;
        let value = &receiver;
        let typ = value.get_type();
        let (shape, columns) = if is_frame {
            self.frame(value, &api, budget)?
        } else {
            // In particular, native PeriodIndex/IntervalIndex and a Sparse
            // Series must not enter the subclass values path and hide their
            // unsupported declared schema behind an empty object ndarray.
            self.family(&errors::attribute(value, c"dtype")?)?;
            let length = value.len()?;
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
                owner: original,
                receiver,
                names: None,
                name_api: is_frame.then(|| self.name_api.bind(py).clone()),
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
        budget: &dyn crate::input::control::Checkpoint,
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
        budget: &dyn crate::input::control::Checkpoint,
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
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<Column<'py>> {
        let py = storage.py();
        crate::input::check(py, budget)?;
        if let Some(native) = self
            .native_arrays
            .iter()
            .find(|native| inherits(storage, native.typ.bind(py)))
        {
            let data = native.data.get(storage)?;
            let dtype = native.dtype.get(storage)?;
            let source = api
                .prepare(&data, budget)?
                .ok_or_else(|| invalid(py, "canonical Pandas native storage must be an ndarray"))?;
            let length = require_vector(&source)?;
            return match native.family {
                Family::Categorical => {
                    let categories = errors::attribute(&dtype, c"categories")?;
                    let base = self
                        .native_indices
                        .iter()
                        .rev()
                        .find(|base| inherits(&categories, base.bind(py)))
                        .ok_or_else(|| {
                            invalid(py, "Pandas categorical labels have no supported Index base")
                        })?;
                    let categories = self.receiver(&categories, base.bind(py), budget)?;
                    let labels = categories.len()?;
                    Ok(Column::Categorical(crate::input::boxed(
                        py,
                        Category {
                            codes: source,
                            categories,
                            labels,
                            length,
                            prepare_native: None,
                            native: None,
                        },
                    )?))
                }
                Family::PythonString => Ok(Column::String {
                    data: source,
                    length,
                }),
                _ if self.dtypes.iter().any(|(typ, family)| {
                    matches!(family, Family::AwareTime) && dtype.get_type().is(typ.bind(py))
                }) =>
                {
                    Ok(Column::Time {
                        ticks: source,
                        length,
                        offset: crate::input::boxed(
                            py,
                            OffsetSource {
                                dtype,
                                api: self.offset_api.bind(py).clone(),
                                zone: None,
                                fixed: None,
                            },
                        )?,
                    })
                }
                _ => Ok(Column::Numpy(source)),
            };
        }
        let receiver_base = self
            .arrays
            .iter()
            .find(|array| inherits(storage, array.bind(py)));
        let receiver = if let Some(base) = receiver_base {
            self.receiver(storage, base.bind(py), budget)?
        } else {
            storage.clone()
        };
        let storage = &receiver;
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
            Family::Nullable => {
                if !self
                    .arrays
                    .iter()
                    .skip(1)
                    .take(3)
                    .any(|base| inherits(storage, base.bind(py)))
                {
                    return Err(invalid(
                        py,
                        "nullable Pandas storage requires a supported actual base array",
                    ));
                }
                let state = crate::input::numpy::MaskState::new(storage, budget)?;
                let data = crate::input::attribute::instance(storage, "_data", budget)?;
                let data = numpy_source(api, &data, budget, length)?;
                if !api.accepts(state.mask()) {
                    return Err(invalid(
                        py,
                        "Pandas nullable mask requires a matching bool ndarray",
                    ));
                }
                let mask = crate::input::numpy::MaskReader::new(state.mask(), &data, api, budget)?;
                state.validate(budget)?;
                Ok(Column::Masked(Masked {
                    data,
                    mask,
                    state,
                    length,
                }))
            }
            Family::PythonString => Err(invalid(
                py,
                "Pandas Python strings require qualified native StringArray storage",
            )),
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
            Family::AwareTime => Err(invalid(
                py,
                "aware Pandas storage requires qualified native DatetimeArray data",
            )),
        }
    }
}

pub(crate) struct PandasSource<'py> {
    owner: Bound<'py, PyAny>,
    receiver: Bound<'py, PyAny>,
    names: Option<Vec<Bound<'py, PyAny>>>,
    name_api: Option<Bound<'py, PyTuple>>,
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
    fn prepare_field_names(&mut self, budget: &dyn Checkpoint) -> PyResult<bool> {
        let Shape::Table { columns, .. } = self.shape else {
            return Ok(false);
        };
        if self.names.is_some() {
            return Ok(true);
        }
        let py = self.owner.py();
        let api = self
            .name_api
            .as_ref()
            .ok_or_else(|| invalid(py, "named Pandas table has no prepared Index API"))?;
        let mut names = Vec::new();
        names
            .try_reserve_exact(columns)
            .map_err(|_| errors::memory_error(py))?;
        if columns != 0 {
            let index = control::callback(py, Some(budget), || {
                errors::attribute(&self.receiver, c"columns")
            })?;
            let mut base = None;
            for value in api.iter().skip(1) {
                let typ = value.cast_into::<PyType>()?;
                if inherits(&index, &typ) {
                    base = Some(typ);
                    break;
                }
            }
            let base = base.ok_or_else(|| {
                invalid(py, "Pandas columns require supported actual Index storage")
            })?;
            let index = receiver_view(&index, &base, &api.get_item(0)?, budget)?;
            let length = control::callback(py, Some(budget), || index.len())?;
            if length != columns {
                return Err(invalid(
                    py,
                    "Pandas column names changed during preparation",
                ));
            }
            for column in 0..columns {
                names.push(control::callback(py, Some(budget), || {
                    index.get_item(crate::input::python_index(py, column)?)
                })?);
            }
        }
        self.names = Some(names);
        Ok(true)
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(self
            .names
            .as_ref()
            .and_then(|names| names.get(column))
            .cloned())
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        for source in &self.columns {
            target.checkpoint(self.owner.py(), 1)?;
            source.schema(target)?;
        }
        Ok(())
    }
    fn python_child(
        &mut self,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if matches!(self.shape, Shape::Vector { .. }) {
            self.columns[0].python_child(index, budget)
        } else {
            Ok(None)
        }
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell_to(row, column, target, budget)
    }
    crate::input::source::cell_destinations!();
}

enum Column<'py> {
    Numpy(NumpySource<'py>),
    Arrow(arrow::Prepared<'py>),
    Masked(Masked<'py>),
    String {
        data: NumpySource<'py>,
        length: usize,
    },
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
        length: usize,
        offset: Box<OffsetSource<'py>>,
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
            Self::String { length, .. } => Ok(*length),
            Self::Categorical(source) => Ok(source.length),
            Self::Range { length, .. }
            | Self::Python { length, .. }
            | Self::Time { length, .. } => Ok(*length),
        }
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        match self {
            Self::Numpy(source) | Self::String { data: source, .. } => source.schema(target),
            Self::Arrow(source) => source.schema(target),
            Self::Time { ticks, .. } => ticks.schema(target),
            _ => Ok(()),
        }
    }

    fn consume(
        &mut self,
        row: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        match self {
            Self::Numpy(source) => source.consume_cell(row, 0, target, budget),
            Self::Arrow(source) => source.consume_cell(row, 0, target, budget),
            Self::Masked(source) => source.consume(row, target, budget, api),
            Self::String { data, .. } => {
                let py = data.owner().py();
                let value = data.object_child(row, 0, budget)?.ok_or_else(|| {
                    invalid(py, "Pandas string storage must expose actual object slots")
                })?;
                if value.is_none()
                    || value
                        .cast::<pyo3::types::PyFloat>()
                        .is_ok_and(|number| number.value().is_nan())
                {
                    target.scalar(py, ValueRef::Null)
                } else {
                    target.python(&value, None)
                }
            }
            Self::Range { owner, native, .. } => {
                if let Some(value) = native
                    .and_then(|(start, step)| (row as i128).checked_mul(step)?.checked_add(start))
                {
                    if let Ok(value) = i64::try_from(value) {
                        return target.scalar(owner.py(), ValueRef::Signed(value));
                    }
                    if let Ok(value) = u64::try_from(value) {
                        return target.scalar(owner.py(), ValueRef::Unsigned(value));
                    }
                }
                let value = control::callback(owner.py(), Some(budget), || {
                    owner.get_item(crate::input::python_index(owner.py(), row)?)
                })?;
                target.python(&value, None)
            }
            Self::Python { owner, .. } => {
                let value = control::callback(owner.py(), Some(budget), || {
                    owner.get_item(crate::input::python_index(owner.py(), row)?)
                })?;
                target.python(&value, None)
            }
            Self::Categorical(source) => source.consume(row, target, budget, api),
            Self::Time { ticks, offset, .. } => {
                let py = ticks.owner().py();
                match ticks.temporal_at(row, budget)? {
                    ValueRef::Null => target.scalar(py, ValueRef::Null),
                    ValueRef::Ticks {
                        value,
                        unit,
                        multiplier,
                        kind: crate::input::temporal::TimeKind::DateTime,
                    } => {
                        let offset_micros = if target.needs_local_offsets() {
                            Some(offset.read(value, unit, multiplier, budget)?)
                        } else {
                            None
                        };
                        target.scalar(
                            py,
                            ValueRef::UtcTicks {
                                value,
                                unit,
                                multiplier,
                                offset_micros,
                            },
                        )
                    }
                    _ => Err(invalid(
                        py,
                        "aware Pandas array must retain actual datetime ticks",
                    )),
                }
            }
        }
    }
}

impl<'py> Column<'py> {
    fn python_child(
        &mut self,
        row: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self {
            Self::Numpy(source) => source.object_child(row, 0, budget),
            Self::Python { owner, .. } => control::callback(owner.py(), Some(budget), || {
                owner.get_item(crate::input::python_index(owner.py(), row)?)
            })
            .map(Some),
            _ => Ok(None),
        }
    }
}

/// Only a local-representation target asks for this source protocol. Exact
/// immutable datetime.timezone offsets are constant; other zones resolve each
/// actual instant, including DST. No timezone rules or results cross calls.
struct OffsetSource<'py> {
    dtype: Bound<'py, PyAny>,
    api: Bound<'py, PyTuple>,
    zone: Option<Bound<'py, PyAny>>,
    fixed: Option<i128>,
}

impl OffsetSource<'_> {
    fn read(
        &mut self,
        ticks: i64,
        unit: TimeUnit,
        multiplier: u64,
        budget: &dyn Checkpoint,
    ) -> PyResult<i128> {
        if let Some(offset) = self.fixed {
            return Ok(offset);
        }
        let py = self.dtype.py();
        if self.zone.is_none() {
            self.zone = Some(control::callback(py, Some(budget), || {
                errors::attribute(&self.dtype, c"tz")
            })?);
        }
        let zone = self
            .zone
            .as_ref()
            .ok_or_else(|| invalid(py, "aware Pandas source has no timezone owner"))?;
        let fixed = zone.get_type().is(&self.api.get_item(2)?);
        let offset = if fixed {
            control::callback(py, Some(budget), || {
                zone.call_method1(c"utcoffset", (py.None(),))
            })?
        } else {
            let unit = match unit {
                TimeUnit::Second => b"s".as_slice(),
                TimeUnit::Millisecond => b"ms".as_slice(),
                TimeUnit::Microsecond => b"us".as_slice(),
                TimeUnit::Nanosecond => b"ns".as_slice(),
                _ => {
                    return Err(invalid(
                        py,
                        "Pandas timezone resolution requires a supported actual fixed unit",
                    ));
                }
            };
            let ticks = i128::from(ticks)
                .checked_mul(i128::from(multiplier))
                .and_then(|n| i64::try_from(n).ok())
                .ok_or_else(|| {
                    invalid(py, "Pandas timezone scalar exceeds its source tick range")
                })?;
            // SAFETY: this fallible public constructor returns the exact signed
            // integer required by the one-value provider timezone protocol.
            let ticks =
                unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromLongLong(ticks)) }?;
            let keywords = errors::dictionary(py)?;
            keywords.set_item(
                pyo3::types::PyString::from_bytes(py, b"unit")?,
                pyo3::types::PyString::from_bytes(py, unit)?,
            )?;
            keywords.set_item(pyo3::types::PyString::from_bytes(py, b"tz")?, zone)?;
            let factory = self.api.get_item(0)?;
            let value = control::callback(py, Some(budget), || {
                factory.call(
                    crate::output::tuple_from_ready(py, [ticks])?,
                    Some(&keywords),
                )
            })?;
            let method = self.api.get_item(1)?;
            control::callback(py, Some(budget), || method.call1((value,)))?
        };
        if offset.is_none() {
            return Err(invalid(
                py,
                "aware Pandas source returned no actual UTC offset",
            ));
        }
        let micros = crate::input::delta(offset.cast::<pyo3::types::PyDelta>()?);
        if micros.unsigned_abs() >= 86_400_000_000 {
            return Err(invalid(
                py,
                "Pandas UTC offset must be strictly within one day",
            ));
        }
        if fixed {
            self.fixed = Some(micros);
        }
        Ok(micros)
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
    fn consume(
        &mut self,
        row: usize,
        target: &mut dyn Target,
        budget: &dyn crate::input::control::Checkpoint,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        let py = self.categories.py();
        let code = self.codes.signed_at(row, budget)?;
        if code == -1 {
            return target.scalar(py, ValueRef::Null);
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
                            return target.python(&label, None);
                        }
                        Column::Numpy(numpy_source(api, &converted, budget, self.labels)?)
                    }
                    Family::PythonString => {
                        let data = GetSet::new(&typ, c"_ndarray", Some(budget))?.get(&storage)?;
                        Column::String {
                            data: numpy_source(api, &data, budget, self.labels)?,
                            length: self.labels,
                        }
                    }
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
                            return target.python(&label, None);
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
            return source.consume(code, target, budget, api);
        }
        let label = self
            .categories
            .get_item(crate::input::python_index(py, code)?)?;
        target.python(&label, None)
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
        budget: &dyn crate::input::control::Checkpoint,
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
        output: &mut (impl Destination + ?Sized),
        context: &Context,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        match self {
            Self::Python(value) => output.python(visitor, value, Some(index)),
            Self::Signed(value) => {
                visitor.push_index(py, index)?;
                let result = visitor.checkpoint(py, 1).and_then(|()| {
                    sink::integer(output, *value, context).map_err(|error| sink::failure(py, error))
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

    crate::input::source::cell_destinations!();
}

struct Masked<'py> {
    data: NumpySource<'py>,
    mask: crate::input::numpy::MaskReader<'py>,
    state: crate::input::numpy::MaskState<'py>,
    length: usize,
}

impl Masked<'_> {
    fn consume(
        &mut self,
        row: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
        _api: &NumpyArrayApi,
    ) -> PyResult<()> {
        self.state.validate(budget)?;
        let missing = self.mask.missing(row, 0, budget)?;
        self.state.validate(budget)?;
        if missing {
            self.data.validate_layout(budget)?;
            target.scalar(self.data.owner().py(), ValueRef::Null)
        } else {
            self.data.consume_cell(row, 0, target, budget)
        }
    }
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
    budget: &dyn crate::input::control::Checkpoint,
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
    budget: &dyn crate::input::control::Checkpoint,
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

impl<'py> PandasSource<'py> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        let length = match self.shape {
            Shape::Vector { length } => length,
            Shape::Table { rows, .. } => rows,
            Shape::Record { .. } | Shape::Scalar | Shape::Tensor { .. } => {
                return Err(invalid(
                    py,
                    "Pandas source requires a vector or table shape",
                ));
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
        target.checkpoint(py, 1)?;
        source.consume(row, target, budget, &self.numpy)
    }
}

impl<'py> Multi<'py> {
    fn write_cell_to(
        &mut self,
        row: usize,
        column: usize,
        output: &mut (impl Destination + ?Sized),
        context: &Context,
        budget: &dyn crate::input::control::Checkpoint,
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
                return output.python(visitor, &value, None);
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
            output.fragment(py, b"(", context)?;
            for (index, label) in labels.iter().enumerate() {
                if index != 0 {
                    output.fragment(py, b",", context)?;
                }
                label.write(py, index, output, context, visitor)?;
            }
            output.fragment(py, b")", context)
        })();
        visitor.leave_native();
        result
    }
}

/// A short canonical receiver header shares the actual Python instance state.
/// Native-backed providers never enter this object.__new__ route.
fn receiver_view<'py>(
    value: &Bound<'py, PyAny>,
    base: &Bound<'py, PyType>,
    object_new: &Bound<'py, PyAny>,
    budget: &dyn Checkpoint,
) -> PyResult<Bound<'py, PyAny>> {
    if value.get_type().is(base) {
        return Ok(value.clone());
    }
    let py = value.py();
    control::callback(py, Some(budget), || {
        // object.__new__ refuses native bases requiring additional C
        // state. This route is only used for known Python-state Pandas
        // bases; native-backed arrays use retained public getters below.
        let view = object_new.call1((base,))?;
        // SAFETY: documented object dictionary APIs return/retain owning
        // references and bypass child attribute hooks. The exact base
        // receiver shares the original state, not a copied value array or
        // alternative value. No provider-private memory layout is read.
        let dictionary = unsafe {
            Bound::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )
        }?;
        // SAFETY: view is a fresh exact Python-state base receiver and
        // dictionary is an owning actual dict from the public getter above.
        let status = unsafe {
            pyo3::ffi::PyObject_GenericSetDict(
                view.as_ptr(),
                dictionary.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(PyErr::fetch(py));
        }
        Ok(view)
    })
}
