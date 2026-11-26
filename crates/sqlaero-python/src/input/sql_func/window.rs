//! Window function bodies. The SQL template supplies the OVER clause.

use super::calls::fixed_calls;
use super::{Kind, SqlFunction, keyword};
use crate::config::Argument;
use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyTuple};
use smallvec::SmallVec;
use sqlaero_core::bind::sql_func::{Form, Keyword, Part, Role};

fixed_calls! {
    CumeDist, "CUME_DIST", (), "Return the current row's cumulative distribution within the partition.";
    DenseRank, "DENSE_RANK", (), "Return the peer-group rank without gaps.";
    PercentRank, "PERCENT_RANK", (), "Return the current row's relative rank within its partition.";
    Rank, "RANK", (), "Return the rank of the current row with gaps for peers.";
    RowNumber, "ROW_NUMBER", (), "Return the sequential number of the current row in its partition.";
}

const MAX_WINDOW_NUMBER: u64 = 1 << 63;
const NULL_TREATMENTS: &[&str] = &["RESPECT NULLS"];
const START_POSITIONS: &[&str] = &["FIRST"];

fn window_integer<'py>(
    value: &Bound<'py, PyAny>,
    function: &str,
    argument: &str,
    minimum: u64,
) -> PyResult<Bound<'py, PyInt>> {
    let py = value.py();
    let integer = value
        .cast::<PyInt>()
        .ok()
        .filter(|_| !value.is_exact_instance_of::<PyBool>())
        .ok_or_else(|| {
            errors::builtin_format::<PyTypeError>(
                py,
                format_args!("{function} {argument} must be an integer literal"),
            )
        })?;
    integer
        .extract::<u64>()
        .ok()
        .filter(|number| (minimum..=MAX_WINDOW_NUMBER).contains(number))
        .map(|number| PyInt::new(py, number))
        .ok_or_else(|| {
            errors::builtin_format::<PyValueError>(
                py,
                format_args!("{function} {argument} must be in {minimum}..={MAX_WINDOW_NUMBER}"),
            )
        })
}

fn null_clause(function: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Keyword>> {
    value
        .map(|value| keyword(function, "null_treatment", value, NULL_TREATMENTS))
        .transpose()
}

fn with_modifiers(
    call: Form,
    count: usize,
    position: Option<Keyword>,
    treatment: Option<Keyword>,
) -> Form {
    if position.is_none() && treatment.is_none() {
        return call;
    }
    let mut parts = SmallVec::<[Part; 11]>::new();
    parts.extend(call.parts(count));
    if let Some(position) = position {
        parts.extend([Part::Syntax(" FROM "), Part::Option(position)]);
    }
    if let Some(treatment) = treatment {
        parts.extend([Part::Syntax(" "), Part::Option(treatment)]);
    }
    Form::Dynamic(parts.into_iter().collect())
}

macro_rules! frame_bound_calls {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = (value, *, null_treatment=None))]
                fn new<'py>(
                    value: Bound<'py, PyAny>, null_treatment: Option<&Bound<'py, PyAny>>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let treatment = null_clause($name, null_treatment)?;
                    let form = with_modifiers(Form::Call(concat!($name, "(")), 1, None, treatment);
                    let operands = PyTuple::new(value.py(), [value])?;
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
    };
}

frame_bound_calls! {
    FirstValue, "FIRST_VALUE", "Return the value for the first row of the window frame.";
    LastValue, "LAST_VALUE", "Return the value for the last row of the window frame.";
}

macro_rules! row_offset_calls {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = (value, offset=Argument::default(), default=Argument::default(), *, null_treatment=None), text_signature = "(value, offset=..., default=..., *, null_treatment=None)")]
                fn new<'py>(
                    value: Bound<'py, PyAny>,
                    offset: Argument<'py>,
                    default: Argument<'py>,
                    null_treatment: Option<&Bound<'py, PyAny>>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let py = value.py();
                    let treatment = null_clause($name, null_treatment)?;
                    let mut operands = SmallVec::<[Bound<'py, PyAny>; 3]>::new();
                    operands.push(value);
                    if let Some(offset) = offset.0 {
                        operands.push(window_integer(&offset, $name, "offset", 0)?.into_any());
                        if let Some(default) = default.0 {
                            operands.push(default);
                        }
                    } else if default.0.is_some() {
                        return Err(errors::builtin_message::<PyTypeError>(
                            py, concat!($name, " default requires an explicit offset"),
                        ));
                    }
                    let form = with_modifiers(
                        Form::Call(concat!($name, "(")), operands.len(), None, treatment,
                    );
                    let expression = SqlFunction::new(
                        $name, Kind::Scalar, form, PyTuple::new(py, operands)?,
                    )?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
    };
}

row_offset_calls! {
    Lag, "LAG", "Return the value a given number of rows before the current row.";
    Lead, "LEAD", "Return the value a given number of rows after the current row.";
}

/// Return the value at a positive position within the window frame.
#[pyclass(extends = SqlFunction, name = "NTH_VALUE", module = "sqlaero.sqlfunc")]
struct NthValue;

#[pymethods]
impl NthValue {
    #[new]
    #[pyo3(signature = (value, position, *, from_position=None, null_treatment=None))]
    fn new<'py>(
        value: Bound<'py, PyAny>,
        position: &Bound<'py, PyAny>,
        from_position: Option<&Bound<'py, PyAny>>,
        null_treatment: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = value.py();
        let row = window_integer(position, "NTH_VALUE", "position", 1)?;
        let starting = from_position
            .map(|position| keyword("NTH_VALUE", "from_position", position, START_POSITIONS))
            .transpose()?;
        let treatment = null_clause("NTH_VALUE", null_treatment)?;
        let form = with_modifiers(Form::Call("NTH_VALUE("), 2, starting, treatment);
        let operands = PyTuple::new(py, [value, row.into_any()])?;
        let expression = SqlFunction::new("NTH_VALUE", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Divide the partition into a positive number of ordered buckets.
#[pyclass(extends = SqlFunction, name = "NTILE", module = "sqlaero.sqlfunc")]
struct Ntile;

#[pymethods]
impl Ntile {
    #[new]
    fn new(buckets: &Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&[
            Part::Syntax("NTILE("),
            Part::Operand(0, Role::Value),
            Part::Syntax(")"),
        ]);
        let buckets = window_integer(buckets, "NTILE", "buckets", 1)?;
        let operands = PyTuple::new(buckets.py(), [buckets])?;
        let expression = SqlFunction::new("NTILE", Kind::Scalar, FORM, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    module.add_class::<FirstValue>()?;
    module.add_class::<LastValue>()?;
    module.add_class::<Lag>()?;
    module.add_class::<Lead>()?;
    module.add_class::<NthValue>()?;
    module.add_class::<Ntile>()
}
