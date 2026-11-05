//! Temporary borrowed field views shared by text and binary output paths.
//!
//! A view never owns a Python object, a result tree or the connection. Its byte
//! borrow cannot outlive the actual row owner. Prepared metadata/output policy
//! determines the final Python type at the binding boundary.

/// The actual row representation, fixed when the request is prepared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RowEncoding {
    /// COM_QUERY length-encoded text fields.
    #[default]
    Text,
    /// COM_STMT_EXECUTE native binary fields and NULL bitmap.
    Binary,
}

/// One native field's value, interpreted without converting numeric binary
/// storage into a temporary decimal string. Calendar fields deliberately retain
/// zero/invalid components so the common strict/fallback policy still applies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldValue<'a> {
    /// Protocol NULL, independent of empty bytes or numeric zero.
    Null,
    /// Original field bytes; the prepared column plan selects text, binary,
    /// exact decimal, JSON, BIT or another approved output interpretation.
    Bytes(&'a [u8]),
    /// Signed native integer after its actual wire width was applied.
    Signed(i64),
    /// Unsigned native integer, including the entire u64 range.
    Unsigned(u64),
    /// Native IEEE-754 number; final finite/range policy remains shared.
    Float(f64),
    /// Raw received DATE components, including a zero date.
    Date {
        /// Actual calendar year.
        year: u16,
        /// Actual calendar month.
        month: u8,
        /// Actual calendar day.
        day: u8,
    },
    /// Raw received DATETIME/TIMESTAMP components, without an invented zone.
    Datetime {
        /// Actual calendar year.
        year: u16,
        /// Actual calendar month.
        month: u8,
        /// Actual calendar day.
        day: u8,
        /// Actual clock hour.
        hour: u8,
        /// Actual clock minute.
        minute: u8,
        /// Actual clock second.
        second: u8,
        /// Actual fractional microseconds.
        microsecond: u32,
    },
    /// Exact signed elapsed microseconds from MySQL TIME binary components.
    Duration(i128),
}
