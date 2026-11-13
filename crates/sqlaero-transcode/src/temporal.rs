//! Original temporal metadata, without format-specific precision or timezone policy.

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
