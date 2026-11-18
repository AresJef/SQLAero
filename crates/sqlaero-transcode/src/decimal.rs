//! Exact decimal facts and bounded format-neutral decimal rendering.
//!
//! Admission and the choice of plain/scientific notation belong to each target.
//! A source may borrow its canonical ASCII coefficient instead of copying it.

mod binary;
pub use binary::Coefficient;

/// The actual nonfinite source category, before a target's null policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Special {
    /// Infinite magnitude.
    Infinity,
    /// Quiet NaN, possibly with a source diagnostic payload.
    QuietNaN,
    /// Signaling NaN, possibly with a source diagnostic payload.
    SignalingNaN,
}

/// An exact coefficient, with any original leading/trailing zeros intact.
#[derive(Clone, Copy, Debug)]
pub enum Digits<'a> {
    /// Numeric digit bytes in 0..=9 from a native numeric source.
    Values(&'a [u8]),
    /// Canonical ASCII digits on either side of an optional source point.
    Ascii(&'a [u8], &'a [u8]),
}

impl Digits<'_> {
    /// Copy a bounded numeric preview from the inspected original coefficient.
    /// # Errors
    /// Rejects storage whose length differs from its already inspected header.
    pub fn preview(self, header: Header, output: &mut [u8]) -> Result<usize, &'static str> {
        if header.special.is_some() {
            return Ok(0);
        }
        if header.leading.checked_add(header.digits) != Some(self.len()) {
            return Err("Decimal coefficient no longer matches its source header");
        }
        let n = header.digits.min(output.len());
        self.copy(header.leading, &mut output[..n]);
        for digit in &mut output[..n] {
            *digit -= b'0';
        }
        Ok(n)
    }
    fn len(self) -> usize {
        match self {
            Self::Values(value) => value.len(),
            Self::Ascii(first, second) => first.len().saturating_add(second.len()),
        }
    }
    fn copy(&self, start: usize, output: &mut [u8]) {
        match *self {
            Self::Values(digits) => {
                for (out, &value) in output.iter_mut().zip(&digits[start..]) {
                    *out = b'0' + value;
                }
            }
            Self::Ascii(first, second) => {
                let prefix = first.len().saturating_sub(start).min(output.len());
                if prefix != 0 {
                    output[..prefix].copy_from_slice(&first[start..start + prefix]);
                }
                if prefix < output.len() {
                    let offset = start.saturating_sub(first.len());
                    let length = output.len() - prefix;
                    output[prefix..].copy_from_slice(&second[offset..offset + length]);
                }
            }
        }
    }
}

/// Validated source facts, including representation and necessary-value facts.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    negative: bool,
    exponent: i64,
    digits: usize,
    leading: usize,
    trailing: usize,
    zero: bool,
    special: Option<Special>,
}

impl Header {
    /// Construct one nonfinite source category without performing arithmetic.
    pub fn special(negative: bool, category: Special) -> Self {
        Self {
            negative,
            exponent: 0,
            digits: 0,
            leading: 0,
            trailing: 0,
            zero: false,
            special: Some(category),
        }
    }
    /// Original sign, including signed zero and special values.
    pub fn negative(self) -> bool {
        self.negative
    }
    /// Original finite base-ten exponent.
    pub fn exponent(self) -> i64 {
        self.exponent
    }
    /// Coefficient digit count after insignificant leading zeros only.
    pub fn digits(self) -> usize {
        self.digits
    }
    /// Number of trailing coefficient zeros, without discarding source scale.
    pub fn trailing(self) -> usize {
        self.trailing
    }
    /// Whether the finite coefficient is zero.
    pub fn is_zero(self) -> bool {
        self.zero
    }
    /// Actual nonfinite category, or None for a finite value.
    pub fn category(self) -> Option<Special> {
        self.special
    }
    /// The adjusted decimal exponent used by GDA-style string notation.
    pub fn adjusted(self) -> i128 {
        self.digits as i128 - 1 + i128::from(self.exponent)
    }
}

/// Incremental coefficient inspection; callers choose their bounded windows.
#[derive(Default)]
pub struct Inspect {
    total: usize,
    first: Option<usize>,
    last: Option<usize>,
}

impl Inspect {
    /// Inspect one actual digit, never a formatted target character.
    /// # Errors
    /// Rejects a malformed digit or an unrepresentable coefficient length.
    pub fn digit(&mut self, value: u8) -> Result<(), &'static str> {
        if value > 9 {
            return Err("invalid Decimal coefficient digit");
        }
        if value != 0 {
            self.first.get_or_insert(self.total);
            self.last = Some(self.total);
        }
        self.total = self
            .total
            .checked_add(1)
            .ok_or("Decimal coefficient is too large")?;
        Ok(())
    }
    /// Finish source facts without normalizing scale or signed zero.
    /// # Errors
    /// Rejects an empty finite coefficient.
    pub fn finish(self, negative: bool, exponent: i64) -> Result<Header, &'static str> {
        if self.total == 0 {
            return Err("empty Decimal coefficient");
        }
        let leading = self.first.unwrap_or(self.total - 1);
        Ok(Header {
            negative,
            exponent,
            digits: self.total - leading,
            leading,
            trailing: self.last.map_or(0, |last| self.total - 1 - last),
            zero: self.first.is_none(),
            special: None,
        })
    }
}

/// Target-selected notation, separate from the original coefficient facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Notation {
    /// An ordinary exact decimal number, including all source fractional zeros.
    Plain,
    /// One leading digit, optional fractional digits and an uppercase exponent.
    Scientific,
}

/// A small rendering plan; it never owns a complete formatted decimal.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    header: Header,
    notation: Notation,
    length: usize,
    point: i128,
}

impl Layout {
    /// Prepare target-selected notation. Plain positive-exponent zero has no
    /// expanded integer zeros, matching the exact ordinary value zero.
    /// # Errors
    /// Rejects a nonfinite header or an output length beyond addressable memory.
    pub fn new(header: Header, notation: Notation) -> Result<Self, &'static str> {
        if header.special.is_some() {
            return Err("nonfinite Decimal has no numeric layout");
        }
        let exponent = if notation == Notation::Plain && header.zero && header.exponent > 0 {
            0
        } else {
            header.exponent
        };
        let point = header.digits as i128 + i128::from(exponent);
        let sign = i128::from(header.negative);
        let length = match notation {
            Notation::Plain if point <= 0 => sign + 2 - point + header.digits as i128,
            Notation::Plain if point >= header.digits as i128 => sign + point,
            Notation::Plain => sign + header.digits as i128 + 1,
            Notation::Scientific => {
                let adjusted = header.adjusted();
                sign + header.digits as i128
                    + i128::from(header.digits > 1)
                    + 1
                    + i128::from(adjusted >= 0)
                    + crate::number::IntegerBuffer::new().format(adjusted).len() as i128
            }
        };
        let length =
            usize::try_from(length).map_err(|_| "Decimal output exceeds addressable memory")?;
        Ok(Self {
            header,
            notation,
            length,
            point,
        })
    }
    /// Exact output bytes before any target quoting or wire prefix.
    pub fn len(self) -> usize {
        self.length
    }
    /// Whether the exact output is empty (always false for a valid decimal).
    pub fn is_empty(self) -> bool {
        self.length == 0
    }
}

#[derive(Clone, Copy)]
enum Segment {
    Empty,
    Text(&'static [u8]),
    Digits { start: usize, length: usize },
    Zeros(usize),
    Exponent,
}

// The longest layout is a sign, leading digit, point, remaining digits and
// exponent. Plain notation needs fewer segments, regardless of value length.
const MAX_SEGMENTS: usize = 5;

/// Bounded ASCII renderer over borrowed original coefficient storage.
pub struct Render<'a> {
    digits: Digits<'a>,
    segments: [Segment; MAX_SEGMENTS],
    segment: usize,
    offset: usize,
    suffix: [u8; 42],
    suffix_len: usize,
}

impl<'a> Render<'a> {
    /// Start a validated source coefficient with its target-selected plan.
    /// # Errors
    /// Rejects storage that no longer matches the inspected coefficient length.
    pub fn new(digits: Digits<'a>, layout: Layout) -> Result<Self, &'static str> {
        let h = layout.header;
        if h.leading.checked_add(h.digits) != Some(digits.len()) {
            return Err("Decimal coefficient no longer matches its source header");
        }
        let mut segments = [Segment::Empty; MAX_SEGMENTS];
        let mut at = 0;
        if h.negative {
            segments[at] = Segment::Text(b"-");
            at += 1;
        }
        let mut suffix = [0; 42];
        let mut suffix_len = 0;
        match layout.notation {
            Notation::Plain if layout.point <= 0 => {
                segments[at] = Segment::Text(b"0.");
                segments[at + 1] = Segment::Zeros((-layout.point) as usize);
                segments[at + 2] = Segment::Digits {
                    start: h.leading,
                    length: h.digits,
                };
            }
            Notation::Plain if layout.point >= h.digits as i128 => {
                segments[at] = Segment::Digits {
                    start: h.leading,
                    length: h.digits,
                };
                segments[at + 1] = Segment::Zeros((layout.point - h.digits as i128) as usize);
            }
            Notation::Plain => {
                let point = layout.point as usize;
                segments[at] = Segment::Digits {
                    start: h.leading,
                    length: point,
                };
                segments[at + 1] = Segment::Text(b".");
                segments[at + 2] = Segment::Digits {
                    start: h.leading + point,
                    length: h.digits - point,
                };
            }
            Notation::Scientific => {
                segments[at] = Segment::Digits {
                    start: h.leading,
                    length: 1,
                };
                at += 1;
                if h.digits > 1 {
                    segments[at] = Segment::Text(b".");
                    segments[at + 1] = Segment::Digits {
                        start: h.leading + 1,
                        length: h.digits - 1,
                    };
                    at += 2;
                }
                segments[at] = Segment::Exponent;
                suffix[0] = b'E';
                suffix_len = 1;
                let exponent = h.adjusted();
                if exponent >= 0 {
                    suffix[1] = b'+';
                    suffix_len += 1;
                }
                let mut buffer = crate::number::IntegerBuffer::new();
                let text = buffer.format(exponent).as_bytes();
                suffix[suffix_len..suffix_len + text.len()].copy_from_slice(text);
                suffix_len += text.len();
            }
        }
        Ok(Self {
            digits,
            segments,
            segment: 0,
            offset: 0,
            suffix,
            suffix_len,
        })
    }

    /// Copy at most the caller's bounded window. With a nonempty output buffer,
    /// a zero return means finished, as with a standard Rust read operation.
    pub fn read(&mut self, output: &mut [u8]) -> usize {
        let mut written = 0;
        while written < output.len() && self.segment < self.segments.len() {
            let segment = self.segments[self.segment];
            let length = match segment {
                Segment::Empty => 0,
                Segment::Text(text) => text.len(),
                Segment::Digits { length, .. } | Segment::Zeros(length) => length,
                Segment::Exponent => self.suffix_len,
            };
            if self.offset == length {
                self.segment += 1;
                self.offset = 0;
                continue;
            }
            let n = (length - self.offset).min(output.len() - written);
            let target = &mut output[written..written + n];
            match segment {
                Segment::Text(text) => target.copy_from_slice(&text[self.offset..self.offset + n]),
                Segment::Digits { start, .. } => self.digits.copy(start + self.offset, target),
                Segment::Zeros(_) => target.fill(b'0'),
                Segment::Exponent => {
                    target.copy_from_slice(&self.suffix[self.offset..self.offset + n])
                }
                Segment::Empty => unreachable!("zero-length segments advance before copying"),
            }
            self.offset += n;
            written += n;
        }
        written
    }
}
