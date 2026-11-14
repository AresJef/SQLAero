//! Native JSON output, independent of connection encoding and SQL syntax.
//!
//! Source recognition and Python ownership stay at the binding boundary. This
//! target consumes shared native facts and writes only the final UTF-8 buffer.

use sqlaero_transcode::buffer::{self, CapacityError};
use sqlaero_transcode::number;
use sqlaero_transcode::value::{Consumer, ScalarRef};

mod temporal;

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
    remaining: Option<usize>,
    key: bool,
}

struct Integer {
    populated: bool,
    zero: bool,
}

/// One privately owned final JSON document, with fallible growth.
/// Bounded text fragments return control to the caller between writes.
#[derive(Default)]
pub struct Writer {
    output: String,
    frames: Vec<Frame>,
    text: Option<Text>,
    integer: Option<Integer>,
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
    pub fn finish(self) -> Result<String, Error> {
        if self.failed || !self.root || !self.frames.is_empty() || self.fragment_open() {
            return Err(Error::State("document is incomplete or failed"));
        }
        Ok(self.output)
    }

    fn fragment_open(&self) -> bool {
        self.text.is_some() || self.integer.is_some()
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

    fn append(&mut self, text: &str) -> Result<(), Error> {
        buffer::append_text(&mut self.output, text, usize::MAX)?;
        Ok(())
    }

    fn value(&mut self) -> Result<(), Error> {
        if self.fragment_open() {
            return Err(Error::State("a scalar fragment is still open"));
        }
        match self.frames.last_mut() {
            Some(Frame::Array { populated }) => {
                let comma = *populated;
                *populated = true;
                if comma {
                    self.append(",")?;
                }
            }
            Some(Frame::Object { waiting, .. }) if *waiting => *waiting = false,
            Some(Frame::Object { .. }) => return Err(Error::State("object value has no key")),
            None if self.root => return Err(Error::State("document already has a root")),
            None => self.root = true,
        }
        Ok(())
    }

    fn begin(&mut self, frame: Frame, opening: &str) -> Result<(), Error> {
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
        self.perform(|writer| writer.begin(Frame::Array { populated: false }, "["))
    }

    /// Finish the current output array.
    /// # Errors
    /// Rejects mismatched containers or an unfinished string.
    pub fn array_end(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            if writer.fragment_open() || !matches!(writer.frames.last(), Some(Frame::Array { .. }))
            {
                return Err(Error::State("array end does not match the current value"));
            }
            writer.append("]")?;
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
                "{",
            )
        })
    }

    /// Finish an object after all its keys have values.
    /// # Errors
    /// Rejects mismatched containers, missing values or unfinished strings.
    pub fn object_end(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            if writer.fragment_open()
                || !matches!(
                    writer.frames.last(),
                    Some(Frame::Object { waiting: false, .. })
                )
            {
                return Err(Error::State(
                    "object end has a missing value or mismatched container",
                ));
            }
            writer.append("}")?;
            writer.frames.pop();
            Ok(())
        })
    }

    fn string_start(&mut self, length: Option<usize>, key: bool) -> Result<(), Error> {
        if key {
            if self.fragment_open() {
                return Err(Error::State("a scalar fragment is still open"));
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
                self.append(",")?;
            }
        } else {
            self.value()?;
        }
        self.append("\"")?;
        self.text = Some(Text {
            remaining: length,
            key,
        });
        Ok(())
    }

    fn string_fragment(&mut self, value: &str) -> Result<(), Error> {
        let remaining = match self
            .text
            .as_ref()
            .ok_or(Error::State("text fragment has no start"))?
            .remaining
        {
            Some(remaining) => Some(
                remaining
                    .checked_sub(value.len())
                    .ok_or(Error::State("text source length changed"))?,
            ),
            None => None,
        };
        const CONTROLS: [&str; 32] = [
            "\\u0000", "\\u0001", "\\u0002", "\\u0003", "\\u0004", "\\u0005", "\\u0006", "\\u0007",
            "\\b", "\\t", "\\n", "\\u000b", "\\f", "\\r", "\\u000e", "\\u000f", "\\u0010",
            "\\u0011", "\\u0012", "\\u0013", "\\u0014", "\\u0015", "\\u0016", "\\u0017", "\\u0018",
            "\\u0019", "\\u001a", "\\u001b", "\\u001c", "\\u001d", "\\u001e", "\\u001f",
        ];
        let mut start = 0;
        for (index, &byte) in value.as_bytes().iter().enumerate() {
            let escape = match byte {
                b'"' => "\\\"",
                b'\\' => "\\\\",
                0..=31 => CONTROLS[usize::from(byte)],
                _ => continue,
            };
            // Every split is at an ASCII escape character, hence a valid UTF-8
            // boundary. Native text ownership stays valid without unsafe casts.
            self.append(&value[start..index])?;
            self.append(escape)?;
            start = index + 1;
        }
        self.append(&value[start..])?;
        // No callback or borrow of external mutable storage occurs above.
        if let Some(text) = &mut self.text {
            text.remaining = remaining;
        }
        Ok(())
    }

    fn string_end(&mut self) -> Result<(), Error> {
        let Some(Text { remaining, key }) = self.text else {
            return Err(Error::State("text end has an incomplete or missing source"));
        };
        if remaining.is_some_and(|remaining| remaining != 0) {
            return Err(Error::State("text source is incomplete"));
        }
        self.append(if key { "\":" } else { "\"" })?;
        self.text = None;
        Ok(())
    }

    /// Begin a key using its complete UTF-8 byte length.
    /// Name admission and duplicate detection occur before this target call.
    /// # Errors
    /// Returns an invalid object state or allocation failure.
    pub fn key_start(&mut self, utf8_len: usize) -> Result<(), Error> {
        self.perform(|writer| writer.string_start(Some(utf8_len), true))
    }

    /// Write a small, already admitted full key without truncating at NUL.
    /// Long-name adapters use key_start and bounded text fragments instead.
    /// # Errors
    /// Returns a structural or allocation failure.
    pub fn key(&mut self, value: &str) -> Result<(), Error> {
        self.perform(|writer| {
            writer.string_start(Some(value.len()), true)?;
            writer.string_fragment(value)?;
            writer.string_end()
        })
    }

    fn temporal(&mut self, text: temporal::Text) -> Result<(), Error> {
        let text = text.as_str()?;
        self.string_start(Some(text.len()), false)?;
        self.string_fragment(text)?;
        self.string_end()
    }

    /// Start one exact integer; callers may return control between digit blocks.
    /// # Errors
    /// Propagates an actual writer-state or output-capacity error.
    pub fn integer_start(&mut self, negative: bool) -> Result<(), Error> {
        self.perform(|writer| {
            writer.value()?;
            if negative {
                writer.append("-")?;
            }
            writer.integer = Some(Integer {
                populated: false,
                zero: false,
            });
            Ok(())
        })
    }

    /// Append admitted ASCII decimal digits without a whole-value string.
    /// # Errors
    /// Rejects non-digits, empty blocks and leading zero spellings.
    pub fn integer_fragment(&mut self, digits: &[u8]) -> Result<(), Error> {
        self.perform(|writer| {
            let Some(integer) = writer.integer.as_ref() else {
                return Err(Error::State("integer fragment has no active integer"));
            };
            if digits.is_empty()
                || !digits.iter().all(u8::is_ascii_digit)
                || (integer.populated && integer.zero)
                || (!integer.populated && digits[0] == b'0' && digits.len() != 1)
            {
                return Err(Error::State("invalid integer digit fragment"));
            }
            let zero = !integer.populated && digits == b"0";
            writer.append(
                std::str::from_utf8(digits)
                    .map_err(|_| Error::State("integer digits were not ASCII"))?,
            )?;
            writer.integer = Some(Integer {
                populated: true,
                zero,
            });
            Ok(())
        })
    }

    /// Finish a nonempty exact integer before another value or publication.
    /// # Errors
    /// Rejects an empty or mismatched integer boundary.
    pub fn integer_end(&mut self) -> Result<(), Error> {
        self.perform(|writer| {
            if !matches!(
                writer.integer,
                Some(Integer {
                    populated: true,
                    ..
                })
            ) {
                return Err(Error::State("integer end has no complete integer"));
            }
            writer.integer = None;
            Ok(())
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
                    writer.string_start(Some(value.len()), false)?;
                    writer.string_fragment(value)?;
                    return writer.string_end();
                }
                ScalarRef::Bytes(_) => return Err(Error::Unsupported("binary")),
                ScalarRef::Uuid(value) => {
                    let text = sqlaero_transcode::identifier::UuidText::new(value);
                    writer.string_start(Some(36), false)?;
                    writer.string_fragment(text.as_str())?;
                    return writer.string_end();
                }
                ScalarRef::Date(date) => {
                    return writer.temporal(temporal::Text::calendar(date, None, None)?);
                }
                ScalarRef::Datetime {
                    date,
                    clock,
                    offset_micros,
                } => {
                    return writer.temporal(temporal::Text::calendar(
                        date,
                        Some(clock),
                        offset_micros,
                    )?);
                }
                ScalarRef::Time {
                    clock,
                    offset_micros,
                } => return writer.temporal(temporal::Text::time(clock, offset_micros)?),
                ScalarRef::Duration(micros) => {
                    return writer.temporal(temporal::Text::duration(micros)?);
                }
                _ => (),
            }
            writer.value()?;
            match value {
                ScalarRef::Null => writer.append("null"),
                ScalarRef::Boolean(value) => writer.append(if value { "true" } else { "false" }),
                ScalarRef::Signed(value) => {
                    writer.append(number::IntegerBuffer::new().format(value))
                }
                ScalarRef::Unsigned(value) => {
                    writer.append(number::IntegerBuffer::new().format(value))
                }
                ScalarRef::Float(value) => {
                    let mut buffer = number::FloatBuffer::new();
                    match buffer.format(value) {
                        Some(text) => writer.append(text),
                        None if options.allow_nan => writer.append("null"),
                        None => Err(Error::NonFinite),
                    }
                }
                _ => Err(Error::Unsupported(
                    "a scalar family not connected to the JSON target yet",
                )),
            }
        })
    }

    fn text_start(&mut self, length: Option<usize>, _: &Options) -> Result<(), Error> {
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
