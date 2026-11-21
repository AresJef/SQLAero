//! Exact native scalar formatting into the current owned SQL request.

use super::writer::{append, reserve};
use crate::protocol::framing::WireError;
use num_bigint::BigInt;
use sqlaero_transcode::decimal::{Digits, Render};
use sqlaero_transcode::number;

/// Necessary bit bound before formatting a signed value with at most 65 digits.
/// Values at this bit width still undergo the exact decimal-length check.
pub const MAX_INTEGER_BITS: u64 = 216;

/// A scalar cannot be represented under the approved SQL value contract.
#[derive(Debug, thiserror::Error)]
pub enum ScalarError {
    /// An accurate finite value exceeds the exact-number policy.
    #[error("{0}")]
    Range(&'static str),
    /// The request limit or native allocation failed.
    #[error(transparent)]
    Wire(#[from] WireError),
}

/// Write a compact integer without a temporary heap or Python string.
///
/// # Errors
/// Returns the actual request limit or reservation failure.
#[inline(always)]
pub fn integer<T: number::Integer>(
    output: &mut Vec<u8>,
    value: T,
    limit: usize,
) -> Result<(), WireError> {
    append(
        output,
        number::IntegerBuffer::new().format(value).as_bytes(),
        limit,
    )
}

/// Write a large exact integer, rejecting more than 65 decimal digits before
/// any value is added to the request. The initial bit bound caps formatting work.
///
/// # Errors
/// Returns a numeric-policy or request-buffer error.
pub fn large_integer(
    output: &mut Vec<u8>,
    value: &BigInt,
    limit: usize,
) -> Result<(), ScalarError> {
    if value.bits() > MAX_INTEGER_BITS {
        return Err(ScalarError::Range(
            "integer exceeds the 65-digit exact SQL range",
        ));
    }
    let text = value.to_str_radix(10);
    if text.trim_start_matches('-').len() > 65 {
        return Err(ScalarError::Range(
            "integer exceeds the 65-digit exact SQL range",
        ));
    }
    append(output, text.as_bytes(), limit)?;
    Ok(())
}

/// Write binary64 through the already selected native shortest formatter.
/// Nonfinite inputs are NULL; exponent-free text gains e0 to retain MySQL's
/// approximate-number category. The formatter preserves the sign of zero.
///
/// # Errors
/// Returns a request-buffer error.
pub fn float(output: &mut Vec<u8>, value: f64, limit: usize) -> Result<(), WireError> {
    let mut buffer = number::FloatBuffer::new();
    let Some(text) = buffer.format(value) else {
        return append(output, b"NULL", limit);
    };
    let text = text.as_bytes();
    let suffix = memchr::memchr2(b'e', b'E', text).is_none();
    reserve(output, text.len() + if suffix { 2 } else { 0 }, limit)?;
    output.extend_from_slice(text);
    if suffix {
        output.extend_from_slice(b"e0");
    }
    Ok(())
}

/// Render an exact Decimal coefficient and exponent, preserving source scale.
/// Necessary precision and actual request capacity remain independent limits.
/// `digits` contains its nonnegative coefficient in most-significant order.
///
/// # Errors
/// Returns malformed digits, more than 65 necessary decimal places, or an
/// actual request-buffer failure. Target-column scale is a server decision.
pub fn decimal(
    output: &mut Vec<u8>,
    negative: bool,
    digits: Digits<'_>,
    exponent: i64,
    limit: usize,
) -> Result<(), ScalarError> {
    let header = digits
        .inspect(negative, exponent)
        .map_err(ScalarError::Range)?;
    let layout = decimal_layout(header)?;
    let mut render = Render::new(digits, layout).map_err(ScalarError::Range)?;
    // Native provider coefficients are bounded; long Python coefficients use
    // this same plan through the fragmented Consumer contract at the boundary.
    let mut chunk = [0; 128];
    loop {
        let length = render.read(&mut chunk);
        if length == 0 {
            break;
        }
        append(output, &chunk[..length], limit)?;
    }
    Ok(())
}

/// Apply only MySQL's necessary-value precision policy, retaining source scale.
/// # Errors
/// Rejects nonfinite misuse, excessive necessary precision or output overflow.
pub fn decimal_layout(
    header: sqlaero_transcode::decimal::Header,
) -> Result<sqlaero_transcode::decimal::Layout, ScalarError> {
    use sqlaero_transcode::decimal::{Layout, Notation};
    if !header.is_zero() && header.category().is_none() {
        let point = header.digits() as i128 + i128::from(header.exponent());
        let fraction = (-i128::from(header.exponent()) - header.trailing() as i128).max(0);
        if point.max(0) + fraction > 65 {
            return Err(ScalarError::Range(
                "Decimal exceeds 65 necessary decimal places",
            ));
        }
    }
    Layout::new(header, Notation::Plain).map_err(ScalarError::Range)
}

/// Format-neutral calendar and native-resolution clock facts.
pub use sqlaero_transcode::temporal::{Clock, Date};

const DAY_MICROS: i128 = 86_400_000_000;

pub(crate) fn month_days(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

fn shift_one(date: &mut Date, forward: bool) {
    if forward {
        if date.day < month_days(date.year, date.month) {
            date.day += 1;
        } else {
            date.day = 1;
            if date.month == 12 {
                date.month = 1;
                date.year += 1;
            } else {
                date.month += 1;
            }
        }
    } else if date.day > 1 {
        date.day -= 1;
    } else {
        if date.month == 1 {
            date.month = 12;
            date.year -= 1;
        } else {
            date.month -= 1;
        }
        date.day = month_days(date.year, date.month);
    }
}

/// Apply a validated offset shorter than a day using exact integer arithmetic.
/// Aware-input admission and the real tzinfo protocol remain at the boundary.
///
/// # Errors
/// Returns a malformed offset failure; no floating timestamp is used.
pub fn utc(
    mut date: Date,
    clock: Clock,
    offset_micros: i128,
) -> Result<(Date, Clock), ScalarError> {
    if offset_micros.unsigned_abs() >= DAY_MICROS as u128 {
        return Err(ScalarError::Range(
            "utcoffset must be strictly between -24h and +24h",
        ));
    }
    let adjusted = clock.micros() - offset_micros;
    if adjusted < 0 {
        shift_one(&mut date, false);
    } else if adjusted >= DAY_MICROS {
        shift_one(&mut date, true);
    }
    let value = adjusted.rem_euclid(DAY_MICROS);
    Ok((
        date,
        Clock {
            hour: (value / 3_600_000_000) as u8,
            minute: (value / 60_000_000 % 60) as u8,
            second: (value / 1_000_000 % 60) as u8,
            microsecond: (value % 1_000_000) as u32,
        },
    ))
}

fn padded<T: itoa::Integer>(
    output: &mut Vec<u8>,
    value: T,
    minimum: usize,
    limit: usize,
) -> Result<(), WireError> {
    let mut buffer = itoa::Buffer::new();
    let text = buffer.format(value).as_bytes();
    let zeroes = minimum.saturating_sub(text.len());
    reserve(output, zeroes + text.len(), limit)?;
    output.resize(output.len() + zeroes, b'0');
    output.extend_from_slice(text);
    Ok(())
}

fn date_body(output: &mut Vec<u8>, value: Date, limit: usize) -> Result<(), WireError> {
    padded(output, value.year, 4, limit)?;
    append(output, b"-", limit)?;
    padded(output, value.month, 2, limit)?;
    append(output, b"-", limit)?;
    padded(output, value.day, 2, limit)
}

/// The caller has established that the value fits in two decimal digits.
fn digit_pair(value: u8) -> [u8; 2] {
    [b'0' + value / 10, b'0' + value % 10]
}

/// Format the common four-digit nonnegative date without target delimiters.
pub(super) fn fixed_calendar(year: i128, month: u8, day: u8) -> Option<[u8; 10]> {
    let year = u16::try_from(year).ok()?;
    if year >= 10_000 || month >= 100 || day >= 100 {
        return None;
    }
    let [year_thousands, year_hundreds] = digit_pair((year / 100) as u8);
    let [year_tens, year_ones] = digit_pair((year % 100) as u8);
    let [month_tens, month_ones] = digit_pair(month);
    let [day_tens, day_ones] = digit_pair(day);
    Some([
        year_thousands,
        year_hundreds,
        year_tens,
        year_ones,
        b'-',
        month_tens,
        month_ones,
        b'-',
        day_tens,
        day_ones,
    ])
}

/// Format a two-digit clock with its optional six-digit microsecond field.
pub(super) fn fixed_clock(
    hours: u128,
    minutes: u8,
    seconds: u8,
    micros: u32,
) -> Option<([u8; 15], usize)> {
    if hours >= 100 || minutes >= 100 || seconds >= 100 || micros >= 1_000_000 {
        return None;
    }
    let mut text = *b"00:00:00.000000";
    text[..2].copy_from_slice(&digit_pair(hours as u8));
    text[3..5].copy_from_slice(&digit_pair(minutes));
    text[6..8].copy_from_slice(&digit_pair(seconds));
    let width = if micros == 0 {
        8
    } else {
        text[9..11].copy_from_slice(&digit_pair((micros / 10_000) as u8));
        text[11..13].copy_from_slice(&digit_pair((micros / 100 % 100) as u8));
        text[13..15].copy_from_slice(&digit_pair((micros % 100) as u8));
        15
    };
    Some((text, width))
}

fn clock_body(
    output: &mut Vec<u8>,
    hours: u128,
    minutes: u8,
    seconds: u8,
    micros: u32,
    limit: usize,
) -> Result<(), WireError> {
    if let Some((text, width)) = fixed_clock(hours, minutes, seconds, micros)
        && limit.saturating_sub(output.len()) >= width
    {
        return append(output, &text[..width], limit);
    }
    // Wide durations and close request limits still use the general writer.
    padded(output, hours, 2, limit)?;
    append(output, b":", limit)?;
    padded(output, minutes, 2, limit)?;
    append(output, b":", limit)?;
    padded(output, seconds, 2, limit)?;
    if micros != 0 {
        append(output, b".", limit)?;
        padded(output, micros, 6, limit)?;
    }
    Ok(())
}

/// Write a date, or a datetime when clock components are present.
///
/// # Errors
/// Returns a request-buffer error. The enclosing failed request is never sent.
pub fn date_time(
    output: &mut Vec<u8>,
    date: Date,
    clock: Option<Clock>,
    limit: usize,
) -> Result<(), WireError> {
    let delimiter = if clock.is_some() { b' ' } else { b'\'' };
    let mut prefix = *b"'0000-00-00'";
    if limit.saturating_sub(output.len()) >= prefix.len()
        && let Some(calendar) = fixed_calendar(i128::from(date.year), date.month, date.day)
    {
        prefix[1..11].copy_from_slice(&calendar);
        prefix[11] = delimiter;
        append(output, &prefix, limit)?;
    } else {
        // Keep exact wide components and the original failing boundary when
        // the request has fewer bytes available than the fixed prefix.
        append(output, b"'", limit)?;
        date_body(output, date, limit)?;
        append(output, &[delimiter], limit)?;
    }
    if let Some(clock) = clock {
        clock_body(
            output,
            u128::from(clock.hour),
            clock.minute,
            clock.second,
            clock.microsecond,
            limit,
        )?;
        append(output, b"'", limit)?;
    }
    Ok(())
}

/// Write a native clock without consulting or applying any time-zone metadata.
///
/// # Errors
/// Returns a request-buffer error.
pub fn time(output: &mut Vec<u8>, clock: Clock, limit: usize) -> Result<(), WireError> {
    if clock.minute >= 60 || clock.second >= 60 || clock.microsecond >= 1_000_000 {
        return duration(output, clock.micros(), limit);
    }
    clock_literal(
        output,
        false,
        u128::from(clock.hour),
        clock.minute,
        clock.second,
        clock.microsecond,
        limit,
    )
}

/// Write signed duration microseconds, including the full Python timedelta
/// range, without narrowing into i64 or applying MySQL column-range clipping.
///
/// # Errors
/// Returns a request-buffer error.
pub fn duration(output: &mut Vec<u8>, micros: i128, limit: usize) -> Result<(), WireError> {
    const HOUR_MICROS: u128 = 3_600_000_000;
    const SECOND_MICROS: u32 = 1_000_000;
    let magnitude = micros.unsigned_abs();
    // One wide quotient and remainder. The remaining hour fits in u32.
    let within_hour = (magnitude % HOUR_MICROS) as u32;
    let whole_seconds = within_hour / SECOND_MICROS;
    clock_literal(
        output,
        micros < 0,
        magnitude / HOUR_MICROS,
        (whole_seconds / 60) as u8,
        (whole_seconds % 60) as u8,
        within_hour % SECOND_MICROS,
        limit,
    )
}

fn clock_literal(
    output: &mut Vec<u8>,
    negative: bool,
    hours: u128,
    minutes: u8,
    seconds: u8,
    micros: u32,
    limit: usize,
) -> Result<(), WireError> {
    append(output, b"'", limit)?;
    if negative {
        append(output, b"-", limit)?;
    }
    clock_body(output, hours, minutes, seconds, micros, limit)?;
    append(output, b"'", limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_block_preserves_fraction_wide_hours_and_request_limit_diagnostics() {
        for (micros, expected) in [
            (-99 * 3_600_000_000 - 1, "'-99:00:00.000001'"),
            (100 * 3_600_000_000, "'100:00:00'"),
            (1_234_567, "'00:00:01.234567'"),
        ] {
            let mut output = Vec::new();
            duration(&mut output, micros, expected.len()).unwrap();
            assert_eq!(output, expected.as_bytes());
        }
        let mut limited = Vec::new();
        assert!(matches!(
            duration(&mut limited, 1_234_567, 13),
            Err(WireError::Limit {
                required: 16,
                limit: 13
            })
        ));
        assert_eq!(limited, b"'00:00:01.");
    }

    #[test]
    fn calendar_prefix_preserves_wide_years_and_request_limit_diagnostics() {
        let date = Date {
            year: 0,
            month: 1,
            day: 9,
        };
        let mut exact = b"x".to_vec();
        date_time(&mut exact, date, None, 13).unwrap();
        assert_eq!(exact, b"x'0000-01-09'");

        let mut limited = b"x".to_vec();
        assert!(matches!(
            date_time(&mut limited, date, None, 12),
            Err(WireError::Limit {
                required: 13,
                limit: 12
            })
        ));
        assert_eq!(limited, b"x'0000-01-09");

        let mut wide = Vec::new();
        date_time(
            &mut wide,
            Date {
                year: 10_000,
                ..date
            },
            None,
            13,
        )
        .unwrap();
        assert_eq!(wide, b"'10000-01-09'");
    }

    #[test]
    fn temporal_minimum_width_keeps_values_wider_than_two_digits() {
        for value in [0_u8, 9, 10, 59, 99, 100, 255] {
            let mut output = Vec::new();
            date_time(
                &mut output,
                Date {
                    year: 2024,
                    month: 1,
                    day: 1,
                },
                Some(Clock {
                    hour: value,
                    minute: value,
                    second: value,
                    microsecond: 1,
                }),
                128,
            )
            .unwrap();
            let expected = format!("'2024-01-01 {value:02}:{value:02}:{value:02}.000001'");
            assert_eq!(output, expected.as_bytes());
        }
    }

    #[test]
    fn decimal_scale_and_necessary_value_precision_are_independent() {
        for (digits, exp, expected) in [
            (&[1, 2, 3, 4, 5, 6, 0, 0][..], -4, "1234.5600"),
            (&[1][..], -3, "0.001"),
            (&[1][..], 3, "1000"),
            (&[0][..], i64::MAX, "0"),
        ] {
            let mut out = Vec::new();
            decimal(&mut out, false, Digits::Values(digits), exp, 1024).unwrap();
            assert_eq!(out, expected.as_bytes());
        }
        let mut high_scale = Vec::new();
        decimal(&mut high_scale, false, Digits::Values(&[1]), -65, 1024).unwrap();
        assert_eq!(high_scale, format!("0.{}1", "0".repeat(64)).as_bytes());
        assert!(decimal(&mut Vec::new(), false, Digits::Values(&[1]), -66, 1024).is_err());
        assert!(decimal(&mut Vec::new(), false, Digits::Values(&[1]), 65, 1024).is_err());
        assert!(decimal(&mut Vec::new(), false, Digits::Values(&[1]), i64::MIN, 1024).is_err());
    }

    #[test]
    fn utc_arithmetic_keeps_fractional_offsets_and_calendar_endpoints() {
        let date = Date {
            year: 1,
            month: 1,
            day: 1,
        };
        let clock = Clock {
            hour: 0,
            minute: 30,
            second: 0,
            microsecond: 1,
        };
        let (date, clock) = utc(date, clock, 3_600_000_001).unwrap();
        let mut out = Vec::new();
        date_time(&mut out, date, Some(clock), 100).unwrap();
        assert_eq!(out, b"'0000-12-31 23:30:00'");
        let mut out = Vec::new();
        duration(&mut out, -999_999_999i128 * DAY_MICROS + 1, 100).unwrap();
        assert_eq!(out, b"'-23999999975:59:59.999999'");
    }
}
