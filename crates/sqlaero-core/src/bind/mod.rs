//! Native SQL request writing shared by initialization and parameter binding.

pub mod batch;
pub mod scalar;
pub mod template;
pub mod writer;

/// One immutable request-time view of confirmed encoding and value policy.
/// Input adaptation uses this snapshot throughout the current native request.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// Prepared actual connection encoding; no per-value label parsing.
    pub codec: crate::charset::codec::Codec,
    /// Actual NO_BACKSLASH_ESCAPES value, not a guessed SQL mode.
    pub no_backslash_escapes: bool,
    /// Whether session time_zone is reliably confirmed as UTC.
    pub utc_confirmed: bool,
    /// Effective native request bound, including command byte when present.
    pub limit: usize,
}
