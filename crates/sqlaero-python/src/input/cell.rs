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

/// A target failure produced without invoking Python while an external native
/// source is borrowed. Normalize it only after that borrow has ended.
pub(crate) enum NativeFailure {
    Mysql(sqlaero_core::bind::value::ValueError),
    Json(sqlaero_core::json::Error),
}

impl NativeFailure {
    pub(crate) fn python(self, py: Python<'_>) -> PyErr {
        match self {
            Self::Mysql(error) => sink::failure(py, error),
            Self::Json(error) => super::json::failure(py, error),
        }
    }
}

/// Native source schema information, independent of any target's admission.
#[derive(Clone, Copy)]
pub(crate) enum Schema {
    Binary,
}

/// Nonreused identity for one native text domain. A target can retain bytes
/// after its source advances without retaining foreign buffers or trusting a
/// recycled address. Exhaustion simply disables this optional reuse path.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextIdentity(std::num::NonZeroU64);
impl TextIdentity {
    pub(crate) fn new() -> Option<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .and_then(std::num::NonZeroU64::new)
        .map(Self)
    }
}

/// An already resolved stable native text label. The public native owner
/// remains at the attached boundary; pure writers never retain it.
/// The channel is a positional hint, never a type or formatting policy.
#[derive(Clone, Copy)]
pub(crate) struct TextReference<'a> {
    pub(crate) identity: Option<TextIdentity>,
    pub(crate) values: &'a super::arrow::ArrayRef,
    pub(crate) index: usize,
    pub(crate) channel: usize,
}

/// Consume borrowed facts before returning. Python references never leave this
/// attached boundary, and sources retain no target-formatted answers.
pub(crate) trait Target {
    /// Optional reuse of stable native text. JSON/prepared targets decline
    /// SQL byte reuse and consume the same already referenced scalar facts.
    fn referenced_text(
        &mut self,
        _py: Python<'_>,
        _reference: TextReference<'_>,
        _budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        Ok(false)
    }
    fn needs_field_names(&self) -> bool {
        false
    }
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
    fn native_scalar(&mut self, value: ScalarRef<'_>) -> Result<(), NativeFailure>;
    fn scalar(&mut self, py: Python<'_>, value: ScalarRef<'_>) -> PyResult<()> {
        self.native_scalar(value).map_err(|error| error.python(py))
    }
    fn float(&mut self, py: Python<'_>, value: f64) -> PyResult<()> {
        self.scalar(py, ScalarRef::Float(value))
    }
    fn native_text_start(&mut self, length: usize) -> Result<(), NativeFailure>;
    fn native_text_fragment(&mut self, value: &str) -> Result<(), NativeFailure>;
    fn native_text_end(&mut self) -> Result<(), NativeFailure>;
    fn text_encoding(&self) -> &'static str;
    fn python(&mut self, value: &Bound<'_, PyAny>, child: Option<usize>) -> PyResult<()>;
    fn text(&mut self, value: &Bound<'_, PyString>, budget: &dyn Checkpoint) -> PyResult<()>;
    fn text_utf8(&mut self, py: Python<'_>, value: &str, budget: &dyn Checkpoint) -> PyResult<()>;
    fn text_minimum(&self, py: Python<'_>, characters: usize) -> PyResult<()>;
    fn binary_minimum(&self, py: Python<'_>, length: usize) -> PyResult<()>;
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
    fn referenced_text(
        &mut self,
        py: Python<'_>,
        reference: TextReference<'_>,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        if let Some(output) = self.output.text_buffer() {
            return self.visitor.referenced_text(py, reference, output, budget);
        }
        Ok(false)
    }
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
    fn native_scalar(&mut self, value: ScalarRef<'_>) -> Result<(), NativeFailure> {
        self.output
            .write(value, self.context)
            .map_err(NativeFailure::Mysql)
    }
    fn float(&mut self, py: Python<'_>, value: f64) -> PyResult<()> {
        if let Some(output) = self.output.text_buffer() {
            // A plain text destination is already selected. Keep the same
            // formatter, exact capacity limit and original error conversion.
            sqlaero_core::bind::scalar::float(output, value, self.context.limit)
                .map_err(|error| crate::errors::wire(py, error))
        } else {
            self.scalar(py, ScalarRef::Float(value))
        }
    }
    fn native_text_start(&mut self, length: usize) -> Result<(), NativeFailure> {
        self.output
            .text_start(Some(length), self.context)
            .map_err(NativeFailure::Mysql)
    }
    fn native_text_fragment(&mut self, value: &str) -> Result<(), NativeFailure> {
        self.output
            .text_fragment(value, self.context)
            .map_err(NativeFailure::Mysql)
    }
    fn native_text_end(&mut self) -> Result<(), NativeFailure> {
        self.output
            .text_end(self.context)
            .map_err(NativeFailure::Mysql)
    }
    fn text_encoding(&self) -> &'static str {
        self.context.codec.name()
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
    fn binary_minimum(&self, py: Python<'_>, length: usize) -> PyResult<()> {
        self.output
            .check_binary_width(length, self.context)
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
