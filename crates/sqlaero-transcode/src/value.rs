//! Borrowed native scalar facts and immediate target consumption.
//!
//! This module has no Python, SQL, encoding or session dependency. A consumer
//! must finish using each borrowed fact before the call returns. Keeping bytes
//! for a later call requires an independently owned destination.

use crate::temporal::{Clock, Date, TimeKind, TimeUnit};
use num_bigint::BigInt;

/// One native scalar, without a formatted SQL or JSON representation.
/// Target-specific limits and spelling are deliberately absent.
#[derive(Debug, Clone, Copy)]
pub enum ScalarRef<'a> {
    /// An explicit null or an already classified source missing value.
    Null,
    /// The actual boolean value.
    Boolean(bool),
    /// An exact signed native-width integer.
    Signed(i64),
    /// An exact unsigned native-width integer.
    Unsigned(u64),
    /// A borrowed arbitrary-precision integer, with no digit limit here.
    BigInteger(&'a BigInt),
    /// Original binary64 bits, including negative zero and nonfinite values.
    Float(f64),
    /// Exact coefficient and exponent, retaining scale and signed zero.
    Decimal {
        /// Original sign.
        negative: bool,
        /// Most-significant-first coefficient digits, each in 0..=9.
        digits: &'a [u8],
        /// Original base-ten exponent.
        exponent: i64,
    },
    /// Valid UTF-8 text. Source surrogate validation belongs to the adapter.
    Text(&'a str),
    /// Source binary bytes; a target may reject binary rather than stringify it.
    Bytes(&'a [u8]),
    /// Calendar date fields.
    Date(Date),
    /// Calendar datetime fields of a microsecond-resolution source.
    Datetime {
        /// Calendar fields.
        date: Date,
        /// Full source clock fields.
        clock: Clock,
    },
    /// Local clock fields of a microsecond-resolution source.
    Time(Clock),
    /// Exact signed microseconds from a source with that native resolution.
    Duration(i128),
    /// Original temporal storage, without truncation or range clipping.
    Ticks {
        /// Actual signed source ticks after its validity check.
        value: i64,
        /// Original source unit.
        unit: TimeUnit,
        /// Original positive unit multiplier.
        multiplier: u64,
        /// Calendar datetime or duration source semantics.
        kind: TimeKind,
    },
    /// Original clock storage, without a target's day-range interpretation.
    ClockTicks {
        /// Actual signed source ticks.
        value: i64,
        /// Original source unit.
        unit: TimeUnit,
        /// Original positive unit multiplier.
        multiplier: u64,
    },
    /// Signed Gregorian day count relative to the Unix epoch.
    DateDays(i64),
    /// Six nonnegative calendar components as exact base-ten ASCII digits.
    /// These are numeric facts, not SQL literals or prevalidated calendar fields.
    CalendarComponents(&'a [&'a [u8]; 6]),
}

/// Immediate consumption of a scalar or bounded fragments of one value.
/// Concrete targets choose their own context and actual error type.
pub trait Consumer {
    /// Prepared target policy, independent of source recognition.
    type Context;
    /// The actual target failure, without an implicit fallback conversion.
    type Error;

    /// Consume one borrowed scalar immediately.
    /// # Errors
    /// Returns the target's actual policy, encoding or capacity failure.
    fn write(&mut self, value: ScalarRef<'_>, context: &Self::Context) -> Result<(), Self::Error>;

    /// Begin one text value, with its exact UTF-8 size when already known.
    /// None permits one-pass bounded conversion of immutable source codepoints;
    /// the adapter must still consume the whole original source exactly once.
    /// # Errors
    /// Returns an actual preparation or destination failure.
    fn text_start(
        &mut self,
        utf8_len: Option<usize>,
        context: &Self::Context,
    ) -> Result<(), Self::Error>;
    /// Consume a complete-character fragment before the borrow expires.
    /// # Errors
    /// Returns an actual conversion or destination failure.
    fn text_fragment(&mut self, text: &str, context: &Self::Context) -> Result<(), Self::Error>;
    /// End the current text value.
    /// # Errors
    /// Returns an actual finalization failure.
    fn text_end(&mut self, context: &Self::Context) -> Result<(), Self::Error>;
    /// Begin one source binary value.
    /// # Errors
    /// Returns an actual policy or destination failure.
    fn bytes_start(&mut self, length: usize, context: &Self::Context) -> Result<(), Self::Error>;
    /// Consume a bounded fragment of the current binary value.
    /// # Errors
    /// Returns an actual policy or destination failure.
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Self::Context) -> Result<(), Self::Error>;
    /// End the current binary value.
    /// # Errors
    /// Returns an actual finalization failure.
    fn bytes_end(&mut self, context: &Self::Context) -> Result<(), Self::Error>;
}
