//! Independent literals and original temporal-component limit boundaries.

use sqlaero_core::bind::scalar::{self, Clock, Date};
use sqlaero_core::protocol::framing::WireError;

#[derive(Clone, Copy)]
enum Value {
    Date(Date, Option<Clock>),
    Time(Clock),
    Duration(i128),
}

struct Case {
    value: Value,
    expected: &'static [u8],
    components: &'static [usize],
}

impl Case {
    fn write(&self, output: &mut Vec<u8>, limit: usize) -> Result<(), WireError> {
        match self.value {
            Value::Date(date, clock) => scalar::date_time(output, date, clock, limit),
            Value::Time(clock) => scalar::time(output, clock, limit),
            Value::Duration(micros) => scalar::duration(output, micros, limit),
        }
    }
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            value: Value::Date(
                Date {
                    year: 0,
                    month: 1,
                    day: 1,
                },
                None,
            ),
            expected: b"'0000-01-01'",
            components: &[1, 4, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: 10000,
                    month: 1,
                    day: 1,
                },
                None,
            ),
            expected: b"'10000-01-01'",
            components: &[1, 5, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: -1,
                    month: 1,
                    day: 1,
                },
                None,
            ),
            expected: b"'00-1-01-01'",
            components: &[1, 4, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: i32::MIN,
                    month: 1,
                    day: 1,
                },
                None,
            ),
            expected: b"'-2147483648-01-01'",
            components: &[1, 11, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: i32::MAX,
                    month: 1,
                    day: 1,
                },
                None,
            ),
            expected: b"'2147483647-01-01'",
            components: &[1, 10, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: 0,
                    month: 12,
                    day: 31,
                },
                Some(Clock {
                    hour: 23,
                    minute: 59,
                    second: 59,
                    microsecond: 0,
                }),
            ),
            expected: b"'0000-12-31 23:59:59'",
            components: &[1, 4, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: 10000,
                    month: 1,
                    day: 1,
                },
                Some(Clock {
                    hour: 0,
                    minute: 0,
                    second: 0,
                    microsecond: 1,
                }),
            ),
            expected: b"'10000-01-01 00:00:00.000001'",
            components: &[1, 5, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: 2024,
                    month: 2,
                    day: 29,
                },
                Some(Clock {
                    hour: 12,
                    minute: 34,
                    second: 56,
                    microsecond: 100000,
                }),
            ),
            expected: b"'2024-02-29 12:34:56.100000'",
            components: &[1, 4, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Date(
                Date {
                    year: i32::MIN,
                    month: u8::MAX,
                    day: u8::MAX,
                },
                Some(Clock {
                    hour: u8::MAX,
                    minute: u8::MAX,
                    second: u8::MAX,
                    microsecond: u32::MAX,
                }),
            ),
            expected: b"'-2147483648-255-255 255:255:255.4294967295'",
            components: &[1, 11, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 10, 1],
        },
        Case {
            value: Value::Time(Clock {
                hour: 12,
                minute: 34,
                second: 56,
                microsecond: 1,
            }),
            expected: b"'12:34:56.000001'",
            components: &[1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Time(Clock {
                hour: 23,
                minute: 59,
                second: 59,
                microsecond: 999999,
            }),
            expected: b"'23:59:59.999999'",
            components: &[1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(i128::MIN),
            expected: b"'-47261439850130342147690917698:51:24.105728'",
            components: &[1, 1, 29, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(i128::MAX),
            expected: b"'47261439850130342147690917698:51:24.105727'",
            components: &[1, 29, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(0),
            expected: b"'00:00:00'",
            components: &[1, 2, 1, 2, 1, 2, 1],
        },
        Case {
            value: Value::Duration(-1),
            expected: b"'-00:00:00.000001'",
            components: &[1, 1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(100000),
            expected: b"'00:00:00.100000'",
            components: &[1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(999999),
            expected: b"'00:00:00.999999'",
            components: &[1, 2, 1, 2, 1, 2, 1, 6, 1],
        },
        Case {
            value: Value::Duration(-999999999 * 86400000000i128 + 1),
            expected: b"'-23999999975:59:59.999999'",
            components: &[1, 1, 11, 1, 2, 1, 2, 1, 6, 1],
        },
    ]
}

#[test]
fn temporal_literals_preserve_full_integer_domains_and_microseconds() {
    for case in cases() {
        let mut output = b"\x03SELECT ".to_vec();
        case.write(&mut output, usize::MAX).unwrap();
        assert_eq!(&output[..8], b"\x03SELECT ");
        assert_eq!(&output[8..], case.expected);
    }
}

#[test]
fn temporal_first_limit_error_keeps_original_component_boundaries() {
    for case in cases() {
        assert_eq!(case.components.iter().sum::<usize>(), case.expected.len());
        for prefix in [&b""[..], &b"\x03SELECT "[..]] {
            let full = prefix.len() + case.expected.len();
            for limit in 0..=full + 1 {
                let mut output = prefix.to_vec();
                let result = case.write(&mut output, limit);
                if limit >= full {
                    result.unwrap();
                    assert_eq!(&output[..prefix.len()], prefix);
                    assert_eq!(&output[prefix.len()..], case.expected);
                } else {
                    let mut required = prefix.len();
                    let expected = case
                        .components
                        .iter()
                        .find_map(|length| {
                            required += *length;
                            (required > limit).then_some(required)
                        })
                        .unwrap();
                    assert!(matches!(result, Err(WireError::Limit {
                        required, limit: actual_limit,
                    }) if required == expected && actual_limit == limit));
                }
            }
        }
    }
}
