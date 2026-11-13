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
