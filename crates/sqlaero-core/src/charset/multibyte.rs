//! The selected mature encodings with factual MySQL-specific mapping deltas.

use super::{Multibyte, multibyte_delta};
use encoding_rs::{DecoderResult, EncoderResult, Encoding};

/// One encoded character, stored inline and never as a Python intermediate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoded {
    bytes: [u8; 4],
    length: usize,
}
impl Encoded {
    /// Borrow the initialized, exact-length encoded character.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    /// Pack the bytes as a stable build-time mapping-table key.
    #[must_use]
    pub fn key(&self) -> u32 {
        key(self.as_bytes())
    }
    fn from_key(value: u32) -> Self {
        let width = ((32 - value.leading_zeros()) as usize).div_ceil(8).max(1);
        let all = value.to_be_bytes();
        let mut bytes = [0; 4];
        bytes[..width].copy_from_slice(&all[4 - width..]);
        Self {
            bytes,
            length: width,
        }
    }
}

fn component(kind: Multibyte) -> &'static Encoding {
    match kind {
        Multibyte::Big5 => encoding_rs::BIG5,
        Multibyte::Cp932 | Multibyte::Sjis => encoding_rs::SHIFT_JIS,
        Multibyte::Eucjpms | Multibyte::Ujis => encoding_rs::EUC_JP,
        Multibyte::Euckr => encoding_rs::EUC_KR,
        Multibyte::Gb18030 => encoding_rs::GB18030,
        Multibyte::Gb2312 | Multibyte::Gbk => encoding_rs::GBK,
    }
}

/// Read one character through the pinned component, without MySQL deltas.
/// Used by the data compiler to preserve only verified necessary differences.
#[must_use]
pub fn base_decode(kind: Multibyte, bytes: &[u8]) -> Option<char> {
    let mut out = [0; 8];
    let (result, read, written) = component(kind)
        .new_decoder_without_bom_handling()
        .decode_to_utf8_without_replacement(bytes, &mut out, true);
    if result != DecoderResult::InputEmpty || read != bytes.len() {
        return None;
    }
    let text = std::str::from_utf8(&out[..written]).ok()?;
    let mut chars = text.chars();
    let point = chars.next()?;
    if chars.next().is_none() {
        Some(point)
    } else {
        None
    }
}

/// Encode one scalar through the pinned component, without MySQL deltas.
/// No replacement, numeric character reference or temporary heap is permitted.
#[must_use]
pub fn base_encode(kind: Multibyte, point: char) -> Option<Encoded> {
    let mut utf8 = [0; 4];
    let input = point.encode_utf8(&mut utf8);
    let mut out = [0; 8];
    let (result, read, written) = component(kind)
        .new_encoder()
        .encode_from_utf8_without_replacement(input, &mut out, true);
    if result != EncoderResult::InputEmpty || read != input.len() || !(1..=4).contains(&written) {
        return None;
    }
    let mut bytes = [0; 4];
    bytes[..written].copy_from_slice(&out[..written]);
    Some(Encoded {
        bytes,
        length: written,
    })
}

fn key(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |value, byte| value << 8 | u32::from(*byte))
}
fn lookup(entries: &[(u32, u32)], key: u32) -> Option<u32> {
    entries
        .binary_search_by_key(&key, |entry| entry.0)
        .ok()
        .map(|index| entries[index].1)
}

/// Decode one actual MySQL character, applying a fixed validity/mapping delta.
#[must_use]
pub fn decode(kind: Multibyte, bytes: &[u8]) -> Option<char> {
    if let Some(point) = lookup(multibyte_delta::DECODE[kind as usize], key(bytes)) {
        return char::from_u32(point);
    }
    base_decode(kind, bytes)
}

/// Encode a losslessly representable MySQL character. A component extension
/// is accepted only if the actual MySQL decoder maps it to this source value.
#[must_use]
pub fn encode(kind: Multibyte, point: char) -> Option<Encoded> {
    let encoded = match lookup(multibyte_delta::ENCODE[kind as usize], u32::from(point)) {
        Some(value) => Encoded::from_key(value),
        None => base_encode(kind, point)?,
    };
    (decode(kind, encoded.as_bytes()) == Some(point)).then_some(encoded)
}

/// Determine the current character's width from observed MySQL lead roles.
/// A bad lead consumes one byte; a missing suffix is a content error at decode.
#[must_use]
pub fn width(kind: Multibyte, input: &[u8]) -> usize {
    let Some(first) = input.first() else {
        return 0;
    };
    // GB18030's four-byte grammar covers entire ranges, including lead values
    // absent from a boundary sample. Width is a protocol rule, not a sample
    // coverage property; the decoder still rejects unassigned range values.
    if kind == Multibyte::Gb18030
        && (0x81..=0xFE).contains(first)
        && input.get(1).is_some_and(u8::is_ascii_digit)
    {
        return 4;
    }
    let mask = multibyte_delta::WIDTHS[kind as usize][usize::from(*first)];
    if mask & 2 != 0 {
        return 1;
    }
    if mask & 16 != 0 && input.get(1).is_some_and(u8::is_ascii_digit) {
        return 4;
    }
    if mask & 8 != 0 {
        return 3;
    }
    if mask & 4 != 0 {
        return 2;
    }
    1
}
