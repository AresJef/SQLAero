//! Private native expression identity and public function constructors.

use super::sql_intv::SqlInterval;
use crate::errors;
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple};
use sqlaero_core::bind::sql_func::{self, Form, Keyword, Part, Role};
use sqlaero_core::bind::sql_intv::EXTRACT_UNITS;

/// The base has no Python constructor or public module export. Concrete
/// classes are final and initialize the recipe through their own constructors.
#[pyclass(subclass, name = "_SQLFunction", module = "sqlaero._sqlaero")]
pub(crate) struct SqlFunction {
    name: &'static str,
    kind: Kind,
    form: Form,
    operands: Option<Py<PyTuple>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Scalar,
    Interval,
}

pub(crate) struct Expression<'py> {
    pub(crate) name: &'static str,
    pub(crate) kind: Kind,
    pub(crate) form: Form,
    pub(crate) operands: Bound<'py, PyTuple>,
}

impl SqlFunction {
    pub(crate) fn new(
        name: &'static str,
        kind: Kind,
        form: Form,
        operands: Bound<'_, PyTuple>,
    ) -> PyResult<Self> {
        for part in form.parts(operands.len()) {
            if let Part::Operand(index, role) = part {
                let value = operands.get_item(index)?;
                validate_role(name, index, role, &value)?;
            }
        }
        Ok(Self {
            name,
            kind,
            form,
            operands: Some(operands.unbind()),
        })
    }

    pub(crate) fn snapshot<'py>(value: &Bound<'py, Self>) -> PyResult<Expression<'py>> {
        let expression = value.try_borrow()?;
        let py = value.py();
        let operands = expression.operands.as_ref().ok_or_else(|| {
            errors::builtin_message::<PyValueError>(py, "SQL expression operands have been cleared")
        })?;
        Ok(Expression {
            name: expression.name,
            kind: expression.kind,
            form: expression.form.clone(),
            operands: operands.bind(py).clone(),
        })
    }
}

#[pymethods]
impl SqlFunction {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.operands)
    }

    fn __clear__(&mut self) {
        self.operands = None;
    }
}

fn validate_role(name: &str, index: usize, role: Role, value: &Bound<'_, PyAny>) -> PyResult<()> {
    let expected = match role {
        Role::Interval if !value.is_instance_of::<SqlInterval>() => "an SQL interval",
        Role::Scalar | Role::Value
            if value.is_instance_of::<PyList>()
                || value.is_exact_instance_of::<PyTuple>()
                || value.is_instance_of::<SqlInterval>() =>
        {
            "one scalar value or expression"
        }
        Role::Value if value.is_instance_of::<SqlFunction>() => "one data value",
        _ => return Ok(()),
    };
    Err(errors::builtin_format::<PyTypeError>(
        value.py(),
        format_args!("{name} argument {} requires {expected}", index + 1),
    ))
}

pub(super) fn keyword(
    name: &str,
    parameter: &str,
    value: &Bound<'_, PyAny>,
    allowed: &'static [&'static str],
) -> PyResult<Keyword> {
    let text = value.cast::<PyString>().map_err(|_| {
        errors::builtin_format::<PyTypeError>(
            value.py(),
            format_args!("{name} {parameter} must be str"),
        )
    })?;
    Keyword::select(text.to_str()?, allowed).ok_or_else(|| {
        errors::builtin_format::<PyValueError>(
            value.py(),
            format_args!("{name} does not support the supplied {parameter}"),
        )
    })
}

/// Return the absolute value of a scalar expression.
#[pyclass(extends = SqlFunction, name = "ABS", module = "sqlaero.sqlfunc")]
struct Abs;

#[pymethods]
impl Abs {
    #[new]
    fn new(value: Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let operands = PyTuple::new(value.py(), [value])?;
        let expression = SqlFunction::new("ABS", Kind::Scalar, Form::Call("ABS("), operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Extract the date part of a date or datetime expression.
#[pyclass(extends = SqlFunction, name = "DATE", module = "sqlaero.sqlfunc")]
struct Date;

#[pymethods]
impl Date {
    #[new]
    fn new(value: Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let operands = PyTuple::new(value.py(), [value])?;
        let expression = SqlFunction::new("DATE", Kind::Scalar, Form::Call("DATE("), operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Extract the requested MySQL temporal unit from a scalar expression.
#[pyclass(extends = SqlFunction, name = "EXTRACT", module = "sqlaero.sqlfunc")]
struct Extract;

#[pymethods]
impl Extract {
    #[new]
    fn new<'py>(
        unit: &Bound<'py, PyAny>,
        value: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let unit = keyword("EXTRACT", "unit", unit, EXTRACT_UNITS)?;
        let operands = PyTuple::new(value.py(), [value])?;
        let form = Form::Dynamic(sql_func::extract(unit).into());
        let expression = SqlFunction::new("EXTRACT", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Add one MySQL interval to a date or datetime expression.
#[pyclass(extends = SqlFunction, name = "DATE_ADD", module = "sqlaero.sqlfunc")]
struct DateAdd;

#[pymethods]
impl DateAdd {
    #[new]
    fn new<'py>(
        date: Bound<'py, PyAny>,
        interval: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&[
            Part::Syntax("DATE_ADD("),
            Part::Operand(0, Role::Scalar),
            Part::Syntax(","),
            Part::Operand(1, Role::Interval),
            Part::Syntax(")"),
        ]);
        let operands = PyTuple::new(date.py(), [date, interval])?;
        let expression = SqlFunction::new("DATE_ADD", Kind::Scalar, FORM, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Abs>()?;
    module.add_class::<Date>()?;
    module.add_class::<Extract>()?;
    module.add_class::<DateAdd>()
}
