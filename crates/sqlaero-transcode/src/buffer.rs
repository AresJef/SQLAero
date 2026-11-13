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
#[inline]
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
#[inline]
pub fn append(output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), CapacityError> {
    reserve(output, bytes.len(), limit)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Append already valid UTF-8 while preserving a typed native text owner.
/// No whole-document UTF-8 validation is needed when the owner is completed.
/// # Errors
/// Returns the actual byte bound or allocation failure before changing text.
#[inline]
pub fn append_text(output: &mut String, text: &str, limit: usize) -> Result<(), CapacityError> {
    let required = output
        .len()
        .checked_add(text.len())
        .ok_or(CapacityError::Limit {
            required: usize::MAX,
            limit,
        })?;
    if required > limit {
        return Err(CapacityError::Limit { required, limit });
    }
    output.try_reserve(text.len())?;
    output.push_str(text);
    Ok(())
}
