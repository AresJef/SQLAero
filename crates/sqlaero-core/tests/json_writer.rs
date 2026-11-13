//! Independent native expectations for the JSON target's minimal output chain.

use sqlaero_core::json::{Error, Options, Writer};
use sqlaero_transcode::value::{Consumer, ScalarRef};

#[test]
fn scalar_values_keep_numeric_bits_and_json_categories() {
    let options = Options::default();
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    for value in [
        ScalarRef::Null,
        ScalarRef::Boolean(true),
        ScalarRef::Boolean(false),
        ScalarRef::Signed(i64::MIN),
        ScalarRef::Unsigned(u64::MAX),
        ScalarRef::Float(-0.0),
        ScalarRef::Float(1.5),
    ] {
        writer.write(value, &options).unwrap();
    }
    writer.array_end().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        b"[null,true,false,-9223372036854775808,18446744073709551615,-0.0,1.5]"
    );
}

#[test]
fn unicode_and_controls_are_escaped_without_ascii_replacement() {
    let mut writer = Writer::new();
    writer
        .write(
            ScalarRef::Text("\"\\\0\u{8}\t\n\u{c}\r\u{1f}/é😀"),
            &Options::default(),
        )
        .unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        "\"\\\"\\\\\\u0000\\b\\t\\n\\f\\r\\u001f/é😀\"".as_bytes()
    );
}

#[test]
fn bounded_fragments_form_exactly_one_string() {
    let mut writer = Writer::new();
    let options = Options::default();
    writer.text_start(6, &options).unwrap();
    writer.text_fragment("a", &options).unwrap();
    writer.text_fragment("😀\0", &options).unwrap();
    writer.text_end(&options).unwrap();
    assert_eq!(writer.finish().unwrap(), "\"a😀\\u0000\"".as_bytes());
}

#[test]
fn all_nonfinite_floats_share_the_explicit_option() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut writer = Writer::new();
        writer
            .write(ScalarRef::Float(value), &Options::default())
            .unwrap();
        assert_eq!(writer.finish().unwrap(), b"null");
        let mut writer = Writer::new();
        assert!(matches!(
            writer.write(ScalarRef::Float(value), &Options { allow_nan: false }),
            Err(Error::NonFinite)
        ));
        assert!(writer.finish().is_err());
    }
}

#[test]
fn incomplete_or_reused_root_cannot_produce_a_successful_document() {
    assert!(Writer::new().finish().is_err());
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    assert!(writer.finish().is_err());
    let mut writer = Writer::new();
    writer.write(ScalarRef::Null, &Options::default()).unwrap();
    assert!(writer.write(ScalarRef::Null, &Options::default()).is_err());
    assert!(writer.finish().is_err());
    let mut writer = Writer::new();
    writer.text_start(2, &Options::default()).unwrap();
    assert!(writer.text_fragment("😀", &Options::default()).is_err());
    assert!(writer.finish().is_err());
    let mut writer = Writer::new();
    writer.object_start().unwrap();
    writer.key("x").unwrap();
    assert!(writer.object_end().is_err());
    assert!(writer.finish().is_err());
}

#[test]
fn actual_output_container_depth_is_64() {
    let mut writer = Writer::new();
    for _ in 0..64 {
        writer.array_start().unwrap();
    }
    writer.write(ScalarRef::Null, &Options::default()).unwrap();
    for _ in 0..64 {
        writer.array_end().unwrap();
    }
    let bytes = writer.finish().unwrap();
    assert_eq!(bytes.len(), 132);
    assert_eq!(&bytes[64..68], b"null");
    let mut writer = Writer::new();
    for _ in 0..64 {
        writer.array_start().unwrap();
    }
    assert!(matches!(writer.array_start(), Err(Error::Depth)));
    assert!(writer.finish().is_err());
}

#[test]
fn object_names_keep_full_nul_suffixes_and_nested_empty_shapes() {
    let mut writer = Writer::new();
    let options = Options::default();
    writer.object_start().unwrap();
    writer.key("a\0x").unwrap();
    writer.write(ScalarRef::Signed(7), &options).unwrap();
    writer.key("a\0y").unwrap();
    writer.write(ScalarRef::Signed(8), &options).unwrap();
    writer.key("rows").unwrap();
    writer.array_start().unwrap();
    writer.object_start().unwrap();
    writer.object_end().unwrap();
    writer.array_start().unwrap();
    writer.array_end().unwrap();
    writer.array_end().unwrap();
    writer.object_end().unwrap();
    assert_eq!(
        writer.finish().unwrap(),
        br#"{"a\u0000x":7,"a\u0000y":8,"rows":[{},[]]}"#
    );
}

#[test]
fn binary_is_rejected_even_when_empty() {
    let mut writer = Writer::new();
    assert!(matches!(
        writer.write(ScalarRef::Bytes(b""), &Options::default()),
        Err(Error::Unsupported("binary"))
    ));
    assert!(writer.finish().is_err());
}
