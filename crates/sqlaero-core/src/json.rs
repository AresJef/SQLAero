//! Native JSON output, independent of connection encoding and SQL syntax.
//!
//! Source recognition and Python ownership stay at the binding boundary. This
//! target consumes shared native facts and writes only the final UTF-8 buffer.

use sqlaero_transcode::buffer::{self, CapacityError};
use sqlaero_transcode::value::{Consumer, ScalarRef};

/// JSON policy prepared once for the current serialization.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Map nonfinite numeric values to null; reject them when false.
    pub allow_nan: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { allow_nan: true }
    }
}

/// A real JSON target failure, before any output is published.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A source family has no permitted JSON mapping.
    #[error("JSON serialization does not support {0}")]
    Unsupported(&'static str),
    /// The explicit strict-numeric option rejected a nonfinite value.
    #[error("JSON serialization encountered a nonfinite number with allow_nan=False")]
    NonFinite,
    /// The writer was called outside its current structural contract.
    #[error("invalid JSON writer state: {0}")]
    State(&'static str),
    /// The actual output contains too many nested containers.
    #[error("JSON output exceeds 64 nested containers")]
    Depth,
    /// Checked output capacity or an allocation failed.
    #[error(transparent)]
    Capacity(#[from] CapacityError),
}

enum Frame {
    Array { populated: bool },
    Object { populated: bool, waiting: bool },
}

struct Text {
    remaining: usize,
    key: bool,
}

/// One privately owned final JSON document, with fallible growth.
/// Bounded text fragments return control to the caller between writes.
#[derive(Default)]
pub struct Writer {
    output: Vec<u8>,
    frames: Vec<Frame>,
    text: Option<Text>,
    root: bool,
    failed: bool,
}

impl Writer {
    /// Start one document without an artificial output-size quota.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return only a complete, successful, independently owned document.
    /// # Errors
    /// Rejects an empty, incomplete or previously failed document.
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        if self.failed || !self.root || !self.frames.is_empty() || self.text.is_some() {
            return Err(Error::State("document is incomplete or failed"));
        }
        Ok(self.output)
    }

    fn perform<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.failed {
            return Err(Error::State("document already failed"));
        }
        let result = action(self);
        self.failed = result.is_err();
        result
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        buffer::append(&mut self.output, bytes, usize::MAX)?;
        Ok(())
    }

    fn value(&mut self) -> Result<(), Error> {
        if self.text.is_some() {
            return Err(Error::State("a text value is still open"));
        }
        match self.frames.last_mut() {
            Some(Frame::Array { populated }) => {
                let comma = *populated;
                *populated = true;
                if comma {
                    self.append(b",")?;
                }
            }
            Some(Frame::Object { waiting, .. }) if *waiting => *waiting = false,
            Some(Frame::Object { .. }) => return Err(Error::State("object value has no key")),
            None if self.root => return Err(Error::State("document already has a root")),
            None => self.root = true,
        }
        Ok(())
    }

    fn begin(&mut self, frame: Frame, opening: &[u8]) -> Result<(), Error> {
        if self.frames.len() >= 64 {
            return Err(Error::Depth);
        }
        self.value()?;
        self.frames
            .try_reserve(1)
            .map_err(CapacityError::Allocation)?;
        self.append(opening)?;
        self.frames.push(frame);
        Ok(())
    }

    /// Begin one actual output array.
    /// # Errors
    /// Returns a depth, structural or allocation failure.
    pub fn array_start(&mut self) -> Result<(), Error> {
        self.perform(|writer| writer.begin(Frame::Array { populated: false }, b"["))
    }

    /// Finish the current output array.
    /// # Errors
    /// Rejects mismatched containers or an unfinished string.
    pub fn array_end(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            if writer.text.is_some() || !matches!(writer.frames.last(), Some(Frame::Array { .. })) {
                return Err(Error::State("array end does not match the current value"));
            }
            writer.append(b"]")?;
            writer.frames.pop();
            Ok(())
        })
    }

    /// Begin one actual output object.
    /// # Errors
    /// Returns a depth, structural or allocation failure.
    pub fn object_start(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            writer.begin(
                Frame::Object {
                    populated: false,
                    waiting: false,
                },
                b"{",
            )
        })
    }

    /// Finish an object after all its keys have values.
    /// # Errors
    /// Rejects mismatched containers, missing values or unfinished strings.
    pub fn object_end(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            if writer.text.is_some()
                || !matches!(
                    writer.frames.last(),
                    Some(Frame::Object { waiting: false, .. })
                )
            {
                return Err(Error::State(
                    "object end has a missing value or mismatched container",
                ));
            }
            writer.append(b"}")?;
            writer.frames.pop();
            Ok(())
        })
    }

    fn string_start(&mut self, length: usize, key: bool) -> Result<(), Error> {
        if key {
            if self.text.is_some() {
                return Err(Error::State("a text value is still open"));
            }
            let Some(Frame::Object { populated, waiting }) = self.frames.last_mut() else {
                return Err(Error::State("a key requires an object"));
            };
            if *waiting {
                return Err(Error::State("the preceding key has no value"));
            }
            let comma = *populated;
            *populated = true;
            *waiting = true;
            if comma {
                self.append(b",")?;
            }
        } else {
            self.value()?;
        }
        self.append(b"\"")?;
        self.text = Some(Text {
            remaining: length,
            key,
        });
        Ok(())
    }

    fn string_fragment(&mut self, value: &str) -> Result<(), Error> {
        let remaining = self
            .text
            .as_ref()
            .ok_or(Error::State("text fragment has no start"))?
            .remaining
            .checked_sub(value.len())
            .ok_or(Error::State("text source length changed"))?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut start = 0;
        for (index, &byte) in value.as_bytes().iter().enumerate() {
            let escape = match byte {
                b'"' => b"\\\"".as_slice(),
                b'\\' => b"\\\\",
                8 => b"\\b",
                9 => b"\\t",
                10 => b"\\n",
                12 => b"\\f",
                13 => b"\\r",
                0..=31 => {
                    self.append(&value.as_bytes()[start..index])?;
                    self.append(&[
                        b'\\',
                        b'u',
                        b'0',
                        b'0',
                        HEX[usize::from(byte >> 4)],
                        HEX[usize::from(byte & 15)],
                    ])?;
                    start = index + 1;
                    continue;
                }
                _ => continue,
            };
            self.append(&value.as_bytes()[start..index])?;
            self.append(escape)?;
            start = index + 1;
        }
        self.append(&value.as_bytes()[start..])?;
        // No callback or borrow of external mutable storage occurs above.
        if let Some(text) = &mut self.text {
            text.remaining = remaining;
        }
        Ok(())
    }

    fn string_end(&mut self) -> Result<(), Error> {
        let Some(Text { remaining: 0, key }) = self.text else {
            return Err(Error::State("text end has an incomplete or missing source"));
        };
        self.append(if key { b"\":" } else { b"\"" })?;
        self.text = None;
        Ok(())
    }

    /// Begin a key using its complete UTF-8 byte length.
    /// Name admission and duplicate detection occur before this target call.
    /// # Errors
    /// Returns an invalid object state or allocation failure.
    pub fn key_start(&mut self, utf8_len: usize) -> Result<(), Error> {
        self.perform(|writer| writer.string_start(utf8_len, true))
    }

    /// Write a small, already admitted full key without truncating at NUL.
    /// Long-name adapters use key_start and bounded text fragments instead.
    /// # Errors
    /// Returns a structural or allocation failure.
    pub fn key(&mut self, value: &str) -> Result<(), Error> {
        self.perform(|writer| {
            writer.string_start(value.len(), true)?;
            writer.string_fragment(value)?;
            writer.string_end()
        })
    }
}

impl Consumer for Writer {
    type Context = Options;
    type Error = Error;

    fn write(&mut self, value: ScalarRef<'_>, options: &Options) -> Result<(), Error> {
        self.perform(|writer| {
            match value {
                ScalarRef::Text(value) => {
                    writer.string_start(value.len(), false)?;
                    writer.string_fragment(value)?;
                    return writer.string_end();
                }
                ScalarRef::Bytes(_) => return Err(Error::Unsupported("binary")),
                ScalarRef::Float(value) if !value.is_finite() && !options.allow_nan => {
                    return Err(Error::NonFinite);
                }
                _ => (),
            }
            writer.value()?;
            match value {
                ScalarRef::Null => writer.append(b"null"),
                ScalarRef::Boolean(value) => writer.append(if value { b"true" } else { b"false" }),
                ScalarRef::Signed(value) => {
                    writer.append(itoa::Buffer::new().format(value).as_bytes())
                }
                ScalarRef::Unsigned(value) => {
                    writer.append(itoa::Buffer::new().format(value).as_bytes())
                }
                ScalarRef::Float(value) if !value.is_finite() => writer.append(b"null"),
                ScalarRef::Float(value) => {
                    writer.append(zmij::Buffer::new().format(value).as_bytes())
                }
                _ => Err(Error::Unsupported(
                    "a scalar family not connected to the JSON target yet",
                )),
            }
        })
    }

    fn text_start(&mut self, length: usize, _: &Options) -> Result<(), Error> {
        self.perform(|writer| writer.string_start(length, false))
    }
    fn text_fragment(&mut self, value: &str, _: &Options) -> Result<(), Error> {
        self.perform(|writer| writer.string_fragment(value))
    }
    fn text_end(&mut self, _: &Options) -> Result<(), Error> {
        self.perform(Self::string_end)
    }
    fn bytes_start(&mut self, _: usize, _: &Options) -> Result<(), Error> {
        self.perform(|_| Err(Error::Unsupported("binary")))
    }
    fn bytes_fragment(&mut self, _: &[u8], _: &Options) -> Result<(), Error> {
        self.perform(|_| Err(Error::Unsupported("binary")))
    }
    fn bytes_end(&mut self, _: &Options) -> Result<(), Error> {
        self.perform(|_| Err(Error::Unsupported("binary")))
    }
}
