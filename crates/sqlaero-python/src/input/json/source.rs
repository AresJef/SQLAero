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
        let result = (|| {
            source.schema(&mut Cell { state: self, path })?;
            match source.shape() {
                Shape::Scalar => {
                    self.typed_cell(source.as_mut(), 0, 0, path, budget)?;
                }
                Shape::Vector { length } => {
                    self.writer
                        .array_start()
                        .map_err(|error| failure(py, error))?;
                    for row in 0..length {
                        let child = path.child(Coordinate::Index(row));
                        self.typed_cell(source.as_mut(), row, 0, &child, budget)?;
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
                            self.typed_cell(source.as_mut(), row, column, &child, budget)?;
                        }
                        self.writer
                            .array_end()
                            .map_err(|error| failure(py, error))?;
                    }
                    self.writer
                        .array_end()
                        .map_err(|error| failure(py, error))?;
                }
                Shape::Record { .. } => {
                    return Err(errors::builtin_message::<PyValueError>(
                        py,
                        "shared field-bearing record layout is not connected yet",
                    ));
                }
            }
            Ok(true)
        })();
        self.active.pop();
        result
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
}
