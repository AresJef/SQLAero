//! Source shape facts; consumers decide parameter or document layout.

/// Logical shape prepared from a source, without a target binding decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// One logical value with no outer container axis, including a plain 0D
    /// array. A payload container does not replan the original root layout.
    Scalar,
    /// A one-dimensional logical sequence, irrespective of physical chunks.
    Vector {
        /// Number of logical values, excluding physical chunk boundaries.
        length: usize,
    },
    /// One field-bearing record. Field count is not an outer array axis.
    Record {
        /// Number of actual fields, excluding name aliases.
        fields: usize,
    },
    /// A source with two actual row and column axes.
    Table {
        /// Actual number of rows, including rows with no columns.
        rows: usize,
        /// Actual number of columns, including schemas with no rows.
        columns: usize,
    },
}
