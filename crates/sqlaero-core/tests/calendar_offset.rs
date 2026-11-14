//! Independent calendar and target-policy boundaries for explicit offsets.

use sqlaero_core::bind::temporal::{MysqlTemporal, components_utc};
use sqlaero_core::bind::{
    Context,
    value::{TextSink, ValueError, ValueRef, ValueSink},
};
use sqlaero_core::charset::{self, codec::Codec};

#[test]
fn explicit_offset_moves_at_most_one_valid_calendar_day_without_narrowing_years() {
    for (parts, offset, expected) in [
        (
            [b"2024".as_slice(), b"1", b"2", b"3", b"4", b"5"],
            28800,
            "'2024-01-01 19:04:05'",
        ),
        (
            [b"2024".as_slice(), b"1", b"2", b"3", b"4", b"5"],
            -18000,
            "'2024-01-02 08:04:05'",
        ),
        (
            [b"2024".as_slice(), b"3", b"1", b"0", b"0", b"0"],
            1,
            "'2024-02-29 23:59:59'",
        ),
        (
            [b"1900".as_slice(), b"3", b"1", b"0", b"0", b"0"],
            1,
            "'1900-02-28 23:59:59'",
        ),
        (
            [b"2000".as_slice(), b"2", b"29", b"23", b"59", b"59"],
            -1,
            "'2000-03-01 00:00:00'",
        ),
        (
            [b"10000".as_slice(), b"1", b"1", b"0", b"0", b"0"],
            1,
            "'9999-12-31 23:59:59'",
        ),
        (
            [b"0".as_slice(), b"1", b"1", b"0", b"0", b"0"],
            1,
            "'-0001-12-31 23:59:59'",
        ),
    ] {
        let mut output = Vec::new();
        components_utc(parts, offset)
            .unwrap()
            .write_literal(&mut output, 1024)
            .unwrap();
        assert_eq!(output, expected.as_bytes());
    }
}

#[test]
fn explicit_zero_still_validates_actual_calendar_and_checked_year_capacity() {
    for parts in [
        [b"1900".as_slice(), b"2", b"29", b"0", b"0", b"0"],
        [b"2024".as_slice(), b"13", b"1", b"0", b"0", b"0"],
        [b"2024".as_slice(), b"1", b"1", b"24", b"0", b"0"],
        [b"2024".as_slice(), b"1", b"1", b"0", b"0", b"60"],
        [
            b"170141183460469231731687303715884105728".as_slice(),
            b"1",
            b"1",
            b"0",
            b"0",
            b"0",
        ],
    ] {
        assert!(components_utc(parts, 0).is_err());
    }
    let parts = [
        b"170141183460469231731687303715884105727".as_slice(),
        b"12",
        b"31",
        b"23",
        b"59",
        b"59",
    ];
    assert!(components_utc(parts, -1).is_err());
    let parts = [b"2024".as_slice(), b"1", b"1", b"0", b"0", b"0"];
    for offset in [-86400, 86400, i32::MIN, i32::MAX] {
        assert!(components_utc(parts, offset).is_err());
    }
}

#[test]
fn no_offset_keeps_server_validation_but_explicit_offset_requires_confirmed_utc() {
    let context = Context {
        codec: Codec::new(charset::resolve(None, None).unwrap().charset),
        no_backslash_escapes: false,
        utc_confirmed: false,
        limit: 1024,
    };
    let parts: [&[u8]; 6] = [b"2024", b"13", b"32", b"25", b"60", b"61"];
    let mut output = Vec::new();
    TextSink::new(&mut output)
        .write(
            ValueRef::CalendarComponents {
                parts: &parts,
                offset_seconds: None,
            },
            &context,
        )
        .unwrap();
    assert_eq!(output, b"'2024-13-32 25:60:61'");
    let previous = output.clone();
    assert!(matches!(
        TextSink::new(&mut output).write(
            ValueRef::CalendarComponents {
                parts: &parts,
                offset_seconds: Some(0),
            },
            &context
        ),
        Err(ValueError::UtcRequired)
    ));
    assert_eq!(output, previous);
}
