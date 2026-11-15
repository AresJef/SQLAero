//! Fallible UTF-8 conversion of bounded native codepoint blocks.

use crate::buffer::CapacityError;

/// Detect ASCII control bytes without interpreting the target output syntax.
/// The caller supplies a bounded block. Non-ASCII UTF-8 bytes are not controls.
#[inline]
pub fn contains_ascii_control(source: &[u8]) -> bool {
    source.iter().copied().min().unwrap_or(u8::MAX) < 0x20
}

/// A source codepoint or scratch-capacity failure, with native coordinates.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// A surrogate or value outside the Unicode scalar range.
    #[error("invalid Unicode codepoint U+{point:04X} at source index {index}")]
    CodePoint {
        /// Zero-based index within the current source block.
        index: usize,
        /// Original source value; no repair or surrogate pairing is performed.
        point: u32,
    },
    /// Checked scratch capacity or allocation failed.
    #[error(transparent)]
    Capacity(#[from] CapacityError),
}

fn prepare(output: &mut String, items: usize, width: usize) -> Result<(), CapacityError> {
    let required = items.checked_mul(width).ok_or(CapacityError::Limit {
        required: usize::MAX,
        limit: usize::MAX,
    })?;
    output.clear();
    output.try_reserve(required)?;
    Ok(())
}

/// Convert Latin-1 codepoint bytes, without treating them as encoded UTF-8.
/// The caller supplies a bounded block and owns the reusable scratch buffer.
/// # Errors
/// Returns checked capacity or allocation failure.
pub fn latin1(source: &[u8], output: &mut String) -> Result<(), CapacityError> {
    prepare(output, source.len(), 2)?;
    for &point in source {
        output.push(char::from(point));
    }
    Ok(())
}

/// Convert scalar codepoints exactly; paired source surrogates remain invalid.
/// Scratch is private and must only be consumed after success. Each call is
/// bounded by the adapter's block contract; no Python callback is made here.
/// # Errors
/// Returns the first invalid source position or actual scratch-capacity failure.
pub fn codepoints<T: Copy + Into<u32>>(
    source: &[T],
    output: &mut String,
) -> Result<(), EncodeError> {
    prepare(output, source.len(), 4)?;
    for (index, point) in source.iter().copied().enumerate() {
        let point = point.into();
        let value = char::from_u32(point).ok_or(EncodeError::CodePoint { index, point })?;
        output.push(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::contains_ascii_control;

    #[test]
    fn control_scan_preserves_every_byte_and_empty_unicode_boundaries() {
        assert!(!contains_ascii_control(&[]));
        assert!(!contains_ascii_control("é中😀".as_bytes()));
        for byte in 0..=u8::MAX {
            let mut block = [b'a'; 33];
            block[16] = byte;
            assert_eq!(contains_ascii_control(&block), byte < 32);
        }
    }
}
