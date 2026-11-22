//! Original temporal metadata, without format-specific precision or timezone policy.

/// Native calendar fields from a microsecond-resolution source.
/// This fact does not impose a target's permitted calendar range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    /// Proleptic Gregorian year.
    pub year: i32,
    /// Calendar month.
    pub month: u8,
    /// Calendar day.
    pub day: u8,
}

/// Full clock fields of a source whose native resolution is microseconds.
/// Higher-resolution sources use their original ticks and unit instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    /// Clock hour.
    pub hour: u8,
    /// Clock minute.
    pub minute: u8,
    /// Clock second.
    pub second: u8,
    /// Source microsecond component.
    pub microsecond: u32,
}

impl Clock {
    /// Return the exact elapsed microseconds represented by these fields.
    pub fn micros(self) -> i128 {
        ((i128::from(self.hour) * 60 + i128::from(self.minute)) * 60 + i128::from(self.second))
            * 1_000_000
            + i128::from(self.microsecond)
    }
}

/// Source storage unit. Calendar periods remain distinct from fixed durations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeUnit {
    /// Gregorian calendar year.
    Year,
    /// Gregorian calendar month.
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
    /// Recognized source metadata; finite-value admission belongs to the target.
    Picosecond,
    /// Recognized source metadata; finite-value admission belongs to the target.
    Femtosecond,
    /// Recognized source metadata; finite-value admission belongs to the target.
    Attosecond,
    /// Unit-less source metadata. Source-defined missing values are classified
    /// before a target tries to interpret a finite tick count.
    Generic,
}

/// Logical meaning of temporal ticks, independent of their target encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeKind {
    /// Ticks relative to the Unix calendar epoch.
    DateTime,
    /// Elapsed duration for fixed units, or a calendar period for year/month.
    Duration,
}

/// Checked failure while interpreting complete native source facts.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    /// Source metadata or arithmetic exceeds the exact admitted native domain.
    #[error("{0}")]
    Range(&'static str),
}

/// Exact logical temporal components, independent of any target format.
/// Fractions are always nanoseconds; no source precision is discarded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    /// A Gregorian date whose year is not narrowed to the MySQL wire range.
    Date {
        /// Signed Gregorian year.
        year: i128,
        /// Calendar month.
        month: u8,
        /// Day of month.
        day: u8,
    },
    /// An epoch-normalized Gregorian date and clock.
    Datetime {
        /// Signed Gregorian year.
        year: i128,
        /// Calendar month.
        month: u8,
        /// Day of month.
        day: u8,
        /// Second within the nonnegative day.
        second: u32,
        /// Fractional second, less than one billion.
        nanosecond: u32,
    },
    /// A signed clock with an unrestricted total hour count.
    Clock {
        /// Original sign, including a negative subsecond value.
        negative: bool,
        /// Absolute whole elapsed seconds.
        second: u128,
        /// Fractional second, less than one billion.
        nanosecond: u32,
    },
    /// An elapsed duration without a MySQL TIME-range restriction.
    Duration {
        /// Original sign, also for negative subsecond durations.
        negative: bool,
        /// Absolute whole elapsed seconds.
        second: u128,
        /// Absolute fractional second, less than one billion.
        nanosecond: u32,
    },
    /// A calendar period, kept distinct from a fixed elapsed duration.
    Period {
        /// Original sign.
        negative: bool,
        /// Absolute number of source calendar periods.
        count: u128,
        /// Year or month; other units are not valid period facts.
        unit: TimeUnit,
    },
}

fn range() -> Error {
    Error::Range("temporal ticks or unit multiplier exceed the native exact range")
}

fn scaled(ticks: i64, multiplier: u64) -> Result<i128, Error> {
    if multiplier == 0 {
        return Err(Error::Range("temporal unit multiplier must be positive"));
    }
    i128::from(ticks)
        .checked_mul(i128::from(multiplier))
        .ok_or_else(range)
}

/// Split an exact fixed-unit value into mathematical floor seconds and fraction.
fn seconds(value: i128, unit: TimeUnit) -> Result<(i128, u32, usize), Error> {
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
        _ => return Err(Error::Range("finite temporal unit is not supported")),
    };
    Ok((value.checked_mul(factor).ok_or_else(range)?, 0, 0))
}

fn civil(days: i128) -> Result<(i128, u8, u8), Error> {
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

fn nanoseconds(part: u32, precision: usize) -> u32 {
    match precision {
        3 => part * 1_000_000,
        6 => part * 1_000,
        _ => part,
    }
}

/// Decompose source ticks once, preserving wide calendar and exact fractions.
/// Source-specific null sentinels and timezone policy belong to the adapter.
///
/// # Errors
/// Returns unsupported units or checked native arithmetic overflow.
pub fn decompose(
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
) -> Result<Value, Error> {
    let value = scaled(ticks, multiplier)?;
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
                        seconds.rem_euclid(86_400) as u32,
                        part,
                        precision,
                    )
                }
            };
            Ok(Value::Datetime {
                year,
                month,
                day,
                second,
                nanosecond: nanoseconds(part, precision),
            })
        }
        TimeKind::Duration => {
            let negative = value < 0;
            let absolute = value.checked_abs().ok_or_else(range)?;
            if matches!(unit, TimeUnit::Year | TimeUnit::Month) {
                return Ok(Value::Period {
                    negative,
                    count: absolute as u128,
                    unit,
                });
            }
            let (second, part, precision) = seconds(absolute, unit)?;
            Ok(Value::Duration {
                negative,
                second: second as u128,
                nanosecond: nanoseconds(part, precision),
            })
        }
    }
}

/// Resolve original UTC ticks with an already acquired source offset, keeping
/// all fractional precision. This performs arithmetic, not timezone lookup.
///
/// # Errors
/// Returns unsupported units or exact native arithmetic overflow.
pub fn decompose_utc(
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    offset_micros: i128,
) -> Result<Value, Error> {
    let (second, part, precision) = seconds(scaled(ticks, multiplier)?, unit)?;
    let fraction =
        i128::from(nanoseconds(part, precision)) + offset_micros.rem_euclid(1_000_000) * 1000;
    let second = second
        .checked_add(offset_micros.div_euclid(1_000_000))
        .and_then(|n| n.checked_add(fraction.div_euclid(1_000_000_000)))
        .ok_or_else(range)?;
    let (year, month, day) = civil(second.div_euclid(86_400))?;
    Ok(Value::Datetime {
        year,
        month,
        day,
        second: second.rem_euclid(86_400) as u32,
        nanosecond: fraction.rem_euclid(1_000_000_000) as u32,
    })
}

/// Decompose a signed clock source without a target-specific day bound.
///
/// # Errors
/// Returns unsupported units or checked exact arithmetic overflow.
pub fn decompose_clock(ticks: i64, unit: TimeUnit, multiplier: u64) -> Result<Value, Error> {
    let value = scaled(ticks, multiplier)?;
    let absolute = value.checked_abs().ok_or_else(range)?;
    let (second, part, precision) = seconds(absolute, unit)?;
    Ok(Value::Clock {
        negative: value < 0,
        second: second as u128,
        nanosecond: nanoseconds(part, precision),
    })
}

/// Decompose Unix-epoch days without narrowing the calendar year.
///
/// # Errors
/// Returns checked calendar arithmetic overflow.
pub fn decompose_date(days: i64) -> Result<Value, Error> {
    let (year, month, day) = civil(i128::from(days))?;
    Ok(Value::Date { year, month, day })
}

/// The caller has established that the value fits in two decimal digits.
fn digit_pair(value: u8) -> [u8; 2] {
    [b'0' + value / 10, b'0' + value % 10]
}

/// Format the common four-digit nonnegative date without target delimiters.
#[inline]
pub fn fixed_calendar(year: i128, month: u8, day: u8) -> Option<[u8; 10]> {
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
#[inline]
pub fn fixed_clock(
    hours: u128,
    minutes: u8,
    seconds: u8,
    micros: u32,
    text: &mut [u8; 15],
) -> Option<usize> {
    if hours >= 100 || minutes >= 100 || seconds >= 100 || micros >= 1_000_000 {
        return None;
    }
    *text = *b"00:00:00.000000";
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
    Some(width)
}

#[cfg(test)]
mod utc_tests {
    use super::*;

    #[test]
    fn civil_matches_an_independent_full_gregorian_cycle_and_wide_era_shifts() {
        let (mut year, mut month, mut day) = (1970_i128, 1_u8, 1_u8);
        for elapsed in 0_i128..146_097 {
            assert_eq!(civil(elapsed).unwrap(), (year, month, day));
            if [0, 31, 58, 59, 60, 365, 789, 11_016, 146_096].contains(&elapsed) {
                for eras in [-5_i128, -1, 1, 10_000_000_000_000_000_000_000_000_000] {
                    assert_eq!(
                        civil(elapsed + eras * 146_097).unwrap(),
                        (year + eras * 400, month, day)
                    );
                }
            }
            // Independent day-by-day calendar progression, not an inverse of
            // the civil decomposition algorithm under test.
            let days = match month {
                4 | 6 | 9 | 11 => 30,
                2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
                2 => 28,
                _ => 31,
            };
            if day < days {
                day += 1;
            } else {
                day = 1;
                if month == 12 {
                    month = 1;
                    year += 1;
                } else {
                    month += 1;
                }
            }
        }
        assert_eq!((year, month, day), (2370, 1, 1));
        assert_eq!(civil(11_016 - 5 * 146_097).unwrap(), (0, 2, 29));
        assert!(civil(i128::MAX).is_err());
    }

    #[test]
    fn offset_keeps_negative_epoch_nanoseconds_and_full_clock_carry() {
        assert_eq!(
            decompose_utc(-1, TimeUnit::Nanosecond, 1, 28_800_000_000).unwrap(),
            Value::Datetime {
                year: 1970,
                month: 1,
                day: 1,
                second: 28_799,
                nanosecond: 999_999_999
            }
        );
        assert_eq!(
            decompose_utc(1, TimeUnit::Nanosecond, 1, -1).unwrap(),
            Value::Datetime {
                year: 1969,
                month: 12,
                day: 31,
                second: 86_399,
                nanosecond: 999_999_001
            }
        );
    }

    #[test]
    fn zero_offset_matches_original_exact_units_without_narrowing() {
        for unit in [
            TimeUnit::Second,
            TimeUnit::Millisecond,
            TimeUnit::Microsecond,
            TimeUnit::Nanosecond,
        ] {
            for ticks in [i64::MIN, -1, 0, 1, i64::MAX] {
                assert_eq!(
                    decompose_utc(ticks, unit, 1, 0).unwrap(),
                    decompose(ticks, unit, 1, TimeKind::DateTime).unwrap()
                );
            }
        }
        assert!(decompose_utc(1, TimeUnit::Month, 1, 0).is_err());
        assert!(decompose_utc(1, TimeUnit::Second, 0, 0).is_err());
    }

    #[test]
    fn oversized_finite_ticks_fail_without_wrapping_or_a_partial_value() {
        // Both extreme products fit i128; scaling them into weeks does not.
        // The fact contract allows u64 multipliers even though NumPy's usual
        // dtype constructors offer a much narrower multiplier domain.
        for ticks in [i64::MIN, i64::MAX] {
            for kind in [TimeKind::DateTime, TimeKind::Duration] {
                assert!(matches!(
                    decompose(ticks, TimeUnit::Week, u64::MAX, kind),
                    Err(Error::Range(_))
                ));
            }
            assert!(matches!(
                decompose_utc(ticks, TimeUnit::Week, u64::MAX, 0),
                Err(Error::Range(_))
            ));
            assert!(matches!(
                decompose_clock(ticks, TimeUnit::Week, u64::MAX),
                Err(Error::Range(_))
            ));
        }
        // A later valid call is independent of the rejected arithmetic.
        assert_eq!(
            decompose(1, TimeUnit::Second, 1, TimeKind::Duration).unwrap(),
            Value::Duration {
                negative: false,
                second: 1,
                nanosecond: 0
            }
        );
    }
}
