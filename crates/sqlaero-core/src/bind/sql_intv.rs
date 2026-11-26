//! Fixed temporal interval grammar, shared by functions and interval units.

use super::sql_func::{Part, Role};

/// Units admitted by MySQL EXTRACT.
pub const EXTRACT_UNITS: &[&str] = &[
    "MICROSECOND",
    "SECOND",
    "MINUTE",
    "HOUR",
    "DAY",
    "WEEK",
    "MONTH",
    "QUARTER",
    "YEAR",
    "SECOND_MICROSECOND",
    "MINUTE_MICROSECOND",
    "MINUTE_SECOND",
    "HOUR_MICROSECOND",
    "HOUR_SECOND",
    "HOUR_MINUTE",
    "DAY_MICROSECOND",
    "DAY_SECOND",
    "DAY_MINUTE",
    "DAY_HOUR",
    "YEAR_MONTH",
];

/// Render the quantity in the current scalar context before the fixed unit.
pub const fn single(unit: &'static str) -> [Part; 3] {
    [
        Part::Syntax("INTERVAL "),
        Part::Operand(0, Role::Scalar),
        Part::Syntax(unit),
    ]
}
