//! Native interval constructors using the same expression identity as functions.

use super::control::Checkpoint;
use super::sql_func::{Kind, SqlFunction};
use super::{check, integer, primitive};
use crate::errors;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyTuple};
use smallvec::SmallVec;
use sqlaero_core::bind::sql_func::{Form, Part};
use sqlaero_core::bind::sql_intv::{self, Component, Compound, CompoundError, Reduce};
use sqlaero_core::bind::value::ValueRef;
use sqlaero_core::error::ClientErrorCode;

#[pyclass(subclass, extends = SqlFunction, name = "_SQLInterval", module = "sqlaero._sqlaero")]
pub(crate) struct SqlInterval;

macro_rules! single_units {
    ($( $class:ident, $unit:literal, $quantity:ident, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlInterval, name = $unit, module = "sqlaero.sqlintv")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new($quantity: Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
                    const FORM: Form = Form::Static(&sql_intv::single(concat!(" ", $unit)));
                    let operands = PyTuple::new($quantity.py(), [$quantity])?;
                    let expression = SqlFunction::new($unit, Kind::Interval, FORM, operands)?;
                    Ok(PyClassInitializer::from(expression)
                        .add_subclass(SqlInterval)
                        .add_subclass(Self))
                }
            }
        )+

        pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
            let intervals = PyModule::new(module.py(), "_sqlintv")?;
            $(intervals.add_class::<$class>()?;)+
            register_compound(&intervals)?;
            module.add_submodule(&intervals)
        }
    };
}

single_units! {
    Year, "YEAR", years, "Create a MySQL interval measured in calendar years.";
    Quarter, "QUARTER", quarters, "Create a MySQL interval measured in calendar quarters.";
    Month, "MONTH", months, "Create a MySQL interval measured in calendar months.";
    Week, "WEEK", weeks, "Create a MySQL interval measured in weeks.";
    Day, "DAY", days, "Create a MySQL interval measured in days.";
    Hour, "HOUR", hours, "Create a MySQL interval measured in hours.";
    Minute, "MINUTE", minutes, "Create a MySQL interval measured in minutes.";
    Second, "SECOND", seconds, "Create a MySQL interval measured in seconds.";
    Microsecond, "MICROSECOND", microseconds, "Create a MySQL interval measured in microseconds.";
}

macro_rules! compound_units {
    ($( $class:ident, $unit:literal, ($first:ident $(, $quantity:ident)+), $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlInterval, name = $unit, module = "sqlaero.sqlintv")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new<'py>(
                    $first: Bound<'py, PyAny>,
                    $($quantity: Bound<'py, PyAny>),+
                ) -> PyResult<PyClassInitializer<Self>> {
                    const FORM: Form = Form::Static(&[Part::Compound(Compound::$class)]);
                    let py = $first.py();
                    let operands = [$first, $($quantity),+];
                    for (index, operand) in operands.iter().enumerate() {
                        validate_integer(operand, $unit, index)?;
                    }
                    let operands = PyTuple::new(py, operands)?;
                    let expression = SqlFunction::new($unit, Kind::Interval, FORM, operands)?;
                    Ok(PyClassInitializer::from(expression)
                        .add_subclass(SqlInterval)
                        .add_subclass(Self))
                }
            }
        )+

        fn register_compound(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

compound_units! {
    YearMonth, "YEAR_MONTH", (years, months), "Create a calendar interval from signed years and months.";
    DayHour, "DAY_HOUR", (days, hours), "Create an interval from signed days and hours.";
    DayMinute, "DAY_MINUTE", (days, hours, minutes), "Create an interval from signed days, hours, and minutes.";
    DaySecond, "DAY_SECOND", (days, hours, minutes, seconds), "Create an interval from signed days, hours, minutes, and seconds.";
    DayMicrosecond, "DAY_MICROSECOND", (days, hours, minutes, seconds, microseconds), "Create an interval from signed days through microseconds.";
    HourMinute, "HOUR_MINUTE", (hours, minutes), "Create an interval from signed hours and minutes.";
    HourSecond, "HOUR_SECOND", (hours, minutes, seconds), "Create an interval from signed hours, minutes, and seconds.";
    HourMicrosecond, "HOUR_MICROSECOND", (hours, minutes, seconds, microseconds), "Create an interval from signed hours through microseconds.";
    MinuteSecond, "MINUTE_SECOND", (minutes, seconds), "Create an interval from signed minutes and seconds.";
    MinuteMicrosecond, "MINUTE_MICROSECOND", (minutes, seconds, microseconds), "Create an interval from signed minutes, seconds, and microseconds.";
    SecondMicrosecond, "SECOND_MICROSECOND", (seconds, microseconds), "Create an interval from signed seconds and microseconds.";
}

fn validate_integer(value: &Bound<'_, PyAny>, unit: &str, index: usize) -> PyResult<()> {
    if !value.is_instance_of::<PyInt>() || value.is_exact_instance_of::<PyBool>() {
        return Err(errors::builtin_format::<PyTypeError>(
            value.py(),
            format_args!("{unit} component {} must be an integer", index + 1),
        ));
    }
    Ok(())
}

enum Integer<'a, 'py> {
    Compact(i64),
    Wide(integer::Export<'a, 'py>),
}

pub(crate) fn write_compound(
    unit: Compound,
    operands: &Bound<'_, PyTuple>,
    output: &mut Vec<u8>,
    limit: usize,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let py = operands.py();
    let values: SmallVec<[Bound<'_, PyAny>; 5]> = operands.iter().collect();
    let mut integers = SmallVec::<[Integer<'_, '_>; 5]>::new();
    for value in &values {
        let integer = match primitive::read(value)? {
            Some(primitive::Source::Native(ValueRef::Signed(value))) => Integer::Compact(value),
            Some(primitive::Source::WideInteger { value, .. }) => {
                Integer::Wide(integer::Export::new(value)?)
            }
            _ => return Err(compound_error(py, unit, CompoundError::Layout)),
        };
        integers.push(integer);
    }
    let mut components = SmallVec::<[Component<'_>; 5]>::new();
    for integer in &integers {
        components.push(match integer {
            Integer::Compact(value) => Component::Compact(*value),
            Integer::Wide(export) => match export.value()? {
                integer::Value::Compact(value) => Component::Compact(value),
                integer::Value::Digits { negative, source } => Component::Wide {
                    negative,
                    digits: source,
                },
            },
        });
    }
    let mut state =
        Reduce::new(unit, &components).map_err(|error| compound_error(py, unit, error))?;
    loop {
        check(py, budget)?;
        if let Some(interval) = state
            .advance(512)
            .map_err(|error| compound_error(py, unit, error))?
        {
            return interval
                .write(output, limit)
                .map_err(|error| errors::wire(py, error));
        }
    }
}

fn compound_error(py: Python<'_>, unit: Compound, error: CompoundError) -> PyErr {
    match error {
        CompoundError::Range => errors::client_format(
            py,
            ClientErrorCode::NumericRange,
            format_args!("{} {error}", unit.name()),
        ),
        CompoundError::Layout => errors::client_format(
            py,
            ClientErrorCode::InternalState,
            format_args!("{} {error}", unit.name()),
        ),
        CompoundError::Integer(error) => integer::failure(py, error),
    }
}
