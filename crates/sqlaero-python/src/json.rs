//! Single-pass JSON output using jiter's validated streaming interface.

use jiter::{Jiter, NumberAny, Peek};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};

// Keep the fixed jiter Python parser's container recursion bound. Its number
// decoder also retains the upstream 4,300-digit resource bound.
const MAX_DEPTH: usize = 200;
// Share a work counter across nested containers. Resetting it per container
// would let many short nested arrays defer interruption for an entire field.
const SIGNAL_WORK: usize = 4096;

/// Decode one complete JSON document directly into its final Python objects.
///
/// The public streaming API lets us reject finite text that overflows f64
/// before constructing a Python float. PythonParse has no such callback; a
/// second validation pass or a temporary generic value tree is unnecessary.
pub(crate) fn decode<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let mut parser = Jiter::new(data);
    let peek = parser.peek().map_err(|error| syntax(&parser, error))?;
    let mut until_check = SIGNAL_WORK;
    let value = read_value(py, &mut parser, peek, 0, &mut until_check)?;
    parser.finish().map_err(|error| syntax(&parser, error))?;
    Ok(value)
}

fn syntax(parser: &Jiter<'_>, error: jiter::JiterError) -> PyErr {
    PyValueError::new_err(error.description(parser))
}

fn read_value<'py>(
    py: Python<'py>,
    parser: &mut Jiter<'_>,
    peek: Peek,
    depth: usize,
    until_check: &mut usize,
) -> PyResult<Bound<'py, PyAny>> {
    match peek {
        Peek::Null => {
            parser.known_null().map_err(|error| syntax(parser, error))?;
            Ok(py.None().into_bound(py))
        }
        Peek::True | Peek::False => Ok(parser
            .known_bool(peek)
            .map_err(|error| syntax(parser, error))?
            .into_pyobject(py)?
            .to_owned()
            .into_any()),
        Peek::String => {
            let text = match parser.known_str() {
                Ok(text) => text,
                Err(error) => return Err(syntax(parser, error)),
            };
            Ok(PyString::new(py, text).into_any())
        }
        Peek::Array => {
            check_depth(depth)?;
            let result = PyList::empty(py);
            let mut next = parser
                .known_array()
                .map_err(|error| syntax(parser, error))?;
            while let Some(item) = next {
                check_work(py, until_check)?;
                result.append(read_value(py, parser, item, depth + 1, until_check)?)?;
                next = parser.array_step().map_err(|error| syntax(parser, error))?;
            }
            Ok(result.into_any())
        }
        Peek::Object => {
            check_depth(depth)?;
            let result = PyDict::new(py);
            let mut next = match parser.known_object() {
                Ok(key) => key.map(|key| PyString::new(py, key)),
                Err(error) => return Err(syntax(parser, error)),
            };
            while let Some(key) = next {
                check_work(py, until_check)?;
                let item = parser.peek().map_err(|error| syntax(parser, error))?;
                result.set_item(key, read_value(py, parser, item, depth + 1, until_check)?)?;
                next = match parser.next_key() {
                    Ok(key) => key.map(|key| PyString::new(py, key)),
                    Err(error) => return Err(syntax(parser, error)),
                };
            }
            Ok(result.into_any())
        }
        _ => match parser
            .known_number(peek)
            .map_err(|error| syntax(parser, error))?
        {
            NumberAny::Float(value) if !value.is_finite() => Err(PyValueError::new_err(
                "JSON number is outside the finite binary64 range",
            )),
            value => Ok(value.into_pyobject(py)?.into_any()),
        },
    }
}

#[inline]
fn check_work(py: Python<'_>, until_check: &mut usize) -> PyResult<()> {
    *until_check -= 1;
    if *until_check == 0 {
        *until_check = SIGNAL_WORK;
        py.check_signals()?;
    }
    Ok(())
}

fn check_depth(depth: usize) -> PyResult<()> {
    if depth >= MAX_DEPTH {
        return Err(PyValueError::new_err(
            "JSON container recursion limit exceeded",
        ));
    }
    Ok(())
}
