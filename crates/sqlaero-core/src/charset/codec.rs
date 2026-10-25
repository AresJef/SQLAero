//! Strict conversion and SQL quoting under one prepared MySQL charset route.

use super::{Charset, CodecRoute, multibyte, single_byte};
use crate::bind::writer::append;
use crate::protocol::framing::WireError;
use std::borrow::Cow;

/// A precise content error or an actual native request-buffer failure.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// One Unicode scalar cannot be encoded. The offset is a UTF-8 byte index;
    /// the binding converts it to an original Python character index on error.
    #[error("character U+{point:04X} at UTF-8 byte {offset} cannot be encoded")]
    Encode {
        /// UTF-8 start offset in the original valid source.
        offset: usize,
        /// Unrepresentable Unicode scalar.
        point: u32,
    },
    /// A known charset rejects this precise input-byte interval.
    #[error("invalid encoded byte sequence at [{start},{end})")]
    Decode {
        /// First invalid byte.
        start: usize,
        /// End of the invalid/incomplete sequence.
        end: usize,
    },
    /// A route has not yet been installed or is excluded by policy.
    #[error("charset conversion route is unavailable")]
    Unavailable,
    /// A logical request or native allocation limit was encountered.
    #[error(transparent)]
    Wire(#[from] WireError),
}

/// A canonical route prepared once for a connection or column strategy.
#[derive(Debug, Clone, Copy)]
pub struct Codec {
    charset: &'static Charset,
}

impl Codec {
    /// Retain a static catalog route, without per-value imports or label parsing.
    #[must_use]
    pub const fn new(charset: &'static Charset) -> Self {
        Self { charset }
    }

    /// Return the canonical MySQL family used in content-error diagnostics.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.charset.name
    }

    /// Return a character width in bytes already produced by this strict codec.
    /// SQL lexical scanning uses it to keep multibyte trail punctuation inside
    /// its character. This is not a replacement for validating remote bytes.
    #[must_use]
    pub fn encoded_width(&self, input: &[u8]) -> usize {
        match self.charset.route {
            CodecRoute::Multibyte(kind) => multibyte::width(kind, input),
            CodecRoute::Utf8 => input.first().map_or(0, |byte| {
                if *byte < 128 {
                    1
                } else {
                    byte.leading_ones() as usize
                }
            }),
            _ => usize::from(!input.is_empty()),
        }
    }

    /// Test the actual MySQL whitespace/control byte class after --. This is
    /// lexical policy, prepared from fixed server observations, not a Unicode
    /// whitespace guess or an extra per-value/server check.
    #[must_use]
    pub fn comment_space(&self, byte: u8) -> bool {
        let masks: &[u64; 4] = match self.charset.route {
            CodecRoute::SingleByte(index) => &super::lexer_space::SINGLE[usize::from(index)],
            CodecRoute::Multibyte(kind) => &super::lexer_space::MULTI[kind as usize],
            CodecRoute::Utf8 => &super::lexer_space::BASIC[0],
            CodecRoute::Ascii => &super::lexer_space::BASIC[1],
            CodecRoute::Binary => &super::lexer_space::BASIC[2],
            CodecRoute::Unavailable => return false,
        };
        masks[usize::from(byte) / 64] & (1 << (byte % 64)) != 0
    }

    /// Encode SQL source text, without treating it as an SQL string literal.
    ///
    /// # Errors
    /// Returns an exact unmappable-character or request-buffer failure.
    pub fn encode(&self, text: &str, output: &mut Vec<u8>, limit: usize) -> Result<(), CodecError> {
        self.write::<0>(text, output, limit)
    }

    /// Encode and quote a SQL value under the actual backslash mode. The mode
    /// is selected outside the conversion loop; multibyte trail bytes must
    /// never be mistaken for independent quote/backslash characters.
    ///
    /// # Errors
    /// Returns an exact unmappable-character or request-buffer failure. The
    /// caller discards the incomplete current request rather than sending it.
    pub fn literal(
        &self,
        text: &str,
        output: &mut Vec<u8>,
        no_backslash: bool,
        limit: usize,
    ) -> Result<(), CodecError> {
        append(output, b"'", limit)?;
        self.literal_fragment(text, output, no_backslash, limit)?;
        append(output, b"'", limit)?;
        Ok(())
    }

    /// Write complete Unicode characters inside an already opened SQL literal.
    /// Binding uses bounded fragments to check deadlines/signals between work
    /// units; fragments never split a source scalar or an encoded character.
    ///
    /// # Errors
    /// Returns the same precise conversion or request-buffer failure as literal.
    pub fn literal_fragment(
        &self,
        text: &str,
        output: &mut Vec<u8>,
        no_backslash: bool,
        limit: usize,
    ) -> Result<(), CodecError> {
        if no_backslash {
            self.write::<2>(text, output, limit)?;
        } else {
            self.write::<1>(text, output, limit)?;
        }
        Ok(())
    }

    fn write<const MODE: u8>(
        &self,
        text: &str,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<(), CodecError> {
        if matches!(self.charset.route, CodecRoute::Utf8) {
            return escaped::<MODE>(text.as_bytes(), output, limit).map_err(Into::into);
        }
        if let CodecRoute::Multibyte(kind) = self.charset.route {
            for (offset, point) in text.char_indices() {
                let encoded = multibyte::encode(kind, point).ok_or(CodecError::Encode {
                    offset,
                    point: u32::from(point),
                })?;
                if encoded.as_bytes().len() == 1 {
                    escaped::<MODE>(encoded.as_bytes(), output, limit)?;
                } else {
                    // A complete verified character is indivisible: a trail
                    // byte equal to 0x5C must not acquire a second backslash.
                    append(output, encoded.as_bytes(), limit)?;
                }
            }
            return Ok(());
        }
        for (offset, point) in text.char_indices() {
            let byte = match self.charset.route {
                CodecRoute::Ascii if point.is_ascii() => point as u8,
                CodecRoute::Binary if u32::from(point) <= 255 => point as u8,
                CodecRoute::SingleByte(index) => {
                    let index = usize::from(index);
                    if point.is_ascii() && single_byte::ASCII_IDENTITY[index] {
                        point as u8
                    } else {
                        let entries = single_byte::ENCODE[index];
                        let found = entries
                            .binary_search_by_key(&u32::from(point), |entry| entry.0)
                            .map_err(|_| CodecError::Encode {
                                offset,
                                point: u32::from(point),
                            })?;
                        entries[found].1
                    }
                }
                CodecRoute::Multibyte(_) | CodecRoute::Unavailable => {
                    return Err(CodecError::Unavailable);
                }
                _ => {
                    return Err(CodecError::Encode {
                        offset,
                        point: u32::from(point),
                    });
                }
            };
            escaped::<MODE>(&[byte], output, limit)?;
        }
        Ok(())
    }

    /// Decode complete known-charset bytes strictly. Valid UTF-8 is borrowed;
    /// another charset owns only the necessary native transcoding buffer.
    ///
    /// # Errors
    /// Returns a precise invalid byte range or actual native allocation failure.
    pub fn decode<'a>(&self, bytes: &'a [u8]) -> Result<Cow<'a, str>, CodecError> {
        if matches!(self.charset.route, CodecRoute::Utf8) {
            return std::str::from_utf8(bytes)
                .map(Cow::Borrowed)
                .map_err(|error| CodecError::Decode {
                    start: error.valid_up_to(),
                    end: error.valid_up_to()
                        + error
                            .error_len()
                            .unwrap_or(bytes.len() - error.valid_up_to()),
                });
        }
        let mut output = String::new();
        output.try_reserve(bytes.len()).map_err(WireError::from)?;
        let mut start = 0;
        while start < bytes.len() {
            let (point, end) = self.decode_point(bytes, start)?;
            output
                .try_reserve(point.len_utf8())
                .map_err(WireError::from)?;
            output.push(point);
            start = end;
        }
        Ok(Cow::Owned(output))
    }

    /// Decode exactly one native character without constructing a String.
    /// Output materialization reuses the same verified MySQL mappings while
    /// charging only the actual bounded transcoding buffer it needs.
    ///
    /// # Errors
    /// Returns the first invalid interval in the original byte slice.
    pub fn decode_point(&self, bytes: &[u8], start: usize) -> Result<(char, usize), CodecError> {
        let byte = *bytes
            .get(start)
            .ok_or(CodecError::Decode { start, end: start })?;
        if let CodecRoute::Multibyte(kind) = self.charset.route {
            let width = multibyte::width(kind, &bytes[start..]);
            let end = start.saturating_add(width).min(bytes.len());
            if end - start != width {
                return Err(CodecError::Decode { start, end });
            }
            let point = multibyte::decode(kind, &bytes[start..end])
                .ok_or(CodecError::Decode { start, end })?;
            return Ok((point, end));
        }
        if let CodecRoute::Utf8 = self.charset.route {
            let width = if byte < 128 {
                1
            } else {
                byte.leading_ones() as usize
            };
            if !(1..=4).contains(&width) {
                return Err(CodecError::Decode {
                    start,
                    end: start + 1,
                });
            }
            let end = start.saturating_add(width).min(bytes.len());
            let point = std::str::from_utf8(&bytes[start..end])
                .map_err(|error| CodecError::Decode {
                    start,
                    end: start + error.error_len().unwrap_or(end - start),
                })?
                .chars()
                .next()
                .ok_or(CodecError::Decode { start, end })?;
            return Ok((point, end));
        }
        let point = match self.charset.route {
            CodecRoute::Ascii if byte < 128 => Some(char::from(byte)),
            CodecRoute::Binary => Some(char::from(byte)),
            CodecRoute::SingleByte(index) => {
                char::from_u32(single_byte::DECODE[usize::from(index)][usize::from(byte)])
            }
            CodecRoute::Unavailable => {
                return Err(CodecError::Unavailable);
            }
            _ => None,
        }
        .ok_or(CodecError::Decode {
            start,
            end: start + 1,
        })?;
        Ok((point, start + 1))
    }
}

fn escape<const MODE: u8>(byte: u8) -> Option<&'static [u8]> {
    if MODE == 0 {
        return None;
    }
    if MODE == 2 {
        return if byte == b'\'' { Some(b"''") } else { None };
    }
    match byte {
        0 => Some(b"\\0"),
        b'\n' => Some(b"\\n"),
        b'\r' => Some(b"\\r"),
        0x1a => Some(b"\\Z"),
        b'\\' => Some(b"\\\\"),
        b'\'' => Some(b"\\'"),
        b'"' => Some(b"\\\""),
        _ => None,
    }
}

fn escaped<const MODE: u8>(
    bytes: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
) -> Result<(), WireError> {
    if MODE == 0 {
        return append(output, bytes, limit);
    }
    let mut start = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if let Some(encoded) = escape::<MODE>(byte) {
            append(output, &bytes[start..index], limit)?;
            append(output, encoded, limit)?;
            start = index + 1;
        }
    }
    append(output, &bytes[start..], limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charset::resolve;
    #[test]
    fn native_utf8_quotes_actual_sql_metacharacters_without_touching_patterns() {
        let codec = Codec::new(resolve(None, None).unwrap().charset);
        let mut output = Vec::new();
        codec
            .literal("中'\\\0%_\n", &mut output, false, 100)
            .unwrap();
        assert_eq!(output, "'中\\'\\\\\\0%_\\n'".as_bytes());
        output.clear();
        codec
            .literal("中'\\\0%_\n", &mut output, true, 100)
            .unwrap();
        assert_eq!(output, "'中''\\\0%_\n'".as_bytes());
    }
    #[test]
    fn swedish_national_replacements_use_encoded_not_unicode_metacharacters() {
        let codec = Codec::new(resolve(Some("swe7"), None).unwrap().charset);
        let mut output = Vec::new();
        codec.literal("Ö", &mut output, false, 100).unwrap();
        assert_eq!(output, b"'\\\\'");
        assert_eq!(codec.decode(b"\\").unwrap(), "Ö");
        assert!(matches!(
            codec.encode("\\", &mut Vec::new(), 100),
            Err(CodecError::Encode { .. })
        ));
    }
}
