//! Checked writes shared by target formats without importing their policies.

use std::collections::TryReserveError;

/// A logical output bound or a reportable native allocation failure.
#[derive(Debug, thiserror::Error)]
pub enum CapacityError {
    /// The requested initialized length exceeds the caller's bound.
    #[error("native output requires {required} bytes, exceeding the {limit}-byte limit")]
    Limit {
        /// Requested initialized length, or `usize::MAX` on length overflow.
        required: usize,
        /// Bound selected by the consumer, without an implicit format quota.
        limit: usize,
    },
    /// The native allocator rejected a checked reservation.
    #[error("cannot reserve native output storage: {0}")]
    Allocation(#[from] TryReserveError),
}

/// Reserve additional bytes without changing the initialized output.
///
/// # Errors
/// Returns the actual length bound or allocation failure. Existing capacity
/// never bypasses the logical bound, and failure preserves initialized bytes.
pub fn reserve(output: &mut Vec<u8>, additional: usize, limit: usize) -> Result<(), CapacityError> {
    let required = output
        .len()
        .checked_add(additional)
        .ok_or(CapacityError::Limit {
            required: usize::MAX,
            limit,
        })?;
    if required > limit {
        return Err(CapacityError::Limit { required, limit });
    }
    output.try_reserve(additional)?;
    Ok(())
}

/// Append bytes after checking their full logical width and allocation.
///
/// # Errors
/// Returns a length bound or allocation failure before changing the content.
pub fn append(output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), CapacityError> {
    reserve(output, bytes.len(), limit)?;
    output.extend_from_slice(bytes);
    Ok(())
}
