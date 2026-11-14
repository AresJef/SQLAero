//! Exact normalized JSON key acquisition, with no repr or JSON-text roundtrip.

use super::{Error, failure};
use crate::errors;
use crate::input::{control::Checkpoint, integer, primitive};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use sqlaero_transcode::{
    buffer,
    value::{Consumer, ScalarRef},
};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

#[derive(Default)]
struct Text(String);

pub(super) struct Key {
    pub(super) value: String,
    pub(super) hash: u64,
}

impl Consumer for Text {
    type Context = ();
    type Error = Error;
    fn write(&mut self, value: ScalarRef<'_>, _: &()) -> Result<(), Error> {
        let ScalarRef::Text(text) = value else {
            return Err(Error::State(
                "key text acquisition received a different fact",
            ));
        };
        self.text_fragment(text, &())
    }
    fn decimal_start(
        &mut self,
        _: sqlaero_transcode::decimal::Header,
        _: &(),
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, Error> {
        Err(Error::Unsupported("Decimal object keys"))
    }
    fn decimal_fragment(&mut self, _: &str, _: &()) -> Result<(), Error> {
        Err(Error::Unsupported("Decimal object keys"))
    }
    fn decimal_end(&mut self, _: &()) -> Result<(), Error> {
        Err(Error::Unsupported("Decimal object keys"))
    }
    fn text_start(&mut self, _: Option<usize>, _: &()) -> Result<(), Error> {
        Ok(())
    }
    fn text_fragment(&mut self, text: &str, _: &()) -> Result<(), Error> {
        buffer::append_text(&mut self.0, text, usize::MAX)?;
        Ok(())
    }
    fn text_end(&mut self, _: &()) -> Result<(), Error> {
        Ok(())
    }
    fn bytes_start(&mut self, _: usize, _: &()) -> Result<(), Error> {
        Err(Error::Unsupported("binary object keys"))
    }
    fn bytes_fragment(&mut self, _: &[u8], _: &()) -> Result<(), Error> {
        Err(Error::Unsupported("binary object keys"))
    }
    fn bytes_end(&mut self, _: &()) -> Result<(), Error> {
        Err(Error::Unsupported("binary object keys"))
    }
}

pub(super) fn read(
    value: &Bound<'_, PyAny>,
    types: &crate::input::ScalarTypes,
    budget: Option<&dyn Checkpoint>,
    seed: &RandomState,
) -> PyResult<Key> {
    let py = value.py();
    let mut output = Text::default();
    match primitive::read(value)? {
        Some(primitive::Source::Text(source)) => crate::input::unicode::write(
            source,
            &mut output,
            &(),
            budget,
            "utf-8",
            |error, _, _| failure(py, error),
        )?,
        Some(primitive::Source::Native(ScalarRef::Signed(value))) => {
            output
                .text_fragment(itoa::Buffer::new().format(value), &())
                .map_err(|error| failure(py, error))?;
        }
        Some(primitive::Source::WideInteger { value, .. }) => {
            let integer = integer::Decimal::acquire(value, budget)?;
            if integer.negative {
                output
                    .text_fragment("-", &())
                    .map_err(|error| failure(py, error))?;
            }
            integer.fragments(py, budget, |text| {
                output
                    .text_fragment(text, &())
                    .map_err(|error| failure(py, error))
            })?;
        }
        None => {
            if let Some(value) = types.uuid_value(value, budget)? {
                output
                    .text_fragment(
                        sqlaero_transcode::identifier::UuidText::new(value).as_str(),
                        &(),
                    )
                    .map_err(|error| failure(py, error))?;
            } else {
                return Err(unsupported(py));
            }
        }
        _ => return Err(unsupported(py)),
    }
    // Fixed chunk boundaries make the hash independent of source acquisition
    // fragments; neither a wide integer nor a long Unicode key is hashed in one
    // uninterruptible call. Random per-object keys resist chosen collisions.
    let mut hasher = seed.build_hasher();
    for fragment in output.0.as_bytes().chunks(crate::input::WORK_BYTES) {
        crate::input::control::bytes(py, budget, fragment.len())?;
        hasher.write(fragment);
    }
    Ok(Key {
        hash: hasher.finish(),
        value: output.0,
    })
}

/// Hash equality is only an index hint. Full native bytes decide equality.
pub(super) fn same(
    py: Python<'_>,
    left: &str,
    right: &str,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<bool> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left
        .as_bytes()
        .chunks(crate::input::WORK_BYTES)
        .zip(right.as_bytes().chunks(crate::input::WORK_BYTES))
    {
        crate::input::control::bytes(py, budget, left.len())?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}

fn unsupported(py: Python<'_>) -> PyErr {
    errors::builtin_message::<PyTypeError>(
        py,
        "JSON object keys must be actual str, int or UUID values; bool and automatic string conversion are not accepted",
    )
}
