//! Canonical records retain schema field order and their original identity.

use super::allocation::Allocation;
use crate::errors;
use crate::input::source::{CellWriter, PreparedSource, Shape};
use pyo3::exceptions::{PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple, PyType};
use sqlaero_core::bind::Context;
use sqlaero_core::transport::deadline::IoBudget;

/// Public field access owns the current value before any recursive callback.
pub(crate) struct RecordSource<'py> {
    owner: Bound<'py, PyAny>,
    fields: Bound<'py, PyTuple>,
    allocation: Allocation<'py>,
}

impl<'py> RecordSource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        array_type: &Py<PyType>,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        let py = value.py();
        let dtype = errors::attribute(value, c"dtype")?;
        let fields = errors::attribute(&dtype, c"names")?.cast_into::<PyTuple>()?;
        for field in fields.iter() {
            crate::input::check(py, budget)?;
            if !field.is_exact_instance_of::<PyString>() {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "NumPy record field names must be strings",
                ));
            }
        }
        Ok(Self {
            owner: value.clone(),
            fields,
            allocation: Allocation::new(value, array_type, budget)?,
        })
    }
}

impl<'py> PreparedSource<'py> for RecordSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        Shape::Record {
            fields: self.fields.len(),
        }
    }

    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        _context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        visitor.checkpoint(py, 1)?;
        if row != 0 || column >= self.fields.len() {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy record coordinate is outside its schema",
            ));
        }
        let name = self.fields.get_item(column)?;
        self.allocation.validate_record(&self.owner)?;
        // The public canonical getter returns an owning reference. No field
        // pointer, private dtype offset, or borrowed Object slot reaches Visit.
        let value = self.owner.get_item(name)?;
        crate::input::check(py, budget)?;
        visitor.python(&value, output, None)
    }
}
