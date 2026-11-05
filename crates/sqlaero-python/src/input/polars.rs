//! Public Polars C exports, with Object values separated before any export.

use std::ffi::CStr;
use std::sync::OnceLock;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};
use sqlaero_core::bind::Context;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;

use super::arrow;
use super::source::{CellWriter, PreparedSource, Shape};
use crate::errors;

enum Column<'py> {
    Native(arrow::Prepared<'py>),
    Object(Bound<'py, PyAny>),
}

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
    series: Py<PyAny>,
    frame: Py<PyAny>,
    dtypes: OnceLock<Dtypes>,
}

fn invalid(py: Python<'_>, message: impl AsRef<str>) -> PyErr {
    errors::builtin_message::<PyTypeError>(py, message)
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
        budget: &IoBudget,
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
            series: series_type.unbind(),
            frame: frame_type.unbind(),
            dtypes: OnceLock::new(),
        }))
    }

    fn dtypes(&self, py: Python<'_>, budget: &IoBudget) -> PyResult<&Dtypes> {
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
        budget: &IoBudget,
    ) -> PyResult<Option<Prepared<'py>>> {
        let py = value.py();
        super::check(py, budget)?;
        let series_type = self.series.bind(py);
        let frame_type = self.frame.bind(py);
        let dtype = value.get_type();
        let series = dtype.is(series_type);
        if !series && !dtype.is(frame_type) {
            return Ok(None);
        }

        // Public clone creates a private COW header, not a full data snapshot. The
        // metadata inspected below and the export come from that same header.
        let snapshot = value.call_method0(c"clone")?;
        super::check(py, budget)?;
        if !snapshot.get_type().is(&dtype) {
            return Err(invalid(py, "Polars clone returned a different source type"));
        }
        let types = self.dtypes(py, budget)?;
        let (shape, storage) = if series {
            let shape = Shape::Vector {
                length: snapshot.len()?,
            };
            let dtype = errors::attribute(&snapshot, c"dtype")?;
            let storage = if types.validate(&dtype, true, 0, budget)? {
                Storage::Object(snapshot)
            } else {
                Storage::Native(arrow::prepare_exported(value, &snapshot, shape, budget)?)
            };
            (shape, storage)
        } else {
            let rows: usize = errors::attribute(&snapshot, c"height")?.extract()?;
            let columns: usize = errors::attribute(&snapshot, c"width")?.extract()?;
            let shape = Shape::Table { rows, columns };
            let dtypes = errors::attribute(&snapshot, c"dtypes")?.cast_into::<PyList>()?;
            if dtypes.len() != columns {
                return Err(invalid(py, "Polars dtype count does not match its width"));
            }
            let mut object = false;
            for index in 0..columns {
                object |= types.validate(&dtypes.get_item(index)?, true, 0, budget)?;
            }
            if object {
                let sources = snapshot
                    .call_method0(c"get_columns")?
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
                    if !column.get_type().is(series_type) || column.len()? != rows {
                        return Err(invalid(
                            py,
                            "Polars column does not match its prepared table",
                        ));
                    }
                    let dtype = errors::attribute(&column, c"dtype")?;
                    prepared.push(if types.validate(&dtype, true, 0, budget)? {
                        Column::Object(column)
                    } else {
                        Column::Native(arrow::prepare_exported(
                            &column,
                            &column,
                            Shape::Vector { length: rows },
                            budget,
                        )?)
                    });
                }
                (shape, Storage::Columns(prepared))
            } else {
                (
                    shape,
                    Storage::Native(arrow::prepare_exported(value, &snapshot, shape, budget)?),
                )
            }
        };
        super::check(py, budget)?;
        Ok(Some(Prepared {
            storage,
            shape,
            owner: value.clone(),
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

    crate::input::source::cell_destinations!();
}

fn write_object(
    series: &Bound<'_, PyAny>,
    row: usize,
    output: &mut (impl crate::input::sink::Destination + ?Sized),
    budget: &IoBudget,
    visitor: &mut dyn CellWriter,
) -> PyResult<()> {
    super::check(series.py(), budget)?;
    // SAFETY: the attached constructor returns an owned exact int or its
    // original error; no infallible Rust-tuple argument conversion is used.
    let index = unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(series.py(), pyo3::ffi::PyLong_FromSize_t(row))?
    };
    let arguments = crate::output::tuple_from_ready(series.py(), [index])?;
    let value = series.call_method1(c"item", arguments)?;
    super::check(series.py(), budget)?;
    output.python(visitor, &value, None)
}

impl<'py> Prepared<'py> {
    fn write_cell_to(
        &mut self,
        row: usize,
        column: usize,
        output: &mut (impl crate::input::sink::Destination + ?Sized),
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        visitor.checkpoint(py, 1)?;
        match &mut self.storage {
            Storage::Native(source) => {
                output.source_cell(source, row, column, context, budget, visitor)
            }
            Storage::Object(series) => {
                if column != 0 {
                    return Err(invalid(py, "Polars Series has only one cell per row"));
                }
                write_object(series, row, output, budget, visitor)
            }
            Storage::Columns(columns) => {
                let column = columns
                    .get_mut(column)
                    .ok_or_else(|| invalid(py, "Polars column is outside its prepared table"))?;
                match column {
                    Column::Native(source) => {
                        output.source_cell(source, row, 0, context, budget, visitor)
                    }
                    Column::Object(series) => write_object(series, row, output, budget, visitor),
                }
            }
        }
    }
}
