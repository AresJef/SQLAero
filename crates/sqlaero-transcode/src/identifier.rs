//! Canonical identifier text derived solely from exact native bits.

/// A UUID's complete lowercase 8-4-4-4-12 ASCII spelling.
pub struct UuidText([u8; 36]);

impl UuidText {
    /// Format all 128 bits, without imposing a UUID version or nonzero policy.
    pub fn new(value: u128) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = [0; 36];
        let mut cursor = 0;
        for (index, byte) in value.to_be_bytes().into_iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                output[cursor] = b'-';
                cursor += 1;
            }
            output[cursor] = HEX[usize::from(byte >> 4)];
            output[cursor + 1] = HEX[usize::from(byte & 15)];
            cursor += 2;
        }
        Self(output)
    }

    /// Borrow the complete native text for immediate target consumption.
    pub fn as_str(&self) -> &str {
        // SAFETY: the private array is fully initialized exclusively with the
        // fixed ASCII hexadecimal alphabet and hyphens in new(). No mutable
        // access or constructor accepting arbitrary bytes is exposed.
        unsafe { std::str::from_utf8_unchecked(&self.0) }
    }
}
