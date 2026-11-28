//! Context-dependent syntax, compatibility functions, and session operations.

use super::calls::{fixed_calls, sequence_snapshot, variadic_calls};
use super::{Kind, SqlFunction, keyword};
use crate::errors;
use crate::input::sql_type;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyInt, PyString, PyTuple};
use smallvec::SmallVec;
use sqlaero_core::bind::{
    sql_func::{Form, Part, Role},
    sql_type::Name,
};

fixed_calls! {
    Benchmark, "BENCHMARK", (count, expression), "Evaluate an expression repeatedly on the server and return zero.";
    ExtractValue, "ExtractValue", (document, xpath), "Extract XML text matching an XPath expression.";
    FormatBytes, "FORMAT_BYTES", (bytes), "Convert a byte count to a human-readable string.";
    FormatPicoTime, "FORMAT_PICO_TIME", (picoseconds), "Convert a duration in picoseconds to a human-readable string.";
    FoundRows, "FOUND_ROWS", (), "Return the row count from the preceding SELECT. This function is deprecated.";
    GetLock, "GET_LOCK", (name, timeout), "Acquire a server-named lock for the current session.";
    IsFreeLock, "IS_FREE_LOCK", (name), "Test whether a server-named lock is free.";
    IsUsedLock, "IS_USED_LOCK", (name), "Return the connection identifier holding a server-named lock.";
    LoadFile, "LOAD_FILE", (path), "Read a server-local file subject to FILE privilege and secure_file_priv.";
    Md5, "MD5", (value), "Return the hexadecimal MD5 digest. Recent MySQL versions require the legacy hashing component.";
    PsCurrentThreadId, "PS_CURRENT_THREAD_ID", (), "Return the Performance Schema thread identifier for the current session.";
    PsThreadId, "PS_THREAD_ID", (connection_id), "Return the Performance Schema thread identifier for a connection.";
    ReleaseAllLocks, "RELEASE_ALL_LOCKS", (), "Release all named locks held by the current session.";
    ReleaseLock, "RELEASE_LOCK", (name), "Release a named lock held by the current session.";
    RolesGraphml, "ROLES_GRAPHML", (), "Return enabled and accessible roles in GraphML, subject to server privileges.";
    Sha, "SHA", (value), "Return the hexadecimal SHA-1 digest. Recent MySQL versions require the legacy hashing component.";
    Sha1, "SHA1", (value), "Return the hexadecimal SHA-1 digest. Recent MySQL versions require the legacy hashing component.";
    Sleep, "SLEEP", (seconds), "Pause the server session for the supplied duration in seconds.";
    StatementDigest, "STATEMENT_DIGEST", (statement), "Return the normalized digest hash of one SQL statement.";
    StatementDigestText, "STATEMENT_DIGEST_TEXT", (statement), "Return the normalized text used to digest one SQL statement.";
    UpdateXml, "UpdateXML", (document, xpath, replacement), "Replace the XML fragment matching an XPath expression.";
    ValidatePasswordStrength, "VALIDATE_PASSWORD_STRENGTH", (password), "Return the password score from the server's validation component.";
    VersionWithUpdate, "VERSION_WITH_UPDATE", (), "Return the four-part server version where the server supports it.";
}

variadic_calls! {
    Etag, "ETAG", (value), values, "Compute a 128-bit row hash from one or more values; requires server support.";
    JsonMerge, "JSON_MERGE", (document, other), documents, "Merge JSON documents and preserve conflicting values using the deprecated SQL name.";
}

macro_rules! column_calls {
    ($( $class:ident, $name:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                fn new(column: Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
                    const FORM: Form = Form::Static(&[
                        Part::Syntax(concat!($name, "(")),
                        Part::Operand(0, Role::Identifier),
                        Part::Syntax(")"),
                    ]);
                    let operands = PyTuple::new(column.py(), [column])?;
                    let expression = SqlFunction::new($name, Kind::Scalar, FORM, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+
    };
}

column_calls! {
    DefaultValue, "DEFAULT", "Return the declared default of a referenced column.";
    Values, "VALUES", "Reference the proposed input column in INSERT ON DUPLICATE KEY UPDATE. This form is deprecated.";
}

const SEARCH_MODES: &[&str] = &[
    "IN NATURAL LANGUAGE MODE",
    "IN NATURAL LANGUAGE MODE WITH QUERY EXPANSION",
    "IN BOOLEAN MODE",
    "WITH QUERY EXPANSION",
];

/// Match one or more indexed columns against a full-text search value.
///
/// The mode, if supplied, is a MySQL full-text search clause such as
/// IN BOOLEAN MODE or WITH QUERY EXPANSION.
#[pyclass(extends = SqlFunction, name = "MATCH", module = "sqlaero.sqlfunc")]
struct Match;

#[pymethods]
impl Match {
    #[new]
    #[pyo3(signature = (columns, search, *, mode=None))]
    fn new<'py>(
        columns: &Bound<'py, PyAny>,
        search: Bound<'py, PyAny>,
        mode: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = columns.py();
        let columns = sequence_snapshot(columns, || {
            errors::builtin_message::<PyTypeError>(
                py,
                "MATCH columns requires a list or tuple of Column references",
            )
        })?;
        if columns.is_empty() {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "MATCH columns requires at least one reference",
            ));
        }
        let mode = mode
            .map(|mode| keyword("MATCH", "mode", mode, SEARCH_MODES))
            .transpose()?;
        let mut parts = SmallVec::<[Part; 10]>::new();
        let mut operands = SmallVec::<[Bound<'py, PyAny>; 4]>::new();
        parts
            .try_reserve_exact(2 * columns.len() + 5)
            .map_err(|_| errors::memory_error(py))?;
        operands
            .try_reserve_exact(columns.len() + 1)
            .map_err(|_| errors::memory_error(py))?;
        parts.push(Part::Syntax("MATCH("));
        for (index, column) in columns.iter().enumerate() {
            if index.is_multiple_of(16_384) {
                py.check_signals()?;
            }
            if index != 0 {
                parts.push(Part::Syntax(","));
            }
            parts.push(Part::Operand(index, Role::Identifier));
            operands.push(column);
        }
        parts.extend([
            Part::Syntax(") AGAINST("),
            Part::Operand(operands.len(), Role::Value),
        ]);
        operands.push(search);
        if let Some(mode) = mode {
            parts.extend([Part::Syntax(" "), Part::Option(mode)]);
        }
        parts.push(Part::Syntax(")"));
        let form = Form::Dynamic(parts.into_iter().collect());
        let expression =
            SqlFunction::new("MATCH", Kind::Scalar, form, PyTuple::new(py, operands)?)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Return a literal and give its result column the supplied literal name.
#[pyclass(extends = SqlFunction, name = "NAME_CONST", module = "sqlaero.sqlfunc")]
struct NameConst;

#[pymethods]
impl NameConst {
    #[new]
    fn new<'py>(
        name: Bound<'py, PyString>,
        value: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&[
            Part::Syntax("NAME_CONST("),
            Part::Operand(0, Role::Value),
            Part::Syntax(","),
            Part::Operand(1, Role::Value),
            Part::Syntax(")"),
        ]);
        let py = name.py();
        if !(value.is_none()
            || value.is_exact_instance_of::<PyString>()
            || value.is_exact_instance_of::<PyBytes>()
            || value.is_exact_instance_of::<PyInt>()
            || value.is_exact_instance_of::<PyFloat>()
            || value.is_exact_instance_of::<PyBool>())
        {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NAME_CONST value must be a string, bytes, number, boolean or None literal",
            ));
        }
        let operands = PyTuple::new(py, [name.into_any(), value])?;
        let expression = SqlFunction::new("NAME_CONST", Kind::Scalar, FORM, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Return a binary collation weight with an optional CHAR(N) or BINARY(N) target.
#[pyclass(extends = SqlFunction, name = "WEIGHT_STRING", module = "sqlaero.sqlfunc")]
struct WeightString;

#[pymethods]
impl WeightString {
    #[new]
    #[pyo3(signature = (value, target=None))]
    fn new<'py>(
        value: Bound<'py, PyAny>,
        target: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = value.py();
        let form = if let Some(target) = target {
            let target = sql_type::target(target, "WEIGHT_STRING")?;
            if !matches!(target.name, Name::Char | Name::Binary)
                || target.first.is_none()
                || target.charset.is_some()
            {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "WEIGHT_STRING target requires CHAR(N) or BINARY(N)",
                ));
            }
            Form::Dynamic(
                [
                    Part::Syntax("WEIGHT_STRING("),
                    Part::Operand(0, Role::Scalar),
                    Part::Syntax(" AS "),
                    Part::Target(target),
                    Part::Syntax(")"),
                ]
                .into(),
            )
        } else {
            Form::Call("WEIGHT_STRING(")
        };
        let expression = SqlFunction::new(
            "WEIGHT_STRING",
            Kind::Scalar,
            form,
            PyTuple::new(py, [value])?,
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_variadic(module)?;
    module.add_class::<DefaultValue>()?;
    module.add_class::<Values>()?;
    module.add_class::<Match>()?;
    module.add_class::<NameConst>()?;
    module.add_class::<WeightString>()
}
