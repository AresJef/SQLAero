//! Canonical records retain schema field order and their original identity.

use super::NumpyArrayApi;
use super::NumpySource;
use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::cell::Target;
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use pyo3::exceptions::{PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

/// Public field views retain the actual owner. Every leaf reuses the checked
/// native reader; no per-numeric-field Python scalar or value tree is built.
pub(crate) struct RecordSource<'py> {
    owner: Bound<'py, PyAny>,
    fields: Bound<'py, PyTuple>,
    dtype: Bound<'py, PyAny>,
    dtype_get: GetSet,
    layout: NumpySource<'py>,
    values: Vec<Field<'py>>,
}

enum Field<'py> {
    Value(Box<NumpySource<'py>>),
    Record(Box<RecordSource<'py>>),
}

impl<'py> Field<'py> {
    fn storage(&self) -> &NumpySource<'py> {
        match self {
            Self::Value(value) => value,
            Self::Record(record) => &record.layout,
        }
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        match self {
            Self::Value(value) => value.schema(target),
            Self::Record(record) => record.schema(target),
        }
    }

    fn shape(&self, prefix: usize) -> PyResult<Shape> {
        let storage = self.storage();
        let rank = rank(storage.shape());
        let dimensions = rank.checked_sub(prefix).ok_or_else(|| {
            errors::builtin_message::<PyTypeError>(
                storage.owner().py(),
                "record field axes do not contain their parent record axes",
            )
        })?;
        let axis = |n| storage.axis_length(prefix + n).unwrap_or(0);
        Ok(match dimensions {
            0 => match self {
                Self::Record(record) => Shape::Record {
                    fields: record.fields.len(),
                },
                Self::Value(_) => Shape::Scalar,
            },
            1 => Shape::Vector { length: axis(0) },
            2 => Shape::Table {
                rows: axis(0),
                columns: axis(1),
            },
            _ => {
                let mut elements = Some(1usize);
                let mut empty = false;
                for current in 0..dimensions {
                    empty |= axis(current) == 0;
                    elements = elements.and_then(|n| n.checked_mul(axis(current)));
                }
                Shape::Tensor {
                    dimensions,
                    elements: if empty {
                        0
                    } else {
                        elements.ok_or_else(|| errors::memory_error(storage.owner().py()))?
                    },
                }
            }
        })
    }

    fn consume(
        &mut self,
        prefix: usize,
        index: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let shape = self.shape(prefix)?;
        let length = elements(shape);
        let start = index
            .checked_mul(length)
            .ok_or_else(|| errors::memory_error(self.storage().owner().py()))?;
        let mut projection = Projection {
            field: self,
            prefix,
            start,
            shape,
        };
        target.source(&mut projection, budget)
    }
}

fn rank(shape: Shape) -> usize {
    match shape {
        Shape::Scalar | Shape::Record { .. } => 0,
        Shape::Vector { .. } => 1,
        Shape::Table { .. } => 2,
        Shape::Tensor { dimensions, .. } => dimensions,
    }
}

fn elements(shape: Shape) -> usize {
    match shape {
        Shape::Scalar | Shape::Record { .. } => 1,
        Shape::Vector { length } => length,
        Shape::Table { rows, columns } => rows.saturating_mul(columns),
        Shape::Tensor { elements, .. } => elements,
    }
}

fn coordinate(shape: Shape, index: usize) -> (usize, usize) {
    match shape {
        Shape::Table { columns, .. } if columns != 0 => (index / columns, index % columns),
        _ => (index, 0),
    }
}

/// A field's actual trailing axes, without materializing a scalar or a subarray.
/// This borrows only prepared owners and metadata, never a live buffer export.
struct Projection<'a, 'py> {
    field: &'a mut Field<'py>,
    prefix: usize,
    start: usize,
    shape: Shape,
}

impl<'py> PreparedSource<'py> for Projection<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.field.storage().owner()
    }
    fn shape(&self) -> Shape {
        self.shape
    }
    fn axis_length(&self, axis: usize) -> Option<usize> {
        self.field.storage().axis_length(self.prefix + axis)
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &*self.field {
            Field::Record(source) => source.field_name(column),
            Field::Value(_) => Ok(None),
        }
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        self.field.schema(target)
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

impl Projection<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if let Shape::Record { .. } = self.shape {
            return match self.field {
                Field::Record(record) => record.consume_field(self.start, column, target, budget),
                Field::Value(value) => Err(errors::builtin_message::<PyTypeError>(
                    value.owner().py(),
                    "record projection has no field source",
                )),
            };
        }
        let index = match self.shape {
            Shape::Table { columns, .. } => {
                row.checked_mul(columns).and_then(|n| n.checked_add(column))
            }
            _ => Some(row),
        }
        .and_then(|n| self.start.checked_add(n))
        .ok_or_else(|| errors::memory_error(self.owner().py()))?;
        match self.field {
            Field::Value(source) => {
                let (row, column) = coordinate(source.shape(), index);
                source.consume_cell_to(row, column, target, budget)
            }
            Field::Record(source) => source.consume_record(index, target, budget),
        }
    }
}

struct Row<'a, 'py> {
    source: &'a mut RecordSource<'py>,
    index: usize,
}

impl<'py> PreparedSource<'py> for Row<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.source.owner
    }
    fn shape(&self) -> Shape {
        Shape::Record {
            fields: self.source.fields.len(),
        }
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.source.field_name(column)
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        self.source.schema(target)
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

impl Row<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        _row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.source
            .consume_field(self.index, column, target, budget)
    }
}

impl<'py> RecordSource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        is_void: bool,
    ) -> PyResult<Option<Self>> {
        let py = value.py();
        if crate::input::actual_instance(value, api.masked.bind(py)) {
            return Ok(None);
        }
        let dtype_get = if is_void {
            &api.record_dtype
        } else {
            &api.dtype
        };
        let dtype = dtype_get.get(value)?;
        let fields = match errors::attribute(&dtype, c"names")?.cast_into::<PyTuple>() {
            Ok(fields) => fields,
            Err(_) => return Ok(None),
        };
        let view = crate::input::control::callback(py, Some(budget), || {
            if is_void {
                api.record_array.bind(py).call1((value,))
            } else {
                // Exact native output type avoids child __array_finalize__.
                api.array_view.bind(py).call1((value, api.array.typ(py)))
            }
        })?;
        if !view.get_type().is(api.array.typ(py)) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical record view must retain exact ndarray storage",
            ));
        }
        let layout = NumpySource::record_layout(if is_void { &view } else { value }, api, budget)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(fields.len())
            .map_err(|_| errors::memory_error(py))?;
        for field in fields.iter() {
            crate::input::check(py, budget)?;
            if !field.is_exact_instance_of::<PyString>() {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "NumPy record field names must be strings",
                ));
            }
            // The retained exact native view's public mapping slot creates a
            // field view once, with complete names and an owning base chain.
            let current = crate::input::control::callback(py, Some(budget), || unsafe {
                // SAFETY: view is an exact qualified ndarray; item has the
                // documented mapping signature and returns an owned reference.
                Bound::from_owned_ptr_or_err(py, (api.item)(view.as_ptr(), field.as_ptr()))
            })?;
            let source = if let Some(source) = Self::new(&current, api, budget, false)? {
                Field::Record(crate::input::boxed(py, source)?)
            } else {
                Field::Value(crate::input::boxed(
                    py,
                    NumpySource::new(&current, api, budget)?,
                )?)
            };
            values.push(source);
        }
        Ok(Some(Self {
            owner: value.clone(),
            fields,
            dtype,
            dtype_get: dtype_get.clone_ref(py),
            layout,
            values,
        }))
    }
}

impl<'py> PreparedSource<'py> for RecordSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        match self.layout.shape() {
            Shape::Scalar => Shape::Record {
                fields: self.fields.len(),
            },
            Shape::Table { rows, columns } => Shape::Tensor {
                dimensions: 2,
                elements: rows.saturating_mul(columns),
            },
            shape => shape,
        }
    }

    fn axis_length(&self, axis: usize) -> Option<usize> {
        self.layout.axis_length(axis)
    }

    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.fields.get_item(column).map(Some)
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        for field in &self.values {
            target.checkpoint(self.owner.py(), 1)?;
            field.schema(target)?;
        }
        Ok(())
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

impl RecordSource<'_> {
    crate::input::cell::destinations!();

    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        target.checkpoint(py, 1)?;
        match self.layout.shape() {
            Shape::Scalar => self.consume_field(0, column, target, budget),
            _ => self.consume_record(row, target, budget),
        }
    }

    fn consume_record(
        &mut self,
        index: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let mut row = Row {
            source: self,
            index,
        };
        target.source(&mut row, budget)
    }

    fn consume_field(
        &mut self,
        index: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        if column >= self.fields.len() {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy record coordinate is outside its schema",
            ));
        }
        if !self.dtype_get.get(&self.owner)?.is(&self.dtype) {
            return Err(super::allocation::bounds_error(py));
        }
        let (row, cell) = coordinate(self.layout.shape(), index);
        self.layout.validate_record_cell(row, cell, budget)?;
        self.values[column].consume(rank(self.layout.shape()), index, target, budget)
    }
}
