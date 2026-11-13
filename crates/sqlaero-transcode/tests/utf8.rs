//! UTF-8 source facts are independent of Python storage and target syntax.

use sqlaero_transcode::utf8::{self, EncodeError};

#[test]
fn latin1_bytes_are_codepoints_not_a_preencoded_utf8_stream() {
    let mut buffer = String::new();
    utf8::latin1(&[0xc3, 0xa9, 0, 0xff], &mut buffer).unwrap();
    assert_eq!(buffer, "Ã©\0ÿ");
}

#[test]
fn complete_scalar_values_keep_their_exact_utf8() {
    let mut buffer = String::from("previous value");
    utf8::codepoints(&[0x61_u32, 0x4e2d, 0x1f600], &mut buffer).unwrap();
    assert_eq!(buffer, "a中😀");
    utf8::codepoints(&[0xe9_u16], &mut buffer).unwrap();
    assert_eq!(buffer, "é");
}

#[test]
fn actual_surrogates_are_rejected_even_when_paired() {
    let mut buffer = String::new();
    assert!(matches!(
        utf8::codepoints(&[0x61_u32, 0xd83d, 0xde00], &mut buffer),
        Err(EncodeError::CodePoint {
            index: 1,
            point: 0xd83d
        })
    ));
    assert!(matches!(
        utf8::codepoints(&[0x11_0000_u32], &mut buffer),
        Err(EncodeError::CodePoint {
            index: 0,
            point: 0x11_0000
        })
    ));
}
