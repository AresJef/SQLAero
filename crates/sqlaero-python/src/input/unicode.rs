//! Bounded, base-storage Unicode reading shared by native output targets.

use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyStringData, PyStringMethods};
use sqlaero_transcode::utf8::{self, EncodeError};
use sqlaero_transcode::value::{Consumer, ScalarRef};

/// Compare complete immutable base names without Python equality, UTF-8
/// materialization or JSON key policy. Source schemas may contain surrogates;
/// only the target's actual name output decides whether they are admissible.
pub(crate) fn same(
    left: &Bound<'_, PyString>,
    right: &Bound<'_, PyString>,
    budget: &dyn Checkpoint,
) -> PyResult<bool> {
    if left.is(right) {
        return Ok(true);
    }
    let py = left.py();
    // SAFETY: both true str owners remain live. Their immutable base character
    // storage follows the same GIL/NoGIL acquisition contract as write below.
    let (left, right) = unsafe { (left.data()?, right.data()?) };
    let length = |value: &PyStringData<'_>| match value {
        PyStringData::Ucs1(values) => values.len(),
        PyStringData::Ucs2(values) => values.len(),
        PyStringData::Ucs4(values) => values.len(),
    };
    if length(&left) != length(&right) {
        return Ok(false);
    }
    let point = |value: &PyStringData<'_>, index: usize| match value {
        PyStringData::Ucs1(values) => u32::from(values[index]),
        PyStringData::Ucs2(values) => u32::from(values[index]),
        PyStringData::Ucs4(values) => values[index],
    };
    for start in (0..length(&left)).step_by(super::WORK_BYTES / 4) {
        super::check(py, budget)?;
        let end = (start + super::WORK_BYTES / 4).min(length(&left));
        let equal = match (&left, &right) {
            (PyStringData::Ucs1(a), PyStringData::Ucs1(b)) => a[start..end] == b[start..end],
            (PyStringData::Ucs2(a), PyStringData::Ucs2(b)) => a[start..end] == b[start..end],
            (PyStringData::Ucs4(a), PyStringData::Ucs4(b)) => a[start..end] == b[start..end],
            _ => (start..end).all(|index| point(&left, index) == point(&right, index)),
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Preserve an exceptional native UCS4 value in a private Python error payload.
/// Normal serialization never creates this intermediate Python string. Every
/// source copy finishes before public Unicode writes or a shared checkpoint;
/// the incomplete string is not published or passed to callbacks.
pub(crate) fn from_ucs4<'py>(
    py: Python<'py>,
    count: usize,
    swap: bool,
    budget: &dyn super::control::Checkpoint,
    mut read: impl FnMut(usize, &mut [u8]) -> PyResult<()>,
) -> PyResult<Bound<'py, pyo3::types::PyString>> {
    super::check(py, budget)?;
    let size = isize::try_from(count).map_err(|_| crate::errors::memory_error(py))?;
    // PyUnicode_New needs the real storage class. An overstated maxchar can
    // make equal ordinary strings compare unequal. Only this exceptional path
    // makes a bounded native scan before allocating the canonical payload.
    let mut chunk = [0u8; 4096];
    let mut start = 0;
    let mut maximum = 0;
    while start < count {
        super::check(py, budget)?;
        let length = (count - start).min(chunk.len() / 4);
        read(start, &mut chunk[..length * 4])?;
        for bytes in chunk[..length * 4].as_chunks::<4>().0 {
            let point = u32::from_ne_bytes(*bytes);
            maximum = maximum.max(if swap { point.swap_bytes() } else { point });
        }
        start += length;
    }
    let maxchar = match maximum {
        0..=0x7f => 0x7f,
        0x80..=0xff => 0xff,
        0x100..=0xffff => 0xffff,
        0x10000..=0x10ffff => 0x10ffff,
        _ => {
            return Err(errors::builtin_message::<pyo3::exceptions::PyValueError>(
                py,
                "native UCS4 source contains a codepoint outside Unicode's range",
            ));
        }
    };
    // SAFETY: size is nonnegative and maxchar is a legal Unicode maximum. The
    // new exact str stays uniquely owned and private until every character has
    // been initialized with the public setter (surrogates are valid str data).
    let output =
        unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyUnicode_New(size, maxchar)) }?;
    start = 0;
    while start < count {
        super::check(py, budget)?;
        let length = (count - start).min(chunk.len() / 4);
        read(start, &mut chunk[..length * 4])?;
        for (index, bytes) in chunk[..length * 4].as_chunks::<4>().0.iter().enumerate() {
            let point = u32::from_ne_bytes(*bytes);
            let point = if swap { point.swap_bytes() } else { point };
            // SAFETY: index is inside this unique unpublished Unicode object.
            // The public API validates the codepoint and preserves its actual
            // error for a physically invalid value beyond Unicode's range.
            if unsafe {
                pyo3::ffi::PyUnicode_WriteChar(output.as_ptr(), (start + index) as isize, point)
            } < 0
            {
                return Err(PyErr::fetch(py));
            }
        }
        start += length;
    }
    super::check(py, budget)?;
    output
        .cast_into::<pyo3::types::PyString>()
        .map_err(Into::into)
}

/// Consume owned UTF-8 in bounded complete-character fragments, independently
/// of the output syntax or original Python source type.
pub(crate) fn native_fragments(
    py: Python<'_>,
    text: &str,
    budget: Option<&dyn Checkpoint>,
    mut consume: impl FnMut(usize, &str) -> PyResult<()>,
) -> PyResult<()> {
    if text.len() <= super::WORK_BYTES {
        super::control::bytes(py, budget, text.len())?;
        return consume(0, text);
    }
    let mut start = 0;
    while start < text.len() {
        super::checkpoint(py, budget)?;
        let end = text.floor_char_boundary(start.saturating_add(super::WORK_BYTES).min(text.len()));
        consume(start, &text[start..end])?;
        start = end;
    }
    super::checkpoint(py, budget)
}

/// Read immutable base storage, reusing bounded cached UTF-8 when suitable.
/// A target error is translated at the boundary with exact source coordinates.
pub(crate) fn write<C: Consumer + ?Sized>(
    source: &Bound<'_, PyString>,
    target: &mut C,
    context: &C::Context,
    budget: Option<&dyn Checkpoint>,
    encoding_label: &str,
    mut failure: impl FnMut(C::Error, usize, &str) -> PyErr,
) -> PyResult<()> {
    let py = source.py();
    // SAFETY: this is an owned true str or subclass; its canonical codepoint
    // storage is immutable and remains owned for every borrowed slice and
    // signal checkpoint. Supported CPython >=3.12 needs no legacy readiness
    // conversion. PyO3 handles each pinned ABI's layout; all eight supported
    // targets are little-endian x86_64/aarch64 and must be covered by CI. No raw
    // view is detached, stored globally or retained by the native consumer.
    let data = unsafe { source.data()? };
    // A bounded short conversion may reuse CPython's immutable UTF-8 cache.
    // The caller checks aggregate work at its traversal/root boundary; adding
    // two signal/deadline calls to every tiny leaf would duplicate that work.
    // Long *first* conversions never pass through to_str before checkpointing.
    const CHARACTERS: usize = super::WORK_BYTES / 4;
    let length = match data {
        PyStringData::Ucs1(values) => values.len(),
        PyStringData::Ucs2(values) => values.len(),
        PyStringData::Ucs4(values) => values.len(),
    };
    if length <= CHARACTERS {
        super::control::bytes(py, budget, length.saturating_mul(4))?;
        let text = source.to_str().map_err(|original| {
            if !original.is_instance_of::<pyo3::exceptions::PyUnicodeEncodeError>(py) {
                return original;
            }
            let invalid = match data {
                PyStringData::Ucs1(_) => None,
                PyStringData::Ucs2(values) => values
                    .iter()
                    .position(|value| (0xD800..=0xDFFF).contains(value)),
                PyStringData::Ucs4(values) => values
                    .iter()
                    .position(|value| (0xD800..=0xDFFF).contains(value)),
            };
            invalid.map_or(original, |index| {
                super::encode_error(
                    source,
                    encoding_label,
                    index,
                    index + 1,
                    "surrogates are not valid input characters",
                )
            })
        })?;
        return target
            .write(ScalarRef::Text(text), context)
            .map_err(|error| failure(error, 0, text));
    }

    super::checkpoint(py, budget)?;
    target
        .text_start(None, context)
        .map_err(|error| failure(error, 0, ""))?;
    let mut buffer = String::new();
    let mut position = 0;
    // Four UTF-8 bytes per source codepoint prove the bounded scratch size.
    macro_rules! chunks {
        ($values:expr) => {
            for values in $values.chunks(CHARACTERS) {
                super::checkpoint(py, budget)?;
                utf8::codepoints(values, &mut buffer).map_err(|error| match error {
                    EncodeError::CodePoint { index, .. } => super::encode_error(
                        source,
                        encoding_label,
                        position + index,
                        position + index + 1,
                        "surrogates and invalid Unicode codepoints are not valid input characters",
                    ),
                    EncodeError::Capacity(_) => errors::memory_error(py),
                })?;
                target
                    .text_fragment(&buffer, context)
                    .map_err(|error| failure(error, position, &buffer))?;
                position += values.len();
            }
        };
    }
    match data {
        PyStringData::Ucs1(values) => {
            for values in values.chunks(CHARACTERS) {
                super::checkpoint(py, budget)?;
                if values.is_ascii() {
                    // SAFETY: this bounded immutable source chunk passed
                    // is_ascii; its owned string remains live across the call.
                    let value = unsafe { std::str::from_utf8_unchecked(values) };
                    target
                        .text_fragment(value, context)
                        .map_err(|error| failure(error, position, value))?;
                } else {
                    utf8::latin1(values, &mut buffer).map_err(|_| errors::memory_error(py))?;
                    target
                        .text_fragment(&buffer, context)
                        .map_err(|error| failure(error, position, &buffer))?;
                }
                position += values.len();
            }
        }
        PyStringData::Ucs2(values) => chunks!(values),
        PyStringData::Ucs4(values) => chunks!(values),
    }
    super::checkpoint(py, budget)?;
    target
        .text_end(context)
        .map_err(|error| failure(error, position, ""))?;
    super::checkpoint(py, budget)
}
