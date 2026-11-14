//! Fixed-width signed source coefficients, independent of target formatting.

use crate::value::ScalarRef;

/// An exact two's-complement coefficient up to 256 bits, with bounded stack
/// ownership. No Python scalar or intermediate decimal string is constructed.
pub struct Coefficient {
    negative: bool,
    digits: [u8; 78],
    start: usize,
}

impl Coefficient {
    /// Read an actual signed little-endian source coefficient.
    /// # Errors
    /// Rejects an empty input or storage wider than 256 bits.
    pub fn from_signed_le(source: &[u8]) -> Result<Self, &'static str> {
        if source.is_empty() || source.len() > 32 {
            return Err("Decimal coefficient must contain between 1 and 32 bytes");
        }
        let source = if source.len() > 16 {
            let fill = if source[15] & 0x80 == 0 { 0 } else { 0xff };
            if source[16..].iter().all(|byte| *byte == fill) {
                &source[..16]
            } else {
                source
            }
        } else {
            source
        };
        let negative = source[source.len() - 1] & 0x80 != 0;
        let mut value = Self {
            negative,
            digits: [0; 78],
            start: 78,
        };
        if source.len() <= 16 {
            let mut bytes = [if negative { 0xff } else { 0 }; 16];
            bytes[..source.len()].copy_from_slice(source);
            let coefficient = i128::from_le_bytes(bytes);
            let mut buffer = itoa::Buffer::new();
            let text = match i64::try_from(coefficient) {
                Ok(n) => buffer.format(n),
                Err(_) => buffer.format(coefficient),
            };
            let ascii = if negative {
                &text.as_bytes()[1..]
            } else {
                text.as_bytes()
            };
            value.start -= ascii.len();
            for (digit, ascii) in value.digits[value.start..].iter_mut().zip(ascii) {
                *digit = *ascii - b'0';
            }
        } else {
            value.wide(source);
        }
        Ok(value)
    }

    /// Borrow the complete coefficient with the source scale still intact.
    pub fn scalar(&self, scale: i8) -> ScalarRef<'_> {
        ScalarRef::Decimal {
            negative: self.negative,
            digits: &self.digits[self.start..],
            exponent: -i64::from(scale),
        }
    }

    /// Divide bounded native limbs by 10^19. The negative minimum is converted
    /// through unsigned two's complement, without overflowing signed abs.
    #[inline(never)]
    fn wide(&mut self, source: &[u8]) {
        let mut magnitude = [0u8; 32];
        magnitude[..source.len()].copy_from_slice(source);
        if self.negative {
            let mut carry = 1u16;
            for byte in &mut magnitude[..source.len()] {
                let n = u16::from(!*byte) + carry;
                *byte = n as u8;
                carry = n >> 8;
            }
        }
        let mut words = [0u64; 4];
        for (word, bytes) in words.iter_mut().zip(magnitude.as_chunks::<8>().0) {
            *word = u64::from_le_bytes(*bytes);
        }
        let mut active = words.len();
        while active != 0 && words[active - 1] == 0 {
            active -= 1;
        }
        if active == 0 {
            self.start -= 1;
        }
        while active != 0 {
            let mut remainder = 0u128;
            for word in words[..active].iter_mut().rev() {
                let n = (remainder << 64) | u128::from(*word);
                *word = (n / 10_000_000_000_000_000_000u128) as u64;
                remainder = n % 10_000_000_000_000_000_000u128;
            }
            while active != 0 && words[active - 1] == 0 {
                active -= 1;
            }
            let mut buffer = itoa::Buffer::new();
            let chunk = buffer.format(remainder as u64);
            let width = if active == 0 { chunk.len() } else { 19 };
            self.start -= width;
            let tail = self.start + width - chunk.len();
            for (digit, ascii) in self.digits[tail..self.start + width]
                .iter_mut()
                .zip(chunk.bytes())
            {
                *digit = ascii - b'0';
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn all_physical_widths_and_signed_extremes_match_independent_big_integer() {
        for width in 1..=32 {
            for choice in 0..6 {
                let mut bytes = [0u8; 32];
                let part = &mut bytes[..width];
                match choice {
                    0 => (),
                    1 => part[0] = 1,
                    2 => part.fill(0xff),
                    3 => part[width - 1] = 0x80,
                    4 => {
                        part.fill(0xff);
                        part[width - 1] = 0x7f;
                    }
                    _ => {
                        for (i, n) in part.iter_mut().enumerate() {
                            *n = (i * 37 + 19) as u8;
                        }
                    }
                }
                let expected = BigInt::from_signed_bytes_le(part).to_string();
                let actual = Coefficient::from_signed_le(part).unwrap();
                let ScalarRef::Decimal {
                    negative,
                    digits,
                    exponent,
                } = actual.scalar(-7)
                else {
                    unreachable!()
                };
                let mut text = String::new();
                if negative {
                    text.push('-');
                }
                for n in digits {
                    text.push(char::from(b'0' + *n));
                }
                assert_eq!(text, expected, "width={width}, choice={choice}");
                assert_eq!(exponent, 7);
            }
        }
    }

    #[test]
    fn invalid_storage_is_rejected_without_reading_it() {
        assert!(Coefficient::from_signed_le(&[]).is_err());
        assert!(Coefficient::from_signed_le(&[0; 33]).is_err());
    }
}
