//! Exact native scalar formatting into the current owned SQL request.

use super::writer::{append, reserve};
use crate::protocol::framing::WireError;
use num_bigint::BigInt;

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
pub fn integer<T: itoa::Integer>(
    output: &mut Vec<u8>,
    value: T,
    limit: usize,
) -> Result<(), WireError> {
    append(output, itoa::Buffer::new().format(value).as_bytes(), limit)
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
    if value.bits() > 216 {
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
    if !value.is_finite() {
        return append(output, b"NULL", limit);
    }
    let mut buffer = zmij::Buffer::new();
    let text = buffer.format_finite(value).as_bytes();
    let suffix = !text.contains(&b'e') && !text.contains(&b'E');
    reserve(output, text.len() + if suffix { 2 } else { 0 }, limit)?;
    output.extend_from_slice(text);
    if suffix {
        output.extend_from_slice(b"e0");
    }
    Ok(())
}

/// Normalize an exact Decimal coefficient and exponent without binary floating
/// point, Python context operations or potentially enormous zero expansion.
/// `digits` contains its nonnegative coefficient in most-significant order.
///
/// # Errors
/// Returns malformed digits, more than 65 necessary digits, more than 30
/// necessary fractional places, or an actual request-buffer failure.
pub fn decimal(
    output: &mut Vec<u8>,
    negative: bool,
    digits: &[u8],
    exponent: i64,
    limit: usize,
) -> Result<(), ScalarError> {
    if digits.iter().any(|digit| *digit > 9) {
        return Err(ScalarError::Range("invalid Decimal coefficient"));
    }
    let Some(first) = digits.iter().position(|digit| *digit != 0) else {
        append(output, if negative { b"-0" } else { b"0" }, limit)?;
        return Ok(());
    };
    let last = digits
        .iter()
        .rposition(|digit| *digit != 0)
        .ok_or(ScalarError::Range("invalid Decimal coefficient"))?;
    let trailing = i64::try_from(digits.len() - last - 1)
        .map_err(|_| ScalarError::Range("Decimal coefficient is too large"))?;
    let exponent = exponent.checked_add(trailing).ok_or(ScalarError::Range(
        "Decimal exponent exceeds the exact SQL range",
    ))?;
    let coefficient = &digits[first..=last];
    let width = i64::try_from(coefficient.len())
        .map_err(|_| ScalarError::Range("Decimal coefficient is too large"))?;
    let decimal_point = width.checked_add(exponent).ok_or(ScalarError::Range(
        "Decimal exponent exceeds the exact SQL range",
    ))?;
    let integer_places = decimal_point.max(0);
    let fractional_places = exponent.saturating_neg().max(0);
    if fractional_places > 30 || integer_places.saturating_add(fractional_places) > 65 {
        return Err(ScalarError::Range(
            "Decimal exceeds 65 necessary digits or 30 fractional places",
        ));
    }
    let width = integer_places.max(1) as usize
        + fractional_places as usize
        + usize::from(fractional_places > 0)
        + usize::from(negative);
    reserve(output, width, limit)?;
    if negative {
        output.push(b'-');
    }
    if decimal_point <= 0 {
        output.extend_from_slice(b"0.");
        output.resize(output.len() + (-decimal_point) as usize, b'0');
        output.extend(coefficient.iter().map(|digit| b'0' + digit));
    } else {
        for (index, digit) in coefficient.iter().enumerate() {
            if index as i64 == decimal_point {
                output.push(b'.');
            }
            output.push(b'0' + digit);
        }
        if exponent > 0 {
            output.resize(output.len() + exponent as usize, b'0');
        }
    }
    Ok(())
}

/// A native calendar date. Input adapters supply an already valid source date;
/// UTC normalization may produce the adjacent year 0000 or 10000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    /// Proleptic Gregorian year, including the approved derived endpoints.
    pub year: i32,
    /// Calendar month, 1 through 12.
    pub month: u8,
    /// Day valid for this month and year.
    pub day: u8,
}

/// Native clock components read at the language boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    /// Hour, 0 through 23.
    pub hour: u8,
    /// Minute, 0 through 59.
    pub minute: u8,
    /// Second, 0 through 59.
    pub second: u8,
    /// Fractional microsecond, 0 through 999999.
    pub microsecond: u32,
}

const DAY_MICROS: i128 = 86_400_000_000;

impl Date {
    fn shift_one(&mut self, forward: bool) {
        let month_days = |year: i32, month: u8| -> u8 {
            match month {
                4 | 6 | 9 | 11 => 30,
                2 => {
                    if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                        29
                    } else {
                        28
                    }
                }
                _ => 31,
            }
        };
        if forward {
            if self.day < month_days(self.year, self.month) {
                self.day += 1;
            } else {
                self.day = 1;
                if self.month == 12 {
                    self.month = 1;
                    self.year += 1;
                } else {
                    self.month += 1;
                }
            }
        } else if self.day > 1 {
            self.day -= 1;
        } else {
            if self.month == 1 {
                self.month = 12;
                self.year -= 1;
            } else {
                self.month -= 1;
            }
            self.day = month_days(self.year, self.month);
        }
    }
}

impl Clock {
    fn micros(self) -> i128 {
        ((i128::from(self.hour) * 60 + i128::from(self.minute)) * 60 + i128::from(self.second))
            * 1_000_000
            + i128::from(self.microsecond)
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
        date.shift_one(false);
    } else if adjusted >= DAY_MICROS {
        date.shift_one(true);
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

fn padded(
    output: &mut Vec<u8>,
    value: u128,
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
    padded(output, value.year as u128, 4, limit)?;
    append(output, b"-", limit)?;
    padded(output, u128::from(value.month), 2, limit)?;
    append(output, b"-", limit)?;
    padded(output, u128::from(value.day), 2, limit)
}
fn clock_body(
    output: &mut Vec<u8>,
    hours: u128,
    minutes: u128,
    seconds: u128,
    micros: u128,
    limit: usize,
) -> Result<(), WireError> {
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
    append(output, b"'", limit)?;
    date_body(output, date, limit)?;
    if let Some(clock) = clock {
        append(output, b" ", limit)?;
        clock_body(
            output,
            u128::from(clock.hour),
            u128::from(clock.minute),
            u128::from(clock.second),
            u128::from(clock.microsecond),
            limit,
        )?;
    }
    append(output, b"'", limit)
}

/// Write a native clock without consulting or applying any time-zone metadata.
///
/// # Errors
/// Returns a request-buffer error.
pub fn time(output: &mut Vec<u8>, clock: Clock, limit: usize) -> Result<(), WireError> {
    duration(output, clock.micros(), limit)
}

/// Write signed duration microseconds, including the full Python timedelta
/// range, without narrowing into i64 or applying MySQL column-range clipping.
///
/// # Errors
/// Returns a request-buffer error.
pub fn duration(output: &mut Vec<u8>, micros: i128, limit: usize) -> Result<(), WireError> {
    append(output, b"'", limit)?;
    if micros < 0 {
        append(output, b"-", limit)?;
    }
    let magnitude = micros.unsigned_abs();
    clock_body(
        output,
        magnitude / 3_600_000_000,
        magnitude / 60_000_000 % 60,
        magnitude / 1_000_000 % 60,
        magnitude % 1_000_000,
        limit,
    )?;
    append(output, b"'", limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_normalization_enforces_exact_precision_without_expansion() {
        for (digits, exp, expected) in [
            (&[1, 2, 3, 4, 5, 6, 0, 0][..], -4, "1234.56"),
            (&[1][..], -3, "0.001"),
            (&[1][..], 3, "1000"),
            (&[0][..], i64::MAX, "0"),
        ] {
            let mut out = Vec::new();
            decimal(&mut out, false, digits, exp, 1024).unwrap();
            assert_eq!(out, expected.as_bytes());
        }
        assert!(decimal(&mut Vec::new(), false, &[1], -31, 1024).is_err());
        assert!(decimal(&mut Vec::new(), false, &[1], 65, 1024).is_err());
        assert!(decimal(&mut Vec::new(), false, &[1], i64::MIN, 1024).is_err());
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
