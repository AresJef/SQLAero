//! Borrowed single-value semantics shared by text and binary request writers.
//!
//! Escape owns Python/provider recognition. A sink receives native values only;
//! it may consume a borrowed slice during this call but must own any bytes kept
//! afterwards. This is not a batch value tree or an escaped-SQL representation.

use super::Context;
use super::scalar::{self, Clock, Date, ScalarError};
use super::temporal::{TimeKind, TimeUnit};
use super::{temporal, writer};
use crate::charset::codec::CodecError;
use crate::protocol::framing::WireError;
use num_bigint::BigInt;

/// One already adapted logical SQL value. Container shape remains the input
/// adapter's responsibility; prepared positions cannot contain SQL fragments.
#[derive(Debug, Clone, Copy)]
pub enum ValueRef<'a> {
    /// A source NULL, including nonfinite values under the common policy.
    Null,
    /// A boolean value, represented numerically by both MySQL protocols.
    Boolean(bool),
    /// A signed native-width integer.
    Signed(i64),
    /// An unsigned native-width integer.
    Unsigned(u64),
    /// A wider exact integer; the existing 65-digit policy still applies.
    BigInteger(&'a BigInt),
    /// A binary64 value; nonfinite input retains the existing NULL policy.
    Float(f64),
    /// Exact decimal coefficient digits (0..=9), sign and base-ten exponent.
    /// Neither writer converts these through a binary floating-point value.
    Decimal {
        /// Original numeric sign, including negative zero.
        negative: bool,
        /// Most-significant-first coefficient, not ASCII and not SQL text.
        digits: &'a [u8],
        /// Exact decimal exponent.
        exponent: i64,
    },
    /// Native Unicode text, encoded using the supplied connection context.
    Text(&'a str),
    /// Exact bytes, independent of the connection character encoding.
    Bytes(&'a [u8]),
    /// Valid source date, possibly at a UTC-normalized adjacent-year boundary.
    Date(Date),
    /// Native datetime whose timezone normalization has already occurred at
    /// the approved input boundary; no Python tzinfo reaches the sink.
    Datetime {
        /// Calendar components.
        date: Date,
        /// Clock and exact microsecond components.
        clock: Clock,
    },
    /// Clock time; approved time inputs retain their local clock semantics.
    Time(Clock),
    /// A signed exact elapsed microsecond count, including multi-day TIME.
    Duration(i128),
    /// Original temporal storage; submicrosecond precision and extended dates
    /// remain available to choose the accurate native or character parameter.
    Ticks {
        /// Signed value after the provider's own null mask has been applied.
        value: i64,
        /// Original unit.
        unit: TimeUnit,
        /// Positive source unit multiplier.
        multiplier: u64,
        /// Calendar datetime or elapsed duration semantics.
        kind: TimeKind,
    },
    /// A time-of-day source retaining its original precision.
    ClockTicks {
        /// Original signed source value.
        value: i64,
        /// Original unit.
        unit: TimeUnit,
        /// Positive source unit multiplier.
        multiplier: u64,
    },
    /// A provider's day count relative to the Unix calendar epoch.
    DateDays(i64),
    /// Nonnegative struct_time components, already read through the approved
    /// integer protocol, with their exact ASCII digits. Server range/calendar
    /// policy is preserved; components are not narrowed to a native date.
    CalendarComponents(&'a [&'a [u8]; 6]),
}

/// Immediate native value consumer. Concrete writers own their destination
/// and error type; no dynamic Python callback is part of this contract.
pub trait ValueSink {
    /// Real preparation, numeric-policy, encoding or allocation failure.
    type Error;

    /// Write one logical value under the same confirmed request context.
    /// Large byte/text adapters retain their existing bounded-work checkpoints;
    /// implementations must not stash this borrowed value for a later call.
    ///
    /// # Errors
    /// Returns the writer's actual failure without sending any request.
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), Self::Error>;

    /// Begin one chunked text value. `utf8_len` is the original UTF-8 size,
    /// not a promise about the encoded byte length in another client charset.
    /// # Errors
    /// Returns actual preparation, bound or allocation failure.
    fn text_start(&mut self, utf8_len: usize, context: &Context) -> Result<(), Self::Error>;
    /// Encode one complete-character fragment of the same logical text value.
    /// # Errors
    /// Returns the first encoding or destination failure.
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), Self::Error>;
    /// Finish exactly one text value without introducing another parameter.
    /// # Errors
    /// Returns actual finalization or destination failure.
    fn text_end(&mut self, context: &Context) -> Result<(), Self::Error>;
    /// Begin one exact-length chunked binary value.
    /// # Errors
    /// Returns actual preparation, bound or allocation failure.
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), Self::Error>;
    /// Write one bounded byte fragment of the same logical binary value.
    /// # Errors
    /// Returns actual destination or allocation failure.
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), Self::Error>;
    /// Finish exactly one binary value without introducing another parameter.
    /// # Errors
    /// Returns actual finalization or destination failure.
    fn bytes_end(&mut self, context: &Context) -> Result<(), Self::Error>;
}

/// Native text writing failure, retaining the original typed source.
#[derive(Debug, thiserror::Error)]
pub enum ValueError {
    /// The scalar cannot satisfy the exact common value policy.
    #[error(transparent)]
    Scalar(#[from] ScalarError),
    /// The selected client encoding cannot represent the actual text.
    #[error(transparent)]
    Encoding(#[from] CodecError),
    /// The destination exceeds its limit or cannot allocate.
    #[error(transparent)]
    Wire(#[from] WireError),
}

/// Direct text destination. It reuses the established scalar/codec writers and
/// never creates a Python SQL object or retains a borrowed source value.
pub struct TextSink<'a> {
    output: &'a mut Vec<u8>,
}

impl<'a> TextSink<'a> {
    /// Borrow the actual current request buffer, rather than a batch value tree.
    pub fn new(output: &'a mut Vec<u8>) -> Self {
        Self { output }
    }
}

impl ValueSink for TextSink<'_> {
    type Error = ValueError;

    // Let direct callers fold an already known native value kind in this
    // shared implementation instead of materializing a generic dispatch.
    #[inline(always)]
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        let output = &mut *self.output;
        let limit = context.limit;
        match value {
            ValueRef::Null => writer::append(output, b"NULL", limit)?,
            ValueRef::Boolean(value) => {
                writer::append(output, if value { b"1" } else { b"0" }, limit)?
            }
            ValueRef::Signed(value) => scalar::integer(output, value, limit)?,
            ValueRef::Unsigned(value) => scalar::integer(output, value, limit)?,
            ValueRef::BigInteger(value) => scalar::large_integer(output, value, limit)?,
            ValueRef::Float(value) => scalar::float(output, value, limit)?,
            ValueRef::Decimal {
                negative,
                digits,
                exponent,
            } => scalar::decimal(output, negative, digits, exponent, limit)?,
            ValueRef::Text(value) => {
                context
                    .codec
                    .literal(value, output, context.no_backslash_escapes, limit)?
            }
            ValueRef::Bytes(value) => writer::binary(output, value, limit)?,
            ValueRef::Date(date) => scalar::date_time(output, date, None, limit)?,
            ValueRef::Datetime { date, clock } => {
                scalar::date_time(output, date, Some(clock), limit)?
            }
            ValueRef::Time(clock) => scalar::time(output, clock, limit)?,
            ValueRef::Duration(value) => scalar::duration(output, value, limit)?,
            ValueRef::Ticks {
                value,
                unit,
                multiplier,
                kind,
            } => temporal::write(output, value, unit, multiplier, kind, limit)?,
            ValueRef::ClockTicks {
                value,
                unit,
                multiplier,
            } => temporal::write_clock(output, value, unit, multiplier, limit)?,
            ValueRef::DateDays(value) => temporal::write_date(output, value, limit)?,
            ValueRef::CalendarComponents(parts) => {
                temporal::write_components(output, *parts, limit)?
            }
        }
        Ok(())
    }

    fn text_start(&mut self, _: usize, context: &Context) -> Result<(), ValueError> {
        writer::append(self.output, b"'", context.limit)?;
        Ok(())
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        context.codec.literal_fragment(
            text,
            self.output,
            context.no_backslash_escapes,
            context.limit,
        )?;
        Ok(())
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        writer::append(self.output, b"'", context.limit)?;
        Ok(())
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        writer::binary_start(self.output, length, context.limit)?;
        Ok(())
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        writer::binary_fragment(self.output, bytes, context.limit)?;
        Ok(())
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        writer::binary_end(self.output, context.limit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn text_sink_reuses_exact_numeric_and_escaping_semantics() {
        let context = context();
        let mut text = Vec::new();
        for (value, expected) in [
            (ValueRef::Signed(-17), b"-17".as_slice()),
            (ValueRef::Unsigned(u64::MAX), b"18446744073709551615"),
            (ValueRef::Float(-0.0), b"-0.0e0"),
            (ValueRef::Float(f64::INFINITY), b"NULL"),
            (ValueRef::Text("O'Reilly"), b"'O\\'Reilly'"),
            (
                ValueRef::Decimal {
                    negative: false,
                    digits: &[1, 2, 3, 4, 5],
                    exponent: -3,
                },
                b"12.345",
            ),
        ] {
            text.clear();
            TextSink::new(&mut text).write(value, &context).unwrap();
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn another_native_sink_receives_numeric_bits_without_sql_roundtrip() {
        struct SignedParameter(Vec<u8>);
        impl ValueSink for SignedParameter {
            type Error = &'static str;
            fn write(&mut self, value: ValueRef<'_>, _: &Context) -> Result<(), Self::Error> {
                let ValueRef::Signed(value) = value else {
                    return Err("expected native signed integer");
                };
                self.0.extend_from_slice(&value.to_le_bytes());
                Ok(())
            }
            fn text_start(&mut self, _: usize, _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
            fn text_fragment(&mut self, _: &str, _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
            fn text_end(&mut self, _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
            fn bytes_start(&mut self, _: usize, _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
            fn bytes_fragment(&mut self, _: &[u8], _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
            fn bytes_end(&mut self, _: &Context) -> Result<(), Self::Error> {
                Err("integer-only contract test")
            }
        }
        let mut parameter = SignedParameter(Vec::new());
        parameter.write(ValueRef::Signed(-17), &context()).unwrap();
        assert_eq!(
            parameter.0,
            [0xef, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn fragments_keep_one_literal_and_both_backslash_modes() {
        for no_backslash_escapes in [false, true] {
            let context = Context {
                no_backslash_escapes,
                ..context()
            };
            let mut text = Vec::new();
            let mut sink = TextSink::new(&mut text);
            sink.text_start(7, &context).unwrap();
            sink.text_fragment("中'", &context).unwrap();
            sink.text_fragment("é", &context).unwrap();
            sink.text_end(&context).unwrap();
            assert_eq!(
                text,
                if no_backslash_escapes {
                    "'中''é'".as_bytes()
                } else {
                    "'中\\'é'".as_bytes()
                }
            );
            text.clear();
            let mut sink = TextSink::new(&mut text);
            sink.bytes_start(3, &context).unwrap();
            sink.bytes_fragment(&[0, 255], &context).unwrap();
            sink.bytes_fragment(&[39], &context).unwrap();
            sink.bytes_end(&context).unwrap();
            assert_eq!(text, b"CAST(X'00FF27' AS BINARY)");
        }
    }
}
