//! Fixed Decimal grammar, width and signed-value expectations at short and long wire widths.

use sqlaero_core::result::decode;

const SIXTY_FIVE_NINES: &str = concat!(
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "99999",
);
const SIXTY_SIX_NINES: &str = concat!(
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "9999999999",
    "999999",
);
const SCALE_SIXTY_FIVE: &str = concat!(
    "0.",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000",
    "1",
);
const SCALE_SIXTY_SIX: &str = concat!(
    "0.",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "0000000000",
    "00000",
    "1",
);

#[test]
fn decimal_width_dispatch_preserves_the_exact_value_domain() {
    assert_eq!(SIXTY_FIVE_NINES.len(), 65);
    assert_eq!(SIXTY_SIX_NINES.len(), 66);
    for text in [
        SIXTY_FIVE_NINES.to_owned(),
        format!("+{SIXTY_FIVE_NINES}"),
        format!("-{SIXTY_FIVE_NINES}"),
        format!("0{SIXTY_FIVE_NINES}"),
        format!("{SIXTY_FIVE_NINES}.000"),
        SCALE_SIXTY_FIVE.to_owned(),
        format!("-{SCALE_SIXTY_FIVE}"),
        concat!(
            "-0.",
            "0000000000",
            "0000000000",
            "0000000000",
            "0000000000",
            "0000000000",
            "0000000000",
            "00000",
        )
        .to_owned(),
    ] {
        assert!(decode::decimal(text.as_bytes()).is_ok(), "{text}");
    }
    for text in [SIXTY_SIX_NINES, SCALE_SIXTY_SIX] {
        assert_eq!(
            decode::decimal(text.as_bytes()).unwrap_err().0,
            "MySQL Decimal exceeds 65 necessary decimal places",
        );
    }
}

#[test]
fn decimal_short_and_long_paths_preserve_original_grammar_error_priority() {
    for data in [&b""[..], &b"+"[..], &b"-"[..], &b"1."[..]] {
        assert_eq!(
            decode::decimal(data).unwrap_err().0,
            "incomplete MySQL Decimal text",
        );
    }
    for data in [
        &b".1"[..],
        &b"."[..],
        &b"1..0"[..],
        &b"1e2"[..],
        &b"NaN"[..],
        &b"sNaN"[..],
        &b"Infinity"[..],
        &b"1\xff"[..],
        &b" 1"[..],
        &b"1 "[..],
        &b"--1"[..],
        &b"1+"[..],
    ] {
        assert_eq!(
            decode::decimal(data).unwrap_err().0,
            "invalid MySQL Decimal text",
        );
    }
    // A malformed final byte wins over an otherwise oversized coefficient.
    // These raw lengths straddle the short/long selection boundary.
    for valid_digits in [64, 65, 66] {
        let mut text = vec![b'9'; valid_digits];
        text.push(b'x');
        assert_eq!(
            decode::decimal(&text).unwrap_err().0,
            "invalid MySQL Decimal text",
        );
        text.pop();
        text.push(b'.');
        assert_eq!(
            decode::decimal(&text).unwrap_err().0,
            "incomplete MySQL Decimal text",
        );
    }
}

#[test]
fn decimal_validation_keeps_signed_zero_and_high_scale_float_results() {
    for (data, expected) in [
        (&b"-0.000"[..], -0.0_f64),
        (&b"+0.000"[..], 0.0_f64),
        (&b"1.2300"[..], 1.23_f64),
        (SCALE_SIXTY_FIVE.as_bytes(), 1e-65_f64),
    ] {
        decode::decimal(data).unwrap();
        assert_eq!(decode::float(data).unwrap().to_bits(), expected.to_bits());
    }
}
