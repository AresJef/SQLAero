//! Immediate target consumption of shared provider facts at the Python boundary.
//!
//! A source acquires values once. Targets own encoding, layout and policy; no
//! SQL context is passed to a source reader or fabricated for JSON.

use super::control::Checkpoint;
use super::sink::{self, Destination};
use super::source::{CellWriter, PreparedSource};
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::Context;
use sqlaero_transcode::value::ScalarRef;

/// Native source schema information, independent of any target's admission.
pub(crate) enum Schema {
    Binary,
}

/// Consume borrowed facts before returning. Python references never leave this
/// attached boundary, and sources retain no target-formatted answers.
pub(crate) trait Target {
    /// Whether source-aware UTC storage needs its original local offset too.
    /// Positional MySQL consumes the original UTC ticks without that protocol.
    fn needs_local_offsets(&self) -> bool {
        false
    }

    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()>;
    fn schema(&mut self, py: Python<'_>, schema: Schema) -> PyResult<()>;
    fn checkpoint(&mut self, py: Python<'_>, work: usize) -> PyResult<()>;
    fn scalar(&mut self, py: Python<'_>, value: ScalarRef<'_>) -> PyResult<()>;
    fn python(&mut self, value: &Bound<'_, PyAny>, child: Option<usize>) -> PyResult<()>;
    fn text(&mut self, value: &Bound<'_, PyString>, budget: &dyn Checkpoint) -> PyResult<()>;
    fn text_utf8(&mut self, py: Python<'_>, value: &str, budget: &dyn Checkpoint) -> PyResult<()>;
    fn text_minimum(&self, py: Python<'_>, characters: usize) -> PyResult<()>;
    fn bytes_start(&mut self, py: Python<'_>, length: usize) -> PyResult<()>;
    fn bytes_fragment(&mut self, py: Python<'_>, value: &[u8]) -> PyResult<()>;
    fn bytes_end(&mut self, py: Python<'_>) -> PyResult<()>;
}

/// Statically specialized adapter for existing text/role/prepared destinations.
pub(crate) struct Mysql<'a, D: Destination + ?Sized> {
    pub(crate) output: &'a mut D,
    pub(crate) context: &'a Context,
    pub(crate) visitor: &'a mut dyn CellWriter,
}

impl<D: Destination + ?Sized> Target for Mysql<'_, D> {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.output
            .source(source, self.context, budget, self.visitor)
    }
    fn schema(&mut self, _py: Python<'_>, _schema: Schema) -> PyResult<()> {
        Ok(())
    }
    fn checkpoint(&mut self, py: Python<'_>, work: usize) -> PyResult<()> {
        self.visitor.checkpoint(py, work)
    }
    #[inline(always)]
    fn scalar(&mut self, py: Python<'_>, value: ScalarRef<'_>) -> PyResult<()> {
        sink::emit(self.output, py, value, self.context)
    }
    fn python(&mut self, value: &Bound<'_, PyAny>, child: Option<usize>) -> PyResult<()> {
        self.output.python(self.visitor, value, child)
    }
    fn text(&mut self, value: &Bound<'_, PyString>, budget: &dyn Checkpoint) -> PyResult<()> {
        self.output.text(value, self.context, Some(budget))
    }
    fn text_utf8(&mut self, py: Python<'_>, value: &str, budget: &dyn Checkpoint) -> PyResult<()> {
        super::text_utf8(py, value, self.output, self.context, Some(budget))
    }
    fn text_minimum(&self, py: Python<'_>, characters: usize) -> PyResult<()> {
        self.output
            .check_text_minimum(characters, self.context)
            .map_err(|error| crate::errors::wire(py, error))
    }
    fn bytes_start(&mut self, py: Python<'_>, length: usize) -> PyResult<()> {
        self.output
            .bytes_start(length, self.context)
            .map_err(|error| sink::failure(py, error))
    }
    fn bytes_fragment(&mut self, py: Python<'_>, value: &[u8]) -> PyResult<()> {
        self.output
            .bytes_fragment(value, self.context)
            .map_err(|error| sink::failure(py, error))
    }
    fn bytes_end(&mut self, py: Python<'_>) -> PyResult<()> {
        self.output
            .bytes_end(self.context)
            .map_err(|error| sink::failure(py, error))
    }
}

/// Reuse the shared source acquisition while preserving specialized SQL sinks.
macro_rules! destinations {
    () => {
        fn write_cell_to(
            &mut self,
            row: usize,
            column: usize,
            output: &mut (impl $crate::input::sink::Destination + ?Sized),
            context: &sqlaero_core::bind::Context,
            budget: &dyn $crate::input::control::Checkpoint,
            visitor: &mut dyn $crate::input::source::CellWriter,
        ) -> pyo3::PyResult<()> {
            self.consume_cell_to(
                row,
                column,
                &mut $crate::input::cell::Mysql {
                    output,
                    context,
                    visitor,
                },
                budget,
            )
        }
    };
}
pub(crate) use destinations;
