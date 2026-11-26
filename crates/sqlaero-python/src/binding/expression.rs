//! Consume native grammar in the existing text visitor.

use super::Visit;
use crate::errors;
use crate::input::{
    check,
    column::Column,
    sink,
    sql_func::{Kind, SqlFunction},
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use sqlaero_core::bind::{
    sql_func::{Part, Role},
    writer,
};

pub(super) enum Output<'a> {
    Text(&'a mut Vec<u8>),
    Parameter(&'a mut dyn sink::ParameterSink),
}

impl Visit<'_> {
    /// Both text and native parameters use the actual expression base type.
    /// A native parameter rejects the object before traversing its operands.
    pub(super) fn expression(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: Output<'_>,
        operand: Option<Role>,
    ) -> PyResult<bool> {
        let Ok(expression) = value.cast::<SqlFunction>() else {
            return Ok(false);
        };
        let py = value.py();
        let output = match output {
            Output::Text(output) => output,
            Output::Parameter(sink) => sink
                .expression_output(&self.context)
                .map_err(|error| sink::failure(py, error))?
                .ok_or_else(|| sink::fragment_error(py))?,
        };
        let expression = SqlFunction::snapshot(expression)?;
        let allowed = match operand {
            None => true,
            Some(Role::Scalar) => expression.kind == Kind::Scalar,
            Some(Role::Interval) => expression.kind == Kind::Interval,
            Some(Role::Value | Role::Row | Role::Identifier) => false,
        };
        if !allowed {
            return Err(errors::builtin_format::<PyTypeError>(
                py,
                format_args!(
                    "{} is not valid in this SQL operand position",
                    expression.name
                ),
            ));
        }
        check(py, self.budget)?;
        self.enter(py, value.as_ptr() as usize)?;
        let result = (|| {
            for part in expression.form.parts(expression.operands.len()) {
                match part {
                    Part::Syntax(text) => self.fixed(py, text, output)?,
                    Part::Option(keyword) => self.fixed(py, keyword.as_str(), output)?,
                    Part::Operand(index, role) => {
                        let argument = expression.operands.get_item(index)?;
                        let result = self.operand(&argument, output, index, role);
                        if let Err(error) = result {
                            errors::note_format(
                                py,
                                &error,
                                format_args!(
                                    "while writing {} argument {}",
                                    expression.name,
                                    index + 1
                                ),
                            );
                            return Err(error);
                        }
                    }
                }
            }
            Ok(())
        })();
        self.active.pop();
        self.active_depth -= 1;
        result?;
        Ok(true)
    }

    fn operand(
        &mut self,
        argument: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        index: usize,
        role: Role,
    ) -> PyResult<()> {
        if role == Role::Identifier
            || (role == Role::Scalar && argument.is_exact_instance_of::<Column>())
        {
            let column = argument.cast_exact::<Column>()?;
            return column
                .get()
                .write(argument.py(), output, &self.context, self.budget);
        }
        self.value_with(argument, output, Some(index), Some(role), |_| ())
    }

    fn fixed(&self, py: Python<'_>, text: &str, output: &mut Vec<u8>) -> PyResult<()> {
        writer::append(output, text.as_bytes(), self.context.limit)
            .map_err(|error| errors::wire(py, error))
    }

    pub(super) fn allow_row(&self, py: Python<'_>, role: Option<Role>) -> PyResult<()> {
        if matches!(role, Some(Role::Scalar | Role::Value | Role::Interval)) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "this SQL operand does not accept a row",
            ));
        }
        Ok(())
    }
}

pub(super) fn requires_row(py: Python<'_>) -> PyErr {
    errors::builtin_message::<PyTypeError>(py, "this SQL operand requires a row")
}
