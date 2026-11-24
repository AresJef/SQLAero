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

/// A complete normalized field name which fits in one unescaped write.
pub struct DirectKey(String);

impl DirectKey {
    /// Qualify the already owned name once. Longer or escaped keys retain
    /// their original storage for the caller's normal fragment path.
    /// # Errors
    /// Returns the original name if it needs escaping or bounded fragments.
    pub fn from_name(name: String) -> Result<Self, String> {
        if name.len() >= 256
            || name
                .bytes()
                .any(|byte| byte < b' ' || matches!(byte, b'"' | b'\\'))
        {
            return Err(name);
        }
        Ok(Self(name))
    }

    /// Use the same complete name for diagnostics and duplicate detection.
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// A real JSON target failure, before any output is published.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Recognized native temporal facts exceed the accurate admitted domain.
    #[error("{0}")]
    Range(&'static str),
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
    drained: bool,
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
        self.complete()?;
        if self.drained {
            return Err(Error::State("document prefix was already consumed"));
        }
        Ok(self.output)
    }

    fn complete(&self) -> Result<(), Error> {
        if self.failed || !self.root || !self.frames.is_empty() || self.fragment_open() {
            return Err(Error::State("document is incomplete or failed"));
        }
        Ok(())
    }

    /// Read the current complete-character UTF-8 fragment without copying it.
    /// A fragment may end inside a JSON value; its consumer retains the prefix
    /// privately until the complete document has successfully finished.
    /// # Errors
    /// A failed writer cannot publish any further fragment.
    pub fn pending_output(&self) -> Result<&str, Error> {
        if self.failed {
            return Err(Error::State("document already failed"));
        }
        Ok(&self.output)
    }

    /// Retire only a successfully consumed fragment, retaining its allocation
    /// and all container/string/integer state for the next native write.
    /// # Errors
    /// Rejects a failed writer instead of resetting or hiding its failure.
    pub fn consume_output(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::State("document already failed"));
        }
        self.drained |= !self.output.is_empty();
        self.output.clear();
        Ok(())
    }

    /// Finish a fragmented document and return only its unconsumed tail.
    /// The caller owns all earlier fragments and must discard them on failure.
    /// # Errors
    /// Rejects an empty, incomplete or previously failed document.
    pub fn finish_tail(self) -> Result<String, Error> {
        self.complete()?;
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

    fn value_delimiter(&mut self) -> Result<bool, Error> {
        if self.fragment_open() {
            return Err(Error::State("a scalar fragment is still open"));
        }
        match self.frames.last_mut() {
            Some(Frame::Array { populated }) => {
                return Ok(std::mem::replace(populated, true));
            }
            Some(Frame::Object { waiting, .. }) if *waiting => *waiting = false,
            Some(Frame::Object { .. }) => return Err(Error::State("object value has no key")),
            None if self.root => return Err(Error::State("document already has a root")),
            None => self.root = true,
        }
        Ok(false)
    }

    fn value(&mut self) -> Result<(), Error> {
        if self.value_delimiter()? {
            self.append(",")?;
        }
        Ok(())
    }

    /// Write one already qualified signed value with the usual JSON state and
    /// integer formatting contract.
    /// # Errors
    /// Propagates the original writer state or reservation error.
    pub fn signed(&mut self, value: i64) -> Result<(), Error> {
        self.perform(|writer| writer.signed_value(value))
    }

    fn signed_value(&mut self, value: i64) -> Result<(), Error> {
        self.integer_value(number::IntegerBuffer::new().format(value))
    }

    fn integer_value(&mut self, text: &str) -> Result<(), Error> {
        if !self.value_delimiter()? {
            return self.append(text);
        }
        // The native formatter supplies at most twenty ASCII bytes. Reserve
        // the comma and the complete number together for populated arrays.
        self.output
            .try_reserve(text.len() + 1)
            .map_err(CapacityError::Allocation)?;
        self.output.push(',');
        self.output.push_str(text);
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

    fn begin_key(&mut self) -> Result<bool, Error> {
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
        Ok(comma)
    }

    fn string_start(&mut self, length: Option<usize>, key: bool) -> Result<(), Error> {
        if key {
            if self.begin_key()? {
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
        let bytes = value.as_bytes();
        // Scan only large, already bounded fragments. Escape bytes are ASCII,
        // so a fragment without them can be appended as its original UTF-8.
        const LONG_FRAGMENT_BYTES: usize = 4096;
        if bytes.len() >= LONG_FRAGMENT_BYTES
            && memchr::memchr2(b'"', b'\\', bytes).is_none()
            && bytes.iter().copied().min().is_some_and(|byte| byte >= b' ')
        {
            self.append(value)?;
        } else {
            let mut text = value;
            while let Some((index, escape)) = text
                .bytes()
                .enumerate()
                .find_map(|(index, byte)| Self::ascii_escape(byte).map(|escape| (index, escape)))
            {
                // Every split is at an ASCII escape and therefore at a UTF-8
                // boundary. Leave isolated escapes on their simple path.
                if index != 0 {
                    self.append(&text[..index])?;
                }
                let rest = &text.as_bytes()[index + 1..];
                let consumed =
                    if let Some(second) = rest.first().copied().and_then(Self::ascii_escape) {
                        self.append_escape_run(escape, second, &rest[1..])?
                    } else {
                        self.append(escape)?;
                        1
                    };
                text = &text[index + consumed..];
            }
            self.append(text)?;
        }
        // No callback or borrow of external mutable storage occurs above.
        if let Some(text) = &mut self.text {
            text.remaining = remaining;
        }
        Ok(())
    }

    #[inline]
    fn ascii_escape(byte: u8) -> Option<&'static str> {
        const CONTROLS: [&str; 32] = [
            "\\u0000", "\\u0001", "\\u0002", "\\u0003", "\\u0004", "\\u0005", "\\u0006", "\\u0007",
            "\\b", "\\t", "\\n", "\\u000b", "\\f", "\\r", "\\u000e", "\\u000f", "\\u0010",
            "\\u0011", "\\u0012", "\\u0013", "\\u0014", "\\u0015", "\\u0016", "\\u0017", "\\u0018",
            "\\u0019", "\\u001a", "\\u001b", "\\u001c", "\\u001d", "\\u001e", "\\u001f",
        ];
        match byte {
            b'"' => Some("\\\""),
            b'\\' => Some("\\\\"),
            0..=31 => Some(CONTROLS[usize::from(byte)]),
            _ => None,
        }
    }

    /// Combine short ASCII escapes inside this already bounded input fragment.
    #[inline(never)]
    fn append_escape_run(
        &mut self,
        first: &'static str,
        second: &'static str,
        rest: &[u8],
    ) -> Result<usize, Error> {
        let mut buffer = [0; 192];
        let mut used = 0;
        let mut consumed = 0;
        let escapes = [first, second]
            .into_iter()
            .chain(rest.iter().copied().map_while(Self::ascii_escape));
        for escape in escapes {
            if used + escape.len() > buffer.len() {
                self.append_ascii_escapes(&buffer[..used])?;
                used = 0;
            }
            let end = used + escape.len();
            buffer[used..end].copy_from_slice(escape.as_bytes());
            used = end;
            consumed += 1;
        }
        self.append_ascii_escapes(&buffer[..used])?;
        Ok(consumed)
    }

    fn append_ascii_escapes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let escaped =
            std::str::from_utf8(bytes).map_err(|_| Error::State("invalid ASCII escapes"))?;
        self.append(escaped)
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

    /// Write a prequalified name, reusing the same object state as other keys.
    /// # Errors
    /// Returns a structural or allocation failure.
    pub fn direct_key(&mut self, key: &DirectKey) -> Result<(), Error> {
        self.perform(|writer| {
            let comma = writer.begin_key()?;
            let name = key.name();
            writer
                .output
                .try_reserve(name.len() + 3 + usize::from(comma))
                .map_err(CapacityError::Allocation)?;
            if comma {
                writer.output.push(',');
            }
            writer.output.push('"');
            writer.output.push_str(name);
            writer.output.push_str("\":");
            Ok(())
        })
    }

    fn temporal(&mut self, text: temporal::Text) -> Result<(), Error> {
        let text = text.as_str()?;
        self.value()?;
        // The temporal formatter supplies only digits and fixed JSON-safe
        // punctuation or unit letters. Its bounded output needs no quoting scan.
        self.output
            .try_reserve(text.len() + 2)
            .map_err(CapacityError::Allocation)?;
        self.output.push('"');
        self.output.push_str(text);
        self.output.push('"');
        Ok(())
    }

    /// Reserve and append a canonical UUID with its full JSON delimiter.
    #[inline(never)]
    fn uuid(&mut self, value: u128) -> Result<(), Error> {
        let uuid = sqlaero_transcode::identifier::UuidText::new(value);
        self.unescaped_string(uuid.as_str())
    }

    fn unescaped_string(&mut self, text: &str) -> Result<(), Error> {
        let comma = self.value_delimiter()?;
        self.output
            .try_reserve(text.len() + 2 + usize::from(comma))
            .map_err(CapacityError::Allocation)?;
        if comma {
            self.output.push(',');
        }
        self.output.push('"');
        self.output.push_str(text);
        self.output.push('"');
        Ok(())
    }

    fn string_value(&mut self, value: &str) -> Result<(), Error> {
        // A complete short root can reserve its whole document in one step.
        // Keep the repeated container-value loop on the fragment writer.
        const SHORT_BYTES: usize = 64;
        if self.frames.is_empty()
            && value.len() <= SHORT_BYTES
            && !value
                .bytes()
                .any(|byte| byte < b' ' || matches!(byte, b'"' | b'\\'))
        {
            return self.unescaped_string(value);
        }
        self.string_start(Some(value.len()), false)?;
        self.string_fragment(value)?;
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
                ScalarRef::Signed(value) => return writer.signed_value(value),
                ScalarRef::Unsigned(value) => {
                    return writer.integer_value(number::IntegerBuffer::new().format(value));
                }
                ScalarRef::Text(value) => return writer.string_value(value),
                ScalarRef::Bytes(_) => return Err(Error::Unsupported("binary")),
                ScalarRef::Decimal {
                    negative,
                    digits,
                    exponent,
                } => {
                    use sqlaero_transcode::decimal::Render;
                    let header = digits.inspect(negative, exponent).map_err(Error::Range)?;
                    if let Some(layout) = writer.decimal_start(header, options)? {
                        let mut render = Render::new(digits, layout).map_err(Error::Range)?;
                        let mut chunk = [0; 4096];
                        loop {
                            let n = render.read(&mut chunk);
                            if n == 0 {
                                break;
                            }
                            let text = std::str::from_utf8(&chunk[..n])
                                .map_err(|_| Error::State("decimal renderer emitted non-ASCII"))?;
                            writer.decimal_fragment(text, options)?;
                        }
                        writer.decimal_end(options)?;
                    }
                    return Ok(());
                }
                ScalarRef::Uuid(value) => {
                    return writer.uuid(value);
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
                ScalarRef::Ticks {
                    value,
                    unit,
                    multiplier,
                    kind,
                } => {
                    let value =
                        sqlaero_transcode::temporal::decompose(value, unit, multiplier, kind)
                            .map_err(temporal::source_error)?;
                    return writer.temporal(temporal::Text::native(value)?);
                }
                ScalarRef::UtcTicks {
                    value,
                    unit,
                    multiplier,
                    offset_micros,
                } => {
                    let offset = offset_micros.ok_or(Error::State(
                        "aware datetime source did not acquire its requested local offset",
                    ))?;
                    return writer
                        .temporal(temporal::Text::utc_ticks(value, unit, multiplier, offset)?);
                }
                ScalarRef::ClockTicks {
                    value,
                    unit,
                    multiplier,
                } => {
                    let value =
                        sqlaero_transcode::temporal::decompose_clock(value, unit, multiplier)
                            .map_err(temporal::source_error)?;
                    return writer.temporal(temporal::Text::native(value)?);
                }
                ScalarRef::DateDays(value) => {
                    let value = sqlaero_transcode::temporal::decompose_date(value)
                        .map_err(temporal::source_error)?;
                    return writer.temporal(temporal::Text::native(value)?);
                }
                _ => (),
            }
            writer.value()?;
            match value {
                ScalarRef::Null => writer.append("null"),
                ScalarRef::Boolean(value) => writer.append(if value { "true" } else { "false" }),
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

    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        options: &Options,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, Error> {
        self.perform(|writer| {
            use sqlaero_transcode::decimal::{Layout, Notation};
            if header.category().is_some() {
                if !options.allow_nan {
                    return Err(Error::NonFinite);
                }
                writer.value()?;
                writer.append("null")?;
                return Ok(None);
            }
            let notation = if header.exponent() <= 0 && header.adjusted() >= -6 {
                Notation::Plain
            } else {
                Notation::Scientific
            };
            let layout = Layout::new(header, notation).map_err(Error::Range)?;
            writer.string_start(Some(layout.len()), false)?;
            Ok(Some(layout))
        })
    }
    fn decimal_fragment(&mut self, ascii: &str, _: &Options) -> Result<(), Error> {
        self.perform(|writer| writer.string_fragment(ascii))
    }
    fn decimal_end(&mut self, _: &Options) -> Result<(), Error> {
        self.perform(Self::string_end)
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
