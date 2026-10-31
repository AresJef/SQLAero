//! Exact temporal tick decomposition without Python objects or floating point.

use super::{scalar::ScalarError, writer};
use std::fmt::{self, Write};

/// Original source unit; calendar months and years remain distinct from days.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeUnit {
    /// Gregorian calendar year, or the approved mean duration year.
    Year,
    /// Gregorian calendar month, or the approved mean duration month.
    Month,
    /// Seven elapsed days.
    Week,
    /// A day of 86,400 elapsed seconds.
    Day,
    /// An elapsed hour.
    Hour,
    /// An elapsed minute.
    Minute,
    /// A second.
    Second,
    /// One thousandth of a second.
    Millisecond,
    /// One millionth of a second.
    Microsecond,
    /// One billionth of a second.
    Nanosecond,
    /// Recognized but not supported for a finite input in this scope.
    Picosecond,
    /// Recognized but not supported for a finite input in this scope.
    Femtosecond,
    /// Recognized but not supported for a finite input in this scope.
    Attosecond,
    /// Unit-less temporal storage; only a source-defined null may use it.
    Generic,
}

/// The value semantics of temporal ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeKind {
    /// Ticks relative to the Unix calendar epoch.
    DateTime,
    /// A signed elapsed duration, including hours beyond one day.
    Duration,
}

/// A bounded stack formatter; all supported i128 dates fit well below 128 bytes.
struct Stamp {
    bytes: [u8; 128],
    length: usize,
}

impl Stamp {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            length: 0,
        }
    }

    fn add(&mut self, args: fmt::Arguments<'_>) -> Result<(), ScalarError> {
        self.write_fmt(args)
            .map_err(|_| ScalarError::Range("temporal text exceeds its native formatting bound"))
    }

    fn finish(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError> {
        writer::append(output, &self.bytes[..self.length], limit).map_err(Into::into)
    }
}

impl Write for Stamp {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        let target = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}

fn range() -> ScalarError {
    ScalarError::Range("temporal ticks or unit multiplier exceed the native exact range")
}

fn scaled(ticks: i64, multiplier: u64) -> Result<i128, ScalarError> {
    if multiplier == 0 {
        return Err(ScalarError::Range(
            "temporal unit multiplier must be positive",
        ));
    }
    i128::from(ticks)
        .checked_mul(i128::from(multiplier))
        .ok_or_else(range)
}

/// Split an exact fixed-unit value into mathematical floor seconds and fraction.
fn seconds(value: i128, unit: TimeUnit) -> Result<(i128, u32, usize), ScalarError> {
    let factor = match unit {
        TimeUnit::Week => 604_800,
        TimeUnit::Day => 86_400,
        TimeUnit::Hour => 3_600,
        TimeUnit::Minute => 60,
        TimeUnit::Second => 1,
        TimeUnit::Millisecond => {
            return Ok((value.div_euclid(1_000), value.rem_euclid(1_000) as u32, 3));
        }
        TimeUnit::Microsecond => {
            return Ok((
                value.div_euclid(1_000_000),
                value.rem_euclid(1_000_000) as u32,
                6,
            ));
        }
        TimeUnit::Nanosecond => {
            return Ok((
                value.div_euclid(1_000_000_000),
                value.rem_euclid(1_000_000_000) as u32,
                9,
            ));
        }
        _ => return Err(ScalarError::Range("finite temporal unit is not supported")),
    };
    Ok((value.checked_mul(factor).ok_or_else(range)?, 0, 0))
}

fn civil(days: i128) -> Result<(i128, u8, u8), ScalarError> {
    // Gregorian eras use Euclidean division, including dates before year zero.
    let z = days.checked_add(719_468).ok_or_else(range)?;
    let era = z.div_euclid(146_097);
    let day = z.rem_euclid(146_097);
    let year_in_era = (day - day / 1_460 + day / 36_524 - day / 146_096) / 365;
    let mut year = year_in_era + era * 400;
    let day_in_year = day - (365 * year_in_era + year_in_era / 4 - year_in_era / 100);
    let month_from_march = (5 * day_in_year + 2) / 153;
    let date = day_in_year - (153 * month_from_march + 2) / 5 + 1;
    let month = month_from_march + if month_from_march < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    Ok((year, month as u8, date as u8))
}

fn date(stamp: &mut Stamp, year: i128, month: u8, day: u8) -> Result<(), ScalarError> {
    if year < 0 {
        stamp.add(format_args!("-{:04}", year.unsigned_abs()))?;
    } else {
        stamp.add(format_args!("{year:04}"))?;
    }
    stamp.add(format_args!("-{month:02}-{day:02}"))
}

fn fraction(stamp: &mut Stamp, mut value: u32, mut precision: usize) -> Result<(), ScalarError> {
    if value == 0 {
        return Ok(());
    }
    while precision != 0 && value.is_multiple_of(10) {
        value /= 10;
        precision -= 1;
    }
    stamp.add(format_args!(".{value:0precision$}"))
}

/// Write an epoch date/time or elapsed duration, preserving necessary nanoseconds.
/// Null sentinels and timezone/session policy belong to the source adapter.
///
/// # Errors
/// Returns unsupported units, checked arithmetic overflow or request failure.
pub fn write(
    output: &mut Vec<u8>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
    limit: usize,
) -> Result<(), ScalarError> {
    let value = scaled(ticks, multiplier)?;
    let mut stamp = Stamp::new();
    stamp.add(format_args!("'"))?;
    match kind {
        TimeKind::DateTime => {
            let (year, month, day, second, part, precision) = match unit {
                TimeUnit::Year => (
                    1970i128.checked_add(value).ok_or_else(range)?,
                    1,
                    1,
                    0,
                    0,
                    0,
                ),
                TimeUnit::Month => {
                    let month = (1970i128 * 12).checked_add(value).ok_or_else(range)?;
                    (
                        month.div_euclid(12),
                        month.rem_euclid(12) as u8 + 1,
                        1,
                        0,
                        0,
                        0,
                    )
                }
                _ => {
                    let (seconds, part, precision) = seconds(value, unit)?;
                    let (year, month, day) = civil(seconds.div_euclid(86_400))?;
                    (
                        year,
                        month,
                        day,
                        seconds.rem_euclid(86_400),
                        part,
                        precision,
                    )
                }
            };
            date(&mut stamp, year, month, day)?;
            stamp.add(format_args!(
                " {:02}:{:02}:{:02}",
                second / 3_600,
                second / 60 % 60,
                second % 60,
            ))?;
            fraction(&mut stamp, part, precision)?;
        }
        TimeKind::Duration => {
            let negative = value < 0;
            let absolute = value.checked_abs().ok_or_else(range)?;
            let (second, part, precision) = match unit {
                TimeUnit::Year => (absolute.checked_mul(31_556_952).ok_or_else(range)?, 0, 0),
                TimeUnit::Month => (absolute.checked_mul(2_629_746).ok_or_else(range)?, 0, 0),
                _ => seconds(absolute, unit)?,
            };
            if negative {
                stamp.add(format_args!("-"))?;
            }
            stamp.add(format_args!(
                "{:02}:{:02}:{:02}",
                second / 3_600,
                second / 60 % 60,
                second % 60,
            ))?;
            fraction(&mut stamp, part, precision)?;
        }
    }
    stamp.add(format_args!("'"))?;
    stamp.finish(output, limit)
}

/// Write an Arrow/Polars clock value, which must lie within a single day.
///
/// # Errors
/// Returns an invalid clock, unsupported unit, overflow or request failure.
pub fn write_clock(
    output: &mut Vec<u8>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    limit: usize,
) -> Result<(), ScalarError> {
    let (second, part, precision) = seconds(scaled(ticks, multiplier)?, unit)?;
    if !(0..86_400).contains(&second) {
        return Err(ScalarError::Range(
            "clock value must be within one nonnegative day",
        ));
    }
    let mut stamp = Stamp::new();
    stamp.add(format_args!(
        "'{:02}:{:02}:{:02}",
        second / 3_600,
        second / 60 % 60,
        second % 60
    ))?;
    fraction(&mut stamp, part, precision)?;
    stamp.add(format_args!("'"))?;
    stamp.finish(output, limit)
}

/// Write a date expressed as Unix-epoch days, without narrowing its year.
///
/// # Errors
/// Returns checked calendar overflow or a request-buffer failure.
pub fn write_date(output: &mut Vec<u8>, days: i64, limit: usize) -> Result<(), ScalarError> {
    let (year, month, day) = civil(i128::from(days))?;
    let mut stamp = Stamp::new();
    stamp.add(format_args!("'"))?;
    date(&mut stamp, year, month, day)?;
    stamp.add(format_args!("'"))?;
    stamp.finish(output, limit)
}

/// Write six nonnegative integer calendar components without imposing a target
/// MySQL calendar or clock range. Input adapters already applied their integer
/// protocol; this layer only checks digits, capacity and SQL framing.
///
/// # Errors
/// Rejects malformed component digits or an actual request-buffer failure.
pub fn write_components(
    output: &mut Vec<u8>,
    parts: [&[u8]; 6],
    limit: usize,
) -> Result<(), ScalarError> {
    if parts
        .iter()
        .any(|value| value.is_empty() || !value.iter().all(u8::is_ascii_digit))
    {
        return Err(ScalarError::Range(
            "calendar components must contain nonnegative integer digits",
        ));
    }
    let size = parts
        .iter()
        .enumerate()
        .try_fold(7usize, |size, (index, value)| {
            size.checked_add(value.len().max(if index == 0 { 4 } else { 2 }))
                .ok_or_else(range)
        })?;
    writer::reserve(output, size, limit)?;
    output.push(b'\'');
    for (index, value) in parts.into_iter().enumerate() {
        if index != 0 {
            output.push(match index {
                1 | 2 => b'-',
                3 => b' ',
                _ => b':',
            });
        }
        let padding = (if index == 0 { 4usize } else { 2usize }).saturating_sub(value.len());
        output.resize(output.len() + padding, b'0');
        output.extend_from_slice(value);
    }
    output.push(b'\'');
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(ticks: i64, unit: TimeUnit, multiplier: u64, kind: TimeKind) -> String {
        let mut out = Vec::new();
        write(&mut out, ticks, unit, multiplier, kind, 512).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn calendar_component_writer_preserves_server_owned_validation_and_atomic_bounds() {
        let parts = [b"2024".as_slice(), b"13", b"31", b"25", b"59", b"61"];
        let mut out = Vec::new();
        write_components(&mut out, parts, 1024).unwrap();
        assert_eq!(out, b"'2024-13-31 25:59:61'");
        let mut prefix = b"x".to_vec();
        assert!(write_components(&mut prefix, parts, 21).is_err());
        assert_eq!(prefix, b"x");
        let mut malformed = parts;
        malformed[1] = b"1'";
        assert!(write_components(&mut prefix, malformed, 1024).is_err());
        assert_eq!(prefix, b"x");
    }

    #[test]
    fn negative_epoch_fraction_uses_floor_calendar_and_keeps_nanoseconds() {
        assert_eq!(
            value(-1, TimeUnit::Nanosecond, 1, TimeKind::DateTime),
            "'1969-12-31 23:59:59.999999999'"
        );
        assert_eq!(
            value(-1, TimeUnit::Nanosecond, 1, TimeKind::Duration),
            "'-00:00:00.000000001'"
        );
    }

    #[test]
    fn dtype_multiplier_and_calendar_months_are_not_lost() {
        assert_eq!(
            value(1, TimeUnit::Day, 2, TimeKind::DateTime),
            "'1970-01-03 00:00:00'"
        );
        assert_eq!(
            value(-1, TimeUnit::Month, 1, TimeKind::DateTime),
            "'1969-12-01 00:00:00'"
        );
        assert_eq!(
            value(-1971, TimeUnit::Year, 1, TimeKind::DateTime),
            "'-0001-01-01 00:00:00'"
        );
    }

    #[test]
    fn calendar_duration_uses_the_explicit_mean_not_an_anchor_date() {
        assert_eq!(
            value(1, TimeUnit::Year, 1, TimeKind::Duration),
            "'8765:49:12'"
        );
        assert_eq!(
            value(1, TimeUnit::Month, 1, TimeKind::Duration),
            "'730:29:06'"
        );
    }

    #[test]
    fn valid_minimum_ticks_are_not_a_generic_null_sentinel() {
        assert_eq!(
            value(i64::MIN, TimeUnit::Nanosecond, 1, TimeKind::Duration),
            "'-2562047:47:16.854775808'"
        );
        let mut out = Vec::new();
        assert!(write_clock(&mut out, -1, TimeUnit::Nanosecond, 1, 100).is_err());
        assert!(write_clock(&mut out, 86_400, TimeUnit::Second, 1, 100).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn formatting_is_checked_before_the_request_is_modified() {
        let mut out = b"prefix".to_vec();
        assert!(write(&mut out, 0, TimeUnit::Second, 1, TimeKind::DateTime, 10).is_err());
        assert_eq!(out, b"prefix");
        assert!(
            write(
                &mut out,
                i64::MAX,
                TimeUnit::Year,
                u64::MAX,
                TimeKind::Duration,
                512
            )
            .is_err()
        );
    }
}
