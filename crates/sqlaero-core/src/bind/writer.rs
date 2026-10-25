//! Checked direct writes into the current owned request buffer.

use crate::protocol::framing::WireError;

/// Append already encoded SQL bytes under the logical request limit.
///
/// # Errors
/// Returns a size or native reservation failure before modifying the buffer.
pub fn append(output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), WireError> {
    reserve(output, bytes.len(), limit)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Append an ordinary binary SQL string using CAST(X'...' AS BINARY).
/// This representation has stable string semantics in numeric expressions and
/// does not depend on NO_BACKSLASH_ESCAPES or a connection text codec.
///
/// # Errors
/// Returns a size/overflow or reservation failure before modifying the buffer.
pub fn binary(output: &mut Vec<u8>, value: &[u8], limit: usize) -> Result<(), WireError> {
    binary_start(output, value.len(), limit)?;
    binary_fragment(output, value, limit)?;
    binary_end(output, limit)
}

/// Reserve and begin one exact-width binary literal before incremental writes.
///
/// # Errors
/// Returns size/overflow or allocation failure before modifying the buffer.
pub fn binary_start(output: &mut Vec<u8>, length: usize, limit: usize) -> Result<(), WireError> {
    let length = length
        .checked_mul(2)
        .and_then(|length| length.checked_add(19))
        .ok_or(WireError::Limit {
            required: usize::MAX,
            limit,
        })?;
    reserve(output, length, limit)?;
    output.extend_from_slice(b"CAST(X'");
    Ok(())
}

/// Append one protected byte fragment to an already started binary literal.
///
/// # Errors
/// Returns an actual request-bound or allocation failure.
pub fn binary_fragment(output: &mut Vec<u8>, value: &[u8], limit: usize) -> Result<(), WireError> {
    let length = value.len().checked_mul(2).ok_or(WireError::Limit {
        required: usize::MAX,
        limit,
    })?;
    reserve(output, length, limit)?;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value {
        output.push(HEX[usize::from(byte >> 4)]);
        output.push(HEX[usize::from(byte & 15)]);
    }
    Ok(())
}

/// Complete the single binary literal after all source fragments were written.
///
/// # Errors
/// Returns an actual request-bound or allocation failure.
pub fn binary_end(output: &mut Vec<u8>, limit: usize) -> Result<(), WireError> {
    append(output, b"' AS BINARY)", limit)
}

pub(crate) fn reserve(output: &mut Vec<u8>, length: usize, limit: usize) -> Result<(), WireError> {
    let required = output.len().checked_add(length).ok_or(WireError::Limit {
        required: usize::MAX,
        limit,
    })?;
    if required > limit {
        return Err(WireError::Limit { required, limit });
    }
    output.try_reserve(length)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_cast_has_exact_width_and_checks_before_any_partial_write() {
        let mut output = vec![3];
        binary(&mut output, b"12", 24).unwrap();
        assert_eq!(&output[1..], b"CAST(X'3132' AS BINARY)");
        assert_eq!(output.len(), 24);
        let before = output.clone();
        assert!(binary(&mut output, b"x", 24).is_err());
        assert_eq!(output, before);
    }
}
