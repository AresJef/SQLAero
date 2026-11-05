//! Independent A-lane encoding and SQL-planning contracts.

use sqlaero_core::charset::{
    codec::{Codec, CodecError},
    resolve,
};

#[test]
fn explicit_utf8mb3_uses_its_canonical_default_and_never_changes_utf8_aliases() {
    let selected = resolve(Some(" UTF8MB3 "), None).unwrap();
    assert_eq!(
        (
            selected.charset.name,
            selected.collation.name,
            selected.collation.id
        ),
        ("utf8mb3", "utf8mb3_general_ci", 33)
    );
    assert_eq!(
        resolve(None, Some("UTF8MB3_BIN")).unwrap().charset.name,
        "utf8mb3"
    );
    assert_eq!(
        resolve(Some("utf8mb3"), Some("utf8mb3_unicode_ci"))
            .unwrap()
            .collation
            .id,
        192
    );
    assert_eq!(
        resolve(Some("utf-8"), None).unwrap().charset.name,
        "utf8mb4"
    );
    assert_eq!(resolve(None, None).unwrap().collation.id, 255);
    assert!(resolve(Some("utf8"), Some("utf8mb3_general_ci")).is_err());
    assert!(resolve(None, Some("utf8_general_ci")).is_err());
}

#[test]
fn utf8mb3_writes_bmp_and_sql_metacharacters_in_both_quoting_modes() {
    let codec = Codec::new(resolve(Some("utf8mb3"), None).unwrap().charset);
    let text = "\0中文'\\\u{ffff}";
    for (no_backslash, expected) in [
        (false, "'\\0中文\\'\\\\\u{ffff}'"),
        (true, "'\0中文''\\\u{ffff}'"),
    ] {
        let mut output = Vec::new();
        codec
            .literal(text, &mut output, no_backslash, 4096)
            .unwrap();
        assert_eq!(output, expected.as_bytes());
        assert_eq!(codec.decode(&output).unwrap(), expected);
    }
}

#[test]
fn utf8mb3_rejects_supplementary_characters_at_the_original_utf8_offset() {
    let codec = Codec::new(resolve(Some("utf8mb3"), None).unwrap().charset);
    for text in ["中\u{10000}", "中😀", "中\u{10ffff}"] {
        let mut output = Vec::new();
        assert!(matches!(
            codec.encode(text, &mut output, 4096),
            Err(CodecError::Encode { offset: 3, .. })
        ));
        let mut output = Vec::new();
        assert!(matches!(
            codec.literal(text, &mut output, false, 4096),
            Err(CodecError::Encode { offset: 3, .. })
        ));
        assert!(codec.decode(text.as_bytes()).is_err());
        assert!(codec.decode_point(text.as_bytes(), 3).is_err());
    }
    let mut output = Vec::new();
    let mb4 = Codec::new(resolve(Some("utf8"), None).unwrap().charset);
    mb4.encode("中😀", &mut output, 4096).unwrap();
    assert_eq!(output, "中😀".as_bytes());
}
