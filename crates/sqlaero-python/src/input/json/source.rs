//! JSON consumes the same checked provider cells as both MySQL destinations.

use super::{Serialize, failure};
use crate::input::cell::{NativeFailure, Schema, Target};
use crate::input::control::Checkpoint;
use crate::input::sink::Destination;
use crate::input::source::{FieldNameIdentity, PreparedSource, Shape};
use crate::{errors, input};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::json::Error;
use sqlaero_transcode::diagnostic::{Coordinate, Path};
use sqlaero_transcode::value::{Consumer, ScalarRef};
use std::collections::HashMap;

struct Names {
    values: Vec<String>,
    allocated: usize,
}

/// Target-owned metadata only. Limits keep lookup growth and retained names
/// bounded; a source beyond either limit keeps the ordinary checked path.
#[derive(Default)]
pub(super) struct NameCache {
    entries: HashMap<FieldNameIdentity, Option<Names>>,
    allocated: usize,
}

impl NameCache {
    fn reserve(
        &mut self,
        py: Python<'_>,
        identity: FieldNameIdentity,
        allocated: usize,
    ) -> PyResult<bool> {
        if self.entries.len() >= 64
            || self.entries.contains_key(&identity)
            || self.allocated.saturating_add(allocated) > input::WORK_BYTES
        {
            return Ok(false);
        }
        self.entries
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        self.entries.insert(identity, None);
        self.allocated += allocated;
        Ok(true)
    }
}

struct Cell<'a, 'b, 'c, D: Destination + ?Sized> {
    state: &'a mut Serialize<'b, D>,
    path: &'a Path<'c>,
}

impl<D: Destination + ?Sized> Target for Cell<'_, '_, '_, D> {
    fn needs_field_names(&self) -> bool {
        true
    }
    fn needs_local_offsets(&self) -> bool {
        true
    }
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
    fn native_scalar(&mut self, value: ScalarRef<'_>) -> Result<(), NativeFailure> {
        self.state.writer.write(value, &self.state.options)
    }
    fn native_text_start(&mut self, length: usize) -> Result<(), NativeFailure> {
        self.state
            .writer
            .text_start(Some(length), &self.state.options)
    }
    fn native_text_fragment(&mut self, value: &str) -> Result<(), NativeFailure> {
        self.state.writer.text_fragment(value, &self.state.options)
    }
    fn native_text_end(&mut self) -> Result<(), NativeFailure> {
        self.state.writer.text_end(&self.state.options)
    }
    fn text_encoding(&self) -> &'static str {
        "utf-8"
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
    fn binary_minimum(&self, py: Python<'_>, _length: usize) -> PyResult<()> {
        Err(failure(py, Error::Unsupported("binary values")))
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

impl<D: Destination + ?Sized> Serialize<'_, D> {
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
        // Native schema addresses identify facts only while this source lives.
        // Nested Python providers get their own scope and restore the parent's
        // metadata afterwards, including on a failed traversal.
        let previous_names = self.native_names.take();
        let previous_repeat = std::mem::replace(&mut self.repeat_native_names, false);
        let result = self.typed_source(source.as_mut(), path, budget);
        self.native_names = previous_names;
        self.repeat_native_names = previous_repeat;
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
        let shape = source.shape();
        // A singleton cannot amortize an index. Nested repeated structures
        // enable reuse from the same neutral facts, without provider checks.
        self.repeat_native_names |= matches!(
            shape,
            Shape::Vector { length: 2.. } | Shape::Table { rows: 2.., .. }
        );
        match shape {
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
                let keys = if rows != 0 && source.prepare_field_names(budget)? {
                    Some(self.source_keys(source, columns, path)?)
                } else {
                    None
                };
                self.writer
                    .array_start()
                    .map_err(|error| failure(py, error))?;
                for row in 0..rows {
                    input::checkpoint(py, Some(budget))?;
                    let current = path.child(Coordinate::Index(row));
                    if let Some(keys) = &keys {
                        self.record_cells(source, row, &keys.values, &current, budget)?;
                    } else {
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

    fn source_keys(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        fields: usize,
        path: &Path<'_>,
    ) -> PyResult<Names> {
        use std::collections::{HashMap, hash_map::RandomState};
        let py = source.owner().py();
        let seed = RandomState::new();
        let mut positions: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        names
            .try_reserve_exact(fields)
            .map_err(|_| errors::memory_error(py))?;
        let mut allocated = names.capacity().saturating_mul(size_of::<String>());
        for column in 0..fields {
            Cell { state: self, path }.checkpoint(py, 1)?;
            let name = source.field_name(column)?.ok_or_else(|| {
                errors::builtin_message::<PyValueError>(py, "native field name is missing")
            })?;
            let key = super::keys::read(&name, self.types, self.budget, &seed)?;
            positions
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            if let Some(previous) = positions.get(&key.hash) {
                for &index in previous {
                    if super::keys::same(py, &names[index], &key.value, self.budget)? {
                        return Err(errors::builtin_format::<PyValueError>(
                            py,
                            format_args!(
                                "JSON object keys collide after normalization at entry {column}"
                            ),
                        ));
                    }
                }
            }
            let previous = positions.entry(key.hash).or_default();
            previous
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            previous.push(column);
            allocated = allocated.saturating_add(key.value.capacity());
            names.push(key.value);
        }
        Ok(Names {
            values: names,
            allocated,
        })
    }

    fn typed_record(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        fields: usize,
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        source.prepare_field_names(budget)?;
        let identity = if self.repeat_native_names {
            source.field_name_identity()
        } else {
            None
        };
        let ready = identity.and_then(|identity| {
            self.native_names
                .as_mut()?
                .entries
                .get_mut(&identity)?
                .take()
        });
        let (names, retain) = if let Some(names) = ready {
            (names, true)
        } else {
            let names = self.source_keys(source, fields, path)?;
            let retain = if let Some(identity) = identity
                && names.allocated <= input::WORK_BYTES
            {
                // Unnamed/scalar roots carry only an optional pointer, rather
                // than moving an unused map through the shared hot state.
                let py = source.owner().py();
                let cache = match &mut self.native_names {
                    Some(cache) => cache,
                    slot => slot.insert(input::boxed(py, NameCache::default())?),
                };
                cache.reserve(py, identity, names.allocated)?
            } else {
                false
            };
            (names, retain)
        };
        let result = self.record_cells(source, 0, &names.values, path, budget);
        if retain
            && let Some(identity) = identity
            && let Some(slot) = self
                .native_names
                .as_mut()
                .and_then(|cache| cache.entries.get_mut(&identity))
        {
            *slot = Some(names);
        }
        result
    }

    fn record_cells(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        row: usize,
        names: &[String],
        path: &Path<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = source.owner().py();
        self.writer
            .object_start()
            .map_err(|error| failure(py, error))?;
        for (column, name) in names.iter().enumerate() {
            Cell { state: self, path }.checkpoint(py, 1)?;
            self.writer
                .key_start(name.len())
                .map_err(|error| failure(py, error))?;
            input::unicode::native_fragments(py, name, self.budget, |_, text| {
                self.writer
                    .text_fragment(text, &self.options)
                    .map_err(|error| failure(py, error))
            })?;
            self.writer
                .text_end(&self.options)
                .map_err(|error| failure(py, error))?;
            let child = path.child(Coordinate::Field {
                name,
                ordinal: column,
            });
            self.typed_cell(source, row, column, &child, budget)?;
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
