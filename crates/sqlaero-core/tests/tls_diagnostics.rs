//! Stable, public-only X.509 diagnostic fixtures; no trust decision is tested.

use num_bigint::{BigInt, Sign};
use sqlaero_core::transport::tls::{certificates, info::Certificate};
use x509_parser::nom::Parser;
use x509_parser::oid_registry::{OID_X509_EXT_BASIC_CONSTRAINTS, OID_X509_EXT_SUBJECT_ALT_NAME};
use x509_parser::prelude::X509CertificateParser;

const RICH: &[u8] = include_bytes!("fixtures/tls-diagnostics/rich.pem");
const NO_EXTENSIONS: &[u8] = include_bytes!("fixtures/tls-diagnostics/no-extensions.pem");

fn der(pem: &[u8]) -> Vec<u8> {
    certificates(pem).unwrap().remove(0).as_ref().to_vec()
}

#[test]
fn full_identity_exact_large_serial_and_ordered_duplicate_san_are_retained() {
    let certificate = Certificate::new(der(RICH).into());
    let details = certificate.details().unwrap();
    assert!(
        details
            .subject
            .as_ref()
            .unwrap()
            .contains("O=\"SQLAero, Diagnostics\"")
    );
    assert!(
        details
            .subject
            .as_ref()
            .unwrap()
            .contains("OU=\"Team\" + CN=\"db.example.invalid\"")
    );
    assert_eq!(
        details.serial_number,
        BigInt::parse_bytes(b"0102030405060708090a0b0c0d0e0f1011121314", 16).unwrap()
    );
    assert_eq!(
        details.dns_names.as_ref().unwrap(),
        &[
            "db.example.invalid",
            "*.internal.invalid",
            "db.example.invalid"
        ]
    );
    assert_eq!(
        details.ip_addresses.as_ref().unwrap(),
        &["192.0.2.10", "2001:db8::1", "192.0.2.10"]
    );
    assert_eq!(details.version, 3);
    assert!(!details.is_ca.as_ref().unwrap());
}

#[test]
fn absent_optional_extensions_have_their_standard_defaults() {
    let certificate = Certificate::new(der(NO_EXTENSIONS).into());
    let details = certificate.details().unwrap();
    assert!(details.dns_names.as_ref().unwrap().is_empty());
    assert!(details.ip_addresses.as_ref().unwrap().is_empty());
    assert!(!details.is_ca.as_ref().unwrap());
}

#[test]
fn invalid_dns_encoding_does_not_hide_valid_ip_addresses() {
    let mut bytes = der(RICH);
    let (_, parsed) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&bytes)
        .unwrap();
    let san = parsed
        .get_extension_unique(&OID_X509_EXT_SUBJECT_ALT_NAME)
        .unwrap()
        .unwrap();
    let offset = san.value.as_ptr() as usize - bytes.as_ptr() as usize + 4;
    bytes[offset] = 0xff;
    let certificate = Certificate::new(bytes.into());
    let details = certificate.details().unwrap();
    assert!(details.dns_names.is_err());
    assert_eq!(
        details.ip_addresses.as_ref().unwrap(),
        &["192.0.2.10", "2001:db8::1", "192.0.2.10"]
    );
    assert!(details.subject.is_ok());
}

#[test]
fn invalid_basic_constraints_does_not_hide_san_or_identity() {
    let mut bytes = der(RICH);
    let (_, parsed) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&bytes)
        .unwrap();
    let extension = parsed
        .get_extension_unique(&OID_X509_EXT_BASIC_CONSTRAINTS)
        .unwrap()
        .unwrap();
    let offset = extension.value.as_ptr() as usize - bytes.as_ptr() as usize;
    bytes[offset] = 0x31;
    let certificate = Certificate::new(bytes.into());
    let details = certificate.details().unwrap();
    assert!(details.is_ca.is_err());
    assert_eq!(details.dns_names.as_ref().unwrap().len(), 3);
    assert!(details.subject.is_ok());
}

#[test]
fn nonconforming_negative_serial_is_not_silently_made_positive() {
    let mut bytes = der(RICH);
    let (_, parsed) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&bytes)
        .unwrap();
    let serial = parsed.raw_serial();
    let offset = serial.as_ptr() as usize - bytes.as_ptr() as usize;
    let length = serial.len();
    bytes[offset] = 0x80;
    let expected = BigInt::from_bytes_be(Sign::Plus, &bytes[offset..offset + length])
        - (BigInt::from(1u8) << (length * 8));
    let certificate = Certificate::new(bytes.into());
    assert_eq!(certificate.details().unwrap().serial_number, expected);
    assert_eq!(
        certificate.details().unwrap().serial_number.sign(),
        Sign::Minus
    );
}

#[test]
fn malformed_signature_oid_is_local_and_never_hides_original_der_or_fingerprint() {
    let mut bytes = der(RICH);
    let (_, parsed) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&bytes)
        .unwrap();
    let oid = parsed.signature_algorithm.algorithm.as_bytes();
    let last = oid.as_ptr() as usize - bytes.as_ptr() as usize + oid.len() - 1;
    bytes[last] = 0x80;
    let certificate = Certificate::new(bytes.clone().into());
    let fingerprint = *certificate.fingerprint();
    let details = certificate.details().unwrap();
    assert!(details.signature_algorithm_oid.is_err());
    assert_eq!(
        details.public_key_algorithm_oid.as_ref().unwrap(),
        "1.2.840.113549.1.1.1"
    );
    assert!(details.subject.is_ok());
    assert_eq!(details.dns_names.as_ref().unwrap().len(), 3);
    assert_eq!(certificate.der(), bytes);
    assert_eq!(*certificate.fingerprint(), fingerprint);
    assert!(
        certificate
            .pem()
            .unwrap()
            .starts_with("-----BEGIN CERTIFICATE-----\n")
    );
}

#[test]
fn malformed_ip_encoding_does_not_hide_ordered_duplicate_dns_names() {
    let mut bytes = der(RICH);
    let (_, parsed) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&bytes)
        .unwrap();
    let san = parsed
        .get_extension_unique(&OID_X509_EXT_SUBJECT_ALT_NAME)
        .unwrap()
        .unwrap();
    let ip = [0x87, 0x04, 192, 0, 2, 10];
    let offset = san.value.as_ptr() as usize - bytes.as_ptr() as usize
        + san
            .value
            .windows(ip.len())
            .position(|value| value == ip)
            .unwrap();
    // Keep the SAN envelope intact: shorten this IP to an invalid two-byte
    // address and use its two remaining bytes as an empty RFC822Name. Neither
    // the enclosing sequence length nor any DNS entry is malformed.
    bytes[offset + 1] = 2;
    bytes[offset + 4] = 0x81;
    bytes[offset + 5] = 0;
    let certificate = Certificate::new(bytes.into());
    let details = certificate.details().unwrap();
    assert!(details.ip_addresses.is_err());
    assert_eq!(
        details.dns_names.as_ref().unwrap(),
        &[
            "db.example.invalid",
            "*.internal.invalid",
            "db.example.invalid"
        ]
    );
    assert!(details.subject.is_ok());
}
