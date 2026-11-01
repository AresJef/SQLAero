//! Prepared input shapes and the shared, boundary-only native column contract.

use pyo3::prelude::*;
use sqlaero_core::bind::Context;
use sqlaero_core::transport::deadline::IoBudget;

/// Prepare a recognized typed provider at the Python boundary. The caller
/// checks ordinary scalar and sequence paths first; no provider is imported
/// merely because an unsupported object was supplied.
pub(crate) fn prepare<'py>(
    value: &Bound<'py, PyAny>,
    types: &super::ScalarTypes,
    budget: &IoBudget,
) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
    use pyo3::types::{PyBytes, PyDate, PyDelta, PyFloat, PyInt, PyString, PyTime};
    // Scalar shape discovery cannot import or prepare a foreign provider. This
    // also keeps repeated ordinary many=True leaves off the optional paths.
    if value.is_none()
        || value.is_instance_of::<PyInt>()
        || value.is_instance_of::<PyFloat>()
        || value.is_instance_of::<PyString>()
        || value.is_instance_of::<PyBytes>()
        || value.is_instance_of::<PyDate>()
        || value.is_instance_of::<PyTime>()
        || value.is_instance_of::<PyDelta>()
        || value.get_type().is(types.decimal_type(value.py()))
        || value.get_type().is(types.struct_time.bind(value.py()))
    {
        return Ok(None);
    }
    if let Some(numpy) = types.numpy_types(value.py())? {
        if let Some(source) = numpy.prepare_array(value, budget)? {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
        if let Some(source) = numpy.prepare_record(value, budget)? {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
        if let Some(pandas) = types.pandas_types(value.py())?
            && let Some(source) = pandas.prepare(value, numpy, budget)?
        {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
    }
    if let Some(polars) = types.polars_types(value.py(), budget)?
        && let Some(source) = polars.prepare(value, budget)?
    {
        return Ok(Some(super::boxed(value.py(), source)?));
    }
    if let Some(source) = super::arrow::prepare(value, budget)? {
        return Ok(Some(super::boxed(value.py(), source)?));
    }
    Ok(None)
}

/// Logical structure, independent of a template's number of placeholders.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape {
    /// One-dimensional typed input; each element remains one logical cell.
    Vector { length: usize },
    /// One record is a single row, including when `many` is true.
    Record { fields: usize },
    /// A natural row source; `many` cannot erase its second dimension.
    Table { rows: usize, columns: usize },
}

/// Reuse the production visitor for Python leaves and native nesting facts.
/// Implementations live at the attached Python boundary, never in the core.
pub(crate) trait CellWriter {
    /// Write a Python leaf or nested value without an intermediate SQL object.
    fn python(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()>;

    /// Account for a native nested sequence under the common depth limit.
    fn enter_native(&mut self, py: Python<'_>) -> PyResult<()>;

    /// Leave a successfully entered native sequence.
    fn leave_native(&mut self);

    /// Retain an element or field position without formatting a diagnostic.
    fn push_index(&mut self, py: Python<'_>, index: usize) -> PyResult<()>;

    /// Restore the parent position after successful child processing.
    fn pop_index(&mut self);

    /// Charge actual work to the existing signal and operation deadline checks.
    fn checkpoint(&mut self, py: Python<'_>, work: usize) -> PyResult<()>;
}

/// Own preparation and traversal of one provider, not a materialized value tree.
/// A stream supports forward rows and re-encoding its most recent row only.
/// No raw view or borrowed slot may survive a callback through `visitor`.
pub(crate) trait PreparedSource<'py> {
    /// Retain the canonical original source for identity and lifetime purposes.
    fn owner(&self) -> &Bound<'py, PyAny>;

    /// Return the prepared logical shape without per-value Python metadata work.
    fn shape(&self) -> Shape;

    /// Write one actual logical cell, checking its current access boundaries.
    /// Arguments select source coordinates, never SQL names or placeholder roles.
    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()>;
}
