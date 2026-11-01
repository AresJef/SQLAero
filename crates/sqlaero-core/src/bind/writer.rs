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
    // The complete literal was checked and reserved before the first write.
    // Streaming callers retain their per-fragment checks below.
    append_hex(output, value);
    output.extend_from_slice(b"' AS BINARY)");
    Ok(())
}

/// Reserve and begin one exact-width binary literal before incremental writes.
///
/// # Errors
/// Returns size/overflow or allocation failure before modifying the buffer.
pub fn binary_start(output: &mut Vec<u8>, length: usize, limit: usize) -> Result<(), WireError> {
    let length = binary_width(output.len(), length, limit)?;
    output.try_reserve(length)?;
    output.extend_from_slice(b"CAST(X'");
    Ok(())
}

/// Check a binary literal's exact width before allocating or reading its source.
///
/// # Errors
/// Returns the same overflow or request-limit error as `binary_start`.
pub fn binary_width(output_len: usize, length: usize, limit: usize) -> Result<usize, WireError> {
    let length = length
        .checked_mul(2)
        .and_then(|length| length.checked_add(19))
        .ok_or(WireError::Limit {
            required: usize::MAX,
            limit,
        })?;
    let required = output_len.checked_add(length).ok_or(WireError::Limit {
        required: usize::MAX,
        limit,
    })?;
    if required > limit {
        return Err(WireError::Limit { required, limit });
    }
    Ok(length)
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
    append_hex(output, value);
    Ok(())
}

/// Append native hex pairs after the caller checked the exact output width.
// Keep the original short loop at its caller: the unified bulk helper was
// measurably outlined on the common small-value path. Only long writes pay a
// separate bulk call and the fixed initialized-region preparation.
#[inline(always)]
fn append_hex(output: &mut Vec<u8>, value: &[u8]) {
    if value.len() >= 128 {
        append_hex_bulk(output, value);
        return;
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value {
        output.extend_from_slice(&[HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]]);
    }
}

#[inline(never)]
fn append_hex_bulk(output: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let start = output.len();
    // The caller checked and reserved the full width before this private call.
    let end = start + value.len() * 2;
    output.resize(end, 0);
    for (pair, byte) in output[start..end]
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(value)
    {
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 15)];
    }
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

    #[test]
    fn binary_preflight_checks_source_width_and_current_output_without_allocating() {
        assert_eq!(binary_width(1, 2, 24).unwrap(), 23);
        assert!(matches!(
            binary_width(1, 2, 23),
            Err(WireError::Limit {
                required: 24,
                limit: 23
            })
        ));
        assert!(matches!(
            binary_width(0, usize::MAX, usize::MAX),
            Err(WireError::Limit {
                required: usize::MAX,
                ..
            })
        ));
        assert!(matches!(
            binary_width(usize::MAX, 0, usize::MAX),
            Err(WireError::Limit {
                required: usize::MAX,
                ..
            })
        ));
    }

    #[test]
    fn complete_and_fragmented_binary_writes_preserve_every_byte() {
        for bytes in [
            Vec::new(),
            vec![0],
            (0..127).collect(),
            (0..128).collect(),
            (0..129).collect(),
            (0..=255).collect::<Vec<u8>>(),
        ] {
            let prefix = b"\x03SELECT ";
            let literal = format!(
                "CAST(X'{}' AS BINARY)",
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<String>()
            );
            let mut expected = prefix.to_vec();
            expected.extend_from_slice(literal.as_bytes());
            let mut rejected = prefix.to_vec();
            assert!(binary(&mut rejected, &bytes, expected.len() - 1).is_err());
            assert_eq!(rejected, prefix);
            let mut complete = prefix.to_vec();
            binary(&mut complete, &bytes, expected.len()).unwrap();
            assert_eq!(complete, expected);
            for fragment_size in [17, 128] {
                let mut fragmented = prefix.to_vec();
                binary_start(&mut fragmented, bytes.len(), expected.len()).unwrap();
                binary_fragment(&mut fragmented, b"", expected.len()).unwrap();
                for fragment in bytes.chunks(fragment_size) {
                    binary_fragment(&mut fragmented, fragment, expected.len()).unwrap();
                }
                binary_end(&mut fragmented, expected.len()).unwrap();
                assert_eq!(fragmented, complete);
            }
        }
    }
}
