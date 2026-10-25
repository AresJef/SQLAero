//! Compare product conversion with complete separately recorded SQL observations.

use sqlaero_core::charset::{codec::Codec, resolve};
use std::path::PathBuf;

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).unwrap())
        .collect()
}

#[test]
#[ignore = "requires the saved SQLAERO_TEST_CHARSETS observation directory"]
fn strict_native_routes_match_all_observed_multibyte_values_and_canonical_encodings() {
    let root = PathBuf::from(std::env::var_os("SQLAERO_TEST_CHARSETS").unwrap());
    let mut routes = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        let Some(name) = filename.to_str().unwrap().strip_suffix("-observed.tsv") else {
            continue;
        };
        let codec = Codec::new(resolve(Some(name), None).unwrap().charset);
        let mut count = 0;
        for line in std::fs::read_to_string(entry.path()).unwrap().lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let bytes = unhex(fields[0]);
            if fields[1] == "-" {
                if let Ok(text) = codec.decode(&bytes) {
                    assert_ne!(
                        text.chars().count(),
                        1,
                        "{name} {} must not decode as one valid character",
                        fields[0]
                    );
                }
            } else {
                let expected_bytes = unhex(fields[1]);
                let expected = std::str::from_utf8(&expected_bytes).unwrap();
                assert_eq!(
                    codec
                        .decode(&bytes)
                        .unwrap_or_else(|error| panic!("{name} {}: {error}", fields[0])),
                    expected,
                    "{name} {}",
                    fields[0]
                );
                if fields[2] != "-" {
                    let mut actual = Vec::new();
                    codec
                        .encode(expected, &mut actual, 16)
                        .unwrap_or_else(|error| {
                            panic!("{name} {} canonical {}: {error}", fields[0], fields[2])
                        });
                    assert_eq!(actual, unhex(fields[2]), "{name} {}", fields[0]);
                }
            }
            count += 1;
        }
        routes += 1;
        println!("{name}: {count} observations matched the product codec");
    }
    assert_eq!(routes, 9);
}

#[test]
#[ignore = "requires the saved SQLAERO_TEST_CHARSETS observation directory"]
fn all_single_byte_positions_match_actual_server_values_and_reverse_choices() {
    let root = PathBuf::from(std::env::var_os("SQLAERO_TEST_CHARSETS").unwrap());
    let raw = std::fs::read_to_string(root.join("single-byte-mysql84.tsv")).unwrap();
    let mut lines = raw.lines().peekable();
    let mut count = 0;
    while let Some(line) = lines.next() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields[0], "M");
        let name = fields[1];
        let byte: u8 = fields[2].parse().unwrap();
        let mut valid = fields[3] != "NULL" && fields[4] != "NULL";
        while lines.peek().is_some_and(|line| !line.starts_with("M\t")) {
            let diagnostic = lines.next().unwrap();
            let diagnostic: Vec<_> = diagnostic.split('\t').collect();
            match diagnostic[1] {
                "1300" => valid = false,
                "1287" => (),
                _ => panic!("unexpected conversion diagnostic"),
            }
        }
        let utf8 = if fields[3] == "NULL" {
            Vec::new()
        } else {
            unhex(fields[3])
        };
        let expected = std::str::from_utf8(&utf8).unwrap();
        if expected == "\u{FFFD}" || (expected == "?" && byte != b'?') {
            valid = false;
        }
        let codec = Codec::new(resolve(Some(name), None).unwrap().charset);
        if valid {
            assert_eq!(
                codec.decode(&[byte]).unwrap(),
                expected,
                "{name}/{byte:02X}"
            );
            let mut output = Vec::new();
            codec.encode(expected, &mut output, 8).unwrap();
            assert_eq!(output, unhex(fields[4]), "{name}/{byte:02X}");
            assert_eq!(
                codec.decode(&output).unwrap(),
                expected,
                "{name}/{byte:02X} must be lossless"
            );
        } else {
            assert!(codec.decode(&[byte]).is_err(), "{name}/{byte:02X}");
        }
        count += 1;
    }
    assert_eq!(count, 24 * 256);
}
