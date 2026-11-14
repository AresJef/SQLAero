//! Independent native expectations for the JSON target's minimal output chain.

use sqlaero_core::json::{Error, Options, Writer};
use sqlaero_transcode::value::{Consumer, ScalarRef};

#[test]
fn drained_prefix_keeps_container_and_open_string_state() {
    let mut writer = Writer::new();
    let options = Options::default();
    writer.array_start().unwrap();
    writer.text_start(None, &options).unwrap();
    writer.text_fragment("é\n", &options).unwrap();
    let mut published = writer.pending_output().unwrap().to_owned();
    writer.consume_output().unwrap();
    writer.text_fragment("tail", &options).unwrap();
    writer.text_end(&options).unwrap();
    writer.write(ScalarRef::Signed(7), &options).unwrap();
    writer.array_end().unwrap();
    published.push_str(&writer.finish_tail().unwrap());
    assert_eq!(published, "[\"é\\ntail\",7]");
}

#[test]
fn drained_writer_cannot_claim_its_tail_is_a_whole_document() {
    let mut writer = Writer::new();
    writer
        .write(ScalarRef::Signed(7), &Options::default())
        .unwrap();
    assert_eq!(writer.pending_output().unwrap(), "7");
    writer.consume_output().unwrap();
    assert!(writer.finish().is_err());
}

#[test]
fn failed_or_incomplete_stream_never_publishes_a_tail() {
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    writer.consume_output().unwrap();
    assert!(writer.finish_tail().is_err());
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    writer.consume_output().unwrap();
    assert!(
        writer
            .write(ScalarRef::Float(f64::NAN), &Options { allow_nan: false })
            .is_err()
    );
    assert!(writer.pending_output().is_err());
    assert!(writer.consume_output().is_err());
    assert!(writer.finish_tail().is_err());
}

#[test]
fn bounded_integer_blocks_remain_one_exact_number() {
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    writer.integer_start(true).unwrap();
    writer.integer_fragment(b"123").unwrap();
    writer.integer_fragment(b"000000007").unwrap();
    writer.integer_end().unwrap();
    writer
        .write(ScalarRef::Signed(8), &Options::default())
        .unwrap();
    writer.array_end().unwrap();
    assert_eq!(writer.finish().unwrap().as_bytes(), b"[-123000000007,8]");
}

#[test]
fn incomplete_or_invalid_integer_cannot_publish_or_close_an_array() {
    let mut writer = Writer::new();
    writer.integer_start(false).unwrap();
    assert!(writer.integer_end().is_err());
    assert!(writer.finish().is_err());
    for digits in [b"".as_slice(), b"01", b"1e9", b"1_0", b"+7"] {
        let mut writer = Writer::new();
        writer.integer_start(false).unwrap();
        assert!(writer.integer_fragment(digits).is_err());
        assert!(writer.finish().is_err());
    }
    let mut writer = Writer::new();
    writer.array_start().unwrap();
    writer.integer_start(false).unwrap();
    writer.integer_fragment(b"7").unwrap();
    assert!(writer.array_end().is_err());
    assert!(writer.finish().is_err());
    let mut writer = Writer::new();
    writer.integer_start(false).unwrap();
    writer.integer_fragment(b"0").unwrap();
    assert!(writer.integer_fragment(b"0").is_err());
    assert!(writer.finish().is_err());
}

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
        writer.finish().unwrap().as_bytes(),
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
        writer.finish().unwrap().as_bytes(),
        "\"\\\"\\\\\\u0000\\b\\t\\n\\f\\r\\u001f/é😀\"".as_bytes()
    );
}

#[test]
fn every_control_codepoint_and_adjacent_unicode_keep_exact_boundaries() {
    let mut input = String::from("é");
    for value in 0_u8..32 {
        input.push(char::from(value));
        input.push('😀');
    }
    let mut writer = Writer::new();
    writer
        .write(ScalarRef::Text(&input), &Options::default())
        .unwrap();
    let expected = concat!(
        "\"é",
        "\\u0000😀\\u0001😀\\u0002😀\\u0003😀\\u0004😀\\u0005😀\\u0006😀\\u0007😀",
        "\\b😀\\t😀\\n😀\\u000b😀\\f😀\\r😀\\u000e😀\\u000f😀",
        "\\u0010😀\\u0011😀\\u0012😀\\u0013😀\\u0014😀\\u0015😀\\u0016😀\\u0017😀",
        "\\u0018😀\\u0019😀\\u001a😀\\u001b😀\\u001c😀\\u001d😀\\u001e😀\\u001f😀\""
    );
    assert_eq!(writer.finish().unwrap(), expected);
}

#[test]
fn bounded_fragments_form_exactly_one_string() {
    let mut writer = Writer::new();
    let options = Options::default();
    writer.text_start(Some(6), &options).unwrap();
    writer.text_fragment("a", &options).unwrap();
    writer.text_fragment("😀\0", &options).unwrap();
    writer.text_end(&options).unwrap();
    assert_eq!(
        writer.finish().unwrap().as_bytes(),
        "\"a😀\\u0000\"".as_bytes()
    );
}

#[test]
fn unmeasured_utf8_fragments_keep_one_pass_source_conversion() {
    let mut writer = Writer::new();
    let options = Options::default();
    writer.text_start(None, &options).unwrap();
    writer.text_fragment("é", &options).unwrap();
    writer.text_fragment("😀", &options).unwrap();
    writer.text_end(&options).unwrap();
    assert_eq!(writer.finish().unwrap().as_bytes(), "\"é😀\"".as_bytes());
}

#[test]
fn all_nonfinite_floats_share_the_explicit_option() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut writer = Writer::new();
        writer
            .write(ScalarRef::Float(value), &Options::default())
            .unwrap();
        assert_eq!(writer.finish().unwrap().as_bytes(), b"null");
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
    writer.text_start(Some(2), &Options::default()).unwrap();
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
    let bytes = writer.finish().unwrap().into_bytes();
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
        writer.finish().unwrap().as_bytes(),
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
