//! A minimal column reference for SQL expression operands.

use super::control::{Checkpoint, identifier};
use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::{Context, writer};

/// Reference a column using one name and optional separate qualifiers.
///
/// A dot in a name is part of that identifier. Pass table and database
/// separately to qualify the reference. Use this object inside SQL functions
/// that accept a column or scalar expression.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Column {
    name: Py<PyString>,
    table: Option<Py<PyString>>,
    database: Option<Py<PyString>>,
}

#[pymethods]
impl Column {
    #[new]
    #[pyo3(signature = (name, *, table=None, database=None))]
    fn new<'py>(
        name: &Bound<'py, PyAny>,
        table: Option<&Bound<'py, PyAny>>,
        database: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        if database.is_some() && table.is_none() {
            return Err(errors::builtin_message::<PyValueError>(
                name.py(),
                "a database qualifier requires a table qualifier",
            ));
        }
        Ok(Self {
            name: component(name, "name")?,
            table: table.map(|table| component(table, "table")).transpose()?,
            database: database.map(|db| component(db, "database")).transpose()?,
        })
    }
}

impl Column {
    pub(crate) fn write(
        &self,
        py: Python<'_>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        for (qualifier, label) in [(&self.database, "database"), (&self.table, "table")] {
            if let Some(qualifier) = qualifier {
                identifier(qualifier.bind(py).as_any(), label, output, context, budget)?;
                writer::append(output, b".", context.limit)
                    .map_err(|error| errors::wire(py, error))?;
            }
        }
        identifier(
            self.name.bind(py).as_any(),
            "column name",
            output,
            context,
            budget,
        )
    }
}

fn component(value: &Bound<'_, PyAny>, label: &str) -> PyResult<Py<PyString>> {
    let py = value.py();
    let text = value.cast::<PyString>().map_err(|_| {
        errors::builtin_format::<PyTypeError>(py, format_args!("Column {label} must be str"))
    })?;
    let contents = text.to_str()?;
    if contents.is_empty() || contents.contains('\0') {
        return Err(errors::builtin_format::<PyValueError>(
            py,
            format_args!("Column {label} must be nonempty and contain no NUL"),
        ));
    }
    if value.is_exact_instance_of::<PyString>() {
        Ok(text.clone().unbind())
    } else {
        // Retain base text, without retaining any subclass state or methods.
        Ok(PyString::from_bytes(py, contents.as_bytes())?.unbind())
    }
}
