//! Shared allocation-free numeric spelling, without a target's SQL/JSON policy.

/// The existing compact integer formatter; it borrows its fixed stack buffer.
pub use itoa::Buffer as IntegerBuffer;
/// Integer types admitted by the existing native formatter.
pub use itoa::Integer;

/// Return the exact two-digit ASCII spelling when that width suffices.
/// Larger values retain their target's existing general-width formatter.
#[inline]
pub fn two_digits(value: u8) -> Option<[u8; 2]> {
    (value < 100).then(|| [b'0' + value / 10, b'0' + value % 10])
}

/// One stack-owned shortest-roundtrip binary64 formatter.
pub struct FloatBuffer(zmij::Buffer);

impl Default for FloatBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatBuffer {
    /// Prepare only native stack storage, without an allocation.
    #[inline]
    pub fn new() -> Self {
        Self(zmij::Buffer::new())
    }

    /// Return the exact finite spelling or the source's nonfinite category.
    /// The target chooses null/rejection and any SQL numeric-category suffix.
    #[inline]
    pub fn format(&mut self, value: f64) -> Option<&str> {
        if value.is_finite() {
            Some(self.0.format_finite(value))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_digit_spelling_matches_standard_formatting_for_every_source_byte() {
        for value in 0_u8..=255 {
            let expected = format!("{value:02}");
            if let Some(actual) = two_digits(value) {
                assert_eq!(actual.as_slice(), expected.as_bytes());
            } else {
                assert!(expected.len() > 2);
            }
        }
    }

    #[test]
    fn finite_values_roundtrip_original_binary64_bits_including_negative_zero() {
        let mut formatter = FloatBuffer::new();
        for value in [
            0.0,
            -0.0,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            f64::MAX,
            -f64::MAX,
            0.1,
            16777217.0,
        ] {
            let parsed = formatter.format(value).unwrap().parse::<f64>().unwrap();
            assert_eq!(parsed.to_bits(), value.to_bits());
        }
        let mut bits = 7_u64;
        for _ in 0..4096 {
            bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = f64::from_bits(bits);
            if value.is_finite() {
                assert_eq!(
                    formatter
                        .format(value)
                        .unwrap()
                        .parse::<f64>()
                        .unwrap()
                        .to_bits(),
                    bits
                );
            } else {
                assert!(formatter.format(value).is_none());
            }
        }
    }

    #[test]
    fn nonfinite_facts_do_not_silently_choose_a_target_null_policy() {
        let mut formatter = FloatBuffer::new();
        for value in [f64::NAN, f64::INFINITY, -f64::INFINITY] {
            assert!(formatter.format(value).is_none());
        }
    }
}
