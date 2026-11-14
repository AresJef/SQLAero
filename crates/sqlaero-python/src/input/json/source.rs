//! JSON consumes the same checked provider cells as both MySQL destinations.

use super::{Serialize, failure};
use crate::input::cell::{Schema, Target};
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use crate::{errors, input};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::json::Error;
use sqlaero_transcode::diagnostic::{Coordinate, Path};
use sqlaero_transcode::value::{Consumer, ScalarRef};

struct Cell<'a, 'b, 'c> {
    state: &'a mut Serialize<'b>,
    path: &'a Path<'c>,
}

impl Target for Cell<'_, '_, '_> {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.state.typed_source(source, self.path, budget)
    }
    fn schema(&mut self, py: Python<'_>, schema: Schema) -> PyResult<()> {
        match schema {
            Schema::Binary => Err(failure(py, Error::Unsupported("binary values"))),
        }
    }
    fn checkpoint(&mut self, py: Python<'_>, work: usize) -> PyResult<()> {
        let previous = self.state.nodes;
        self.state.nodes = previous.saturating_add(work);
        if previous == 0 || self.state.nodes >= 256 {
            input::checkpoint(py, self.state.budget)?;
            if self.state.nodes >= 256 {
                self.state.nodes = 0;
            }
        }
        Ok(())
    }
    fn scalar(&mut self, py: Python<'_>, value: ScalarRef<'_>) -> PyResult<()> {
        self.state
            .writer
            .write(value, &self.state.options)
            .map_err(|error| failure(py, error))
    }
    fn python(&mut self, value: &Bound<'_, PyAny>, child: Option<usize>) -> PyResult<()> {
        match child {
            Some(index) => self
                .state
                .value_at(value, &self.path.child(Coordinate::Index(index))),
            None => self.state.value_at(value, self.path),
        }
    }
    fn text(&mut self, value: &Bound<'_, PyString>, budget: &dyn Checkpoint) -> PyResult<()> {
        input::unicode::write(
            value,
            &mut self.state.writer,
            &self.state.options,
            Some(budget),
            "utf-8",
            |error, _, _| failure(value.py(), error),
        )
    }
    fn text_utf8(&mut self, py: Python<'_>, value: &str, budget: &dyn Checkpoint) -> PyResult<()> {
        if value.len() <= input::WORK_BYTES {
            input::control::bytes(py, Some(budget), value.len())?;
            return self.scalar(py, ScalarRef::Text(value));
        }
        self.state
            .writer
            .text_start(Some(value.len()), &self.state.options)
            .map_err(|error| failure(py, error))?;
        input::unicode::native_fragments(py, value, Some(budget), |_, text| {
            self.state
                .writer
                .text_fragment(text, &self.state.options)
                .map_err(|error| failure(py, error))
        })?;
        self.state
            .writer
            .text_end(&self.state.options)
            .map_err(|error| failure(py, error))
    }
    fn text_minimum(&self, _py: Python<'_>, _characters: usize) -> PyResult<()> {
        // Standalone JSON has no connection packet quota. Actual reservations
        // and every target write remain fallible and bounded by work checks.
        Ok(())
    }
    fn bytes_start(&mut self, py: Python<'_>, _length: usize) -> PyResult<()> {
        self.schema(py, Schema::Binary)
    }
    fn bytes_fragment(&mut self, py: Python<'_>, _value: &[u8]) -> PyResult<()> {
        self.schema(py, Schema::Binary)
    }
    fn bytes_end(&mut self, py: Python<'_>) -> PyResult<()> {
        self.schema(py, Schema::Binary)
    }
}

impl Serialize<'_> {
    pub(super) fn typed(&mut self, value: &Bound<'_, PyAny>, path: &Path<'_>) -> PyResult<bool> {
        let py = value.py();
        let budget = self.budget.ok_or_else(|| {
            errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::InternalState,
                "JSON provider traversal has no original work control",
            )
        })?;
        let Some(mut source) = input::source::prepare(value, self.types, budget)? else {
            return Ok(false);
        };
        self.active
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        self.active.push(value.as_ptr() as usize);
        let result = self.typed_source(source.as_mut(), path, budget);
        self.active.pop();
        result.map(|()| true)
    }

    /// Native projections already belong to the original active input. Their
    /// actual output containers still pass through the writer's depth checks.
    fn typed_source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = source.owner().py();
        source.schema(&mut Cell { state: self, path })?;
        match source.shape() {
            Shape::Scalar => {
                self.typed_cell(source, 0, 0, path, budget)?;
            }
            Shape::Vector { length } => {
                self.writer
                    .array_start()
                    .map_err(|error| failure(py, error))?;
                for row in 0..length {
                    let child = path.child(Coordinate::Index(row));
                    self.typed_cell(source, row, 0, &child, budget)?;
                }
                self.writer
                    .array_end()
                    .map_err(|error| failure(py, error))?;
            }
            Shape::Table { rows, columns } => {
                self.writer
                    .array_start()
                    .map_err(|error| failure(py, error))?;
                for row in 0..rows {
                    input::checkpoint(py, Some(budget))?;
                    let current = path.child(Coordinate::Index(row));
                    self.writer
                        .array_start()
                        .map_err(|error| failure(py, error))?;
                    for column in 0..columns {
                        let child = current.child(Coordinate::Index(column));
                        self.typed_cell(source, row, column, &child, budget)?;
                    }
                    self.writer
                        .array_end()
                        .map_err(|error| failure(py, error))?;
                }
                self.writer
                    .array_end()
                    .map_err(|error| failure(py, error))?;
            }
            Shape::Tensor {
                dimensions,
                elements,
            } => {
                let mut position = 0;
                self.tensor(source, 0..dimensions, &mut position, path, budget)?;
                if position != elements {
                    return Err(errors::builtin_message::<PyValueError>(
                        py,
                        "native array axes disagree with the prepared logical cell count",
                    ));
                }
            }
            Shape::Record { fields } => self.typed_record(source, fields, path, budget)?,
        }
        Ok(())
    }

    fn typed_record(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        fields: usize,
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        use std::collections::{HashMap, hash_map::RandomState};
        let py = source.owner().py();
        let seed = RandomState::new();
        let mut names: HashMap<u64, Vec<String>> = HashMap::new();
        self.writer
            .object_start()
            .map_err(|error| failure(py, error))?;
        for column in 0..fields {
            Cell { state: self, path }.checkpoint(py, 1)?;
            let name = source.field_name(column)?.ok_or_else(|| {
                errors::builtin_message::<PyValueError>(py, "native record field name is missing")
            })?;
            let key = super::keys::read(&name, self.types, self.budget, &seed)?;
            names.try_reserve(1).map_err(|_| errors::memory_error(py))?;
            if let Some(previous) = names.get(&key.hash) {
                for name in previous {
                    if super::keys::same(py, name, &key.value, self.budget)? {
                        return Err(errors::builtin_format::<PyValueError>(
                            py,
                            format_args!(
                                "JSON object keys collide after normalization at entry {column}"
                            ),
                        ));
                    }
                }
            }
            self.writer
                .key_start(key.value.len())
                .map_err(|error| failure(py, error))?;
            input::unicode::native_fragments(py, &key.value, self.budget, |_, text| {
                self.writer
                    .text_fragment(text, &self.options)
                    .map_err(|error| failure(py, error))
            })?;
            self.writer
                .text_end(&self.options)
                .map_err(|error| failure(py, error))?;
            let child = path.child(Coordinate::Field {
                name: &key.value,
                ordinal: column,
            });
            self.typed_cell(source, 0, column, &child, budget)?;
            let previous = names.entry(key.hash).or_default();
            previous
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            previous.push(key.value);
        }
        self.writer.object_end().map_err(|error| failure(py, error))
    }

    fn typed_cell(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        row: usize,
        column: usize,
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let result = source.consume_cell(row, column, &mut Cell { state: self, path }, budget);
        if let Err(error) = &result {
            self.locate(source.owner().py(), error, path);
        }
        result
    }

    /// Output axes are target structure, not source chunking or SQL row layout.
    /// Only actual reached arrays consume depth; an empty early axis ends here.
    fn tensor(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        axes: std::ops::Range<usize>,
        position: &mut usize,
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = source.owner().py();
        let axis = axes.start;
        let length = source.axis_length(axis).ok_or_else(|| {
            errors::builtin_message::<PyValueError>(py, "native array axis length is missing")
        })?;
        Cell { state: self, path }.checkpoint(py, 1)?;
        self.writer
            .array_start()
            .map_err(|error| failure(py, error))?;
        for index in 0..length {
            let child = path.child(Coordinate::Index(index));
            if axis + 1 == axes.end {
                self.typed_cell(source, *position, 0, &child, budget)?;
                *position = position
                    .checked_add(1)
                    .ok_or_else(|| errors::memory_error(py))?;
            } else {
                self.tensor(source, axis + 1..axes.end, position, &child, budget)?;
            }
        }
        self.writer.array_end().map_err(|error| failure(py, error))
    }
}
