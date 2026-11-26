//! Independent expectations for two targets consuming the same original facts.

use sqlaero_core::bind::{
    Context,
    value::{TextSink, ValueError},
};
use sqlaero_core::charset::{self, codec::Codec};
use sqlaero_core::json::{Options, Writer};
use sqlaero_core::protocol::prepared::Parameters;
use sqlaero_transcode::temporal::{Clock, Date};
use sqlaero_transcode::value::{Consumer, ScalarRef};

fn context() -> Context {
    Context {
        codec: Codec::new(charset::resolve(None, None).unwrap().charset),
        no_backslash_escapes: false,
        ansi_quotes: false,
        utc_confirmed: true,
        limit: 4096,
    }
}

#[test]
fn the_same_original_aware_fact_has_target_specific_policy() {
    let source = ScalarRef::Datetime {
        date: Date {
            year: 2026,
            month: 1,
            day: 1,
        },
        clock: Clock {
            hour: 0,
            minute: 1,
            second: 2,
            microsecond: 3,
        },
        offset_micros: Some(3_600_000_000),
    };
    let mut sql = Vec::new();
    TextSink::new(&mut sql).write(source, &context()).unwrap();
    assert_eq!(sql, b"'2025-12-31 23:01:02.000003'");
    let mut json = Writer::new();
    json.write(source, &Options::default()).unwrap();
    assert_eq!(
        json.finish().unwrap(),
        "\"2026-01-01T00:01:02.000003+01:00\""
    );
    let mut parameters = Parameters::new(1, 1, 4096).unwrap();
    parameters.write(source, &context()).unwrap();
    let payload = parameters.finish().unwrap();
    // One parameter: fixed command/header(14), followed by binary datetime.
    assert_eq!(&payload[14..], &[11, 0xe9, 7, 12, 31, 23, 1, 2, 3, 0, 0, 0]);
}

#[test]
fn only_mysql_target_requires_the_authoritative_utc_session() {
    let mut context = context();
    context.utc_confirmed = false;
    for offset in [Some(0), Some(-3_600_000_000)] {
        let source = ScalarRef::Datetime {
            date: Date {
                year: 2026,
                month: 1,
                day: 2,
            },
            clock: Clock {
                hour: 3,
                minute: 0,
                second: 0,
                microsecond: 0,
            },
            offset_micros: offset,
        };
        assert!(matches!(
            TextSink::new(&mut Vec::new()).write(source, &context),
            Err(ValueError::UtcRequired)
        ));
        assert!(matches!(
            Parameters::new(1, 1, 4096).unwrap().write(source, &context),
            Err(ValueError::UtcRequired)
        ));
        let mut json = Writer::new();
        json.write(source, &Options::default()).unwrap();
        assert!(json.finish().is_ok());
    }
}

#[test]
fn year_width_negative_duration_and_offset_seconds_remain_exact() {
    for (source, expected) in [
        (
            ScalarRef::Date(Date {
                year: -1,
                month: 1,
                day: 2,
            }),
            "\"-0001-01-02\"",
        ),
        (
            ScalarRef::Date(Date {
                year: 10000,
                month: 1,
                day: 2,
            }),
            "\"10000-01-02\"",
        ),
        (ScalarRef::Duration(-86_399_999_999), "\"-PT86399.999999S\""),
        (
            ScalarRef::Time {
                clock: Clock {
                    hour: 3,
                    minute: 0,
                    second: 0,
                    microsecond: 0,
                },
                offset_micros: Some(-30_500_000),
            },
            "\"03:00:00-00:00:30.500000\"",
        ),
    ] {
        let mut json = Writer::new();
        json.write(source, &Options::default()).unwrap();
        assert_eq!(json.finish().unwrap(), expected);
    }
}
