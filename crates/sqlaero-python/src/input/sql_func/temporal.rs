//! MySQL temporal function bodies and their controlled structural arguments.

use super::super::{sql_intv::SqlInterval, sql_type};
use super::calls::{fixed_calls, optional_calls};
use super::{Kind, SqlFunction, keyword};
use crate::config::Argument;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::{Form, Part, Role, date_shift};

fixed_calls! {
    Addtime, "ADDTIME", (value, time), "Add a time value to a time or datetime.";
    ConvertTz, "CONVERT_TZ", (value, from_zone, to_zone), "Convert a datetime from one time zone to another.";
    CurrentDate, "CURRENT_DATE", (), "Return the current date in the session time zone." => [Curdate, "CURDATE"];
    DateFormat, "DATE_FORMAT", (value, format), "Format a date or datetime with a MySQL format string.";
    DateDiff, "DATEDIFF", (left, right), "Return left minus right as a difference in calendar days.";
    Day, "DAY", (value), "Return the day of the month." => [DayOfMonth, "DAYOFMONTH"];
    DayName, "DAYNAME", (value), "Return the localized name of the weekday.";
    DayOfWeek, "DAYOFWEEK", (value), "Return the weekday index, with Sunday as 1.";
    DayOfYear, "DAYOFYEAR", (value), "Return the day of the year starting at 1.";
    FromDays, "FROM_DAYS", (days), "Convert a MySQL day number to a date.";
    Hour, "HOUR", (value), "Extract the hour component of a time or datetime.";
    LastDay, "LAST_DAY", (value), "Return the date of the last day of the input month.";
    MakeDate, "MAKEDATE", (year, day_of_year), "Make a date from a year and day of the year.";
    MakeTime, "MAKETIME", (hour, minute, second), "Make a time from hours, minutes, and seconds.";
    Microsecond, "MICROSECOND", (value), "Extract the microsecond component of a temporal value.";
    Minute, "MINUTE", (value), "Extract the minute component of a temporal value.";
    Month, "MONTH", (value), "Return the number of the month.";
    MonthName, "MONTHNAME", (value), "Return the localized name of the month.";
    PeriodAdd, "PERIOD_ADD", (period, months), "Add months to a MySQL year-month period.";
    PeriodDiff, "PERIOD_DIFF", (left, right), "Return the difference in months between two year-month periods.";
    Quarter, "QUARTER", (value), "Return the quarter of the year.";
    SecToTime, "SEC_TO_TIME", (seconds), "Convert a number of seconds to a MySQL time.";
    Second, "SECOND", (value), "Extract the seconds component of a temporal value.";
    StrToDate, "STR_TO_DATE", (value, format), "Parse a string using a MySQL temporal format.";
    Subtime, "SUBTIME", (value, time), "Subtract a time value from a time or datetime.";
    Time, "TIME", (value), "Extract the time component of a temporal expression.";
    TimeFormat, "TIME_FORMAT", (value, format), "Format a time with a MySQL format string.";
    TimeToSec, "TIME_TO_SEC", (value), "Convert a time to a number of seconds.";
    TimeDiff, "TIMEDIFF", (left, right), "Return the difference between two times or datetimes.";
    ToDays, "TO_DAYS", (value), "Return the MySQL day number for a date.";
    ToSeconds, "TO_SECONDS", (value), "Return the number of seconds since year zero.";
    UtcDate, "UTC_DATE", (), "Return the current date in UTC.";
    Weekday, "WEEKDAY", (value), "Return the weekday index, with Monday as 0.";
    WeekOfYear, "WEEKOFYEAR", (value), "Return the ISO week number of a date.";
    Year, "YEAR", (value), "Return the year of a date or datetime.";
}

optional_calls! {
    FromUnixtime, "FROM_UNIXTIME", (seconds), (format), "(seconds, format=...)",
        "Convert Unix epoch seconds to the session time zone, optionally formatting the result.";
    Timestamp, "TIMESTAMP", (value), (time), "(value, time=...)",
        "Return a datetime, optionally adding a time value.";
    UnixTimestamp, "UNIX_TIMESTAMP", (), (value), "(value=...)",
        "Return Unix epoch seconds for a session datetime or for the current time.";
    Week, "WEEK", (value), (mode), "(value, mode=...)",
        "Return the week number using MySQL's optional week mode.";
    YearWeek, "YEARWEEK", (value), (mode), "(value, mode=...)",
        "Return the corresponding year and week number using MySQL's optional week mode.";
}

macro_rules! define_clock_call {
    ($class:ident, $name:literal, $sql_name:literal, $description:literal) => {
        #[doc = $description]
        #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
        struct $class;

        #[pymethods]
        impl $class {
            #[new]
            #[pyo3(signature = (fsp=Argument::default()), text_signature = "(fsp=...)")]
            fn new(py: Python<'_>, fsp: Argument<'_>) -> PyResult<PyClassInitializer<Self>> {
                const FRACTION: [&[Part]; 7] = [
                    &[Part::Syntax(concat!($sql_name, "(0)"))],
                    &[Part::Syntax(concat!($sql_name, "(1)"))],
                    &[Part::Syntax(concat!($sql_name, "(2)"))],
                    &[Part::Syntax(concat!($sql_name, "(3)"))],
                    &[Part::Syntax(concat!($sql_name, "(4)"))],
                    &[Part::Syntax(concat!($sql_name, "(5)"))],
                    &[Part::Syntax(concat!($sql_name, "(6)"))],
                ];
                let normalized = sql_type::number(fsp.0.as_ref(), concat!($name, " fsp"), 0..=6)?;
                let form = normalized.map_or(Form::Call(concat!($sql_name, "(")), |digits| {
                    Form::Static(FRACTION[digits as usize])
                });
                let expression = SqlFunction::new($name, Kind::Scalar, form, PyTuple::empty(py))?;
                Ok(PyClassInitializer::from(expression).add_subclass(Self))
            }
        }
    };
}

macro_rules! clock_calls {
    ($( $class:ident, $name:literal, $description:literal $(=> [$($alias:ident, $alias_name:literal),+])?; )+) => {
        $(
            define_clock_call!($class, $name, $name, $description);
            $($(define_clock_call!($alias, $alias_name, $name, $description);)+)?
        )+

        fn register_clocks(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(
                module.add_class::<$class>()?;
                $($(module.add_class::<$alias>()?;)+)?
            )+
            Ok(())
        }
    };
}

clock_calls! {
    CurrentTime, "CURRENT_TIME", "Return the current session time with optional fractional precision." => [Curtime, "CURTIME"];
    CurrentTimestamp, "CURRENT_TIMESTAMP", "Return the current session datetime with optional fractional precision.";
    Now, "NOW", "Return the session datetime at statement start with optional fractional precision." => [Localtime, "LOCALTIME", LocalTimestamp, "LOCALTIMESTAMP"];
    Sysdate, "SYSDATE", "Return the system datetime at invocation with optional fractional precision.";
    UtcTime, "UTC_TIME", "Return the current UTC time with optional fractional precision.";
    UtcTimestamp, "UTC_TIMESTAMP", "Return the current UTC datetime with optional fractional precision.";
}

macro_rules! date_quantity_calls {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new<'py>(date: Bound<'py, PyAny>, days: Bound<'py, PyAny>) -> PyResult<PyClassInitializer<Self>> {
                    const INTERVAL: Form = Form::Static(&date_shift(concat!($name, "("), Role::Interval));
                    let form = if days.is_instance_of::<SqlInterval>() {
                        INTERVAL
                    } else {
                        Form::Call(concat!($name, "("))
                    };
                    let operands = PyTuple::new(date.py(), [date, days])?;
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
    };
}

date_quantity_calls! {
    AddDate, "ADDDATE", "Add a number of days or an explicit MySQL interval to a date.";
    SubDate, "SUBDATE", "Subtract a number of days or an explicit MySQL interval from a date.";
}

/// Subtract an explicit MySQL interval from a date or datetime.
#[pyclass(extends = SqlFunction, name = "DATE_SUB", module = "sqlaero.sqlfunc")]
struct DateSub;

#[pymethods]
impl DateSub {
    #[new]
    fn new<'py>(
        date: Bound<'py, PyAny>,
        interval: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&date_shift("DATE_SUB(", Role::Interval));
        let operands = PyTuple::new(date.py(), [date, interval])?;
        let expression = SqlFunction::new("DATE_SUB", Kind::Scalar, FORM, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

const FORMAT_TYPES: &[&str] = &["DATE", "TIME", "DATETIME", "TIMESTAMP"];

/// Return a MySQL format string for the given temporal type and format name.
#[pyclass(extends = SqlFunction, name = "GET_FORMAT", module = "sqlaero.sqlfunc")]
struct GetFormat;

#[pymethods]
impl GetFormat {
    #[new]
    fn new<'py>(
        r#type: &Bound<'py, PyAny>,
        format: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let data_type = keyword("GET_FORMAT", "type", r#type, FORMAT_TYPES)?;
        let form = Form::Dynamic(
            [
                Part::Syntax("GET_FORMAT("),
                Part::Option(data_type),
                Part::Syntax(","),
                Part::Operand(0, Role::Scalar),
                Part::Syntax(")"),
            ]
            .into(),
        );
        let operands = PyTuple::new(format.py(), [format])?;
        let expression = SqlFunction::new("GET_FORMAT", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

const TIMESTAMP_UNITS: &[&str] = &[
    "MICROSECOND",
    "SECOND",
    "MINUTE",
    "HOUR",
    "DAY",
    "WEEK",
    "MONTH",
    "QUARTER",
    "YEAR",
    "SQL_TSI_MICROSECOND",
    "SQL_TSI_SECOND",
    "SQL_TSI_MINUTE",
    "SQL_TSI_HOUR",
    "SQL_TSI_DAY",
    "SQL_TSI_WEEK",
    "SQL_TSI_MONTH",
    "SQL_TSI_QUARTER",
    "SQL_TSI_YEAR",
];

macro_rules! timestamp_units_calls {
    ($( $class:ident, $name:literal, ($from:ident, $to:ident), $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new<'py>(
                    unit: &Bound<'py, PyAny>,
                    $from: Bound<'py, PyAny>,
                    $to: Bound<'py, PyAny>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let unit = keyword($name, "unit", unit, TIMESTAMP_UNITS)?;
                    let form = Form::Dynamic(
                        [
                            Part::Syntax(concat!($name, "(")),
                            Part::Option(unit),
                            Part::Syntax(","),
                            Part::Operand(0, Role::Scalar),
                            Part::Syntax(","),
                            Part::Operand(1, Role::Scalar),
                            Part::Syntax(")"),
                        ]
                        .into(),
                    );
                    let operands = PyTuple::new($from.py(), [$from, $to])?;
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
    };
}

timestamp_units_calls! {
    TimestampAdd, "TIMESTAMPADD", (quantity, value), "Add an integer quantity in a selected unit to a temporal value.";
    TimestampDiff, "TIMESTAMPDIFF", (start, end), "Return end minus start as a count of completed units.";
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    register_clocks(module)?;
    module.add_class::<AddDate>()?;
    module.add_class::<SubDate>()?;
    module.add_class::<DateSub>()?;
    module.add_class::<GetFormat>()?;
    module.add_class::<TimestampAdd>()?;
    module.add_class::<TimestampDiff>()
}
