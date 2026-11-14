//! Native COM_STMT request encoding and prepare-response validation.

use crate::bind::temporal::MysqlTemporal;
use crate::bind::temporal::Temporal;
use crate::bind::value::{ValueError, ValueRef, ValueSink};
use crate::bind::{Context, scalar, temporal, writer};
use crate::protocol::framing::{MAX_MESSAGE_PAYLOAD, WireError};

/// One current EXECUTE payload, written directly from native adapted values.
pub struct Parameters {
    output: Vec<u8>,
    limit: usize,
    expected: usize,
    position: usize,
    types: usize,
    fragment: Option<Fragment>,
    failed: bool,
}

enum Fragment {
    Text {
        prefix: usize,
        expected: Option<usize>,
        written: usize,
    },
    Bytes {
        remaining: usize,
    },
    Decimal {
        remaining: usize,
    },
}

impl Parameters {
    /// Prepare one EXECUTE header and its exact number of parameter positions.
    ///
    /// # Errors
    /// Returns actual request capacity or allocation failure.
    pub fn new(statement_id: u32, expected: u16, limit: usize) -> Result<Self, ValueError> {
        let expected = usize::from(expected);
        let bitmap = expected.div_ceil(8);
        let types = 11 + bitmap;
        let length = if expected == 0 {
            10
        } else {
            types + expected * 2
        };
        let limit = limit.min(MAX_MESSAGE_PAYLOAD);
        let mut output = Vec::new();
        writer::reserve(&mut output, length, limit)?;
        output.resize(length, 0);
        output[0] = 0x17;
        output[1..5].copy_from_slice(&statement_id.to_le_bytes());
        // No server cursor, no query attributes and exactly one iteration.
        output[6] = 1;
        if expected != 0 {
            output[10 + bitmap] = 1;
        }
        Ok(Self {
            output,
            limit,
            expected,
            position: 0,
            types,
            fragment: None,
            failed: false,
        })
    }

    /// Return the actual request only after every expected value was written.
    ///
    /// # Errors
    /// Rejects missing values or an unfinished fragmented parameter.
    pub fn finish(self) -> Result<Vec<u8>, ValueError> {
        if self.failed || self.fragment.is_some() || self.position != self.expected {
            return Err(WireError::Malformed("prepared parameter count mismatch").into());
        }
        Ok(self.output)
    }

    fn complete(&mut self, kind: u8, unsigned: bool) {
        self.output[self.types + self.position * 2] = kind;
        self.output[self.types + self.position * 2 + 1] = if unsigned { 128 } else { 0 };
        if kind == 6 {
            self.output[10 + self.position / 8] |= 1 << (self.position % 8);
        }
        self.position += 1;
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), ValueError> {
        writer::append(&mut self.output, bytes, self.limit)?;
        Ok(())
    }

    fn ready(&self) -> Result<(), ValueError> {
        if self.failed || self.fragment.is_some() || self.position >= self.expected {
            return Err(WireError::Malformed("invalid prepared parameter writer state").into());
        }
        Ok(())
    }

    fn begin_text(&mut self, utf8_length: Option<usize>) -> Result<(), ValueError> {
        self.ready()?;
        let prefix = self.output.len();
        // The marker carries the actual encoded byte count, filled at end.
        // MySQL net_field_length_ll accepts this fixed length-integer form;
        // it avoids an unbounded whole-value move after chunked conversion.
        self.append(&[254, 0, 0, 0, 0, 0, 0, 0, 0])?;
        self.fragment = Some(Fragment::Text {
            prefix,
            expected: utf8_length,
            written: 0,
        });
        Ok(())
    }

    fn append_text(&mut self, value: &str, context: &Context) -> Result<(), ValueError> {
        let Some(Fragment::Text {
            expected, written, ..
        }) = &self.fragment
        else {
            return Err(WireError::Malformed("text fragment without a text parameter").into());
        };
        let next = written
            .checked_add(value.len())
            .filter(|next| expected.is_none_or(|expected| *next <= expected))
            .ok_or(WireError::Malformed("text parameter source length changed"))?;
        context.codec.encode(value, &mut self.output, self.limit)?;
        if let Some(Fragment::Text { written, .. }) = &mut self.fragment {
            *written = next;
        }
        Ok(())
    }

    fn finish_text(&mut self) -> Result<(), ValueError> {
        let Some(Fragment::Text {
            prefix,
            expected,
            written,
        }) = self.fragment
        else {
            return Err(WireError::Malformed("text end without a text parameter").into());
        };
        if expected.is_some_and(|expected| written != expected) {
            return Err(WireError::Malformed("incomplete prepared text parameter").into());
        }
        let length = (self.output.len() - prefix - 9) as u64;
        self.output[prefix + 1..prefix + 9].copy_from_slice(&length.to_le_bytes());
        self.fragment = None;
        Ok(())
    }

    fn begin_bytes(&mut self, length: usize) -> Result<(), ValueError> {
        self.ready()?;
        let (prefix, width) = length_prefix(length);
        let total = width.checked_add(length).ok_or(WireError::Limit {
            required: usize::MAX,
            limit: self.limit,
        })?;
        writer::reserve(&mut self.output, total, self.limit)?;
        self.output.extend_from_slice(&prefix[..width]);
        self.fragment = Some(Fragment::Bytes { remaining: length });
        Ok(())
    }

    fn append_bytes(&mut self, value: &[u8]) -> Result<(), ValueError> {
        let Some(Fragment::Bytes { remaining }) = self.fragment else {
            return Err(WireError::Malformed("bytes fragment without a binary parameter").into());
        };
        let remaining = remaining
            .checked_sub(value.len())
            .ok_or(WireError::Malformed(
                "binary parameter source length changed",
            ))?;
        self.append(value)?;
        self.fragment = Some(Fragment::Bytes { remaining });
        Ok(())
    }

    fn finish_bytes(&mut self) -> Result<(), ValueError> {
        if !matches!(self.fragment, Some(Fragment::Bytes { remaining: 0 })) {
            return Err(WireError::Malformed("incomplete prepared binary parameter").into());
        }
        self.fragment = None;
        Ok(())
    }

    fn bytes(&mut self, bytes: &[u8]) -> Result<(), ValueError> {
        let (prefix, width) = length_prefix(bytes.len());
        let total = width.checked_add(bytes.len()).ok_or(WireError::Limit {
            required: usize::MAX,
            limit: self.limit,
        })?;
        writer::reserve(&mut self.output, total, self.limit)?;
        self.output.extend_from_slice(&prefix[..width]);
        self.output.extend_from_slice(bytes);
        Ok(())
    }

    fn temporal(&mut self, value: Temporal) -> Result<u8, ValueError> {
        let value = temporal::mysql_precision(value)?;
        match value {
            Temporal::Date { year, month, day } if normal_date(year, month, day) => {
                self.append(&[4, year as u8, (year >> 8) as u8, month, day])?;
                return Ok(10);
            }
            Temporal::Datetime {
                year,
                month,
                day,
                second,
                nanosecond,
            } if normal_date(year, month, day)
                && second < 86_400
                && nanosecond.is_multiple_of(1000)
                && nanosecond < 1_000_000_000 =>
            {
                let micros = nanosecond / 1000;
                self.append(&[
                    if micros == 0 { 7 } else { 11 },
                    year as u8,
                    (year >> 8) as u8,
                    month,
                    day,
                    (second / 3600) as u8,
                    (second / 60 % 60) as u8,
                    (second % 60) as u8,
                ])?;
                if micros != 0 {
                    self.append(&micros.to_le_bytes())?;
                }
                return Ok(12);
            }
            Temporal::Clock {
                negative,
                second,
                nanosecond,
            }
            | Temporal::Duration {
                negative,
                second,
                nanosecond,
            } if second <= 3_020_399 && nanosecond < 1_000_000_000 => {
                self.native_duration(negative, second, nanosecond / 1000)?;
                return Ok(11);
            }
            _ => (),
        }
        let mut text = Vec::new();
        value.write_text(&mut text, self.limit)?;
        self.bytes(&text)?;
        Ok(253)
    }

    fn native_duration(
        &mut self,
        negative: bool,
        seconds: u128,
        micros: u32,
    ) -> Result<(), ValueError> {
        if seconds == 0 && micros == 0 {
            return self.append(&[0]);
        }
        let days = (seconds / 86_400) as u32;
        self.append(&[if micros == 0 { 8 } else { 12 }, u8::from(negative)])?;
        self.append(&days.to_le_bytes())?;
        self.append(&[
            (seconds / 3600 % 24) as u8,
            (seconds / 60 % 60) as u8,
            (seconds % 60) as u8,
        ])?;
        if micros != 0 {
            self.append(&micros.to_le_bytes())?;
        }
        Ok(())
    }

    fn encode(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(u8, bool), ValueError> {
        let kind = match value {
            ValueRef::Null => 6,
            ValueRef::Boolean(value) => {
                self.append(&[u8::from(value)])?;
                1
            }
            ValueRef::Signed(value) => {
                self.append(&value.to_le_bytes())?;
                8
            }
            ValueRef::Unsigned(value) => {
                self.append(&value.to_le_bytes())?;
                return Ok((8, true));
            }
            ValueRef::Float(value) if !value.is_finite() => 6,
            ValueRef::Float(value) => {
                self.append(&value.to_le_bytes())?;
                5
            }
            ValueRef::BigInteger(value) => {
                let mut text = Vec::new();
                scalar::large_integer(&mut text, value, self.limit)?;
                self.bytes(&text)?;
                246
            }
            ValueRef::Decimal {
                negative,
                digits,
                exponent,
            } => {
                use sqlaero_transcode::decimal::{Digits, Inspect, Render};
                let mut inspect = Inspect::default();
                for &digit in digits {
                    inspect.digit(digit).map_err(scalar::ScalarError::Range)?;
                }
                let header = inspect
                    .finish(negative, exponent)
                    .map_err(scalar::ScalarError::Range)?;
                let layout = scalar::decimal_layout(header)?;
                let mut render = Render::new(Digits::Values(digits), layout)
                    .map_err(scalar::ScalarError::Range)?;
                self.begin_bytes(layout.len())?;
                let mut chunk = [0; 128];
                loop {
                    let n = render.read(&mut chunk);
                    if n == 0 {
                        break;
                    }
                    self.append_bytes(&chunk[..n])?;
                }
                self.finish_bytes()?;
                246
            }
            ValueRef::Bytes(value) => {
                self.begin_bytes(value.len())?;
                self.append_bytes(value)?;
                self.finish_bytes()?;
                252
            }
            ValueRef::Text(value) => {
                self.begin_text(Some(value.len()))?;
                self.append_text(value, context)?;
                self.finish_text()?;
                253
            }
            ValueRef::Uuid(value) => {
                let text = sqlaero_transcode::identifier::UuidText::new(value);
                self.begin_text(Some(36))?;
                self.append_text(text.as_str(), context)?;
                self.finish_text()?;
                253
            }
            ValueRef::Date(value) => self.temporal(Temporal::Date {
                year: i128::from(value.year),
                month: value.month,
                day: value.day,
            })?,
            ValueRef::Datetime {
                date,
                clock,
                offset_micros,
            } => {
                let (date, clock) =
                    crate::bind::value::mysql_datetime(date, clock, offset_micros, context)?;
                self.temporal(Temporal::Datetime {
                    year: i128::from(date.year),
                    month: date.month,
                    day: date.day,
                    second: u32::from(clock.hour) * 3600
                        + u32::from(clock.minute) * 60
                        + u32::from(clock.second),
                    nanosecond: clock.microsecond * 1000,
                })?
            }
            ValueRef::Time { clock, .. } => self.temporal(Temporal::Clock {
                negative: false,
                second: u128::from(clock.hour) * 3600
                    + u128::from(clock.minute) * 60
                    + u128::from(clock.second),
                nanosecond: clock.microsecond * 1000,
            })?,
            ValueRef::Duration(micros) => {
                let absolute = micros.unsigned_abs();
                self.temporal(Temporal::Duration {
                    negative: micros < 0,
                    second: absolute / 1_000_000,
                    nanosecond: (absolute % 1_000_000) as u32 * 1000,
                })?
            }
            ValueRef::Ticks {
                value,
                unit,
                multiplier,
                kind,
            } => self.temporal(temporal::decompose(value, unit, multiplier, kind)?)?,
            ValueRef::ClockTicks {
                value,
                unit,
                multiplier,
            } => self.temporal(temporal::decompose_clock(value, unit, multiplier)?)?,
            ValueRef::DateDays(value) => self.temporal(temporal::decompose_date(value)?)?,
            ValueRef::CalendarComponents {
                parts,
                offset_seconds,
            } => {
                if let Some(value) =
                    crate::bind::value::mysql_components(*parts, offset_seconds, context)?
                {
                    self.temporal(value)?
                } else {
                    let mut values = [0_u32; 6];
                    let native = parts.iter().zip(&mut values).all(|(part, target)| {
                        if part.is_empty() {
                            return false;
                        }
                        let Some(value) = part.iter().try_fold(0_u32, |value, digit| {
                            if !digit.is_ascii_digit() {
                                return None;
                            }
                            value.checked_mul(10)?.checked_add(u32::from(digit - b'0'))
                        }) else {
                            return false;
                        };
                        *target = value;
                        true
                    });
                    if native
                        && normal_date(
                            i128::from(values[0]),
                            values[1].min(255) as u8,
                            values[2].min(255) as u8,
                        )
                        && values[1] <= 12
                        && values[2] <= 31
                        && values[3] < 24
                        && values[4] < 60
                        && values[5] < 60
                    {
                        self.temporal(Temporal::Datetime {
                            year: i128::from(values[0]),
                            month: values[1] as u8,
                            day: values[2] as u8,
                            second: values[3] * 3600 + values[4] * 60 + values[5],
                            nanosecond: 0,
                        })?
                    } else {
                        let length = parts.iter().enumerate().try_fold(
                            5_usize,
                            |length, (index, part)| {
                                length
                                    .checked_add(part.len().max(if index == 0 { 4 } else { 2 }))
                                    .ok_or(WireError::Limit {
                                        required: usize::MAX,
                                        limit: self.limit,
                                    })
                            },
                        )?;
                        let (prefix, width) = length_prefix(length);
                        self.append(&prefix[..width])?;
                        temporal::write_components_text(&mut self.output, *parts, self.limit)?;
                        253
                    }
                }
            }
        };
        Ok((kind, false))
    }
}

impl ValueSink for Parameters {
    type Context = Context;
    type Error = ValueError;

    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        if let Err(error) = self.ready() {
            self.failed = true;
            return Err(error);
        }
        let start = self.output.len();
        match self.encode(value, context) {
            Ok((kind, unsigned)) => {
                self.complete(kind, unsigned);
                Ok(())
            }
            Err(error) => {
                self.output.truncate(start);
                self.fragment = None;
                self.failed = true;
                Err(error)
            }
        }
    }

    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        let result = (|| {
            if header.category().is_some() {
                self.write(ValueRef::Null, context)?;
                return Ok(None);
            }
            let layout = scalar::decimal_layout(header)?;
            self.begin_bytes(layout.len())?;
            self.fragment = Some(Fragment::Decimal {
                remaining: layout.len(),
            });
            Ok(Some(layout))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn decimal_fragment(&mut self, ascii: &str, _: &Context) -> Result<(), ValueError> {
        let result = (|| {
            let Some(Fragment::Decimal { remaining }) = self.fragment else {
                return Err(
                    WireError::Malformed("decimal fragment without a decimal parameter").into(),
                );
            };
            if self.failed {
                return Err(WireError::Malformed("failed prepared decimal writer").into());
            }
            let remaining = remaining
                .checked_sub(ascii.len())
                .ok_or(WireError::Malformed("decimal source length changed"))?;
            self.append(ascii.as_bytes())?;
            self.fragment = Some(Fragment::Decimal { remaining });
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn decimal_end(&mut self, _: &Context) -> Result<(), ValueError> {
        if self.failed || !matches!(self.fragment, Some(Fragment::Decimal { remaining: 0 })) {
            self.failed = true;
            return Err(WireError::Malformed("incomplete prepared decimal parameter").into());
        }
        self.fragment = None;
        self.complete(246, false);
        Ok(())
    }

    fn text_start(&mut self, utf8_length: Option<usize>, _: &Context) -> Result<(), ValueError> {
        let result = self.begin_text(utf8_length);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn text_fragment(&mut self, value: &str, context: &Context) -> Result<(), ValueError> {
        let result = if self.failed {
            Err(WireError::Malformed("failed prepared parameter writer").into())
        } else {
            self.append_text(value, context)
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn text_end(&mut self, _: &Context) -> Result<(), ValueError> {
        let result = if self.failed {
            Err(WireError::Malformed("failed prepared parameter writer").into())
        } else {
            self.finish_text()
        };
        match result {
            Ok(()) => {
                self.complete(253, false);
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }

    fn bytes_start(&mut self, length: usize, _: &Context) -> Result<(), ValueError> {
        let result = self.begin_bytes(length);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn bytes_fragment(&mut self, value: &[u8], _: &Context) -> Result<(), ValueError> {
        let result = if self.failed {
            Err(WireError::Malformed("failed prepared parameter writer").into())
        } else {
            self.append_bytes(value)
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn bytes_end(&mut self, _: &Context) -> Result<(), ValueError> {
        let result = if self.failed {
            Err(WireError::Malformed("failed prepared parameter writer").into())
        } else {
            self.finish_bytes()
        };
        match result {
            Ok(()) => {
                self.complete(252, false);
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}

fn normal_date(year: i128, month: u8, day: u8) -> bool {
    (1000..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day != 0
        && day <= scalar::month_days(year as i32, month)
}

fn length_prefix(length: usize) -> ([u8; 9], usize) {
    let mut prefix = [0; 9];
    let bytes = (length as u64).to_le_bytes();
    let width = match length {
        0..=250 => {
            prefix[0] = length as u8;
            return (prefix, 1);
        }
        251..=65535 => 3,
        65536..=16777215 => 4,
        _ => 9,
    };
    prefix[0] = match width {
        3 => 252,
        4 => 253,
        _ => 254,
    };
    prefix[1..width].copy_from_slice(&bytes[..width - 1]);
    (prefix, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bind::scalar::{Clock, Date};
    use crate::charset::codec::Codec;

    fn context() -> Context {
        Context {
            codec: Codec::new(crate::charset::resolve(None, None).unwrap().charset),
            no_backslash_escapes: false,
            utc_confirmed: true,
            limit: 4096,
        }
    }

    #[test]
    fn execute_header_null_bitmap_and_type_table_describe_each_current_value() {
        let mut writer = Parameters::new(0x1234_5678, 4, 4096).unwrap();
        for value in [
            ValueRef::Signed(i64::MIN),
            ValueRef::Unsigned(u64::MAX),
            ValueRef::Null,
            ValueRef::Float(-0.0),
        ] {
            writer.write(value, &context()).unwrap();
        }
        let bytes = writer.finish().unwrap();
        assert_eq!(
            &bytes[..12],
            &[0x17, 0x78, 0x56, 0x34, 0x12, 0, 1, 0, 0, 0, 4, 1]
        );
        assert_eq!(&bytes[12..20], &[8, 0, 8, 128, 6, 0, 5, 0]);
        assert_eq!(&bytes[20..28], &i64::MIN.to_le_bytes());
        assert_eq!(&bytes[28..36], &u64::MAX.to_le_bytes());
        assert_eq!(&bytes[36..44], &(-0.0_f64).to_le_bytes());
    }

    #[test]
    fn exact_decimal_payload_keeps_numeric_type_and_full_digits() {
        let mut writer = Parameters::new(9, 2, 4096).unwrap();
        let big = num_bigint::BigInt::parse_bytes(b"1234567890123456789012345678901234567890", 10)
            .unwrap();
        writer
            .write(ValueRef::BigInteger(&big), &context())
            .unwrap();
        writer
            .write(
                ValueRef::Decimal {
                    negative: true,
                    digits: &[1, 2, 3, 4, 5, 0, 0],
                    exponent: -5,
                },
                &context(),
            )
            .unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[12..16], &[246, 0, 246, 0]);
        assert_eq!(
            &bytes[16..],
            b"\x281234567890123456789012345678901234567890\x09-12.34500"
        );
    }

    #[test]
    fn fragmented_decimal_keeps_numeric_kind_and_rejects_incomplete_or_crossed_values() {
        use sqlaero_transcode::decimal::Inspect;
        let mut inspect = Inspect::default();
        for digit in [1, 2, 0, 0] {
            inspect.digit(digit).unwrap();
        }
        let header = inspect.finish(false, -2).unwrap();
        let ctx = context();
        let mut value = Parameters::new(7, 1, 4096).unwrap();
        assert_eq!(value.decimal_start(header, &ctx).unwrap().unwrap().len(), 5);
        value.decimal_fragment("12.", &ctx).unwrap();
        value.decimal_fragment("00", &ctx).unwrap();
        value.decimal_end(&ctx).unwrap();
        let bytes = value.finish().unwrap();
        assert_eq!(&bytes[12..14], &[246, 0]);
        assert_eq!(&bytes[14..], b"\x0512.00");

        for action in 0..4 {
            let mut value = Parameters::new(7, 1, 4096).unwrap();
            value.decimal_start(header, &ctx).unwrap();
            assert!(
                match action {
                    0 => value.decimal_end(&ctx),
                    1 => value.decimal_fragment("123456", &ctx),
                    2 => value.bytes_fragment(b"12.00", &ctx),
                    _ => value.text_fragment("12.00", &ctx),
                }
                .is_err()
            );
            assert!(value.finish().is_err());
        }
        let mut limited = Parameters::new(7, 1, 16).unwrap();
        assert!(limited.decimal_start(header, &ctx).is_err());
        assert!(limited.finish().is_err());
    }

    #[test]
    fn native_temporal_payloads_keep_microseconds_without_text_roundtrip() {
        let mut writer = Parameters::new(9, 2, 4096).unwrap();
        writer
            .write(
                ValueRef::Datetime {
                    date: Date {
                        year: 2026,
                        month: 11,
                        day: 5,
                    },
                    clock: Clock {
                        hour: 1,
                        minute: 2,
                        second: 3,
                        microsecond: 4,
                    },
                    offset_micros: None,
                },
                &context(),
            )
            .unwrap();
        writer
            .write(ValueRef::Duration(-183_845_123_456), &context())
            .unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[12..16], &[12, 0, 11, 0]);
        assert_eq!(&bytes[16..28], &[11, 0xea, 7, 11, 5, 1, 2, 3, 4, 0, 0, 0]);
        assert_eq!(
            &bytes[28..],
            &[12, 1, 2, 0, 0, 0, 3, 4, 5, 0x40, 0xe2, 1, 0]
        );
    }

    #[test]
    fn zero_parameter_execute_omits_bind_header_and_counts_are_strict() {
        assert_eq!(
            Parameters::new(1, 0, 10).unwrap().finish().unwrap(),
            &[0x17, 1, 0, 0, 0, 0, 1, 0, 0, 0]
        );
        assert!(Parameters::new(1, 1, 4096).unwrap().finish().is_err());
        assert!(
            Parameters::new(1, 0, 4096)
                .unwrap()
                .write(ValueRef::Null, &context())
                .is_err()
        );
        assert!(Parameters::new(1, u16::MAX, 4096).is_err());
    }

    #[test]
    fn chunked_text_counts_actual_encoded_bytes_and_one_parameter_position() {
        let mut context = context();
        context.codec = Codec::new(
            crate::charset::resolve(Some("latin1"), None)
                .unwrap()
                .charset,
        );
        let mut writer = Parameters::new(1, 2, 4096).unwrap();
        writer.text_start(Some(3), &context).unwrap();
        writer.text_fragment("é", &context).unwrap();
        writer.text_fragment("'", &context).unwrap();
        writer.text_end(&context).unwrap();
        writer.write(ValueRef::Signed(7), &context).unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[12..16], &[253, 0, 8, 0]);
        assert_eq!(&bytes[16..27], &[254, 2, 0, 0, 0, 0, 0, 0, 0, 0xe9, b'\'']);
        assert_eq!(&bytes[27..], &7_i64.to_le_bytes());
    }

    #[test]
    fn unmeasured_source_utf8_keeps_actual_encoding_length_and_one_position() {
        let mut context = context();
        context.codec = Codec::new(
            crate::charset::resolve(Some("latin1"), None)
                .unwrap()
                .charset,
        );
        let mut writer = Parameters::new(1, 1, 4096).unwrap();
        writer.text_start(None, &context).unwrap();
        writer.text_fragment("é", &context).unwrap();
        writer.text_fragment("'", &context).unwrap();
        writer.text_end(&context).unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[12..14], &[253, 0]);
        assert_eq!(&bytes[14..], &[254, 2, 0, 0, 0, 0, 0, 0, 0, 0xe9, b'\'']);
    }

    #[test]
    fn fragmented_payload_cannot_finish_after_wrong_lengths_or_encoding_error() {
        let mut writer = Parameters::new(1, 1, 4096).unwrap();
        writer.bytes_start(3, &context()).unwrap();
        writer.bytes_fragment(b"ab", &context()).unwrap();
        assert!(writer.bytes_end(&context()).is_err());
        assert!(writer.finish().is_err());
        let mut writer = Parameters::new(1, 1, 4096).unwrap();
        writer.text_start(Some(3), &context()).unwrap();
        writer.text_fragment("ab", &context()).unwrap();
        assert!(writer.text_end(&context()).is_err());
        assert!(writer.finish().is_err());
        let mut writer = Parameters::new(1, 1, 4096).unwrap();
        writer.text_start(Some(1), &context()).unwrap();
        assert!(writer.text_fragment("é", &context()).is_err());
        assert!(writer.finish().is_err());
        let mut ascii = context();
        ascii.codec = Codec::new(
            crate::charset::resolve(Some("ascii"), None)
                .unwrap()
                .charset,
        );
        let mut writer = Parameters::new(1, 1, 4096).unwrap();
        writer.text_start(Some(2), &ascii).unwrap();
        assert!(writer.text_fragment("é", &ascii).is_err());
        assert!(writer.finish().is_err());
    }

    #[test]
    fn temporal_target_truncation_precedes_native_admission_and_wide_fallback() {
        let mut writer = Parameters::new(1, 3, 4096).unwrap();
        writer
            .write(
                ValueRef::Date(Date {
                    year: 10000,
                    month: 1,
                    day: 1,
                }),
                &context(),
            )
            .unwrap();
        writer
            .write(
                ValueRef::Ticks {
                    value: 1,
                    unit: temporal::TimeUnit::Nanosecond,
                    multiplier: 1,
                    kind: temporal::TimeKind::DateTime,
                },
                &context(),
            )
            .unwrap();
        writer
            .write(
                ValueRef::CalendarComponents {
                    parts: &[b"2026", b"13", b"32", b"24", b"60", b"61"],
                    offset_seconds: None,
                },
                &context(),
            )
            .unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[12..18], &[253, 0, 12, 0, 253, 0]);
        assert_eq!(
            &bytes[18..],
            b"\x0b10000-01-01\x07\xb2\x07\x01\x01\x00\x00\x00\x132026-13-32 24:60:61"
        );
    }
}
