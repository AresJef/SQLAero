//! Single-pass JSON output using jiter's validated streaming interface.

use jiter::{
    Jiter, NumberAny, NumberInt, Peek,
    progress::{CheckedError, Progress},
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};

// Keep the fixed jiter Python parser's container recursion bound. Its number
// decoder also retains the upstream 4,300-digit resource bound.
const MAX_DEPTH: usize = 200;
// Share a work counter across nested containers. Resetting it per container
// would let many short nested arrays defer interruption for an entire field.
const SIGNAL_WORK: usize = 4096;

/// Preserve the origin of a failure instead of classifying user exceptions by
/// their Python class. A signal handler may itself raise ValueError.
pub(crate) enum DecodeFailure {
    Invalid(PyErr),
    Python(PyErr),
}

impl From<PyErr> for DecodeFailure {
    fn from(error: PyErr) -> Self {
        Self::Python(error)
    }
}

impl DecodeFailure {
    pub(crate) fn into_python(self) -> PyErr {
        match self {
            Self::Invalid(error) | Self::Python(error) => error,
        }
    }
}

/// Decode one complete JSON document directly into its final Python objects.
///
/// The public streaming API lets us reject finite text that overflows f64
/// before constructing a Python float. PythonParse has no such callback; a
/// second validation pass or a temporary generic value tree is unnecessary.
pub(crate) fn decode<'py>(
    py: Python<'py>,
    data: &[u8],
) -> Result<Bound<'py, PyAny>, DecodeFailure> {
    let mut parser = Jiter::new(data);
    let peek = parser.peek().map_err(|error| syntax(&parser, error))?;
    let mut until_check = SIGNAL_WORK;
    let mut progress = Progress::new(|_work| py.check_signals());
    let value = read_value(py, &mut parser, peek, 0, &mut until_check, &mut progress)?;
    parser.finish().map_err(|error| syntax(&parser, error))?;
    Ok(value)
}

fn syntax(parser: &Jiter<'_>, error: jiter::JiterError) -> DecodeFailure {
    DecodeFailure::Invalid(PyValueError::new_err(error.description(parser)))
}

/// Bound conversion and copying work as well as JSON token decoding.
///
/// The unpublished Unicode object is uniquely owned until every character is
/// initialized. No hashing, UTF-8 access or reference cloning may occur before
/// completion. Signal handlers cannot discover this untracked, private object;
/// failure drops its sole owner without publishing a partly initialized value.
fn string<'py, F>(
    py: Python<'py>,
    text: &str,
    progress: &mut Progress<F>,
) -> PyResult<Bound<'py, PyString>>
where
    F: FnMut(usize) -> PyResult<()>,
{
    const WINDOW: usize = jiter::progress::WORK_WINDOW;
    if text.len() <= WINDOW {
        return PyString::from_bytes(py, text.as_bytes());
    }
    let mut count = 0;
    let mut maxchar = 0x7f;
    let mut start = 0;
    while start < text.len() {
        let mut end = start.saturating_add(WINDOW).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = &text[start..end];
        if chunk.is_ascii() {
            count += chunk.len();
        } else {
            count += chunk.chars().count();
            // The parser already validated UTF-8. Lead-byte classes select
            // sufficient Unicode storage without decoding each scalar twice.
            // Continuation bytes are below every lead-byte threshold here.
            let largest = chunk.bytes().max().unwrap_or_default();
            let required = if largest >= 0xf0 {
                0x10ffff
            } else if largest >= 0xc4 {
                0xffff
            } else {
                0xff
            };
            maxchar = maxchar.max(required);
        }
        progress.checkpoint(end - start)?;
        start = end;
    }
    // SAFETY: count is no greater than the owned input's byte length, which
    // fits Py_ssize_t. The measured maxchar selects a sufficient public Unicode
    // storage kind. From-owned conversion does not increment its reference.
    let value = unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyUnicode_New(count as isize, maxchar),
        )?
    };
    // SAFETY: value is a new, unique Unicode object from the public constructor.
    // Keep its owner alive; never expose its buffer outside this function.
    let (kind, output) = unsafe {
        (
            pyo3::ffi::PyUnicode_KIND(value.as_ptr()),
            pyo3::ffi::PyUnicode_DATA(value.as_ptr()),
        )
    };
    let mut start = 0;
    let mut position = 0;
    while start < text.len() {
        let mut end = start.saturating_add(WINDOW).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = &text[start..end];
        if maxchar <= 0x7f {
            // SAFETY: ASCII has one byte per character, output is kind 1, the
            // ranges are independent and the measured allocation covers them.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    chunk.as_ptr(),
                    output.cast::<u8>().add(position),
                    chunk.len(),
                );
            }
            position += chunk.len();
        } else {
            for point in chunk.chars() {
                // SAFETY: each scalar fits the measured kind and position is
                // below count. No borrowed reference survives a signal check.
                unsafe {
                    match kind {
                        pyo3::ffi::PyUnicode_1BYTE_KIND => {
                            output.cast::<u8>().add(position).write(point as u8)
                        }
                        pyo3::ffi::PyUnicode_2BYTE_KIND => {
                            output.cast::<u16>().add(position).write(point as u16)
                        }
                        _ => output.cast::<u32>().add(position).write(u32::from(point)),
                    }
                }
                position += 1;
            }
        }
        progress.checkpoint(end - start)?;
        start = end;
    }
    value.cast_into::<PyString>().map_err(Into::into)
}

fn checked_error(parser: &Jiter<'_>, error: CheckedError<PyErr>) -> DecodeFailure {
    match error {
        CheckedError::Json(error) => syntax(parser, error),
        CheckedError::Interrupted(error) => DecodeFailure::Python(error),
        CheckedError::Allocation(error) => {
            DecodeFailure::Python(pyo3::exceptions::PyMemoryError::new_err(error.to_string()))
        }
    }
}

fn read_value<'py, F>(
    py: Python<'py>,
    parser: &mut Jiter<'_>,
    peek: Peek,
    depth: usize,
    until_check: &mut usize,
    progress: &mut Progress<F>,
) -> Result<Bound<'py, PyAny>, DecodeFailure>
where
    F: FnMut(usize) -> PyResult<()>,
{
    match peek {
        Peek::Null => {
            parser.known_null().map_err(|error| syntax(parser, error))?;
            Ok(py.None().into_bound(py))
        }
        Peek::True | Peek::False => Ok(parser
            .known_bool(peek)
            .map_err(|error| syntax(parser, error))?
            .into_pyobject(py)
            .map_err(PyErr::from)?
            .to_owned()
            .into_any()),
        Peek::String => {
            let text = match parser.known_str_checked(progress) {
                Ok(text) => text,
                Err(error) => return Err(checked_error(parser, error)),
            };
            Ok(string(py, text, progress)?.into_any())
        }
        Peek::Array => {
            check_depth(depth)?;
            // SAFETY: the public constructor returns a new exact list or NULL.
            // Keep allocation failure as the original MemoryError, not a panic.
            let result = unsafe {
                Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyList_New(0))?
                    .cast_into_unchecked::<PyList>()
            };
            let mut next = parser
                .known_array()
                .map_err(|error| syntax(parser, error))?;
            while let Some(item) = next {
                check_work(py, until_check)?;
                result.append(read_value(
                    py,
                    parser,
                    item,
                    depth + 1,
                    until_check,
                    progress,
                )?)?;
                next = parser.array_step().map_err(|error| syntax(parser, error))?;
            }
            Ok(result.into_any())
        }
        Peek::Object => {
            check_depth(depth)?;
            // SAFETY: the public constructor returns a new exact dict or NULL.
            let result = unsafe {
                Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyDict_New())?
                    .cast_into_unchecked::<PyDict>()
            };
            let mut next = match parser.known_object_checked(progress) {
                Ok(key) => key.map(|key| string(py, key, progress)).transpose()?,
                Err(error) => return Err(checked_error(parser, error)),
            };
            while let Some(key) = next {
                check_work(py, until_check)?;
                let item = parser.peek().map_err(|error| syntax(parser, error))?;
                result.set_item(
                    key,
                    read_value(py, parser, item, depth + 1, until_check, progress)?,
                )?;
                next = match parser.next_key_checked(progress) {
                    Ok(key) => key.map(|key| string(py, key, progress)).transpose()?,
                    Err(error) => return Err(checked_error(parser, error)),
                };
            }
            Ok(result.into_any())
        }
        _ => match parser
            .known_number(peek)
            .map_err(|error| syntax(parser, error))?
        {
            NumberAny::Float(value) if !value.is_finite() => Err(DecodeFailure::Invalid(
                PyValueError::new_err("JSON number is outside the finite binary64 range"),
            )),
            NumberAny::Float(value) => {
                // SAFETY: the public constructor returns an exact float or
                // NULL with the original allocation error set.
                unsafe {
                    Ok(Bound::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyFloat_FromDouble(value),
                    )?)
                }
            }
            NumberAny::Int(NumberInt::Int(value)) => {
                // SAFETY: the public constructor accepts every i64 and returns
                // a new integer reference or NULL with an allocation error.
                unsafe {
                    Ok(Bound::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyLong_FromLongLong(value),
                    )?)
                }
            }
            NumberAny::Int(NumberInt::BigInt(value)) => {
                // The parser already enforces its 4,300-decimal-digit bound.
                // Base 16 avoids Python's separate configurable decimal-input
                // limit and uses the same public fallible API on 3.12–3.15.
                let mut text = value.to_str_radix(16);
                text.try_reserve(1)
                    .map_err(|error| pyo3::exceptions::PyMemoryError::new_err(error.to_string()))?;
                text.push('\0');
                // SAFETY: the owned text is NUL terminated, contains only a
                // sign and hexadecimal digits, and lives through this call.
                unsafe {
                    Ok(Bound::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyLong_FromString(
                            text.as_ptr().cast_mut().cast(),
                            std::ptr::null_mut(),
                            16,
                        ),
                    )?)
                }
            }
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

fn check_depth(depth: usize) -> Result<(), DecodeFailure> {
    if depth >= MAX_DEPTH {
        return Err(DecodeFailure::Invalid(PyValueError::new_err(
            "JSON container recursion limit exceeded",
        )));
    }
    Ok(())
}
