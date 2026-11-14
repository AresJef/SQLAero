//! Native facts remain unchanged while each target selects its own policy.

use sqlaero_core::json::{Options, Writer};
use sqlaero_transcode::temporal::{self, TimeKind, TimeUnit, Value};
use sqlaero_transcode::value::{Consumer, ScalarRef};

fn json(value: ScalarRef<'_>) -> String {
    let mut writer = Writer::new();
    writer.write(value, &Options { allow_nan: true }).unwrap();
    writer.finish().unwrap()
}

#[test]
fn json_uses_full_tick_precision_and_keeps_calendar_periods_distinct() {
    for (ticks, unit, kind, expected) in [
        (
            -1,
            TimeUnit::Nanosecond,
            TimeKind::DateTime,
            "\"1969-12-31T23:59:59.999999999\"",
        ),
        (
            120_000,
            TimeUnit::Microsecond,
            TimeKind::DateTime,
            "\"1970-01-01T00:00:00.120000\"",
        ),
        (
            120_000_000,
            TimeUnit::Nanosecond,
            TimeKind::DateTime,
            "\"1970-01-01T00:00:00.120000\"",
        ),
        (
            123_456_900,
            TimeUnit::Nanosecond,
            TimeKind::DateTime,
            "\"1970-01-01T00:00:00.123456900\"",
        ),
        (
            -1,
            TimeUnit::Nanosecond,
            TimeKind::Duration,
            "\"-PT0.000000001S\"",
        ),
        (
            -1_123_456_900,
            TimeUnit::Nanosecond,
            TimeKind::Duration,
            "\"-PT1.1234569S\"",
        ),
        (86490, TimeUnit::Second, TimeKind::Duration, "\"P1DT90S\""),
        (0, TimeUnit::Second, TimeKind::Duration, "\"P0D\""),
        (0, TimeUnit::Month, TimeKind::Duration, "\"P0M\""),
        (1, TimeUnit::Year, TimeKind::Duration, "\"P1Y\""),
        (-14, TimeUnit::Month, TimeKind::Duration, "\"-P14M\""),
    ] {
        assert_eq!(
            json(ScalarRef::Ticks {
                value: ticks,
                unit,
                multiplier: 1,
                kind
            }),
            expected
        );
    }
}

#[test]
fn extended_clocks_keep_sign_and_total_hours_in_json_without_day_wrapping() {
    for (ticks, unit, expected) in [
        (-1, TimeUnit::Nanosecond, "\"-00:00:00.000000001\""),
        (86400, TimeUnit::Second, "\"24:00:00\""),
        (-90061, TimeUnit::Second, "\"-25:01:01\""),
        (123_450_000, TimeUnit::Nanosecond, "\"00:00:00.123450\""),
    ] {
        assert_eq!(
            json(ScalarRef::ClockTicks {
                value: ticks,
                unit,
                multiplier: 1
            }),
            expected
        );
    }
}

#[test]
fn shared_decomposition_has_no_provider_specific_nat_or_mysql_precision() {
    let native = temporal::decompose(-1, TimeUnit::Nanosecond, 1, TimeKind::DateTime).unwrap();
    assert_eq!(
        native,
        Value::Datetime {
            year: 1969,
            month: 12,
            day: 31,
            second: 86399,
            nanosecond: 999_999_999
        }
    );
    let lowered = sqlaero_core::bind::temporal::mysql_precision(native).unwrap();
    assert_eq!(
        lowered,
        Value::Datetime {
            year: 1969,
            month: 12,
            day: 31,
            second: 86399,
            nanosecond: 999_999_000
        }
    );
    assert_eq!(
        native,
        Value::Datetime {
            year: 1969,
            month: 12,
            day: 31,
            second: 86399,
            nanosecond: 999_999_999
        }
    );
    assert!(matches!(
        temporal::decompose(i64::MIN, TimeUnit::Nanosecond, 1, TimeKind::Duration).unwrap(),
        Value::Duration { .. }
    ));
}
