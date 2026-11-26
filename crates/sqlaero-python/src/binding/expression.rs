//! Consume native grammar in the existing text visitor.

use super::Visit;
use crate::errors;
use crate::input::{
    check,
    column::Column,
    custom_escape::{self, CustomEscape},
    sink,
    sql_func::{Kind, SqlFunction},
    sql_intv,
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

impl<'a> Output<'a> {
    fn text(
        self,
        py: Python<'_>,
        context: &sqlaero_core::bind::Context,
    ) -> PyResult<&'a mut Vec<u8>> {
        match self {
            Self::Text(output) => Ok(output),
            Self::Parameter(sink) => sink
                .expression_output(context)
                .map_err(|error| sink::failure(py, error))?
                .ok_or_else(|| sink::fragment_error(py)),
        }
    }
}

impl Visit<'_> {
    /// One input gate covers the library expression family and CustomEscape.
    /// Native prepared parameters reject both before reaching their writer.
    pub(super) fn structured(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: Output<'_>,
        operand: Option<Role>,
    ) -> PyResult<bool> {
        let py = value.py();
        if value.is_exact_instance_of::<CustomEscape>() {
            if !matches!(operand, None | Some(Role::Scalar)) {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "CustomEscape is not valid in this SQL operand position",
                ));
            }
            let output = output.text(py, &self.context)?;
            let converter = value.cast_exact()?;
            let text = match &mut self.conversions {
                Some(conversions) => conversions.convert(self.slot, converter, self.budget)?,
                None => CustomEscape::convert(converter, self.budget)?,
            };
            let start = output.len();
            self.standalone |= !custom_escape::write(&text, output, &self.context, self.budget)?;
            if self.guard_splices {
                let splices = self.splices.get_or_insert_with(Vec::new);
                custom_escape::boundary(py, splices, start, true)?;
                custom_escape::boundary(py, splices, output.len(), true)?;
            }
            return Ok(true);
        }
        if !value.is_instance_of::<SqlFunction>() {
            return Ok(false);
        }
        let output = output.text(py, &self.context)?;
        let expression = SqlFunction::snapshot(value.cast::<SqlFunction>()?)?;
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
                    Part::Target(target) => target
                        .write(output, self.context.limit)
                        .map_err(|error| errors::wire(py, error))?,
                    Part::Compound(unit) => sql_intv::write_compound(
                        unit,
                        &expression.operands,
                        output,
                        self.context.limit,
                        self.budget,
                    )?,
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
