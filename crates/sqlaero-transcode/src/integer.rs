//! Bounded, fallible conversion of immutable native integer digits.
//!
//! The source owner and any interpreter export token stay with the adapter.
//! This module borrows only native bytes and returns control after a specified
//! number of digit operations, including the first source copy and reduction.

use std::collections::TryReserveError;

/// Native integer metadata or allocation failed before publication.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The supplied native representation is not internally consistent.
    #[error("invalid native integer representation")]
    Representation,
    /// The actual private working storage could not be allocated.
    #[error("native integer conversion allocation failed: {0}")]
    Allocation(#[from] TryReserveError),
}

/// A live, immutable magnitude in the source's original base-two layout.
#[derive(Clone, Copy)]
pub struct Digits<'a> {
    bytes: &'a [u8],
    bits: u8,
    width: u8,
    least_first: bool,
    little_endian: bool,
}

impl<'a> Digits<'a> {
    /// Number of bits occupied by each digit of the native radix.
    pub fn radix_bits(self) -> u8 {
        self.bits
    }

    /// Number of digits in the complete native magnitude.
    pub fn digit_count(self) -> usize {
        self.len()
    }

    /// Read one digit in least-significant-first order, zero extending it.
    /// # Errors
    /// Returns an error if a stored digit is outside the declared radix.
    pub fn digit(self, index: usize) -> Result<u32, Error> {
        if index < self.len() {
            self.word(index)
        } else {
            Ok(0)
        }
    }

    /// Read a canonical source which fits an unsigned native 128-bit value.
    /// The declared digit width bounds work before any source digit is read.
    /// # Errors
    /// Returns malformed source-layout errors; None means the canonical source
    /// cannot fit this native width. No decimal formatting or rounding occurs.
    pub fn unsigned128(self) -> Result<Option<u128>, Error> {
        if self.len() > 128_usize.div_ceil(usize::from(self.bits)) {
            return Ok(None);
        }
        let radix = 1_u128 << self.bits;
        let mut value = 0_u128;
        for index in (0..self.len()).rev() {
            let word = self.word(index)?;
            let Some(next) = value
                .checked_mul(radix)
                .and_then(|value| value.checked_add(u128::from(word)))
            else {
                return Ok(None);
            };
            value = next;
        }
        Ok(Some(value))
    }
    /// Admit layout facts without scanning or copying the source.
    /// # Errors
    /// Rejects unsupported widths, invalid digit bits or a partial digit.
    pub fn new(
        bytes: &'a [u8],
        bits: u8,
        width: u8,
        least_first: bool,
        little_endian: bool,
    ) -> Result<Self, Error> {
        if !matches!(width, 1 | 2 | 4 | 8)
            || bits == 0
            || bits > 32
            || bits > width * 8
            || !bytes.len().is_multiple_of(usize::from(width))
        {
            return Err(Error::Representation);
        }
        Ok(Self {
            bytes,
            bits,
            width,
            least_first,
            little_endian,
        })
    }

    fn len(self) -> usize {
        self.bytes.len() / usize::from(self.width)
    }

    fn word(self, index: usize) -> Result<u32, Error> {
        let index = if self.least_first {
            index
        } else {
            self.len() - 1 - index
        };
        let width = usize::from(self.width);
        let bytes = &self.bytes[index * width..(index + 1) * width];
        let mut word = [0; 8];
        let value = if self.little_endian {
            word[..width].copy_from_slice(bytes);
            u64::from_le_bytes(word)
        } else {
            word[8 - width..].copy_from_slice(bytes);
            u64::from_be_bytes(word)
        };
        if value >> self.bits != 0 {
            return Err(Error::Representation);
        }
        Ok(value as u32)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Copy,
    Divide,
    Trim,
    Done,
}

/// One private integer conversion, never a tree or a collection of source rows.
/// Decimal groups are exact, least-significant-first, in base 1,000,000,000.
pub struct DecimalDigits<'a> {
    source: Digits<'a>,
    words: Vec<u32>,
    groups: Vec<u32>,
    active: usize,
    cursor: usize,
    carry: u64,
    phase: Phase,
}

impl<'a> DecimalDigits<'a> {
    /// Prepare fallible private work space. The source is not yet copied.
    /// # Errors
    /// Propagates the actual working-storage allocation failure.
    pub fn new(source: Digits<'a>) -> Result<Self, Error> {
        let mut words = Vec::new();
        words.try_reserve_exact(source.len())?;
        // Each decimal group holds more than 29 binary bits. Reserve a
        // checked upper bound once, without scanning the source or growing
        // this buffer during the bounded division steps. Zero needs one group.
        let bits = source
            .len()
            .checked_mul(usize::from(source.bits))
            .ok_or(Error::Representation)?;
        let mut groups = Vec::new();
        groups.try_reserve_exact(bits.div_ceil(29).max(1))?;
        Ok(Self {
            source,
            words,
            groups,
            active: 0,
            cursor: 0,
            carry: 0,
            phase: Phase::Copy,
        })
    }

    /// Perform at most `work` source/reduction/trim digit operations.
    /// A zero allowance makes no progress and never spins internally.
    /// # Errors
    /// Returns a source-layout or actual allocation error; no partial answer
    /// becomes available through `groups` after an unsuccessful step.
    pub fn step(&mut self, mut work: usize) -> Result<bool, Error> {
        while work != 0 && self.phase != Phase::Done {
            match self.phase {
                Phase::Copy => {
                    if self.words.len() == self.source.len() {
                        self.active = self.words.len();
                        self.phase = Phase::Trim;
                    } else {
                        self.words.push(self.source.word(self.words.len())?);
                        work -= 1;
                    }
                }
                Phase::Divide => {
                    if self.cursor == 0 {
                        self.groups.push(self.carry as u32);
                        self.carry = 0;
                        self.phase = Phase::Trim;
                    } else {
                        self.cursor -= 1;
                        // carry < 1e9 and bits <= 32, so this fits in u64.
                        let value =
                            (self.carry << self.source.bits) | u64::from(self.words[self.cursor]);
                        self.words[self.cursor] = (value / 1_000_000_000) as u32;
                        self.carry = value % 1_000_000_000;
                        work -= 1;
                    }
                }
                Phase::Trim => {
                    if self.active != 0 && self.words[self.active - 1] == 0 {
                        self.active -= 1;
                        work -= 1;
                    } else if self.active == 0 {
                        if self.groups.is_empty() {
                            self.groups.push(0);
                        }
                        self.phase = Phase::Done;
                    } else {
                        self.cursor = self.active;
                        self.phase = Phase::Divide;
                    }
                }
                Phase::Done => (),
            }
        }
        Ok(self.phase == Phase::Done)
    }

    /// Borrow a complete exact answer only after conversion has finished.
    pub fn groups(&self) -> Option<&[u32]> {
        (self.phase == Phase::Done).then_some(self.groups.as_slice())
    }

    /// Release the source borrow and arithmetic workspace after conversion.
    /// No incomplete or failed conversion publishes decimal groups.
    pub fn into_groups(self) -> Option<Vec<u32>> {
        (self.phase == Phase::Done).then_some(self.groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_native_width_checks_the_declared_bound_before_reading_words() {
        let maximum = [0xff; 16];
        assert_eq!(
            Digits::new(&maximum, 8, 1, true, true)
                .unwrap()
                .unsigned128()
                .unwrap(),
            Some(u128::MAX)
        );
        let oversized = [1; 17];
        assert_eq!(
            Digits::new(&oversized, 8, 1, true, true)
                .unwrap()
                .unsigned128()
                .unwrap(),
            None
        );
        let malformed = [0, 1];
        assert!(
            Digits::new(&malformed, 8, 2, true, true)
                .unwrap()
                .unsigned128()
                .is_err()
        );
    }

    #[test]
    fn completed_groups_outlive_the_source_while_incomplete_state_cannot_publish() {
        let groups = {
            let source = [0x15, 0xcd, 0x5b, 0x07];
            let digits = Digits::new(&source, 8, 1, true, true).unwrap();
            assert!(DecimalDigits::new(digits).unwrap().into_groups().is_none());
            let mut conversion = DecimalDigits::new(digits).unwrap();
            while !conversion.step(1).unwrap() {}
            conversion.into_groups().unwrap()
        };
        assert_eq!(groups, [123_456_789]);
    }

    fn decimal(bytes: &[u8], bits: u8, width: u8, order: bool, endian: bool) -> String {
        let mut value =
            DecimalDigits::new(Digits::new(bytes, bits, width, order, endian).unwrap()).unwrap();
        assert!(!value.step(0).unwrap());
        assert!(value.groups().is_none());
        let capacity = value.groups.capacity();
        let mut calls = 0;
        while !value.step(1).unwrap() {
            calls += 1;
            assert!(calls < 10_000_000);
        }
        assert_eq!(
            value.groups.capacity(),
            capacity,
            "bounded work must not grow decimal groups"
        );
        let mut answer = String::new();
        for (index, group) in value.groups().unwrap().iter().rev().enumerate() {
            if index == 0 {
                answer.push_str(&group.to_string());
            } else {
                use std::fmt::Write;
                write!(answer, "{group:09}").unwrap();
            }
        }
        answer
    }

    #[test]
    fn one_unit_of_work_cannot_copy_or_convert_a_whole_large_source() {
        let bytes = [255_u8; 4096];
        let mut value = DecimalDigits::new(Digits::new(&bytes, 8, 1, true, true).unwrap()).unwrap();
        assert!(!value.step(1).unwrap());
        assert!(value.groups().is_none());
        assert!(!value.step(1).unwrap());
        assert!(value.groups().is_none());
    }

    #[test]
    fn integers_match_independent_fixed_width_formatting() {
        let mut state = 7_u128;
        for _ in 0..512 {
            state = state.wrapping_mul(0x123456789abcdef).wrapping_add(12345);
            assert_eq!(
                decimal(&state.to_le_bytes(), 8, 1, true, true),
                state.to_string()
            );
            assert_eq!(
                decimal(&state.to_be_bytes(), 8, 1, false, false),
                state.to_string()
            );
        }
    }

    #[test]
    fn source_layout_and_word_order_do_not_change_the_exact_value() {
        // In base 2^15, [32767, 1, 2] is exactly 2147549183.
        let words = [32767_u16, 1, 2];
        let little: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let big: Vec<u8> = words
            .iter()
            .rev()
            .flat_map(|word| word.to_be_bytes())
            .collect();
        assert_eq!(decimal(&little, 15, 2, true, true), "2147549183");
        assert_eq!(decimal(&big, 15, 2, false, false), "2147549183");
        assert_eq!(decimal(&[0, 0, 0, 0], 30, 4, true, true), "0");
        assert_eq!(decimal(&[], 30, 4, true, true), "0");
    }

    #[test]
    fn full_native_digits_match_independent_decimal_values_at_group_boundaries() {
        use num_bigint::BigUint;

        for (bits, width) in [
            (1, 1),
            (7, 1),
            (8, 1),
            (15, 2),
            (16, 2),
            (29, 4),
            (30, 4),
            (31, 4),
            (32, 4),
        ] {
            for count in [0, 1, 8, 29, 30, 31, 100] {
                let expected = if count == 0 {
                    "0".to_owned()
                } else {
                    ((BigUint::from(1_u8) << (usize::from(bits) * count)) - 1_u8).to_str_radix(10)
                };
                let word = (1_u64 << bits) - 1;
                for little_endian in [false, true] {
                    let bytes = if little_endian {
                        word.to_le_bytes()[..usize::from(width)].to_vec()
                    } else {
                        word.to_be_bytes()[8 - usize::from(width)..].to_vec()
                    };
                    let magnitude = bytes.repeat(count);
                    for least_first in [false, true] {
                        assert_eq!(
                            decimal(&magnitude, bits, width, least_first, little_endian),
                            expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn arbitrary_precision_uses_a_bounded_step_instead_of_python_string_limits() {
        use std::str::FromStr;
        let expected = format!("1{}123", "0".repeat(5997));
        let value = num_bigint::BigUint::from_str(&expected).unwrap();
        assert_eq!(decimal(&value.to_bytes_le(), 8, 1, true, true), expected);
    }

    #[test]
    fn invalid_metadata_or_out_of_layout_bits_are_rejected() {
        assert!(Digits::new(&[0], 0, 1, true, true).is_err());
        assert!(Digits::new(&[0], 9, 1, true, true).is_err());
        assert!(Digits::new(&[0], 8, 3, true, true).is_err());
        assert!(Digits::new(&[0], 15, 2, true, true).is_err());
        let mut invalid =
            DecimalDigits::new(Digits::new(&[255, 255], 15, 2, true, true).unwrap()).unwrap();
        assert!(invalid.step(1).is_err());
        assert!(invalid.groups().is_none());
    }
}
