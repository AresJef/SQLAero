//! Public column preparation and row-wise writes, without Python row objects.

use crate::errors;
use crate::input::attribute::{GetSet, receiver_view};
use crate::input::cell::Target;
use crate::input::control::{self, Checkpoint};
use crate::input::source::{PreparedSource, Shape};
use crate::input::{
    arrow,
    numpy::{NumpyArrayApi, NumpySource, NumpyTypes},
};
use pyo3::exceptions::{PyIndexError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple, PyType};
use sqlaero_core::bind::value::ValueRef;
use sqlaero_transcode::temporal::TimeUnit;
use std::ffi::CStr;

mod multi;
use std::fmt::Write;

/// Fixed public dtype families. Unsupported extension schemas never disappear
/// merely because an array is empty or every value is missing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Nullable,
    PythonString,
    Arrow,
    Categorical,
    AwareTime,
    Numpy,
}

#[pyclass(frozen, module = "sqlaero._sqlaero", name = "_PandasSources")]
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
        catalog: &Py<Self>,
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
            let column = self.column(value, &api, budget, catalog)?;
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
            self.frame(value, &api, budget, catalog)?
        } else {
            // In particular, native PeriodIndex/IntervalIndex and a Sparse
            // Series must not enter the subclass values path and hide their
            // unsupported declared schema behind an empty object ndarray.
            self.family(&errors::attribute(value, c"dtype")?)?;
            let length = value.len()?;
            let column = if typ.is(self.range.bind(py)) {
                self.index_column(value, &api, budget, catalog)?
            } else if typ.is(self.multi.bind(py)) {
                return Ok(Some(crate::input::boxed(
                    py,
                    multi::prepare(catalog, original, value.clone(), length, api, budget)?,
                )?));
            } else if is_series || is_index {
                let storage = errors::attribute(value, c"array")?;
                self.column(&storage, &api, budget, catalog)?
            } else {
                self.column(value, &api, budget, catalog)?
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

    fn frame<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn crate::input::control::Checkpoint,
        catalog: &Py<Self>,
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
                .column(&storage, api, budget, catalog)
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

    fn index_column<'py>(
        &self,
        index: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        catalog: &Py<Self>,
    ) -> PyResult<Column<'py>> {
        let py = index.py();
        if index.get_type().is(self.range.bind(py)) {
            let length = control::callback(py, Some(budget), || index.len())?;
            let start = control::callback(py, Some(budget), || errors::attribute(index, c"start"))?;
            let step = control::callback(py, Some(budget), || errors::attribute(index, c"step"))?;
            let native = match native_integer(&start)? {
                Some(start) => native_integer(&step)?.map(|step| (start, step)),
                None => None,
            };
            return Ok(Column::Range {
                owner: index.clone(),
                length,
                native,
            });
        }
        let storage = control::callback(py, Some(budget), || errors::attribute(index, c"array"))?;
        self.column(&storage, api, budget, catalog)
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

    /// Admit declared label families before looking at codes. Missing rows do
    /// not erase an unsupported schema, and no unused label array is read.
    fn validate_declared(
        &self,
        dtype: &Bound<'_, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Family> {
        let root = self.family(dtype)?;
        if root != Family::Categorical {
            return Ok(root);
        }
        let py = dtype.py();
        let mut current = dtype.clone();
        let mut retained = Vec::new();
        let mut seen = std::collections::HashSet::new();
        loop {
            crate::input::check(py, budget)?;
            seen.try_reserve(1).map_err(|_| errors::memory_error(py))?;
            if !seen.insert(current.as_ptr() as usize) {
                return Err(invalid(
                    py,
                    "declared Pandas categorical schema contains a cycle",
                ));
            }
            retained
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            retained.push(current.clone());
            if self.family(&current)? != Family::Categorical {
                return Ok(root);
            }
            let categories = control::callback(py, Some(budget), || {
                errors::attribute(&current, c"categories")
            })?;
            let base = self
                .native_indices
                .iter()
                .rev()
                .find(|base| inherits(&categories, base.bind(py)))
                .ok_or_else(|| {
                    invalid(py, "Pandas categorical labels have no supported Index base")
                })?;
            let receiver = self.receiver(&categories, base.bind(py), budget)?;
            current =
                control::callback(py, Some(budget), || errors::attribute(&receiver, c"dtype"))?;
        }
    }

    fn column<'py>(
        &self,
        storage: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn crate::input::control::Checkpoint,
        catalog: &Py<Self>,
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
                    let label_dtype = control::callback(py, Some(budget), || {
                        errors::attribute(&categories, c"dtype")
                    })?;
                    self.validate_declared(&label_dtype, budget)?;
                    let labels = categories.len()?;
                    Ok(Column::Categorical(crate::input::boxed(
                        py,
                        Category {
                            codes: source,
                            categories,
                            labels,
                            length,
                            catalog: catalog.clone_ref(py),
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
                self.validate_declared(&dtype, budget)?;
                let labels = categories.len()?;
                let codes = numpy_source(api, &codes, budget, length)?;
                // Standard Index values may use the already validated native
                // storage contract. Object labels and real Index subclasses
                // retain their actual scalar getter and conversion protocols.
                Ok(Column::Categorical(crate::input::boxed(
                    py,
                    Category {
                        codes,
                        categories,
                        labels,
                        length,
                        catalog: catalog.clone_ref(py),
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
            self.columns[0].python_child(index, budget, &self.numpy)
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
            Self::Range { length, .. } | Self::Time { length, .. } => Ok(*length),
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
            Self::Categorical(source) => source.consume(row, target, budget, api),
            Self::Time { ticks, offset, .. } => {
                let py = ticks.owner().py();
                match ticks.temporal_at(row, budget)? {
                    ValueRef::Null => target.scalar(py, ValueRef::Null),
                    ValueRef::Ticks {
                        value,
                        unit,
                        multiplier,
                        kind: sqlaero_transcode::temporal::TimeKind::DateTime,
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
    /// Acquire only the current row's owned facts before any target output.
    /// Native variable-width sources remain retained projections; numeric and
    /// Object storage is copied/owned before a later source protocol can run.
    fn capture(&mut self, row: usize, budget: &dyn Checkpoint) -> PyResult<multi::Leaf<'py>> {
        match self {
            Self::Numpy(data) => {
                if data.is_object() {
                    return data
                        .object_child(row, 0, budget)?
                        .map(multi::Leaf::Python)
                        .ok_or_else(|| {
                            invalid(
                                data.owner().py(),
                                "MultiIndex Object label has no actual owning reference",
                            )
                        });
                }
                Ok(data
                    .scalar_at(row, 0, budget)?
                    .map_or(multi::Leaf::Deferred(row), multi::Leaf::Native))
            }
            Self::String { data, .. } => {
                let value = data.object_child(row, 0, budget)?.ok_or_else(|| {
                    invalid(
                        data.owner().py(),
                        "Pandas string label has no actual owning reference",
                    )
                })?;
                if value.is_none()
                    || value
                        .cast::<pyo3::types::PyFloat>()
                        .is_ok_and(|number| number.value().is_nan())
                {
                    Ok(multi::Leaf::Native(ValueRef::Null))
                } else {
                    Ok(multi::Leaf::Python(value))
                }
            }
            Self::Masked(source) => {
                source.state.validate(budget)?;
                let missing = source.mask.missing(row, 0, budget)?;
                source.state.validate(budget)?;
                if missing {
                    source.data.validate_layout(budget)?;
                    Ok(multi::Leaf::Native(ValueRef::Null))
                } else {
                    Ok(source
                        .data
                        .scalar_at(row, 0, budget)?
                        .map_or(multi::Leaf::Deferred(row), multi::Leaf::Native))
                }
            }
            _ => Ok(multi::Leaf::Deferred(row)),
        }
    }

    fn python_child(
        &mut self,
        row: usize,
        budget: &dyn Checkpoint,
        api: &NumpyArrayApi,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self {
            Self::Numpy(source) => source.object_child(row, 0, budget),
            Self::Categorical(source) => source.python_child(row, budget, api),
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
    catalog: Py<SourceTypes>,
    native: Option<Box<Column<'py>>>,
}

impl<'py> Category<'py> {
    fn code(&mut self, row: usize, budget: &dyn Checkpoint) -> PyResult<Option<usize>> {
        let py = self.categories.py();
        let code = self.codes.signed_at(row, budget)?;
        if code == -1 {
            return Ok(None);
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
        Ok(Some(code))
    }

    fn python_child(
        &mut self,
        row: usize,
        budget: &dyn Checkpoint,
        api: &NumpyArrayApi,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        let Some(code) = self.code(row, budget)? else {
            return Ok(None);
        };
        self.prepare_labels(api, budget)?;
        self.native
            .as_mut()
            .ok_or_else(|| {
                invalid(
                    self.categories.py(),
                    "Pandas categorical labels were not prepared",
                )
            })?
            .python_child(code, budget, api)
    }

    fn prepare_labels(&mut self, api: &NumpyArrayApi, budget: &dyn Checkpoint) -> PyResult<()> {
        if self.native.is_some() {
            return Ok(());
        }
        let py = self.categories.py();
        // Retain the single module/interpreter-owned registry; this source has
        // neither a copied type catalog nor an alternative label-value reader.
        let mut source = self.catalog.bind(py).get().index_column(
            &self.categories,
            api,
            budget,
            &self.catalog,
        )?;
        if source.length()? != self.labels {
            return Err(invalid(
                py,
                "Pandas categorical label storage length changed during encoding",
            ));
        }
        if let Column::Arrow(source) = &mut source {
            source.enable_random(budget)?;
        }
        self.native = Some(crate::input::boxed(py, source)?);
        Ok(())
    }

    fn consume(
        &mut self,
        row: usize,
        target: &mut dyn Target,
        budget: &dyn crate::input::control::Checkpoint,
        api: &NumpyArrayApi,
    ) -> PyResult<()> {
        let py = self.categories.py();
        let Some(code) = self.code(row, budget)? else {
            return target.scalar(py, ValueRef::Null);
        };
        self.prepare_labels(api, budget)?;
        self.native
            .as_mut()
            .ok_or_else(|| invalid(py, "Pandas categorical labels were not prepared"))?
            .consume(code, target, budget, api)
    }
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
