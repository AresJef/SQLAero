//! JSON spellings for original temporal facts. This target never normalizes UTC.

use super::Error;
use sqlaero_transcode::number::{Integer, IntegerBuffer};
use sqlaero_transcode::temporal::{Clock, Date};

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

    fn date(&mut self, date: Date) -> Result<(), Error> {
        if date.year < 0 {
            self.put(b"-")?;
        }
        self.number(date.year.unsigned_abs(), 4)?;
        self.put(b"-")?;
        self.number(date.month, 2)?;
        self.put(b"-")?;
        self.number(date.day, 2)
    }

    fn clock(&mut self, clock: Clock) -> Result<(), Error> {
        self.number(clock.hour, 2)?;
        self.put(b":")?;
        self.number(clock.minute, 2)?;
        self.put(b":")?;
        self.number(clock.second, 2)?;
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
        self.number(absolute / 3_600_000_000, 2)?;
        self.put(b":")?;
        self.number(absolute / 60_000_000 % 60, 2)?;
        let seconds = absolute / 1_000_000 % 60;
        let micros = absolute % 1_000_000;
        if seconds != 0 || micros != 0 {
            self.put(b":")?;
            self.number(seconds, 2)?;
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
        let mut text = Self::new();
        if micros < 0 {
            text.put(b"-")?;
        }
        text.put(b"P")?;
        let absolute = micros.unsigned_abs();
        let days = absolute / 86_400_000_000;
        let seconds = absolute / 1_000_000 % 86_400;
        let micros = absolute % 1_000_000;
        if days != 0 || absolute == 0 {
            text.number(days, 0)?;
            text.put(b"D")?;
        }
        if seconds != 0 || micros != 0 {
            text.put(b"T")?;
            text.number(seconds, 0)?;
            if micros != 0 {
                text.put(b".")?;
                let mut digits = micros;
                let mut width = 6;
                while digits.is_multiple_of(10) {
                    digits /= 10;
                    width -= 1;
                }
                text.number(digits, width)?;
            }
            text.put(b"S")?;
        }
        Ok(text)
    }

    pub(super) fn as_str(&self) -> Result<&str, Error> {
        std::str::from_utf8(&self.bytes[..self.length])
            .map_err(|_| Error::State("temporal text was not ASCII"))
    }
}
