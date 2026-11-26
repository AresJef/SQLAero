//! Aggregate expressions and GROUP_CONCAT's identifier and value clauses.

use super::calls::{fixed_calls, prefix_arguments, variadic_calls};
use super::{Kind, SqlFunction, keyword};
use crate::config::Argument;
use crate::errors;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple};
use smallvec::SmallVec;
use sqlaero_core::bind::sql_func::{Form, Part, Role};

fixed_calls! {
    AnyValue, "ANY_VALUE", (value), "Return a value without an ONLY_FULL_GROUP_BY check.";
    BitAnd, "BIT_AND", (value), "Return the bitwise AND over the values in a group.";
    BitOr, "BIT_OR", (value), "Return the bitwise OR over the values in a group.";
    BitXor, "BIT_XOR", (value), "Return the bitwise XOR over the values in a group.";
    JsonArrayAgg, "JSON_ARRAYAGG", (value), "Aggregate the group values into one JSON array.";
    JsonObjectAgg, "JSON_OBJECTAGG", (key, value), "Aggregate keys and values into one JSON object.";
    StddevPop, "STDDEV_POP", (value), "Return the population standard deviation of the group.";
    StddevSamp, "STDDEV_SAMP", (value), "Return the sample standard deviation of the group.";
    VarPop, "VAR_POP", (value), "Return the population variance of the group.";
    VarSamp, "VAR_SAMP", (value), "Return the sample variance of the group.";
}

variadic_calls! {
    Grouping, "GROUPING", (value), values, "Return a bit mask identifying rollup values for the given grouping expressions.";
}

pub(super) fn distinct_flag(distinct: Argument<'_>) -> PyResult<bool> {
    distinct
        .0
        .as_ref()
        .map_or(Ok(false), |flag| crate::config::boolean(flag, "distinct"))
}

macro_rules! distinct_calls {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = (value, *, distinct=crate::config::Argument::default()), text_signature = "(value, *, distinct=False)")]
                fn new<'py>(
                    value: Bound<'py, PyAny>,
                    distinct: crate::config::Argument<'py>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let form = if super::aggregate::distinct_flag(distinct)? {
                        Form::Static(&[
                            Part::Syntax(concat!($name, "(DISTINCT ")),
                            Part::Operand(0, Role::Scalar),
                            Part::Syntax(")"),
                        ])
                    } else {
                        Form::Call(concat!($name, "("))
                    };
                    let operands = PyTuple::new(value.py(), [value])?;
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+

        fn register_distinct(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

pub(super) use distinct_calls;

self::distinct_calls! {
    Avg, "AVG", "Return the average of the group, optionally over distinct values.";
    Max, "MAX", "Return the largest group value. DISTINCT does not affect the result.";
    Min, "MIN", "Return the smallest group value. DISTINCT does not affect the result.";
    Sum, "SUM", "Return the sum of the group, optionally over distinct values.";
}

/// Count all rows, non-NULL values, or distinct tuples of non-NULL values.
///
/// With no arguments COUNT() represents SQL COUNT(*).
#[pyclass(extends = SqlFunction, name = "COUNT", module = "sqlaero.sqlfunc")]
struct Count;

#[pymethods]
impl Count {
    #[new]
    #[pyo3(signature = (*values, distinct=Argument::default()), text_signature = "(*values, distinct=False)")]
    fn new(
        values: &Bound<'_, PyTuple>,
        distinct: Argument<'_>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let distinct = distinct_flag(distinct)?;
        let form = match (values.len(), distinct) {
            (0, false) => Form::Static(&[Part::Syntax("COUNT(*)")]),
            (1, false) => Form::Call("COUNT("),
            (1.., true) => Form::Call("COUNT(DISTINCT "),
            _ => {
                return Err(errors::builtin_message::<PyTypeError>(
                    values.py(),
                    "COUNT accepts at most one value unless distinct=True, which requires at least one",
                ));
            }
        };
        let expression = SqlFunction::new("COUNT", Kind::Scalar, form, values.clone())?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

const DIRECTIONS: &[&str] = &["ASC", "DESC"];

fn sort_entries<'py>(order_by: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyTuple>> {
    if let Ok(tuple) = order_by.cast::<PyTuple>() {
        return Ok(tuple.clone());
    }
    let list = order_by.cast::<PyList>().map_err(|_| {
        errors::builtin_message::<PyTypeError>(
            order_by.py(),
            "GROUP_CONCAT order_by requires a list or tuple of (Column, direction) pairs",
        )
    })?;
    // SAFETY: The caller is attached and holds the list. CPython takes an
    // owned snapshot, synchronizing list access on free-threaded builds.
    let tuple = unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(list.py(), pyo3::ffi::PyList_AsTuple(list.as_ptr()))?
    };
    Ok(tuple.cast_into::<PyTuple>()?)
}

/// Concatenate non-NULL values with an optional explicit order and separator.
///
/// Each order_by entry is a (Column, direction) pair. The direction may be
/// ASC or DESC. When omitted, the separator is a comma.
#[pyclass(extends = SqlFunction, name = "GROUP_CONCAT", module = "sqlaero.sqlfunc")]
struct GroupConcat;

#[pymethods]
impl GroupConcat {
    #[new]
    #[pyo3(signature = (value, *values, distinct=Argument::default(), order_by=None, separator=None), text_signature = "(value, *values, distinct=False, order_by=None, separator=None)")]
    fn new<'py>(
        py: Python<'py>,
        value: Bound<'py, PyAny>,
        values: &Bound<'py, PyTuple>,
        distinct: Argument<'py>,
        order_by: Option<&Bound<'py, PyAny>>,
        separator: Option<Bound<'py, PyString>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let distinct = distinct_flag(distinct)?;
        let call = Form::Call(if distinct {
            "GROUP_CONCAT(DISTINCT "
        } else {
            "GROUP_CONCAT("
        });
        let value_count = values.len() + 1;
        let ordered = order_by.map(sort_entries).transpose()?;
        let ordered = ordered.filter(|keys| !keys.is_empty());
        let expression = if ordered.is_none() && separator.is_none() {
            SqlFunction::new(
                "GROUP_CONCAT",
                Kind::Scalar,
                call,
                prefix_arguments(py, [value], values)?,
            )?
        } else {
            let key_count = ordered.as_ref().map_or(0, PyTupleMethods::len);
            let mut parts = SmallVec::<[Part; 12]>::new();
            let mut operands = SmallVec::<[Bound<'py, PyAny>; 6]>::new();
            parts
                .try_reserve_exact(2 * value_count + 4 * key_count + 3)
                .map_err(|_| errors::memory_error(py))?;
            operands
                .try_reserve_exact(value_count + key_count + 1)
                .map_err(|_| errors::memory_error(py))?;
            parts.extend(call.parts(value_count).take(2 * value_count));
            operands.push(value);
            for (index, argument) in values.iter().enumerate() {
                if index.is_multiple_of(16_384) {
                    py.check_signals()?;
                }
                operands.push(argument);
            }
            if let Some(keys) = ordered {
                parts.push(Part::Syntax(" ORDER BY "));
                for (index, entry) in keys.iter().enumerate() {
                    if index.is_multiple_of(16_384) {
                        py.check_signals()?;
                    }
                    let invalid = || {
                        errors::builtin_message::<PyTypeError>(
                            py,
                            "GROUP_CONCAT order_by requires (Column, direction) pairs",
                        )
                    };
                    let pair = entry.cast::<PyTuple>().map_err(|_| invalid())?;
                    if pair.len() != 2 {
                        return Err(invalid());
                    }
                    let column = pair.get_item(0)?;
                    let direction =
                        keyword("GROUP_CONCAT", "direction", &pair.get_item(1)?, DIRECTIONS)?;
                    if index != 0 {
                        parts.push(Part::Syntax(","));
                    }
                    parts.extend([
                        Part::Operand(operands.len(), Role::Identifier),
                        Part::Syntax(" "),
                        Part::Option(direction),
                    ]);
                    operands.push(column);
                }
            }
            if let Some(separator) = separator {
                parts.extend([
                    Part::Syntax(" SEPARATOR "),
                    Part::Operand(operands.len(), Role::Value),
                ]);
                operands.push(separator.into_any());
            }
            parts.push(Part::Syntax(")"));
            let form = Form::Dynamic(parts.into_iter().collect());
            SqlFunction::new(
                "GROUP_CONCAT",
                Kind::Scalar,
                form,
                PyTuple::new(py, operands)?,
            )?
        };
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_variadic(module)?;
    register_distinct(module)?;
    module.add_class::<Count>()?;
    module.add_class::<GroupConcat>()
}
