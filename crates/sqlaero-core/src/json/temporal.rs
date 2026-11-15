//! JSON spellings for original temporal facts. This target never normalizes UTC.

use super::Error;
use sqlaero_transcode::number::{Integer, IntegerBuffer};
use sqlaero_transcode::temporal::{Clock, Date, TimeUnit, Value};

pub(super) fn source_error(error: sqlaero_transcode::temporal::Error) -> Error {
    match error {
        sqlaero_transcode::temporal::Error::Range(message) => Error::Range(message),
    }
}

/// Every admitted fixed-width date, offset and i128 duration fits this stack
/// owner. It contains only ASCII; a checked view avoids an unsafe UTF-8 cast.
pub(super) struct Text {
    bytes: [u8; 128],
    length: usize,
}

impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            length: 0,
        }
    }

    fn put(&mut self, text: &[u8]) -> Result<(), Error> {
        let end = self
            .length
            .checked_add(text.len())
            .ok_or(Error::State("temporal text width overflow"))?;
        let target = self
            .bytes
            .get_mut(self.length..end)
            .ok_or(Error::State("temporal text exceeds its native width"))?;
        target.copy_from_slice(text);
        self.length = end;
        Ok(())
    }

    fn number(&mut self, value: impl Integer, width: usize) -> Result<(), Error> {
        let mut buffer = IntegerBuffer::new();
        let text = buffer.format(value).as_bytes();
        for _ in text.len()..width {
            self.put(b"0")?;
        }
        self.put(text)
    }

    fn pair(&mut self, value: u8) -> Result<(), Error> {
        if let Some(digits) = sqlaero_transcode::number::two_digits(value) {
            self.put(&digits)
        } else {
            self.number(value, 2)
        }
    }

    fn date(&mut self, date: Date) -> Result<(), Error> {
        self.wide_date(i128::from(date.year), date.month, date.day)
    }

    fn wide_date(&mut self, year: i128, month: u8, day: u8) -> Result<(), Error> {
        if year < 0 {
            self.put(b"-")?;
        }
        self.number(year.unsigned_abs(), 4)?;
        self.put(b"-")?;
        self.pair(month)?;
        self.put(b"-")?;
        self.pair(day)
    }

    fn clock(&mut self, clock: Clock) -> Result<(), Error> {
        self.pair(clock.hour)?;
        self.put(b":")?;
        self.pair(clock.minute)?;
        self.put(b":")?;
        self.pair(clock.second)?;
        if clock.microsecond != 0 {
            self.put(b".")?;
            self.number(clock.microsecond, 6)?;
        }
        Ok(())
    }

    fn offset(&mut self, offset: Option<i128>) -> Result<(), Error> {
        let Some(offset) = offset else {
            return Ok(());
        };
        let absolute = offset.unsigned_abs();
        if absolute >= 86_400_000_000 {
            return Err(Error::State("source UTC offset is outside one day"));
        }
        self.put(if offset < 0 { b"-" } else { b"+" })?;
        // The checked offset is strictly below one day; each component fits u8.
        self.pair((absolute / 3_600_000_000) as u8)?;
        self.put(b":")?;
        self.pair((absolute / 60_000_000 % 60) as u8)?;
        let seconds = absolute / 1_000_000 % 60;
        let micros = absolute % 1_000_000;
        if seconds != 0 || micros != 0 {
            self.put(b":")?;
            self.pair(seconds as u8)?;
            if micros != 0 {
                self.put(b".")?;
                self.number(micros, 6)?;
            }
        }
        Ok(())
    }

    pub(super) fn calendar(
        date: Date,
        clock: Option<Clock>,
        offset: Option<i128>,
    ) -> Result<Self, Error> {
        let mut text = Self::new();
        text.date(date)?;
        if let Some(clock) = clock {
            text.put(b"T")?;
            text.clock(clock)?;
            text.offset(offset)?;
        }
        Ok(text)
    }

    pub(super) fn time(clock: Clock, offset: Option<i128>) -> Result<Self, Error> {
        let mut text = Self::new();
        text.clock(clock)?;
        text.offset(offset)?;
        Ok(text)
    }

    pub(super) fn duration(micros: i128) -> Result<Self, Error> {
        let absolute = micros.unsigned_abs();
        Self::native(Value::Duration {
            negative: micros < 0,
            second: absolute / 1_000_000,
            nanosecond: (absolute % 1_000_000) as u32 * 1000,
        })
    }

    pub(super) fn utc_ticks(
        ticks: i64,
        unit: TimeUnit,
        multiplier: u64,
        offset: i128,
    ) -> Result<Self, Error> {
        let value = sqlaero_transcode::temporal::decompose_utc(ticks, unit, multiplier, offset)
            .map_err(source_error)?;
        let mut text = Self::native(value)?;
        text.offset(Some(offset))?;
        Ok(text)
    }

    fn fraction(&mut self, nanosecond: u32, trim: bool) -> Result<(), Error> {
        if nanosecond >= 1_000_000_000 {
            return Err(Error::Range("fraction must be smaller than one second"));
        }
        if nanosecond == 0 {
            return Ok(());
        }
        let (mut digits, mut width) = if nanosecond.is_multiple_of(1000) {
            (nanosecond / 1000, 6)
        } else {
            (nanosecond, 9)
        };
        if trim {
            while digits.is_multiple_of(10) {
                digits /= 10;
                width -= 1;
            }
        }
        self.put(b".")?;
        self.number(digits, width)
    }

    fn clock_seconds(
        &mut self,
        negative: bool,
        second: u128,
        nanosecond: u32,
    ) -> Result<(), Error> {
        if negative {
            self.put(b"-")?;
        }
        self.number(second / 3600, 2)?;
        self.put(b":")?;
        // Remainders are below sixty even when the total hour count is wide.
        self.pair((second / 60 % 60) as u8)?;
        self.put(b":")?;
        self.pair((second % 60) as u8)?;
        self.fraction(nanosecond, false)
    }

    pub(super) fn native(value: Value) -> Result<Self, Error> {
        let mut text = Self::new();
        match value {
            Value::Date { year, month, day } => text.wide_date(year, month, day)?,
            Value::Datetime {
                year,
                month,
                day,
                second,
                nanosecond,
            } => {
                text.wide_date(year, month, day)?;
                text.put(b"T")?;
                text.clock_seconds(false, u128::from(second), nanosecond)?;
            }
            Value::Clock {
                negative,
                second,
                nanosecond,
            } => text.clock_seconds(negative, second, nanosecond)?,
            Value::Period {
                negative,
                count,
                unit,
            } => {
                if negative {
                    text.put(b"-")?;
                }
                text.put(b"P")?;
                text.number(count, 0)?;
                text.put(match unit {
                    TimeUnit::Year => b"Y",
                    TimeUnit::Month => b"M",
                    _ => return Err(Error::Range("calendar period unit must be year or month")),
                })?;
            }
            Value::Duration {
                negative,
                second,
                nanosecond,
            } => {
                if negative {
                    text.put(b"-")?;
                }
                text.put(b"P")?;
                let days = second / 86_400;
                let seconds = second % 86_400;
                if days != 0 || (second == 0 && nanosecond == 0) {
                    text.number(days, 0)?;
                    text.put(b"D")?;
                }
                if seconds != 0 || nanosecond != 0 {
                    text.put(b"T")?;
                    text.number(seconds, 0)?;
                    text.fraction(nanosecond, true)?;
                    text.put(b"S")?;
                }
            }
        }
        Ok(text)
    }

    pub(super) fn as_str(&self) -> Result<&str, Error> {
        std::str::from_utf8(&self.bytes[..self.length])
            .map_err(|_| Error::State("temporal text was not ASCII"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_minimum_width_keeps_values_wider_than_two_digits() {
        for value in [0_u8, 9, 10, 59, 99, 100, 255] {
            let text = Text::calendar(
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
                None,
            )
            .unwrap();
            let expected = format!("2024-01-01T{value:02}:{value:02}:{value:02}.000001");
            assert_eq!(text.as_str().unwrap(), expected);
        }
    }
}
