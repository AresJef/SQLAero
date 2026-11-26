//! CAST and CONVERT use native target descriptors and canonical options.

use super::{Kind, SqlFunction};
use crate::config::{self, Argument};
use crate::errors;
use crate::input::sql_type;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};
use sqlaero_core::bind::{
    sql_func::{Form, Part, Role},
    sql_type::Name,
};

/// Convert a value to the selected sqltype target.
///
/// at_utc uses MySQL's UTC TIMESTAMP conversion form and requires DATETIME.
#[pyclass(extends = SqlFunction, name = "CAST", module = "sqlaero.sqlfunc")]
struct Cast;

#[pymethods]
impl Cast {
    #[new]
    #[pyo3(signature = (value, target, *, at_utc=Argument::default()), text_signature = "(value, target, *, at_utc=False)")]
    fn new<'py>(
        value: Bound<'py, PyAny>,
        target: &Bound<'py, PyAny>,
        at_utc: Argument<'py>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = value.py();
        let target = sql_type::target(target, "CAST")?;
        let at_utc = at_utc
            .0
            .as_ref()
            .map_or(Ok(false), |flag| config::boolean(flag, "at_utc"))?;
        let separator = if at_utc {
            if target.name != Name::Datetime {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "CAST at_utc requires a DATETIME target",
                ));
            }
            " AT TIME ZONE '+00:00' AS "
        } else {
            " AS "
        };
        let form = Form::Dynamic(
            [
                Part::Syntax("CAST("),
                Part::Operand(0, Role::Scalar),
                Part::Syntax(separator),
                Part::Target(target),
                Part::Syntax(")"),
            ]
            .into(),
        );
        let operands = PyTuple::new(py, [value])?;
        let expression = SqlFunction::new("CAST", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Convert a value with a sqltype target or transcode it with a MySQL charset.
///
/// Supply exactly one of target and using.
#[pyclass(extends = SqlFunction, name = "CONVERT", module = "sqlaero.sqlfunc")]
struct Convert;

#[pymethods]
impl Convert {
    #[new]
    #[pyo3(signature = (value, target=Argument::default(), *, using=None), text_signature = "(value, target=..., *, using=None)")]
    fn new<'py>(
        value: Bound<'py, PyAny>,
        target: Argument<'py>,
        using: Option<&Bound<'py, PyString>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = value.py();
        let (separator, converted) = match (target.0, using) {
            (Some(target), None) => (",", Part::Target(sql_type::target(&target, "CONVERT")?)),
            (None, Some(using)) => (
                " USING ",
                Part::Option(sql_type::charset_option(using, "CONVERT using")?),
            ),
            _ => {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "CONVERT requires exactly one of target and using",
                ));
            }
        };
        let form = Form::Dynamic(
            [
                Part::Syntax("CONVERT("),
                Part::Operand(0, Role::Scalar),
                Part::Syntax(separator),
                converted,
                Part::Syntax(")"),
            ]
            .into(),
        );
        let operands = PyTuple::new(py, [value])?;
        let expression = SqlFunction::new("CONVERT", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Cast>()?;
    module.add_class::<Convert>()
}
