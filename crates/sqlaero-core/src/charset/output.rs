//! Prepared output encodings, including server-only Unicode metadata families.

use super::{
    CodecRoute, by_id,
    codec::{Codec, CodecError},
};

/// A column/output route. Server-only UTF families do not become valid client
/// connection charsets merely because their returned data can be decoded.
#[derive(Debug, Clone, Copy)]
pub enum OutputCodec {
    /// Native UTF-8 with a direct final-object path at the Python boundary.
    Utf8(Codec),
    /// Native ASCII with a direct final-object path at the Python boundary.
    Ascii(Codec),
    /// MySQL byte-valued text, distinct from binary-field Python bytes.
    ByteValues(Codec),
    /// One of the verified MySQL client codec mappings.
    Mysql(Codec),
    /// MySQL's BMP-only UTF-8 repertoire.
    Utf8mb3,
    /// Big-endian UTF-16, without implicit BOM removal.
    Utf16,
    /// Little-endian UTF-16, without implicit BOM removal.
    Utf16le,
    /// Big-endian UCS-2; surrogate code units are invalid.
    Ucs2,
    /// Big-endian UTF-32 scalar values.
    Utf32,
    /// An actual unrecognized collation; decoding must not guess UTF-8.
    Unknown(u16),
}
impl OutputCodec {
    /// Resolve metadata once. This does not relax connection input selection.
    #[must_use]
    pub fn for_collation(id: u16) -> Self {
        let Some((_, charset)) = by_id(id) else {
            return Self::Unknown(id);
        };
        match charset.route {
            CodecRoute::Utf8 => return Self::Utf8(Codec::new(charset)),
            CodecRoute::Ascii => return Self::Ascii(Codec::new(charset)),
            CodecRoute::Binary => return Self::ByteValues(Codec::new(charset)),
            CodecRoute::Unavailable => (),
            _ => return Self::Mysql(Codec::new(charset)),
        }
        match charset.name {
            "utf8mb3" => Self::Utf8mb3,
            "utf16" => Self::Utf16,
            "utf16le" => Self::Utf16le,
            "ucs2" => Self::Ucs2,
            "utf32" => Self::Utf32,
            _ => Self::Unknown(id),
        }
    }
    /// Canonical MySQL name used by precise Unicode content diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mysql(codec)
            | Self::Utf8(codec)
            | Self::Ascii(codec)
            | Self::ByteValues(codec) => codec.name(),
            Self::Utf8mb3 => "utf8mb3",
            Self::Utf16 => "utf16",
            Self::Utf16le => "utf16le",
            Self::Ucs2 => "ucs2",
            Self::Utf32 => "utf32",
            Self::Unknown(_) => "unknown MySQL collation",
        }
    }
    /// Read one scalar and its following byte position, without allocation.
    ///
    /// # Errors
    /// Rejects an actual invalid/incomplete sequence or unavailable encoding.
    pub fn point(self, data: &[u8], start: usize) -> Result<(char, usize), CodecError> {
        if let Self::Mysql(codec)
        | Self::Utf8(codec)
        | Self::Ascii(codec)
        | Self::ByteValues(codec) = self
        {
            return codec.decode_point(data, start);
        }
        if let Self::Unknown(_) = self {
            return Err(CodecError::Unavailable);
        }
        let fail = |width: usize| CodecError::Decode {
            start,
            end: start.saturating_add(width).min(data.len()),
        };
        if matches!(self, Self::Utf8mb3) {
            let byte = *data.get(start).ok_or_else(|| fail(0))?;
            let width = if byte < 128 {
                1
            } else {
                byte.leading_ones() as usize
            };
            if !(1..=3).contains(&width) {
                return Err(fail(1));
            }
            let end = start.saturating_add(width).min(data.len());
            let text = std::str::from_utf8(&data[start..end])
                .map_err(|error| fail(error.error_len().unwrap_or(end - start)))?;
            return Ok((text.chars().next().ok_or_else(|| fail(0))?, end));
        }
        if matches!(self, Self::Utf32) {
            let raw: [u8; 4] = data
                .get(start..start.saturating_add(4))
                .ok_or_else(|| fail(4))?
                .try_into()
                .map_err(|_| fail(4))?;
            return Ok((
                char::from_u32(u32::from_be_bytes(raw)).ok_or_else(|| fail(4))?,
                start + 4,
            ));
        }
        let word = |index: usize| -> Option<u16> {
            let pair: [u8; 2] = data.get(index..index.checked_add(2)?)?.try_into().ok()?;
            Some(if matches!(self, Self::Utf16le) {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            })
        };
        let first = word(start).ok_or_else(|| fail(2))?;
        let (value, width) = if (0xd800..=0xdbff).contains(&first) && !matches!(self, Self::Ucs2) {
            let second = word(start + 2)
                .filter(|value| (0xdc00..=0xdfff).contains(value))
                .ok_or_else(|| fail(2))?;
            (
                0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00),
                4,
            )
        } else {
            (u32::from(first), 2)
        };
        Ok((
            char::from_u32(value).ok_or_else(|| fail(width))?,
            start + width,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_unicode_is_direct_and_does_not_reopen_client_charsets() {
        assert_eq!(
            OutputCodec::Utf16
                .point(&[0xd8, 0x3d, 0xde, 0x00], 0)
                .unwrap(),
            ('😀', 4)
        );
        assert_eq!(
            OutputCodec::Utf16le.point(&[0x2d, 0x4e], 0).unwrap(),
            ('中', 2)
        );
        assert_eq!(
            OutputCodec::Utf32.point(&[0, 1, 0xf6, 0], 0).unwrap(),
            ('😀', 4)
        );
        assert!(OutputCodec::Ucs2.point(&[0xd8, 0x3d], 0).is_err());
        assert!(OutputCodec::Utf8mb3.point("😀".as_bytes(), 0).is_err());
        assert!(OutputCodec::Utf16.point(&[0xd8, 0, 0x41], 0).is_err());
        assert!(super::super::resolve(Some("utf16"), None).is_err());
    }
}
