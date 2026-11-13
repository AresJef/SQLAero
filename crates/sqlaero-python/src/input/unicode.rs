//! Bounded, base-storage Unicode reading shared by native output targets.

use crate::errors;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyStringData, PyStringMethods};
use sqlaero_core::transport::deadline::IoBudget;
use sqlaero_transcode::utf8::{self, EncodeError};
use sqlaero_transcode::value::{Consumer, ScalarRef};

/// Read immutable base storage, reusing bounded cached UTF-8 when suitable.
/// A target error is translated at the boundary with exact source coordinates.
pub(crate) fn write<C: Consumer + ?Sized>(
    source: &Bound<'_, PyString>,
    target: &mut C,
    context: &C::Context,
    budget: Option<&IoBudget>,
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
