//! Prepared input shapes and the shared, boundary-only native column contract.

use pyo3::prelude::*;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{ValueError, ValueSink};
pub(crate) use sqlaero_transcode::structure::Shape;

/// Prepare a recognized typed provider at the Python boundary. The caller
/// checks ordinary scalar and sequence paths first; no provider is imported
/// merely because an unsupported object was supplied.
pub(crate) fn prepare<'py>(
    value: &Bound<'py, PyAny>,
    types: &super::ScalarTypes,
    budget: &dyn crate::input::control::Checkpoint,
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
    if let Some(numpy) = types.numpy_types(value.py(), Some(budget))? {
        if let Some(source) = numpy.prepare_masked(value, budget)? {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
        if let Some(source) = numpy.prepare_record(value, budget)? {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
        if let Some(source) = numpy.prepare_array(value, budget)? {
            return Ok(Some(super::boxed(value.py(), source)?));
        }
        if let Some(pandas) = types.pandas_types(value.py(), Some(budget))?
            && let Some(source) = pandas.prepare(value, numpy, budget)?
        {
            return Ok(Some(source));
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

/// The writer's structural role, independent of literal bytes or Python type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FragmentRole {
    Scalar,
    Row,
}

/// One actual child, not a normalized Python scalar or an encoded answer.
/// Native selection borrows the existing prepared owner without allocating a
/// provider per row. Object storage instead returns its actual owning reference.
pub(crate) enum Child<'a, 'py> {
    Python(Bound<'py, PyAny>),
    Native(Selected<'a, 'py>),
}

pub(crate) struct Selected<'a, 'py> {
    parent: &'a mut dyn PreparedSource<'py>,
    index: usize,
    shape: Shape,
}

impl<'a, 'py> Selected<'a, 'py> {
    /// Borrow an already qualified actual child shape. The source remains the
    /// owner and revalidates its physical cell at consumption, never a writer.
    pub(crate) fn new(parent: &'a mut dyn PreparedSource<'py>, index: usize, shape: Shape) -> Self {
        Self {
            parent,
            index,
            shape,
        }
    }
}

impl<'py> PreparedSource<'py> for Selected<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.parent.owner()
    }
    fn shape(&self) -> Shape {
        self.shape
    }
    fn schema(&self, _target: &mut dyn super::cell::Target) -> PyResult<()> {
        // The retained root already admitted its complete schema.
        Ok(())
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn super::cell::Target,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell_to(row, column, target, budget)
    }
    crate::input::source::cell_destinations!();
}

impl Selected<'_, '_> {
    super::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn super::cell::Target,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<()> {
        self.parent
            .consume_child(self.index, row, column, target, budget)
    }
}

/// Reuse the production visitor for Python leaves and native nesting facts.
/// Implementations live at the attached Python boundary, never in the core.
pub(crate) trait CellWriter {
    /// Reuse target-formatted native text within one operation. Validators
    /// decline it so source checks cannot be bypassed by a successful cache.
    fn referenced_text(
        &mut self,
        _py: Python<'_>,
        _reference: super::cell::TextReference<'_>,
        _output: &mut Vec<u8>,
        _budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<bool> {
        Ok(false)
    }

    /// Consume an already prepared native projection, without recognizing a
    /// Python class again or treating its retained owner as a new input cycle.
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        output: &mut Vec<u8>,
    ) -> PyResult<FragmentRole>;

    /// Write a Python leaf or nested value without an intermediate SQL object.
    fn python(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()>;

    /// Return the structural role from the same actual value traversal.
    fn python_role(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<FragmentRole>;

    /// Read the same Python leaf into one immediate native scalar sink.
    fn native_python(
        &mut self,
        value: &Bound<'_, PyAny>,
        sink: &mut dyn ValueSink<Context = sqlaero_core::bind::Context, Error = ValueError>,
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

/// Opaque identity of complete immutable names in one retained source.
/// Consumers may compare this token only while that prepared source is alive;
/// the address is never dereferenced or reused across source traversals.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldNameIdentity {
    source: usize,
    node: usize,
}

impl FieldNameIdentity {
    pub(super) fn new<T>(source: &T, node: usize) -> Self {
        Self {
            source: std::ptr::from_ref(source) as usize,
            node,
        }
    }
}

/// Own preparation and traversal of one provider, not a materialized value tree.
/// A stream supports forward rows and re-encoding its most recent row only.
/// No raw view or borrowed slot may survive a callback through `visitor`.
pub(crate) trait PreparedSource<'py> {
    /// Reborrow the concrete provider; implemented with the destination methods.
    fn as_source(&mut self) -> &mut dyn PreparedSource<'py>;

    /// Retain the canonical original source for identity and lifetime purposes.
    fn owner(&self) -> &Bound<'py, PyAny>;

    /// Return the prepared logical shape without per-value Python metadata work.
    fn shape(&self) -> Shape;

    /// Acquire complete name facts only when the caller needs them. Anonymous
    /// matrices return false; a named table can prepare its stable schema once.
    fn prepare_field_names(
        &mut self,
        _budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<bool> {
        Ok(false)
    }

    /// Identify stable, fully prepared name facts, not target-formatted keys.
    /// The default does not promise reusable names. A consumer must discard
    /// every token before this prepared source and its native projections die.
    fn field_name_identity(&self) -> Option<FieldNameIdentity> {
        None
    }

    /// A source owns child facts; the caller owns root/row policy such as many.
    fn child<'a>(
        &'a mut self,
        index: usize,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<Child<'a, 'py>> {
        if let Some(value) = self.python_child(index, budget)? {
            return Ok(Child::Python(value));
        }
        let shape = self.child_shape(index)?;
        Ok(Child::Native(Selected {
            parent: self.as_source(),
            index,
            shape,
        }))
    }

    fn python_child(
        &mut self,
        _index: usize,
        _budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(None)
    }

    fn child_shape(&self, index: usize) -> PyResult<Shape> {
        match self.shape() {
            Shape::Vector { length } if index < length => Ok(Shape::Scalar),
            Shape::Table { rows, columns } if index < rows => Ok(Shape::Vector { length: columns }),
            _ => Err(crate::errors::builtin_message::<
                pyo3::exceptions::PyIndexError,
            >(
                self.owner().py(),
                "source child is outside its prepared shape",
            )),
        }
    }

    fn consume_child(
        &mut self,
        index: usize,
        row: usize,
        _column: usize,
        target: &mut dyn super::cell::Target,
        budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<()> {
        match self.shape() {
            Shape::Vector { .. } => self.consume_cell(index, 0, target, budget),
            Shape::Table { .. } => self.consume_cell(index, row, target, budget),
            _ => Err(crate::errors::builtin_message::<
                pyo3::exceptions::PyTypeError,
            >(
                self.owner().py(),
                "source child consumption requires explicit field facts",
            )),
        }
    }

    /// Complete actual field name, acquired only by targets that need names.
    /// Positional targets consume the same values without JSON key work.
    fn field_name(&self, _column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(None)
    }

    /// Return one actual axis length without rereading Python metadata.
    /// Higher-rank sources implement this over their prepared native facts.
    fn axis_length(&self, axis: usize) -> Option<usize> {
        match (self.shape(), axis) {
            (Shape::Vector { length }, 0) => Some(length),
            (Shape::Table { rows, .. }, 0) => Some(rows),
            (Shape::Table { columns, .. }, 1) => Some(columns),
            _ => None,
        }
    }

    /// Present source schema facts without reading values or formatting output.
    /// Each target independently admits the schema it can represent.
    fn schema(&self, _target: &mut dyn super::cell::Target) -> PyResult<()> {
        Err(crate::errors::builtin_message::<
            pyo3::exceptions::PyTypeError,
        >(
            self.owner().py(),
            "this typed provider's shared schema migration is not connected yet",
        ))
    }

    /// Immediate format-neutral cell consumption. Provider families migrate
    /// their existing checked reader here without creating a second catalog.
    fn consume_cell(
        &mut self,
        _row: usize,
        _column: usize,
        _target: &mut dyn super::cell::Target,
        _budget: &dyn crate::input::control::Checkpoint,
    ) -> PyResult<()> {
        Err(crate::errors::builtin_message::<
            pyo3::exceptions::PyTypeError,
        >(
            self.owner().py(),
            "this typed provider's shared cell migration is not connected yet",
        ))
    }

    /// Write one actual logical cell, checking its current access boundaries.
    /// Arguments select source coordinates, never SQL names or placeholder roles.
    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &dyn crate::input::control::Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()>;

    /// Observe structural writes during the same single cell acquisition.
    fn write_role_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &dyn crate::input::control::Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<FragmentRole>;

    /// Read the same actual logical cell into a prepared parameter sink.
    /// No escaped SQL bytes or per-value Python SQL strings are involved.
    fn write_parameter_cell(
        &mut self,
        row: usize,
        column: usize,
        sink: &mut dyn ValueSink<Context = sqlaero_core::bind::Context, Error = ValueError>,
        context: &Context,
        budget: &dyn crate::input::control::Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()>;
}

/// Keep text statically specialized while sharing actual cell acquisition.
macro_rules! cell_destinations {
    () => {
        fn as_source(&mut self) -> &mut dyn $crate::input::source::PreparedSource<'py> {
            self
        }
        fn write_cell(
            &mut self,
            row: usize,
            column: usize,
            output: &mut Vec<u8>,
            context: &sqlaero_core::bind::Context,
            budget: &dyn crate::input::control::Checkpoint,
            visitor: &mut dyn $crate::input::source::CellWriter,
        ) -> pyo3::PyResult<()> {
            self.write_cell_to(row, column, output, context, budget, visitor)
        }
        fn write_role_cell(
            &mut self,
            row: usize,
            column: usize,
            output: &mut Vec<u8>,
            context: &sqlaero_core::bind::Context,
            budget: &dyn crate::input::control::Checkpoint,
            visitor: &mut dyn $crate::input::source::CellWriter,
        ) -> pyo3::PyResult<$crate::input::source::FragmentRole> {
            let mut destination = $crate::input::sink::RoleSink::new(output);
            self.write_cell_to(row, column, &mut destination, context, budget, visitor)?;
            Ok(destination.role())
        }
        fn write_parameter_cell(
            &mut self,
            row: usize,
            column: usize,
            sink: &mut dyn sqlaero_core::bind::value::ValueSink<
                Context = sqlaero_core::bind::Context,
                Error = sqlaero_core::bind::value::ValueError,
            >,
            context: &sqlaero_core::bind::Context,
            budget: &dyn crate::input::control::Checkpoint,
            visitor: &mut dyn $crate::input::source::CellWriter,
        ) -> pyo3::PyResult<()> {
            self.write_cell_to(
                row,
                column,
                &mut $crate::input::sink::Native(sink),
                context,
                budget,
                visitor,
            )
        }
    };
}
pub(crate) use cell_destinations;
