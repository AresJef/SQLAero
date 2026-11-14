//! Independent UUID byte-order and width expectations.

use sqlaero_transcode::identifier::UuidText;

#[test]
fn exact_uuid_spelling_keeps_all_bits_and_canonical_hyphens() {
    assert_eq!(
        UuidText::new(0).as_str(),
        "00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(
        UuidText::new(u128::MAX).as_str(),
        "ffffffff-ffff-ffff-ffff-ffffffffffff"
    );
    assert_eq!(
        UuidText::new(0x0001_0203_0405_0607_0809_0a0b_0c0d_0e0f).as_str(),
        "00010203-0405-0607-0809-0a0b0c0d0e0f"
    );
    for byte in 0_u8..=255 {
        let text = UuidText::new(u128::from_be_bytes([byte; 16]));
        assert_eq!(text.as_str().len(), 36);
        assert!(
            text.as_str()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        );
    }
}
