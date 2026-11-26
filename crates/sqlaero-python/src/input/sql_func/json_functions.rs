//! JSON document expressions, paired arguments, and literal-only clauses.

use super::calls::{fixed_calls, optional_calls, prefix_arguments, variadic_calls};
use super::{Kind, SqlFunction};
use crate::config::Argument;
use crate::errors;
use crate::input::sql_type;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString, PyTuple};
use smallvec::SmallVec;
use sqlaero_core::bind::{
    sql_func::{Form, Part, Role},
    sql_type::Name,
};

fixed_calls! {
    JsonDepth, "JSON_DEPTH", (document), "Return the maximum depth of a JSON document.";
    JsonOverlaps, "JSON_OVERLAPS", (document, other), "Check whether JSON documents share an element or key-value pair.";
    JsonPretty, "JSON_PRETTY", (document), "Return an indented representation of a JSON document.";
    JsonQuote, "JSON_QUOTE", (value), "Quote a string as a JSON string literal.";
    JsonSchemaValid, "JSON_SCHEMA_VALID", (schema, document), "Check a JSON document against a JSON Schema.";
    JsonSchemaValidationReport, "JSON_SCHEMA_VALIDATION_REPORT", (schema, document), "Return the JSON Schema validation report for a document.";
    JsonStorageFree, "JSON_STORAGE_FREE", (document), "Return bytes freed in a JSON column by in-place updates.";
    JsonStorageSize, "JSON_STORAGE_SIZE", (document), "Return the binary storage size of a JSON document.";
    JsonType, "JSON_TYPE", (document), "Return the type name of the JSON value.";
    JsonUnquote, "JSON_UNQUOTE", (value), "Unquote a JSON string to yield a SQL string.";
    JsonValid, "JSON_VALID", (document), "Check whether a value is a valid JSON document.";
}

optional_calls! {
    JsonContains, "JSON_CONTAINS", (document, candidate), (path),
        "(document, candidate, path=...)", "Check whether a JSON document contains a candidate at an optional path.";
    JsonKeys, "JSON_KEYS", (document), (path),
        "(document, path=...)", "Return the keys of a JSON object at an optional path.";
    JsonLength, "JSON_LENGTH", (document), (path),
        "(document, path=...)", "Return the length of a JSON value at an optional path.";
}

variadic_calls! {
    JsonContainsPath, "JSON_CONTAINS_PATH", (document, mode, path), paths, "Check for data at one or all given JSON paths.";
    JsonExtract, "JSON_EXTRACT", (document, path), paths, "Extract JSON data at the specified paths.";
    JsonMergePatch, "JSON_MERGE_PATCH", (document, other), documents, "Merge JSON documents using JSON Merge Patch.";
    JsonMergePreserve, "JSON_MERGE_PRESERVE", (document, other), documents, "Merge JSON documents while preserving conflicting values.";
    JsonRemove, "JSON_REMOVE", (document, path), paths, "Remove JSON values at the specified paths.";
}

fn validate_pairs(name: &str, parameter: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
    if !values.len().is_multiple_of(2) {
        return Err(errors::builtin_format::<PyTypeError>(
            values.py(),
            format_args!("{name} {parameter} must be supplied in pairs"),
        ));
    }
    Ok(())
}

macro_rules! json_modifiers {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = (document, path, value, *path_values))]
                fn new<'py>(
                    py: Python<'py>,
                    document: Bound<'py, PyAny>,
                    path: Bound<'py, PyAny>,
                    value: Bound<'py, PyAny>,
                    path_values: &Bound<'py, PyTuple>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    validate_pairs($name, "paths and values", path_values)?;
                    let operands = prefix_arguments(py, [document, path, value], path_values)?;
                    let expression = SqlFunction::new(
                        $name, Kind::Scalar, Form::Call(concat!($name, "(")), operands,
                    )?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
        fn register_modifiers(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

json_modifiers! {
    JsonArrayAppend, "JSON_ARRAY_APPEND", "Append values to arrays at the specified JSON paths.";
    JsonArrayInsert, "JSON_ARRAY_INSERT", "Insert values at JSON array positions.";
    JsonInsert, "JSON_INSERT", "Insert values at missing JSON paths.";
    JsonReplace, "JSON_REPLACE", "Replace values at existing JSON paths.";
    JsonSet, "JSON_SET", "Insert or replace values at the specified JSON paths.";
}

/// Build a JSON array from zero or more SQL values.
#[pyclass(extends = SqlFunction, name = "JSON_ARRAY", module = "sqlaero.sqlfunc")]
struct JsonArray;

#[pymethods]
impl JsonArray {
    #[new]
    #[pyo3(signature = (*values))]
    fn new(values: &Bound<'_, PyTuple>) -> PyResult<PyClassInitializer<Self>> {
        let expression = SqlFunction::new(
            "JSON_ARRAY",
            Kind::Scalar,
            Form::Call("JSON_ARRAY("),
            values.clone(),
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Build a JSON object from zero or more alternating SQL keys and values.
#[pyclass(extends = SqlFunction, name = "JSON_OBJECT", module = "sqlaero.sqlfunc")]
struct JsonObject;

#[pymethods]
impl JsonObject {
    #[new]
    #[pyo3(signature = (*key_values))]
    fn new(key_values: &Bound<'_, PyTuple>) -> PyResult<PyClassInitializer<Self>> {
        validate_pairs("JSON_OBJECT", "keys and values", key_values)?;
        let expression = SqlFunction::new(
            "JSON_OBJECT",
            Kind::Scalar,
            Form::Call("JSON_OBJECT("),
            key_values.clone(),
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Find one or all matching strings in a JSON document.
///
/// The optional escape_char must be None, an empty string, or one character.
#[pyclass(extends = SqlFunction, name = "JSON_SEARCH", module = "sqlaero.sqlfunc")]
struct JsonSearch;

#[pymethods]
impl JsonSearch {
    #[new]
    #[pyo3(signature = (document, mode, search, escape_char=Argument::default(), *paths), text_signature = "(document, mode, search, escape_char=..., *paths)")]
    fn new<'py>(
        py: Python<'py>,
        document: Bound<'py, PyAny>,
        mode: Bound<'py, PyAny>,
        search: Bound<'py, PyAny>,
        escape_char: Argument<'py>,
        paths: &Bound<'py, PyTuple>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let call = Form::Call("JSON_SEARCH(");
        let (form, operands) = if let Some(escape) = escape_char.0 {
            if !escape.is_none() {
                let value = escape.cast::<PyString>().map_err(|_| {
                    errors::builtin_message::<PyTypeError>(
                        py,
                        "JSON_SEARCH escape_char must be str or None",
                    )
                })?;
                if value.to_str()?.chars().nth(1).is_some() {
                    return Err(errors::builtin_message::<PyValueError>(
                        py,
                        "JSON_SEARCH escape_char must contain at most one character",
                    ));
                }
            }
            let operands = prefix_arguments(py, [document, mode, search, escape], paths)?;
            let parts = call
                .parts(operands.len())
                .map(|part| match part {
                    Part::Operand(3, _) => Part::Operand(3, Role::Value),
                    other => other,
                })
                .collect();
            (Form::Dynamic(parts), operands)
        } else {
            if !paths.is_empty() {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "JSON_SEARCH paths require an explicit escape_char",
                ));
            }
            (call, PyTuple::new(py, [document, mode, search])?)
        };
        let expression = SqlFunction::new("JSON_SEARCH", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

fn literal_default(value: &Bound<'_, PyAny>) -> bool {
    value.is_none()
        || value.is_exact_instance_of::<PyString>()
        || value.is_exact_instance_of::<PyInt>()
        || value.is_exact_instance_of::<PyFloat>()
        || value.is_exact_instance_of::<PyBool>()
}

fn append_policy<'py>(
    policy: Bound<'py, PyAny>,
    parameter: &str,
    suffix: &'static str,
    operands: &mut SmallVec<[Bound<'py, PyAny>; 4]>,
    parts: &mut SmallVec<[Part; 14]>,
) -> PyResult<()> {
    let invalid = || {
        errors::builtin_format::<PyTypeError>(
            policy.py(),
            format_args!(
                "JSON_VALUE {parameter} requires 'null', 'error', or ('default', literal)"
            ),
        )
    };
    if let Ok(key) = policy.cast_exact::<PyString>() {
        let key = key.to_str()?;
        if key.eq_ignore_ascii_case("null") {
            parts.push(Part::Syntax(" NULL"));
        } else if key.eq_ignore_ascii_case("error") {
            parts.push(Part::Syntax(" ERROR"));
        } else {
            return Err(invalid());
        }
    } else {
        let pair = policy.cast_exact::<PyTuple>().map_err(|_| invalid())?;
        if pair.len() != 2 {
            return Err(invalid());
        }
        let key = pair.get_item(0)?;
        let value = pair.get_item(1)?;
        let key = key.cast_exact::<PyString>().map_err(|_| invalid())?;
        if !key.to_str()?.eq_ignore_ascii_case("default") || !literal_default(&value) {
            return Err(invalid());
        }
        parts.push(Part::Syntax(" DEFAULT "));
        parts.push(Part::Operand(operands.len(), Role::Value));
        operands.push(value);
    }
    parts.push(Part::Syntax(suffix));
    Ok(())
}

/// Extract a JSON scalar at a literal path, optionally with a conversion target.
///
/// Use "null", "error", or ("default", literal) independently for on_empty
/// and on_error. Both policies default to NULL. Invalid documents or paths
/// raise server errors before either policy is applied.
#[pyclass(extends = SqlFunction, name = "JSON_VALUE", module = "sqlaero.sqlfunc")]
struct JsonValue;

#[pymethods]
impl JsonValue {
    #[new]
    #[pyo3(signature = (document, path, *, returning=None, on_empty=None, on_error=None))]
    fn new<'py>(
        py: Python<'py>,
        document: Bound<'py, PyAny>,
        path: Bound<'py, PyString>,
        returning: Option<&Bound<'py, PyAny>>,
        on_empty: Option<Bound<'py, PyAny>>,
        on_error: Option<Bound<'py, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let mut operands = SmallVec::new();
        operands.extend([document, path.into_any()]);
        let mut parts = SmallVec::new();
        // MySQL can apply ON ERROR to invalid document text. Read the whole
        // document first so the source is evaluated once and its errors escape
        // the extraction policy without changing which source types are valid.
        parts.extend([
            Part::Syntax("JSON_VALUE(JSON_EXTRACT("),
            Part::Operand(0, Role::Scalar),
            Part::Syntax(",'$'),"),
            Part::Operand(1, Role::Value),
        ]);
        if let Some(source) = returning {
            let target = sql_type::target(source, "JSON_VALUE RETURNING")?;
            if !matches!(
                target.name,
                Name::Float
                    | Name::Double
                    | Name::Decimal
                    | Name::Signed
                    | Name::Unsigned
                    | Name::Date
                    | Name::Time
                    | Name::Datetime
                    | Name::Year
                    | Name::Char
                    | Name::Json
            ) {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "JSON_VALUE RETURNING does not support this target type",
                ));
            }
            parts.extend([Part::Syntax(" RETURNING "), Part::Target(target)]);
        }
        if let Some(policy) = on_empty {
            append_policy(policy, "on_empty", " ON EMPTY", &mut operands, &mut parts)?;
        }
        if let Some(policy) = on_error {
            append_policy(policy, "on_error", " ON ERROR", &mut operands, &mut parts)?;
        }
        parts.push(Part::Syntax(")"));
        let form = Form::Dynamic(parts.into_iter().collect());
        let expression = SqlFunction::new(
            "JSON_VALUE",
            Kind::Scalar,
            form,
            PyTuple::new(py, operands)?,
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    register_variadic(module)?;
    register_modifiers(module)?;
    module.add_class::<JsonArray>()?;
    module.add_class::<JsonObject>()?;
    module.add_class::<JsonSearch>()?;
    module.add_class::<JsonValue>()
}
