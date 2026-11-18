//! Independent exact coefficient, scale and bounded rendering expectations.

use sqlaero_transcode::decimal::{Digits, Inspect, Layout, Notation, Render};

fn rendered(
    coefficient: &[u8],
    exponent: i64,
    negative: bool,
    notation: Notation,
    width: usize,
) -> String {
    let mut inspected = Inspect::default();
    for digit in coefficient {
        inspected.digit(*digit).unwrap();
    }
    let header = inspected.finish(negative, exponent).unwrap();
    let layout = Layout::new(header, notation).unwrap();
    let mut read = Render::new(Digits::Values(coefficient), layout).unwrap();
    let mut chunk = vec![0; width];
    let mut output = Vec::new();
    loop {
        let n = read.read(&mut chunk);
        if n == 0 {
            break;
        }
        assert!(n <= width);
        output.extend_from_slice(&chunk[..n]);
    }
    assert_eq!(output.len(), layout.len());
    String::from_utf8(output).unwrap()
}

#[test]
fn bounded_layout_preserves_actual_scale_sign_and_both_notations() {
    for width in [1, 2, 7, 4096] {
        for (digits, exponent, negative, plain, scientific) in [
            (&[1, 2, 5, 0, 0][..], -3, false, "12.500", "1.2500E+1"),
            (&[1, 0, 0][..], 1, false, "1000", "1.00E+3"),
            (&[0][..], -2, true, "-0.00", "-0E-2"),
            (&[0][..], 3, false, "0", "0E+3"),
            (
                &[0, 0, 1, 2, 3, 0, 0][..],
                -5,
                false,
                "0.12300",
                "1.2300E-1",
            ),
            (&[0, 0, 0][..], -7, false, "0.0000000", "0E-7"),
        ] {
            assert_eq!(
                rendered(digits, exponent, negative, Notation::Plain, width),
                plain
            );
            assert_eq!(
                rendered(digits, exponent, negative, Notation::Scientific, width),
                scientific
            );
        }
    }
}

#[test]
fn negative_scientific_keeps_every_part_across_tiny_windows() {
    for width in [1, 2, 128] {
        assert_eq!(
            rendered(&[1, 2, 5, 0, 0], -3, true, Notation::Scientific, width),
            "-1.2500E+1"
        );
    }
}

#[test]
fn original_ascii_split_is_borrowed_without_an_owned_digit_vector() {
    let mut inspected = Inspect::default();
    for digit in [0, 0, 1, 2, 5, 0, 0] {
        inspected.digit(digit).unwrap();
    }
    let header = inspected.finish(false, -3).unwrap();
    for split in 0..=7 {
        let text = b"0012500";
        let mut read = Render::new(
            Digits::Ascii(&text[..split], &text[split..]),
            Layout::new(header, Notation::Plain).unwrap(),
        )
        .unwrap();
        let mut output = [0; 32];
        let n = read.read(&mut output);
        assert_eq!(&output[..n], b"12.500");
        assert_eq!(read.read(&mut output), 0);
    }
    assert!(
        Render::new(
            Digits::Values(&[1]),
            Layout::new(header, Notation::Plain).unwrap()
        )
        .is_err()
    );
}

#[test]
fn source_inspection_does_not_apply_a_target_precision_limit() {
    let mut inspected = Inspect::default();
    for _ in 0..100_000 {
        inspected.digit(9).unwrap();
    }
    let header = inspected.finish(false, -100_000).unwrap();
    assert_eq!(header.digits(), 100_000);
    assert_eq!(header.exponent(), -100_000);
    assert_eq!(Layout::new(header, Notation::Plain).unwrap().len(), 100_002);
    assert!(Inspect::default().finish(false, 0).is_err());
    assert!(Inspect::default().digit(10).is_err());
}
