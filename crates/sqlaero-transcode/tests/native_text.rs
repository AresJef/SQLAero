//! UTF-8 ownership and byte-capacity expectations independent of target format.

use sqlaero_transcode::buffer::{CapacityError, append_text};

#[test]
fn logical_byte_limit_preserves_the_existing_unicode_owner() {
    let mut text = String::with_capacity(128);
    append_text(&mut text, "é", 2).unwrap();
    assert!(matches!(
        append_text(&mut text, "😀", 5),
        Err(CapacityError::Limit {
            required: 6,
            limit: 5
        })
    ));
    assert_eq!(text, "é");
    append_text(&mut text, "😀", 6).unwrap();
    assert_eq!(text.as_bytes(), "é😀".as_bytes());
}
