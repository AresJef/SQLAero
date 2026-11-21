//! Exact temporal tick decomposition without Python objects or floating point.

use super::{
    scalar::{ScalarError, fixed_calendar, fixed_clock},
    writer,
};
pub use sqlaero_transcode::temporal::{TimeKind, TimeUnit};

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

    fn append(&mut self, text: &[u8]) -> Result<(), ScalarError> {
        let end = self
            .length
            .checked_add(text.len())
            .ok_or_else(stamp_bound)?;
        let target = self
            .bytes
            .get_mut(self.length..end)
            .ok_or_else(stamp_bound)?;
        target.copy_from_slice(text);
        self.length = end;
        Ok(())
    }

    /// Write ASCII digits directly; padding never allocates or uses fmt.
    fn padded<T: itoa::Integer>(&mut self, value: T, minimum: usize) -> Result<(), ScalarError> {
        let mut buffer = itoa::Buffer::new();
        let text = buffer.format(value).as_bytes();
        let zeroes = minimum.saturating_sub(text.len());
        let end = self
            .length
            .checked_add(zeroes)
            .and_then(|n| n.checked_add(text.len()))
            .ok_or_else(stamp_bound)?;
        let target = self
            .bytes
            .get_mut(self.length..end)
            .ok_or_else(stamp_bound)?;
        target[..zeroes].fill(b'0');
        target[zeroes..].copy_from_slice(text);
        self.length = end;
        Ok(())
    }

    fn finish(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError> {
        writer::append(output, &self.bytes[..self.length], limit).map_err(Into::into)
    }
}

fn stamp_bound() -> ScalarError {
    ScalarError::Range("temporal text exceeds its native formatting bound")
}

fn range() -> ScalarError {
    ScalarError::Range("temporal ticks or unit multiplier exceed the native exact range")
}

fn date(stamp: &mut Stamp, year: i128, month: u8, day: u8) -> Result<(), ScalarError> {
    if let Some(text) = fixed_calendar(year, month, day) {
        return stamp.append(&text);
    }
    if year < 0 {
        stamp.append(b"-")?;
    }
    stamp.padded(year.unsigned_abs(), 4)?;
    stamp.append(b"-")?;
    stamp.padded(month, 2)?;
    stamp.append(b"-")?;
    stamp.padded(day, 2)
}

/// Callers supply a nonnegative, already decomposed second count.
fn clock(stamp: &mut Stamp, second: u128, micros: u32) -> Result<(), ScalarError> {
    let hours = second / 3_600;
    let minutes = (second / 60 % 60) as u8;
    let seconds = (second % 60) as u8;
    if let Some((text, width)) = fixed_clock(hours, minutes, seconds, micros) {
        return stamp.append(&text[..width]);
    }
    stamp.padded(hours, 2)?;
    stamp.append(b":")?;
    stamp.padded(minutes, 2)?;
    stamp.append(b":")?;
    stamp.padded(seconds, 2)?;
    if micros != 0 {
        stamp.append(b".")?;
        stamp.padded(micros, 6)?;
    }
    Ok(())
}

pub use sqlaero_transcode::temporal::Value as Temporal;

/// The MySQL target's exact rendering operations on neutral temporal facts.
pub trait MysqlTemporal {
    /// Write a complete quoted SQL literal after microsecond truncation.
    /// # Errors
    /// Returns checked conversion, formatting or capacity failure.
    fn write_literal(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError>;
    /// Write a character parameter after the same microsecond policy.
    /// # Errors
    /// Returns checked conversion, formatting or capacity failure.
    fn write_text(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError>;
}
impl MysqlTemporal for Temporal {
    fn write_literal(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError> {
        stamp(self, true)?.finish(output, limit)
    }
    fn write_text(self, output: &mut Vec<u8>, limit: usize) -> Result<(), ScalarError> {
        stamp(self, false)?.finish(output, limit)
    }
}

/// Apply only MySQL's approved microsecond and calendar-average policies.
/// # Errors
/// Rejects invalid native fractions and checked calendar-period overflow.
pub fn mysql_precision(value: Temporal) -> Result<Temporal, ScalarError> {
    Ok(match value {
        Temporal::Period {
            negative,
            count,
            unit,
        } => {
            let factor = match unit {
                TimeUnit::Year => 31_556_952,
                TimeUnit::Month => 2_629_746,
                _ => return Err(range()),
            };
            let second = count
                .checked_mul(factor)
                .filter(|value| *value <= i128::MAX as u128)
                .ok_or_else(range)?;
            Temporal::Duration {
                negative: negative && second != 0,
                second,
                nanosecond: 0,
            }
        }
        Temporal::Datetime {
            year,
            month,
            day,
            second,
            nanosecond,
        } => {
            if nanosecond >= 1_000_000_000 {
                return Err(range());
            }
            Temporal::Datetime {
                year,
                month,
                day,
                second,
                nanosecond: nanosecond / 1000 * 1000,
            }
        }
        Temporal::Clock {
            negative,
            second,
            nanosecond,
        } => {
            if nanosecond >= 1_000_000_000 {
                return Err(range());
            }
            let nanosecond = nanosecond / 1000 * 1000;
            Temporal::Clock {
                negative: negative && (second != 0 || nanosecond != 0),
                second,
                nanosecond,
            }
        }
        Temporal::Duration {
            negative,
            second,
            nanosecond,
        } => {
            if nanosecond >= 1_000_000_000 {
                return Err(range());
            }
            let nanosecond = nanosecond / 1000 * 1000;
            Temporal::Duration {
                negative: negative && (second != 0 || nanosecond != 0),
                second,
                nanosecond,
            }
        }
        value => value,
    })
}

fn source_error(error: sqlaero_transcode::temporal::Error) -> ScalarError {
    match error {
        sqlaero_transcode::temporal::Error::Range(message) => ScalarError::Range(message),
    }
}
/// Decompose full source ticks without applying MySQL precision or spelling.
/// # Errors
/// Returns unsupported metadata or checked source arithmetic overflow.
pub fn decompose(
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
) -> Result<Temporal, ScalarError> {
    sqlaero_transcode::temporal::decompose(ticks, unit, multiplier, kind).map_err(source_error)
}
/// Decompose full signed clock ticks without narrowing their total hours.
/// # Errors
/// Returns unsupported metadata or checked source arithmetic overflow.
pub fn decompose_clock(
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
) -> Result<Temporal, ScalarError> {
    sqlaero_transcode::temporal::decompose_clock(ticks, unit, multiplier).map_err(source_error)
}
/// Decompose full Gregorian epoch days without narrowing the year.
/// # Errors
/// Returns checked source arithmetic overflow.
pub fn decompose_date(days: i64) -> Result<Temporal, ScalarError> {
    sqlaero_transcode::temporal::decompose_date(days).map_err(source_error)
}

fn stamp(value: Temporal, quoted: bool) -> Result<Stamp, ScalarError> {
    let value = mysql_precision(value)?;
    let mut stamp = Stamp::new();
    if quoted {
        stamp.append(b"'")?;
    }
    match value {
        Temporal::Date { year, month, day } => date(&mut stamp, year, month, day)?,
        Temporal::Datetime {
            year,
            month,
            day,
            second,
            nanosecond,
        } => {
            date(&mut stamp, year, month, day)?;
            stamp.append(b" ")?;
            clock(&mut stamp, u128::from(second), nanosecond / 1000)?;
        }
        Temporal::Clock {
            negative,
            second,
            nanosecond,
        } => {
            if negative {
                stamp.append(b"-")?;
            }
            clock(&mut stamp, second, nanosecond / 1000)?;
        }
        Temporal::Period { .. } => return Err(range()),
        Temporal::Duration {
            negative,
            second,
            nanosecond,
        } => {
            if negative {
                stamp.append(b"-")?;
            }
            clock(&mut stamp, second, nanosecond / 1000)?;
        }
    }
    if quoted {
        stamp.append(b"'")?;
    }
    Ok(stamp)
}

/// Write exact source ticks as a complete SQL temporal literal.
///
/// # Errors
/// Returns unsupported units, checked overflow or destination failure.
pub fn write(
    output: &mut Vec<u8>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
    limit: usize,
) -> Result<(), ScalarError> {
    decompose(ticks, unit, multiplier, kind)?.write_literal(output, limit)
}

/// Write a clock source as a complete SQL temporal literal.
///
/// # Errors
/// Returns invalid clock values, unsupported units or destination failure.
pub fn write_clock(
    output: &mut Vec<u8>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    limit: usize,
) -> Result<(), ScalarError> {
    decompose_clock(ticks, unit, multiplier)?.write_literal(output, limit)
}

/// Write Unix-epoch days as a complete SQL date literal.
///
/// # Errors
/// Returns checked calendar overflow or destination failure.
pub fn write_date(output: &mut Vec<u8>, days: i64, limit: usize) -> Result<(), ScalarError> {
    decompose_date(days)?.write_literal(output, limit)
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
    components(output, parts, limit, true)
}

/// Write original calendar components as character-parameter content.
/// This preserves server-owned calendar validation without parsing SQL text.
///
/// # Errors
/// Returns malformed digits, checked overflow or destination failure.
pub fn write_components_text(
    output: &mut Vec<u8>,
    parts: [&[u8]; 6],
    limit: usize,
) -> Result<(), ScalarError> {
    components(output, parts, limit, false)
}

/// Normalize an explicitly offset Gregorian calendar without Python's narrow
/// year range. Since offsets are shorter than one day, only adjacent calendar
/// fields change; no huge epoch multiplication or floating point is needed.
/// # Errors
/// Rejects invalid components, offsets or checked native-year overflow.
pub fn components_utc(parts: [&[u8]; 6], offset: i32) -> Result<Temporal, ScalarError> {
    if !(-86_399..=86_399).contains(&offset) {
        return Err(ScalarError::Range(
            "UTC offset must be strictly within 24 hours",
        ));
    }
    let mut values = [0_i128; 6];
    for (part, value) in parts.into_iter().zip(&mut values) {
        if part.is_empty() {
            return Err(ScalarError::Range("calendar component is empty"));
        }
        *value = part
            .iter()
            .try_fold(0_i128, |value, digit| {
                if !digit.is_ascii_digit() {
                    return None;
                }
                value.checked_mul(10)?.checked_add(i128::from(digit - b'0'))
            })
            .ok_or(ScalarError::Range(
                "calendar component exceeds its exact native range",
            ))?;
    }
    let [mut year, month, day, hour, minute, second] = values;
    if !(1..=12).contains(&month)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
    {
        return Err(ScalarError::Range(
            "calendar month or clock component is invalid",
        ));
    }
    let mut month = month as u8;
    let mut day = u8::try_from(day)
        .ok()
        .filter(|day| *day != 0 && *day <= month_days(year, month))
        .ok_or(ScalarError::Range(
            "calendar day is invalid for the actual month and year",
        ))?;
    let local_second = (hour * 3600 + minute * 60 + second) as i32;
    let second = local_second - offset;
    if second < 0 {
        if day > 1 {
            day -= 1;
        } else {
            if month == 1 {
                year = year.checked_sub(1).ok_or_else(range)?;
                month = 12;
            } else {
                month -= 1;
            }
            day = month_days(year, month);
        }
    } else if second >= 86_400 {
        if day < month_days(year, month) {
            day += 1;
        } else {
            day = 1;
            if month == 12 {
                year = year.checked_add(1).ok_or_else(range)?;
                month = 1;
            } else {
                month += 1;
            }
        }
    }
    Ok(Temporal::Datetime {
        year,
        month,
        day,
        second: second.rem_euclid(86_400) as u32,
        nanosecond: 0,
    })
}

fn month_days(year: i128, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn components(
    output: &mut Vec<u8>,
    parts: [&[u8]; 6],
    limit: usize,
    quoted: bool,
) -> Result<(), ScalarError> {
    if parts
        .iter()
        .any(|value| value.is_empty() || !value.iter().all(u8::is_ascii_digit))
    {
        return Err(ScalarError::Range(
            "calendar components must contain nonnegative integer digits",
        ));
    }
    let size = parts.iter().enumerate().try_fold(
        5 + 2 * usize::from(quoted),
        |size, (index, value)| {
            size.checked_add(value.len().max(if index == 0 { 4 } else { 2 }))
                .ok_or_else(range)
        },
    )?;
    writer::reserve(output, size, limit)?;
    if quoted {
        output.push(b'\'');
    }
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
    if quoted {
        output.push(b'\'');
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_decomposition_and_unquoted_display_share_exact_source_semantics() {
        let value = decompose(-1, TimeUnit::Nanosecond, 1, TimeKind::DateTime).unwrap();
        assert_eq!(
            value,
            Temporal::Datetime {
                year: 1969,
                month: 12,
                day: 31,
                second: 86_399,
                nanosecond: 999_999_999,
            }
        );
        let mut output = Vec::new();
        value.write_text(&mut output, 128).unwrap();
        assert_eq!(output, b"1969-12-31 23:59:59.999999");
        assert_eq!(
            decompose_clock(123_456_789, TimeUnit::Nanosecond, 1).unwrap(),
            Temporal::Clock {
                negative: false,
                second: 0,
                nanosecond: 123_456_789
            }
        );
        assert_eq!(
            decompose_date(0).unwrap(),
            Temporal::Date {
                year: 1970,
                month: 1,
                day: 1
            }
        );
        assert_eq!(
            decompose(-1, TimeUnit::Month, 1, TimeKind::Duration).unwrap(),
            Temporal::Period {
                negative: true,
                count: 1,
                unit: TimeUnit::Month
            }
        );
    }

    #[test]
    fn unquoted_calendar_components_keep_original_digits_and_atomic_limits() {
        let parts = [b"2024".as_slice(), b"13", b"31", b"25", b"59", b"61"];
        let mut output = Vec::new();
        write_components_text(&mut output, parts, 19).unwrap();
        assert_eq!(output, b"2024-13-31 25:59:61");
        let mut prefix = b"x".to_vec();
        assert!(write_components_text(&mut prefix, parts, 19).is_err());
        assert_eq!(prefix, b"x");
    }

    #[test]
    fn wide_years_and_duration_hours_keep_exact_ascii_and_atomic_limits() {
        assert_eq!(
            value(i64::MAX, TimeUnit::Year, u64::MAX, TimeKind::DateTime),
            "'170141183460469231704017187605319780275-01-01 00:00:00'"
        );
        assert_eq!(
            value(-i64::MAX, TimeUnit::Year, u64::MAX, TimeKind::DateTime),
            "'-170141183460469231704017187605319776335-01-01 00:00:00'"
        );
        assert_eq!(
            value(i64::MAX, TimeUnit::Second, u64::MAX, TimeKind::Duration),
            "'47261439850130342140004774334811049:31:45'"
        );
        let mut output = b"prefix".to_vec();
        assert!(
            write(
                &mut output,
                1,
                TimeUnit::Nanosecond,
                1,
                TimeKind::DateTime,
                26
            )
            .is_err()
        );
        assert_eq!(output, b"prefix");
    }

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
    fn negative_epoch_fraction_uses_floor_calendar_before_target_truncation() {
        assert_eq!(
            value(-1, TimeUnit::Nanosecond, 1, TimeKind::DateTime),
            "'1969-12-31 23:59:59.999999'"
        );
        assert_eq!(
            value(-1, TimeUnit::Nanosecond, 1, TimeKind::Duration),
            "'00:00:00'"
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
            "'-2562047:47:16.854775'"
        );
        let mut out = Vec::new();
        write_clock(&mut out, -1, TimeUnit::Nanosecond, 1, 100).unwrap();
        write_clock(&mut out, 86_400, TimeUnit::Second, 1, 100).unwrap();
        assert_eq!(out, b"'00:00:00''24:00:00'");
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
