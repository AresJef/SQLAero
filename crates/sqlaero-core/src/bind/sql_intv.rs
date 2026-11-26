//! Fixed temporal interval grammar, shared by functions and interval units.

use super::sql_func::{Part, Role};
use super::writer;
use crate::protocol::framing::WireError;
use sqlaero_transcode::integer::{Digits, Error as IntegerError};

/// Units admitted by MySQL EXTRACT.
pub const EXTRACT_UNITS: &[&str] = &[
    "MICROSECOND",
    "SECOND",
    "MINUTE",
    "HOUR",
    "DAY",
    "WEEK",
    "MONTH",
    "QUARTER",
    "YEAR",
    "SECOND_MICROSECOND",
    "MINUTE_MICROSECOND",
    "MINUTE_SECOND",
    "HOUR_MICROSECOND",
    "HOUR_SECOND",
    "HOUR_MINUTE",
    "DAY_MICROSECOND",
    "DAY_SECOND",
    "DAY_MINUTE",
    "DAY_HOUR",
    "YEAR_MONTH",
];

/// Render the quantity in the current scalar context before the fixed unit.
pub const fn single(unit: &'static str) -> [Part; 3] {
    [
        Part::Syntax("INTERVAL "),
        Part::Operand(0, Role::Scalar),
        Part::Syntax(unit),
    ]
}

/// Fixed MySQL compound interval layouts.
#[derive(Clone, Copy)]
pub enum Compound {
    /// YEAR, MONTH.
    YearMonth,
    /// DAY, HOUR.
    DayHour,
    /// DAY, HOUR, MINUTE.
    DayMinute,
    /// DAY, HOUR, MINUTE, SECOND.
    DaySecond,
    /// DAY, HOUR, MINUTE, SECOND, MICROSECOND.
    DayMicrosecond,
    /// HOUR, MINUTE.
    HourMinute,
    /// HOUR, MINUTE, SECOND.
    HourSecond,
    /// HOUR, MINUTE, SECOND, MICROSECOND.
    HourMicrosecond,
    /// MINUTE, SECOND.
    MinuteSecond,
    /// MINUTE, SECOND, MICROSECOND.
    MinuteMicrosecond,
    /// SECOND, MICROSECOND.
    SecondMicrosecond,
}

#[derive(Clone, Copy)]
struct Field {
    scale: u64,
    separator: &'static [u8],
    width: usize,
    leading_max: u64,
}

// The MySQL composite parser accumulates digits in a signed 64-bit integer.
// Its pre-multiply bound also excludes its top eight positive values.
const PARSER_MAX: u64 = i64::MAX as u64 - 8;
// The leading year/day/hour fields additionally pass through unsigned long.
// Use its portable width so targets with 32-bit long cannot narrow a value.
const ULONG_MAX: u64 = u32::MAX as u64;

const YEAR: Field = Field {
    scale: 12,
    separator: b"",
    width: 0,
    leading_max: ULONG_MAX,
};
const MONTH: Field = Field {
    scale: 1,
    separator: b"-",
    width: 2,
    leading_max: ULONG_MAX,
};
const DAY: Field = Field {
    scale: 86_400_000_000,
    separator: b"",
    width: 0,
    leading_max: ULONG_MAX,
};
const HOUR: Field = Field {
    scale: 3_600_000_000,
    separator: b" ",
    width: 2,
    leading_max: ULONG_MAX,
};
const MINUTE: Field = Field {
    scale: 60_000_000,
    separator: b":",
    width: 2,
    leading_max: PARSER_MAX,
};
const SECOND: Field = Field {
    scale: 1_000_000,
    separator: b":",
    width: 2,
    leading_max: PARSER_MAX,
};
const MICROSECOND: Field = Field {
    scale: 1,
    separator: b".",
    width: 6,
    leading_max: PARSER_MAX,
};

impl Compound {
    /// The fixed keyword and signed component layout.
    fn layout(self) -> (&'static str, &'static [Field]) {
        match self {
            Self::YearMonth => ("YEAR_MONTH", &[YEAR, MONTH]),
            Self::DayHour => ("DAY_HOUR", &[DAY, HOUR]),
            Self::DayMinute => ("DAY_MINUTE", &[DAY, HOUR, MINUTE]),
            Self::DaySecond => ("DAY_SECOND", &[DAY, HOUR, MINUTE, SECOND]),
            Self::DayMicrosecond => ("DAY_MICROSECOND", &[DAY, HOUR, MINUTE, SECOND, MICROSECOND]),
            Self::HourMinute => ("HOUR_MINUTE", &[HOUR, MINUTE]),
            Self::HourSecond => ("HOUR_SECOND", &[HOUR, MINUTE, SECOND]),
            Self::HourMicrosecond => ("HOUR_MICROSECOND", &[HOUR, MINUTE, SECOND, MICROSECOND]),
            Self::MinuteSecond => ("MINUTE_SECOND", &[MINUTE, SECOND]),
            Self::MinuteMicrosecond => ("MINUTE_MICROSECOND", &[MINUTE, SECOND, MICROSECOND]),
            Self::SecondMicrosecond => ("SECOND_MICROSECOND", &[SECOND, MICROSECOND]),
        }
    }

    /// Canonical unit name for SQL and diagnostics.
    pub fn name(self) -> &'static str {
        self.layout().0
    }
}

/// One signed source component, with the original native integer magnitude.
#[derive(Clone, Copy)]
pub enum Component<'a> {
    /// A signed integer that already fits the compact representation.
    Compact(i64),
    /// An explicit sign and unchanged source digits.
    Wide {
        /// True for a negative value.
        negative: bool,
        /// Immutable magnitude owned by the caller.
        digits: Digits<'a>,
    },
}

/// A compound interval failed before a complete literal could be written.
#[derive(Debug, thiserror::Error)]
pub enum CompoundError {
    /// The exact total has no lossless native interval representation.
    #[error("normalized interval exceeds the supported range")]
    Range,
    /// The inputs do not match the expected native layout.
    #[error("invalid interval component layout")]
    Layout,
    /// A retained source has invalid native digits.
    #[error(transparent)]
    Integer(#[from] IntegerError),
}

/// Bounded accumulation of signed source components before any SQL is emitted.
pub struct Reduce<'a, 'src> {
    components: &'a [Component<'src>],
    fields: &'static [Field],
    unit: &'static str,
    remaining: usize,
    bits: u8,
    total: i128,
    maximum: i128,
    weight_sum: i128,
}

impl<'a, 'src> Reduce<'a, 'src> {
    /// Prepare the reduction without allocating or reading any source digits.
    /// # Errors
    /// Returns a layout error for mismatched component counts or radices.
    pub fn new(unit: Compound, components: &'a [Component<'src>]) -> Result<Self, CompoundError> {
        let (unit, fields) = unit.layout();
        if components.len() != fields.len() {
            return Err(CompoundError::Layout);
        }
        let mut bits = None;
        let mut remaining = 0;
        for component in components {
            if let Component::Wide { digits, .. } = component {
                let found = digits.radix_bits();
                if bits
                    .replace(found)
                    .is_some_and(|previous| previous != found)
                {
                    return Err(CompoundError::Layout);
                }
                remaining = remaining.max(digits.digit_count());
            }
        }
        let bits = bits.unwrap_or(32);
        remaining = remaining.max(64_usize.div_ceil(usize::from(bits)));
        let leading = fields[0];
        let maximum = (i128::from(leading.leading_max) + 1) * i128::from(leading.scale) - 1;
        Ok(Self {
            components,
            fields,
            unit,
            remaining,
            bits,
            total: 0,
            maximum,
            weight_sum: fields.iter().map(|field| i128::from(field.scale)).sum(),
        })
    }

    /// Reduce at most `work` digits from each component before yielding.
    /// # Errors
    /// Returns a range error when the exact total cannot be represented,
    /// or a layout error when a source digit is invalid.
    pub fn advance(&mut self, work: usize) -> Result<Option<Normalized>, CompoundError> {
        let radix = 1_i128 << self.bits;
        for _ in 0..work.min(self.remaining) {
            self.remaining -= 1;
            let index = self.remaining;
            // Even the largest minute layout is below 2^90 total ticks.
            // The largest supported digit adds at most 32 bits. The previous
            // prefix was already range-checked, so this step fits i128.
            self.total *= radix;
            for (component, field) in self.components.iter().zip(self.fields) {
                let digit = match *component {
                    Component::Compact(value) => {
                        let shift = index.saturating_mul(usize::from(self.bits));
                        let digit = if shift < 64 {
                            (value.unsigned_abs() >> shift) & ((1_u64 << self.bits) - 1)
                        } else {
                            0
                        };
                        if value < 0 {
                            -i128::from(digit)
                        } else {
                            i128::from(digit)
                        }
                    }
                    Component::Wide { negative, digits } => {
                        let digit = i128::from(digits.digit(index)?);
                        if negative { -digit } else { digit }
                    }
                };
                self.total += digit * i128::from(field.scale);
            }
            // With k digits left, the maximum opposing suffix is
            // weight_sum * (radix^k - 1). Reject only an unrecoverable
            // prefix, keeping exact cancellation between very large values.
            if self.total.abs() > self.maximum + self.weight_sum {
                return Err(CompoundError::Range);
            }
        }
        if self.remaining != 0 {
            return Ok(None);
        }
        if self.total.abs() > self.maximum {
            return Err(CompoundError::Range);
        }
        Ok(Some(Normalized {
            unit: self.unit,
            fields: self.fields,
            total: self.total,
        }))
    }
}

/// An exact native interval ready to append to the request.
pub struct Normalized {
    unit: &'static str,
    fields: &'static [Field],
    total: i128,
}

impl Normalized {
    /// Append the signed canonical interval using its closed grammar.
    /// # Errors
    /// Returns a wire error if the request quota or reservation fails.
    pub fn write(self, output: &mut Vec<u8>, limit: usize) -> Result<(), WireError> {
        let mut magnitude = self.total.unsigned_abs();
        writer::append(
            output,
            if self.total < 0 {
                b"INTERVAL '-"
            } else {
                b"INTERVAL '"
            },
            limit,
        )?;
        for (index, field) in self.fields.iter().enumerate() {
            let scale = u128::from(field.scale);
            let value = magnitude / scale;
            magnitude %= scale;
            let mut buffer = itoa::Buffer::new();
            let digits = buffer.format(value);
            if index != 0 {
                writer::append(output, field.separator, limit)?;
                let padding = field.width.saturating_sub(digits.len());
                writer::append(output, &b"000000"[..padding], limit)?;
            }
            writer::append(output, digits.as_bytes(), limit)?;
        }
        writer::append(output, b"' ", limit)?;
        writer::append(output, self.unit.as_bytes(), limit)
    }
}
