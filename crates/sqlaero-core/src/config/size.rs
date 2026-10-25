//! Exact byte-size parsing shared by all public memory and packet limits.

use std::fmt;

/// Why a textual byte size cannot be represented as a native size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeError {
    /// The input does not begin with an unsigned decimal integer.
    InvalidNumber,
    /// The suffix is not one of the supported binary unit names.
    UnknownUnit,
    /// The integer or its product with the unit exceeds `usize`.
    Overflow,
}

impl fmt::Display for SizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidNumber => "size must start with an unsigned decimal integer",
            Self::UnknownUnit => "size unit must be B, K/KB/KiB, M/MB/MiB or G/GB/GiB",
            Self::Overflow => "size exceeds the native integer range",
        })
    }
}

impl std::error::Error for SizeError {}

/// Parse an integer byte count, optionally followed by a binary size unit.
///
/// Surrounding whitespace and whitespace between the number and unit are
/// accepted. Unit names are ASCII case-insensitive. K, M and G always mean
/// powers of 1024; decimals, signs and unknown suffixes are rejected.
///
/// This parser accepts zero. Each configuration field separately enforces its
/// positive range and upper bound, without allocating the requested capacity.
///
/// # Errors
/// Returns [`SizeError`] for invalid syntax, an unknown unit or integer overflow.
///
/// # Examples
/// ```
/// use sqlaero_core::config::size::parse_size;
/// assert_eq!(parse_size("16 KiB"), Ok(16_384));
/// ```
pub fn parse_size(input: &str) -> Result<usize, SizeError> {
    let input = input.trim();
    let number_end = input.bytes().take_while(u8::is_ascii_digit).count();
    if number_end == 0 {
        return Err(SizeError::InvalidNumber);
    }
    let number = input[..number_end]
        .parse::<usize>()
        .map_err(|_| SizeError::Overflow)?;
    let unit = input[number_end..].trim();
    let multiplier = if unit.is_empty() || unit.eq_ignore_ascii_case("B") {
        1
    } else if ["K", "KB", "KiB"]
        .iter()
        .any(|item| unit.eq_ignore_ascii_case(item))
    {
        1_024
    } else if ["M", "MB", "MiB"]
        .iter()
        .any(|item| unit.eq_ignore_ascii_case(item))
    {
        1_048_576
    } else if ["G", "GB", "GiB"]
        .iter()
        .any(|item| unit.eq_ignore_ascii_case(item))
    {
        1_073_741_824
    } else {
        return Err(SizeError::UnknownUnit);
    };
    number.checked_mul(multiplier).ok_or(SizeError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::{SizeError, parse_size};

    #[test]
    fn supported_units_are_exact_and_case_insensitive() {
        for input in ["16K", "16KB", "16KiB", "  16  kIb \t"] {
            assert_eq!(parse_size(input), Ok(16_384));
        }
        assert_eq!(parse_size("64MB"), Ok(67_108_864));
        assert_eq!(parse_size("1G"), Ok(1_073_741_824));
        assert_eq!(parse_size("0"), Ok(0));
        assert_eq!(parse_size("16384 B"), Ok(16_384));
    }

    #[test]
    fn malformed_and_overflowing_sizes_are_rejected() {
        for input in ["", "-1", "+1", ".5MB"] {
            assert_eq!(parse_size(input), Err(SizeError::InvalidNumber));
        }
        for input in ["1.5MB", "1e3", "1TB", "1 K B", "1\0B"] {
            assert_eq!(parse_size(input), Err(SizeError::UnknownUnit));
        }
        assert_eq!(
            parse_size(&format!("{}K", usize::MAX)),
            Err(SizeError::Overflow)
        );
        assert_eq!(
            parse_size("999999999999999999999999999999999"),
            Err(SizeError::Overflow)
        );
    }
}
