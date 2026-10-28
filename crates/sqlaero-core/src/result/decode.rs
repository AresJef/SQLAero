//! Native field parsing; final Python objects belong to the output boundary.

use crate::bind::scalar::{Clock, Date, month_days};
use crate::charset::output::OutputCodec;

/// One prepared native action for a non-NULL field.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// Strict signed/unsigned integer width.
    Integer {
        /// Actual unsigned metadata flag.
        unsigned: bool,
        /// Actual storage width in bits.
        bits: u8,
    },
    /// Correctly rounded received FLOAT/DOUBLE/decimal-as-float text.
    Float,
    /// MySQL YEAR: zero or a stored year from 1901 through 2155.
    Year,
    /// Exact Decimal construction from original ASCII text.
    Decimal {
        /// Keep exact Decimal rather than directly producing binary64.
        exact: bool,
    },
    /// Final independent Python bytes.
    Bytes,
    /// Direct unsigned BIT integer with its declared width.
    Bit {
        /// Actual declared bit width.
        width: u32,
        /// Produce int when true, otherwise preserve independent bytes.
        integer: bool,
    },
    /// Known encoded text.
    Text,
    /// Ordered SET text members.
    Set,
    /// One complete JSON value through the selected native parser.
    Json,
    /// Calendar date, with approved strict/fallback behavior.
    Date {
        /// Reject content that cannot become date, rather than retaining text.
        strict: bool,
    },
    /// Naive datetime, with approved strict/fallback behavior.
    Datetime {
        /// Reject content that cannot become datetime, rather than retaining text.
        strict: bool,
    },
    /// Exact duration or representable nonnegative clock time.
    Time {
        /// Return the full timedelta instead of a representable clock time.
        duration: bool,
    },
    /// A NULL-type column must not invent non-NULL data.
    Null,
}

/// Prepared from real metadata and one execute-time policy snapshot.
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    /// Concrete action; field loops do not repeat schema classification.
    pub kind: Kind,
    /// Actual encoding for applicable text, including converted JSON results.
    pub text: OutputCodec,
    /// Applicable text recovery, never a JSON repair option.
    pub strict_text: bool,
}
impl Plan {
    /// Prepare once per result column. Unknown semantic types remain bytes;
    /// unknown text encodings stay explicit and fail only when decoding data.
    #[must_use]
    pub fn new(
        type_code: u8,
        flags: u16,
        collation_id: u16,
        length: u32,
        options: Options,
    ) -> Self {
        use mysql_common::constants::ColumnFlags;
        let unsigned = flags & ColumnFlags::UNSIGNED_FLAG.bits() != 0;
        let text = OutputCodec::for_collation(collation_id);
        let kind = match type_code {
            1 => Kind::Integer { unsigned, bits: 8 },
            2 => Kind::Integer { unsigned, bits: 16 },
            3 => Kind::Integer { unsigned, bits: 32 },
            8 => Kind::Integer { unsigned, bits: 64 },
            9 => Kind::Integer { unsigned, bits: 24 },
            13 => Kind::Year,
            4 | 5 => Kind::Float,
            0 | 246 => Kind::Decimal {
                exact: options.use_decimal,
            },
            6 => Kind::Null,
            7 | 12 | 17 | 18 => Kind::Datetime {
                strict: options.decode_date_strict,
            },
            10 | 14 => Kind::Date {
                strict: options.decode_date_strict,
            },
            11 | 19 => Kind::Time {
                duration: options.use_timedelta,
            },
            16 => Kind::Bit {
                width: length,
                integer: options.decode_bit,
            },
            245 => {
                if options.decode_json {
                    Kind::Json
                } else {
                    Kind::Text
                }
            }
            248 => {
                if options.decode_set {
                    Kind::Set
                } else {
                    Kind::Text
                }
            }
            247 => Kind::Text,
            15 | 249..=254 => {
                if flags & ColumnFlags::SET_FLAG.bits() != 0 {
                    if options.decode_set {
                        Kind::Set
                    } else {
                        Kind::Text
                    }
                } else if flags & ColumnFlags::ENUM_FLAG.bits() != 0 {
                    Kind::Text
                } else if collation_id == 63 {
                    Kind::Bytes
                } else {
                    Kind::Text
                }
            }
            _ => Kind::Bytes,
        };
        // Numeric and calendar text uses the protocol's ASCII representation,
        // even when character_set_results converts metadata or normal strings.
        // A calendar fallback must retain these original bytes in that format.
        let text = if matches!(
            kind,
            Kind::Decimal { .. } | Kind::Date { .. } | Kind::Datetime { .. }
        ) {
            OutputCodec::for_collation(11)
        } else if type_code == 245 && collation_id == 63 {
            OutputCodec::for_collation(255)
        } else {
            text
        };
        Self {
            kind,
            text,
            strict_text: options.decode_text_strict,
        }
    }
}

/// Immutable execute-time output policy. Public setters validate exact bools;
/// native field decoding never interprets Python truthiness or current state.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Decode actual JSON fields into their final Python value.
    pub decode_json: bool,
    /// Split actual SET text into an ordered tuple, preserving empty members.
    pub decode_set: bool,
    /// Decode BIT directly as an unsigned integer.
    pub decode_bit: bool,
    /// Preserve exact Decimal scale instead of parsing directly to binary64.
    pub use_decimal: bool,
    /// Preserve the entire TIME duration, instead of requiring a clock time.
    pub use_timedelta: bool,
    /// Reject invalid text instead of applicable surrogateescape recovery.
    pub decode_text_strict: bool,
    /// Reject unrepresentable dates instead of returning the original text.
    pub decode_date_strict: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            decode_json: false,
            decode_set: false,
            decode_bit: false,
            use_decimal: true,
            use_timedelta: true,
            decode_text_strict: true,
            decode_date_strict: true,
        }
    }
}

/// A supported field's actual content failure, not a fallback to raw bytes.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("{0}")]
pub struct ContentError(pub &'static str);

/// Parsed exact integer without a string, float or generic value tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integer {
    /// A signed integer, including the exact i64 minimum.
    Signed(i64),
    /// An unsigned integer through the exact u64 maximum.
    Unsigned(u64),
}

/// Read actual signed/unsigned MySQL integer width from canonical ASCII data.
/// Leading zeroes used by ZEROFILL do not change the value.
///
/// # Errors
/// Rejects missing/non-digit text, negative unsigned values and real overflow.
pub fn integer(data: &[u8], unsigned: bool, bits: u8) -> Result<Integer, ContentError> {
    if bits == 0 || bits > 64 {
        return Err(ContentError("invalid integer column width"));
    }
    let (negative, digits) = match data.first() {
        Some(b'-') if !unsigned => (true, &data[1..]),
        Some(b'+') => (false, &data[1..]),
        _ => (false, data),
    };
    if digits.is_empty() {
        return Err(ContentError("empty MySQL integer value"));
    }
    let mut magnitude = 0_u64;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return Err(ContentError("invalid MySQL integer text"));
        }
        magnitude = magnitude
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(byte - b'0')))
            .ok_or(ContentError("MySQL integer exceeds its declared range"))?;
    }
    if unsigned {
        let max = if bits == 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        if magnitude > max {
            return Err(ContentError(
                "unsigned MySQL integer exceeds its declared range",
            ));
        }
        Ok(Integer::Unsigned(magnitude))
    } else {
        let bound = 1_u64 << (bits - 1);
        if magnitude > bound || (!negative && magnitude == bound) {
            return Err(ContentError(
                "signed MySQL integer exceeds its declared range",
            ));
        }
        let value = if negative {
            if magnitude == 1_u64 << 63 {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };
        Ok(Integer::Signed(value))
    }
}

/// Correctly rounded binary64 from the complete received numeric text.
/// FLOAT is deliberately not quantized back to f32; signed underflow remains.
///
/// # Errors
/// Rejects invalid text, overflow and abnormal NaN/Infinity results.
pub fn float(data: &[u8]) -> Result<f64, ContentError> {
    let value: f64 = std::str::from_utf8(data)
        .map_err(|_| ContentError("non-ASCII MySQL floating text"))?
        .parse()
        .map_err(|_| ContentError("invalid MySQL floating text"))?;
    if !value.is_finite() {
        return Err(ContentError("non-finite MySQL floating result"));
    }
    Ok(value)
}

/// Validate Decimal text while preserving its sign, trailing zeroes and scale.
/// The output boundary passes this exact ASCII field to Decimal once.
///
/// # Errors
/// Rejects non-decimal syntax; no exponent/NaN/Infinity extension is admitted.
pub fn decimal(data: &[u8]) -> Result<(), ContentError> {
    let data = if matches!(data.first(), Some(b'-' | b'+')) {
        &data[1..]
    } else {
        data
    };
    let mut point = false;
    let mut digits = 0;
    let mut fraction = 0;
    for byte in data {
        if *byte == b'.' && !point && digits != 0 {
            point = true;
        } else if byte.is_ascii_digit() {
            digits += 1;
            fraction += usize::from(point);
        } else {
            return Err(ContentError("invalid MySQL Decimal text"));
        }
    }
    if digits == 0 || digits > 65 || fraction > 30 || (point && fraction == 0) {
        return Err(ContentError("incomplete MySQL Decimal text"));
    }
    Ok(())
}

/// Read BIT bytes directly as their declared unsigned big-endian value.
///
/// # Errors
/// Rejects invalid bit width, field length and nonzero unused high bits.
pub fn bit(data: &[u8], width: u32) -> Result<u64, ContentError> {
    if !(1..=64).contains(&width) || data.len() != width.div_ceil(8) as usize {
        return Err(ContentError(
            "BIT field length disagrees with its declared width",
        ));
    }
    let mut value = 0_u64;
    for byte in data {
        value = (value << 8) | u64::from(*byte);
    }
    if width < 64 && value >> width != 0 {
        return Err(ContentError(
            "BIT field has nonzero bits outside its declared width",
        ));
    }
    Ok(value)
}

fn digits(data: &[u8]) -> Option<u32> {
    if data.is_empty() {
        return None;
    }
    let mut value = 0_u32;
    for byte in data {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

/// Gregorian date representable by Python date, without floating timestamps.
///
/// # Errors
/// Rejects zero/incomplete dates, invalid calendars and malformed text.
pub fn date(data: &[u8]) -> Result<Date, ContentError> {
    let error = ContentError("DATE value is not a representable Gregorian date");
    if data.len() != 10 || data[4] != b'-' || data[7] != b'-' {
        return Err(error);
    }
    let year = digits(&data[..4]).ok_or(error)? as i32;
    let month = digits(&data[5..7]).ok_or(error)? as u8;
    let day = digits(&data[8..]).ok_or(error)? as u8;
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > month_days(year, month) {
        return Err(error);
    }
    Ok(Date { year, month, day })
}

fn fraction(data: &[u8]) -> Option<u32> {
    if data.is_empty() {
        return Some(0);
    }
    if data.first() != Some(&b'.') || !(2..=7).contains(&data.len()) {
        return None;
    }
    Some(digits(&data[1..])? * 10_u32.pow(7 - data.len() as u32))
}

/// Naive DATETIME/TIMESTAMP components exactly as the server sent them.
///
/// # Errors
/// Rejects noncalendar data and fractional seconds outside zero to six digits.
pub fn datetime(data: &[u8]) -> Result<(Date, Clock), ContentError> {
    let error = ContentError("DATETIME/TIMESTAMP value is not a representable date and clock");
    if data.len() < 19 || data[10] != b' ' || data[13] != b':' || data[16] != b':' {
        return Err(error);
    }
    let date = date(&data[..10])?;
    let hour = digits(&data[11..13]).ok_or(error)? as u8;
    let minute = digits(&data[14..16]).ok_or(error)? as u8;
    let second = digits(&data[17..19]).ok_or(error)? as u8;
    let microsecond = fraction(&data[19..]).ok_or(error)?;
    if hour > 23 || minute > 59 || second > 59 {
        return Err(error);
    }
    Ok((
        date,
        Clock {
            hour,
            minute,
            second,
            microsecond,
        },
    ))
}

/// Exact signed TIME microseconds, including negative and cross-day durations.
///
/// # Errors
/// Rejects malformed text, excess fractional digits and MySQL TIME overflow.
pub fn time(data: &[u8]) -> Result<i64, ContentError> {
    let error = ContentError("invalid or out-of-range MySQL TIME value");
    let (negative, data) = if data.first() == Some(&b'-') {
        (true, &data[1..])
    } else {
        (false, data)
    };
    let colon = data.iter().position(|byte| *byte == b':').ok_or(error)?;
    if !(2..=3).contains(&colon) || data.len() < colon + 6 || data[colon + 3] != b':' {
        return Err(error);
    }
    let hours = digits(&data[..colon]).ok_or(error)?;
    let minute = digits(&data[colon + 1..colon + 3]).ok_or(error)?;
    let second = digits(&data[colon + 4..colon + 6]).ok_or(error)?;
    let micro = fraction(&data[colon + 6..]).ok_or(error)?;
    if hours > 838 || minute > 59 || second > 59 {
        return Err(error);
    }
    let value = i64::from((hours * 60 + minute) * 60 + second) * 1_000_000 + i64::from(micro);
    Ok(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_integer_widths_and_received_float_precision_are_preserved() {
        assert_eq!(
            integer(b"-9223372036854775808", false, 64).unwrap(),
            Integer::Signed(i64::MIN)
        );
        assert_eq!(
            integer(b"18446744073709551615", true, 64).unwrap(),
            Integer::Unsigned(u64::MAX)
        );
        assert!(integer(b"128", false, 8).is_err());
        assert!(integer(b"-1", true, 64).is_err());
        assert!(integer(b"18446744073709551616", true, 64).is_err());
        assert_eq!(float(b"0.1").unwrap(), 0.1_f64);
        assert!(float(b"-1e-400").unwrap().is_sign_negative());
        assert!(float(b"1e400").is_err());
        assert!(float(b"NaN").is_err());
        assert!(decimal(b"-0.0000").is_ok());
        assert!(decimal(b"1.2300").is_ok());
        assert!(decimal(b"1e4").is_err());
    }
    #[test]
    fn calendar_fractional_seconds_and_full_time_duration_are_integer_exact() {
        assert!(date(b"2024-02-29").is_ok());
        assert!(date(b"1900-02-29").is_err());
        assert!(date(b"0000-00-00").is_err());
        assert_eq!(
            datetime(b"2026-10-23 12:34:56.1").unwrap().1.microsecond,
            100000
        );
        assert_eq!(
            datetime(b"2026-10-23 12:34:56.000001")
                .unwrap()
                .1
                .microsecond,
            1
        );
        assert!(datetime(b"2026-10-23 12:34:56.1234567").is_err());
        assert_eq!(time(b"-25:00:00").unwrap(), -90_000_000_000);
        assert_eq!(time(b"-00:00:00.000001").unwrap(), -1);
        assert!(time(b"839:00:00").is_err());
        assert_eq!(bit(&[0xff; 8], 64).unwrap(), u64::MAX);
        assert!(bit(&[2], 1).is_err());
    }
}
