//! Public Polars C exports, with Object values separated before any export.

use crate::input::cell::Target;
use crate::input::control::{self, Checkpoint};
use std::ffi::CStr;
use std::sync::OnceLock;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyType};
use sqlaero_core::error::ClientErrorCode;

use super::arrow;
use super::source::{Child, PreparedSource, Selected, Shape};
use crate::errors;

#[expect(
    clippy::large_enum_variant,
    reason = "native columns are the ordinary case; retain the existing inline reader rather than add a heap allocation and indirection to every Polars column"
)]
enum Column<'py> {
    Native(arrow::Prepared<'py>),
    Object(Bound<'py, PyAny>),
}

#[expect(
    clippy::large_enum_variant,
    reason = "the prepared source is already boxed at its outer boundary; keep its ordinary native reader inline without a second heap allocation"
)]
enum Storage<'py> {
    Native(arrow::Prepared<'py>),
    Object(Bound<'py, PyAny>),
    Columns(Vec<Column<'py>>),
}

/// The original source retains identity; private Polars COW shells retain the
/// schema/column selection without copying their value buffers or SQL rows.
pub(crate) struct Prepared<'py> {
    storage: Storage<'py>,
    shape: Shape,
    owner: Bound<'py, PyAny>,
    names: Option<arrow::names::Source<'py>>,
}

struct Dtypes {
    primitive: Vec<Py<PyAny>>,
    list: Py<PyAny>,
    array: Py<PyAny>,
    structure: Py<PyAny>,
    object: Py<PyAny>,
}

/// Immutable identities owned by this interpreter's ScalarTypes. Entry types
/// are prepared separately so an unrelated value does not prepare dtype APIs.
pub(crate) struct PolarsTypes {
    module: Py<PyAny>,
    series: Py<PyType>,
    frame: Py<PyType>,
    object_new: Py<PyAny>,
    dtypes: OnceLock<Dtypes>,
}

fn invalid(py: Python<'_>, message: impl AsRef<str>) -> PyErr {
    errors::builtin_message::<PyTypeError>(py, message)
}

fn export_name(
    name: &Bound<'_, PyAny>,
    location: std::fmt::Arguments<'_>,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    if super::unicode::contains_nul(name.cast::<PyString>()?, budget)? {
        return Err(errors::builtin_format::<PyTypeError>(
            name.py(),
            format_args!(
                "Polars {location} contains NUL; the supported native C Data exporter cannot safely export this name and can abort the process. The source was rejected before export; its name and values were not changed"
            ),
        ));
    }
    Ok(())
}

impl Dtypes {
    fn new(module: &Bound<'_, PyAny>) -> PyResult<Self> {
        const NATIVE: &[&CStr] = &[
            c"Int8",
            c"Int16",
            c"Int32",
            c"Int64",
            c"UInt8",
            c"UInt16",
            c"UInt32",
            c"UInt64",
            c"Float32",
            c"Float64",
            c"Boolean",
            c"Null",
            c"String",
            c"Binary",
            c"Date",
            c"Time",
            c"Datetime",
            c"Duration",
            c"Decimal",
            c"Categorical",
            c"Enum",
        ];
        let mut primitive = Vec::new();
        primitive
            .try_reserve(NATIVE.len())
            .map_err(|_| errors::memory_error(module.py()))?;
        for name in NATIVE {
            primitive.push(errors::attribute(module, name)?.unbind());
        }
        Ok(Self {
            primitive,
            list: errors::attribute(module, c"List")?.unbind(),
            array: errors::attribute(module, c"Array")?.unbind(),
            structure: errors::attribute(module, c"Struct")?.unbind(),
            object: errors::attribute(module, c"Object")?.unbind(),
        })
    }

    /// Return true only for an explicitly permitted top-level Object column.
    fn validate(
        &self,
        dtype: &Bound<'_, PyAny>,
        top: bool,
        depth: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        let py = dtype.py();
        super::check(py, budget)?;
        if depth >= 64 {
            return Err(errors::client(
                py,
                ClientErrorCode::ContainerNesting,
                "Polars dtype exceeds 64 nested container levels",
            ));
        }
        let base = dtype.call_method0(c"base_type")?;
        if self.primitive.iter().any(|kind| base.is(kind.bind(py))) {
            return Ok(false);
        }
        if base.is(self.object.bind(py)) {
            return if top {
                Ok(true)
            } else {
                Err(invalid(py, "nested Polars Object input is not supported"))
            };
        }
        if base.is(self.list.bind(py)) || base.is(self.array.bind(py)) {
            self.validate(
                &errors::attribute(dtype, c"inner")?,
                false,
                depth + 1,
                budget,
            )?;
            return Ok(false);
        }
        if base.is(self.structure.bind(py)) {
            let fields = errors::attribute(dtype, c"fields")?.cast_into::<PyList>()?;
            for index in 0..fields.len() {
                let field = fields.get_item(index)?;
                let name =
                    control::callback(py, Some(budget), || errors::attribute(&field, c"name"))?;
                export_name(
                    &name,
                    format_args!("schema field {index} at depth {depth}"),
                    budget,
                )?;
                self.validate(
                    &errors::attribute(&field, c"dtype")?,
                    false,
                    depth + 1,
                    budget,
                )?;
            }
            return Ok(false);
        }
        Err(invalid(py, "unsupported Polars logical input dtype"))
    }
}

impl PolarsTypes {
    /// Discover only an already-loaded provider. Missing/None modules do not
    /// become a negative cache, and real lookup exceptions remain unchanged.
    pub(crate) fn loaded(py: Python<'_>) -> PyResult<Option<Self>> {
        // SAFETY: this attached thread receives a borrowed live sys.modules dict;
        // Bound obtains its own reference and uses safe owned dictionary lookups.
        let modules = unsafe { Bound::from_borrowed_ptr(py, pyo3::ffi::PyImport_GetModuleDict()) }
            .cast_into::<PyDict>()?;
        let Some(module) = modules.get_item(PyString::from_bytes(py, b"polars")?)? else {
            return Ok(None);
        };
        if module.is_none() {
            return Ok(None);
        }
        let series_type = errors::attribute(&module, c"Series")?;
        let frame_type = errors::attribute(&module, c"DataFrame")?;
        Ok(Some(Self {
            module: module.unbind(),
            series: series_type.cast_into::<PyType>()?.unbind(),
            frame: frame_type.cast_into::<PyType>()?.unbind(),
            object_new: errors::attribute(py.get_type::<PyAny>().as_any(), c"__new__")?.unbind(),
            dtypes: OnceLock::new(),
        }))
    }

    fn dtypes(&self, py: Python<'_>, budget: &dyn Checkpoint) -> PyResult<&Dtypes> {
        if let Some(types) = self.dtypes.get() {
            return Ok(types);
        }
        super::check(py, budget)?;
        // All Python work and candidate cleanup happen outside OnceLock's
        // short publication. In a race only a complete candidate is visible.
        let candidate = Dtypes::new(self.module.bind(py))?;
        super::check(py, budget)?;
        let _ = self.dtypes.set(candidate);
        self.dtypes
            .get()
            .ok_or_else(|| invalid(py, "Polars dtype identities were not published"))
    }

    /// Reuse immutable identities, but resolve actual data, methods, dtype
    /// structure and export ownership anew for every independent call.
    pub(crate) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Prepared<'py>>> {
        let py = value.py();
        super::check(py, budget)?;
        let series_type = self.series.bind(py);
        let frame_type = self.frame.bind(py);
        let dtype = value.get_type();
        let series = dtype.is(series_type) || super::actual_instance(value, series_type);
        if !series && !dtype.is(frame_type) && !super::actual_instance(value, frame_type) {
            return Ok(None);
        }

        // Public clone creates a private COW header, not a full data snapshot. The
        // metadata inspected below and the export come from that same header.
        let base = if series { series_type } else { frame_type };
        let receiver =
            super::attribute::receiver_view(value, base, self.object_new.bind(py), budget)?;
        let snapshot = control::callback(py, Some(budget), || receiver.call_method0(c"clone"))?;
        if !snapshot.get_type().is(base) {
            return Err(invalid(py, "Polars clone returned a different source type"));
        }
        let types = self.dtypes(py, budget)?;
        let mut names = None;
        let (shape, storage) = if series {
            let shape = Shape::Vector {
                length: control::callback(py, Some(budget), || snapshot.len())?,
            };
            let dtype =
                control::callback(py, Some(budget), || errors::attribute(&snapshot, c"dtype"))?;
            let storage = if types.validate(&dtype, true, 0, budget)? {
                Storage::Object(snapshot)
            } else {
                let name =
                    control::callback(py, Some(budget), || errors::attribute(&snapshot, c"name"))?;
                export_name(&name, format_args!("Series name"), budget)?;
                let mut source = arrow::prepare_exported(value, &snapshot, shape, budget)?;
                source.polars_names(dtype, None);
                Storage::Native(source)
            };
            (shape, storage)
        } else {
            let rows: usize = control::callback(py, Some(budget), || {
                errors::attribute(&snapshot, c"height")?.extract()
            })?;
            let columns: usize = control::callback(py, Some(budget), || {
                errors::attribute(&snapshot, c"width")?.extract()
            })?;
            let shape = Shape::Table { rows, columns };
            let dtypes =
                control::callback(py, Some(budget), || errors::attribute(&snapshot, c"dtypes"))?
                    .cast_into::<PyList>()?;
            if dtypes.len() != columns {
                return Err(invalid(py, "Polars dtype count does not match its width"));
            }
            let mut object = false;
            for index in 0..columns {
                object |= types.validate(&dtypes.get_item(index)?, true, 0, budget)?;
            }
            if object {
                let mut prepared_names =
                    arrow::names::Source::polars(dtypes.clone().into_any(), Some(snapshot.clone()));
                let sources =
                    control::callback(py, Some(budget), || snapshot.call_method0(c"get_columns"))?
                        .cast_into::<PyList>()?;
                if sources.len() != columns {
                    return Err(invalid(py, "Polars column count does not match its width"));
                }
                let mut prepared = Vec::new();
                prepared
                    .try_reserve(columns)
                    .map_err(|_| errors::memory_error(py))?;
                for index in 0..columns {
                    let column = sources.get_item(index)?;
                    if !column.get_type().is(series_type)
                        || control::callback(py, Some(budget), || column.len())? != rows
                    {
                        return Err(invalid(
                            py,
                            "Polars column does not match its prepared table",
                        ));
                    }
                    let dtype = control::callback(py, Some(budget), || {
                        errors::attribute(&column, c"dtype")
                    })?;
                    prepared_names.bind_polars_column(index, column.clone(), budget)?;
                    prepared.push(if types.validate(&dtype, true, 0, budget)? {
                        Column::Object(column)
                    } else {
                        let name = prepared_names.acquire_field(index, budget)?;
                        export_name(
                            &name,
                            format_args!("column name at position {index}"),
                            budget,
                        )?;
                        let mut source = arrow::prepare_exported(
                            &column,
                            &column,
                            Shape::Vector { length: rows },
                            budget,
                        )?;
                        source.polars_names(dtype, None);
                        Column::Native(source)
                    });
                }
                names = Some(prepared_names);
                (shape, Storage::Columns(prepared))
            } else {
                let mut prepared_names =
                    arrow::names::Source::polars(dtypes.into_any(), Some(snapshot.clone()));
                for index in 0..columns {
                    let name = prepared_names.acquire_field(index, budget)?;
                    export_name(
                        &name,
                        format_args!("column name at position {index}"),
                        budget,
                    )?;
                }
                let mut source = arrow::prepare_exported(value, &snapshot, shape, budget)?;
                source.source_names(prepared_names);
                (shape, Storage::Native(source))
            }
        };
        super::check(py, budget)?;
        Ok(Some(Prepared {
            storage,
            shape,
            owner: value.clone(),
            names,
        }))
    }
}

impl<'py> PreparedSource<'py> for Prepared<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        self.shape
    }

    fn prepare_field_names(&mut self, budget: &dyn Checkpoint) -> PyResult<bool> {
        if let Storage::Native(source) = &mut self.storage {
            return source.prepare_field_names(budget);
        }
        if let (Shape::Table { columns, .. }, Some(names)) = (self.shape, &mut self.names) {
            names.prepare_fields(0, columns, budget)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        if let Storage::Native(source) = &self.storage {
            return source.field_name(column);
        }
        self.names
            .as_ref()
            .map(|names| names.field(0, column))
            .transpose()
            .map(Option::flatten)
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        match &self.storage {
            Storage::Native(source) => source.schema(target),
            Storage::Object(_) => Ok(()),
            Storage::Columns(columns) => {
                for column in columns {
                    target.checkpoint(self.owner.py(), 1)?;
                    if let Column::Native(source) = column {
                        source.schema(target)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn child<'a>(&'a mut self, index: usize, budget: &dyn Checkpoint) -> PyResult<Child<'a, 'py>> {
        if matches!(self.storage, Storage::Columns(_)) {
            let shape = self.child_shape(index)?;
            return Ok(Child::Native(Selected::new(self, index, shape)));
        }
        match &mut self.storage {
            Storage::Native(source) => source.child(index, budget),
            Storage::Object(series) => Ok(Child::Python(object_value(series, index, budget)?)),
            Storage::Columns(_) => {
                unreachable!("column rows were selected before borrowing storage")
            }
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

fn object_value<'py>(
    series: &Bound<'py, PyAny>,
    row: usize,
    budget: &dyn Checkpoint,
) -> PyResult<Bound<'py, PyAny>> {
    super::check(series.py(), budget)?;
    // SAFETY: the attached constructor returns an owned exact int or its
    // original error; no infallible Rust-tuple argument conversion is used.
    let index = unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(series.py(), pyo3::ffi::PyLong_FromSize_t(row))?
    };
    let arguments = crate::output::tuple_from_ready(series.py(), [index])?;
    control::callback(series.py(), Some(budget), || {
        series.call_method1(c"item", arguments)
    })
}

impl<'py> Prepared<'py> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        target.checkpoint(py, 1)?;
        match &mut self.storage {
            Storage::Native(source) => source.consume_cell(row, column, target, budget),
            Storage::Object(series) => {
                if column != 0 {
                    return Err(invalid(py, "Polars Series has only one cell per row"));
                }
                target.python(&object_value(series, row, budget)?, None)
            }
            Storage::Columns(columns) => {
                let column = columns
                    .get_mut(column)
                    .ok_or_else(|| invalid(py, "Polars column is outside its prepared table"))?;
                match column {
                    Column::Native(source) => source.consume_cell(row, 0, target, budget),
                    Column::Object(series) => {
                        target.python(&object_value(series, row, budget)?, None)
                    }
                }
            }
        }
    }
}
