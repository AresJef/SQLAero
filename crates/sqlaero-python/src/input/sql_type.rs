//! Checked input descriptors for MySQL conversion targets.

use super::json::Json;
use crate::config::{self, Argument};
use crate::errors;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyString};
use sqlaero_core::bind::{
    sql_func::Keyword,
    sql_type::{Name, Target},
};
use std::ops::RangeInclusive;

#[pyclass(subclass, frozen, name = "_SQLType", module = "sqlaero._sqlaero")]
struct SqlType(Target);

macro_rules! fixed_targets {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlType, name = $name, module = "sqlaero.sqltype")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new() -> PyClassInitializer<Self> {
                    PyClassInitializer::from(SqlType(Target::plain(Name::$class))).add_subclass(Self)
                }
            }
        )+

        fn register_fixed(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }

        fn fixed_default(value: &Bound<'_, PyAny>) -> Option<Target> {
            let py = value.py();
            $(if value.is(&py.get_type::<$class>()) { return Some(Target::plain(Name::$class)); })+
            None
        }
    };
}

fixed_targets! {
    Double, "DOUBLE", "Use MySQL DOUBLE as the conversion target.";
    Real, "REAL", "Use MySQL REAL, whose precision follows REAL_AS_FLOAT mode.";
    Date, "DATE", "Use MySQL DATE as the conversion target.";
    Year, "YEAR", "Use MySQL YEAR as the conversion target.";
    Point, "POINT", "Use MySQL POINT as the geometry conversion target.";
    LineString, "LINESTRING", "Use MySQL LINESTRING as the geometry conversion target.";
    Polygon, "POLYGON", "Use MySQL POLYGON as the geometry conversion target.";
    MultiPoint, "MULTIPOINT", "Use MySQL MULTIPOINT as the geometry conversion target.";
    MultiLineString, "MULTILINESTRING", "Use MySQL MULTILINESTRING as the geometry conversion target.";
    MultiPolygon, "MULTIPOLYGON", "Use MySQL MULTIPOLYGON as the geometry conversion target.";
    GeometryCollection, "GEOMETRYCOLLECTION", "Use MySQL GEOMETRYCOLLECTION as the geometry conversion target.";
}

macro_rules! sized_targets {
    ($( $class:ident, $name:literal, $parameter:ident, $label:literal, $maximum:expr, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlType, name = $name, module = "sqlaero.sqltype")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = ($parameter=None))]
                fn new($parameter: Option<&Bound<'_, PyAny>>) -> PyResult<PyClassInitializer<Self>> {
                    let first = number($parameter, $label, 0..=$maximum)?;
                    let target = Target { first, ..Target::plain(Name::$class) };
                    Ok(PyClassInitializer::from(SqlType(target)).add_subclass(Self))
                }
            }
        )+

        fn register_sized(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }

        fn sized_default(value: &Bound<'_, PyAny>) -> Option<Target> {
            let py = value.py();
            $(if value.is(&py.get_type::<$class>()) { return Some(Target::plain(Name::$class)); })+
            None
        }
    };
}

sized_targets! {
    Binary, "BINARY", length, "BINARY length", u32::MAX, "Use BINARY with an optional byte length.";
    NChar, "NCHAR", length, "NCHAR length", u32::MAX, "Use NCHAR with an optional character length.";
    Float, "FLOAT", precision, "FLOAT precision", 53, "Use FLOAT with an optional binary precision.";
    Time, "TIME", fsp, "TIME fractional precision", 6, "Use TIME with an optional fractional-second precision.";
    Datetime, "DATETIME", fsp, "DATETIME fractional precision", 6, "Use DATETIME with an optional fractional-second precision.";
}

macro_rules! integer_targets {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlType, name = $name, module = "sqlaero.sqltype")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = (integer=Argument::default()), text_signature = "(integer=False)")]
                fn new(integer: Argument<'_>) -> PyResult<PyClassInitializer<Self>> {
                    let integer = integer.0.as_ref()
                        .map_or(Ok(false), |value| config::boolean(value, "integer"))?;
                    let target = Target { integer, ..Target::plain(Name::$class) };
                    Ok(PyClassInitializer::from(SqlType(target)).add_subclass(Self))
                }
            }
        )+

        fn register_integers(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }

        fn integer_default(value: &Bound<'_, PyAny>) -> Option<Target> {
            let py = value.py();
            $(if value.is(&py.get_type::<$class>()) { return Some(Target::plain(Name::$class)); })+
            None
        }
    };
}

integer_targets! {
    Signed, "SIGNED", "Use SIGNED with an optional INTEGER suffix.";
    Unsigned, "UNSIGNED", "Use UNSIGNED with an optional INTEGER suffix.";
}

/// Use DECIMAL with optional precision and scale.
#[pyclass(extends = SqlType, name = "DECIMAL", module = "sqlaero.sqltype")]
struct Decimal;

#[pymethods]
impl Decimal {
    #[new]
    #[pyo3(signature = (precision=None, scale=None))]
    fn new(
        precision: Option<&Bound<'_, PyAny>>,
        scale: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let first = number(precision, "DECIMAL precision", 1..=65)?;
        if let Some(scale) = scale
            && first.is_none()
        {
            return Err(errors::builtin_message::<PyValueError>(
                scale.py(),
                "DECIMAL scale requires precision",
            ));
        }
        let second = number(scale, "DECIMAL scale", 0..=first.unwrap_or(0).min(30))?;
        let target = Target {
            first,
            second,
            ..Target::plain(Name::Decimal)
        };
        Ok(PyClassInitializer::from(SqlType(target)).add_subclass(Self))
    }
}

/// Use CHAR with an optional length and MySQL charset.
#[pyclass(extends = SqlType, name = "CHAR", module = "sqlaero.sqltype")]
struct Char;

#[pymethods]
impl Char {
    #[new]
    #[pyo3(signature = (length=None, *, charset=None))]
    fn new(
        length: Option<&Bound<'_, PyAny>>,
        charset: Option<&Bound<'_, PyString>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let first = number(length, "CHAR length", 0..=u32::MAX)?;
        let charset = charset
            .map(|name| charset_option(name, "CHAR"))
            .transpose()?;
        let target = Target {
            first,
            charset,
            ..Target::plain(Name::Char)
        };
        Ok(PyClassInitializer::from(SqlType(target)).add_subclass(Self))
    }
}

pub(crate) fn charset_option(value: &Bound<'_, PyString>, function: &str) -> PyResult<Keyword> {
    Keyword::charset(value.to_str()?).ok_or_else(|| {
        errors::builtin_format::<PyValueError>(
            value.py(),
            format_args!("{function} requires a server charset name"),
        )
    })
}

fn number(
    value: Option<&Bound<'_, PyAny>>,
    label: &str,
    range: RangeInclusive<u32>,
) -> PyResult<Option<u32>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let py = value.py();
    let integer = value
        .cast::<PyInt>()
        .ok()
        .filter(|_| !value.is_exact_instance_of::<PyBool>())
        .ok_or_else(|| {
            errors::builtin_format::<PyTypeError>(py, format_args!("{label} must be an integer"))
        })?;
    let number = integer
        .extract::<u32>()
        .ok()
        .filter(|number| range.contains(number))
        .ok_or_else(|| {
            errors::builtin_format::<PyValueError>(
                py,
                format_args!("{label} must be in {}..={}", range.start(), range.end()),
            )
        })?;
    Ok(Some(number))
}

pub(crate) fn target(value: &Bound<'_, PyAny>, function: &str) -> PyResult<Target> {
    let py = value.py();
    if value.is_instance_of::<SqlType>() {
        return Ok(value.cast::<SqlType>()?.get().0);
    }
    for (constructor, name) in [
        (py.get_type::<Json>(), Name::Json),
        (py.get_type::<Decimal>(), Name::Decimal),
        (py.get_type::<Char>(), Name::Char),
    ] {
        if value.is(&constructor) {
            return Ok(Target::plain(name));
        }
    }
    fixed_default(value)
        .or_else(|| sized_default(value))
        .or_else(|| integer_default(value))
        .ok_or_else(|| {
            errors::builtin_format::<PyTypeError>(
                py,
                format_args!("{function} target must be a sqltype descriptor or its default class"),
            )
        })
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let targets = PyModule::new(module.py(), "_sqltype")?;
    register_fixed(&targets)?;
    register_sized(&targets)?;
    register_integers(&targets)?;
    targets.add_class::<Decimal>()?;
    targets.add_class::<Char>()?;
    module.add_submodule(&targets)
}
