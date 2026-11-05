//! X.509 metadata preparation without signature verification or borrowed caches.

use num_bigint::BigInt;
use std::borrow::Cow;
use std::collections::TryReserveError;
use std::fmt;
use std::fmt::Write;
use std::net::{Ipv4Addr, Ipv6Addr};
use x509_parser::asn1_rs::{BmpString, Tag};
use x509_parser::nom::Parser;
use x509_parser::oid_registry::{OID_X509_EXT_BASIC_CONSTRAINTS, OID_X509_EXT_SUBJECT_ALT_NAME};
use x509_parser::prelude::*;

#[derive(Debug, Clone)]
enum Cause {
    X509(x509_parser::nom::Err<X509Error>),
    Asn1(x509_parser::asn1_rs::Error),
    Utf8(std::str::Utf8Error),
}
impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X509(error) => fmt::Display::fmt(error, formatter),
            Self::Asn1(error) => fmt::Display::fmt(error, formatter),
            Self::Utf8(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for Cause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::X509(error) => error,
            Self::Asn1(error) => error,
            Self::Utf8(error) => error,
        })
    }
}

/// A deterministic certificate-content failure, with an unformatted native cause.
#[derive(Debug, Clone)]
pub struct ContentError {
    reason: &'static str,
    cause: Option<Cause>,
}
impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)?;
        if let Some(cause) = &self.cause {
            write!(formatter, ": {cause}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause as &dyn std::error::Error)
    }
}
impl ContentError {
    /// Explain an exact native field which cannot be represented at the host boundary.
    #[must_use]
    pub const fn representation(reason: &'static str) -> Self {
        Self {
            reason,
            cause: None,
        }
    }
    fn x509(reason: &'static str, error: x509_parser::nom::Err<X509Error>) -> Self {
        Self {
            reason,
            cause: Some(Cause::X509(error)),
        }
    }
}

/// A preparation failure; allocator errors never become cached bad content.
#[derive(Debug, thiserror::Error)]
pub(super) enum PreparationError {
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error("cannot allocate certificate metadata: {0}")]
    Allocation(#[from] TryReserveError),
}

type Prepared<T> = Result<T, PreparationError>;

fn field<T>(value: Prepared<T>) -> Prepared<Result<T, ContentError>> {
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(PreparationError::Content(error)) => Ok(Err(error)),
        Err(error) => Err(error),
    }
}
fn append(output: &mut String, value: &str) -> Result<(), TryReserveError> {
    output.try_reserve(value.len())?;
    output.push_str(value);
    Ok(())
}
fn character(output: &mut String, value: char) -> Result<(), TryReserveError> {
    output.try_reserve(value.len_utf8())?;
    output.push(value);
    Ok(())
}
fn copied(value: &str) -> Result<String, TryReserveError> {
    let mut output = String::new();
    append(&mut output, value)?;
    Ok(output)
}
fn quoted(output: &mut String, value: &str) -> Result<(), TryReserveError> {
    character(output, '"')?;
    for value in value.chars().flat_map(char::escape_debug) {
        character(output, value)?;
    }
    character(output, '"')
}

fn oid_text(oid: &x509_parser::asn1_rs::Oid<'_>) -> Prepared<String> {
    let bytes = oid.as_bytes();
    if bytes.is_empty() {
        return Err(ContentError::representation("empty OID encoding").into());
    }
    let mut start = true;
    for byte in bytes {
        if start && *byte == 0x80 {
            return Err(ContentError::representation("non-minimal OID subidentifier").into());
        }
        start = byte & 0x80 == 0;
    }
    if !start {
        return Err(ContentError::representation("unterminated OID subidentifier").into());
    }
    // Use the mature arbitrary-precision subidentifier iterator without its
    // single-byte first-arc formatter. This is an exact projection, not repair.
    let relative = x509_parser::asn1_rs::Oid::new_relative(Cow::Borrowed(bytes));
    let mut arcs = relative.iter_bigint();
    let packed = arcs
        .next()
        .ok_or_else(|| ContentError::representation("empty OID encoding"))?;
    let (first, second) = if packed < 40u8.into() {
        ('0', packed)
    } else if packed < 80u8.into() {
        ('1', packed - 40u8)
    } else {
        ('2', packed - 80u8)
    };
    let mut output = String::new();
    character(&mut output, first)?;
    character(&mut output, '.')?;
    append(&mut output, &second.to_str_radix(10))?;
    for arc in arcs {
        character(&mut output, '.')?;
        append(&mut output, &arc.to_str_radix(10))?;
    }
    Ok(output)
}

/// UTC calendar components, kept native until a Python datetime is requested.
#[derive(Debug, Clone, Copy)]
pub struct UtcDateTime {
    /// Absolute UTC year.
    pub year: i32,
    /// Calendar month, 1 through 12.
    pub month: u8,
    /// Day in the month.
    pub day: u8,
    /// Hour in UTC.
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// Second.
    pub second: u8,
    /// Fractional nanoseconds; the boundary rejects unrepresentable precision.
    pub nanosecond: u32,
}
impl UtcDateTime {
    fn from_asn1(value: ASN1Time) -> Self {
        // The certificate DER parser enforces the UTC Z time-zone encoding.
        let value = value.to_datetime();
        Self {
            year: value.year(),
            month: value.month() as u8,
            day: value.day(),
            hour: value.hour(),
            minute: value.minute(),
            second: value.second(),
            nanosecond: value.nanosecond(),
        }
    }
}

/// Complete owned metadata with independent optional-extension dependencies.
#[derive(Debug)]
pub struct Details {
    /// Full Subject DN with explicitly quoted and escaped attribute values.
    pub subject: Result<String, ContentError>,
    /// Full Issuer DN, using the same display convention.
    pub issuer: Result<String, ContentError>,
    /// Exact ASN.1 INTEGER value, including its original sign.
    pub serial_number: BigInt,
    /// Human X.509 version number, rather than the zero-based encoding.
    pub version: u32,
    /// Absolute validity start in UTC.
    pub not_before: UtcDateTime,
    /// Absolute validity end in UTC.
    pub not_after: UtcDateTime,
    /// SAN DNS projection with original order, duplicates and wildcard text.
    pub dns_names: Result<Vec<String>, ContentError>,
    /// SAN IP projection with original order and duplicates.
    pub ip_addresses: Result<Vec<String>, ContentError>,
    /// Basic constraints CA bit; absent means false, invalid means failure.
    pub is_ca: Result<bool, ContentError>,
    /// Outer certificate signature algorithm OID.
    pub signature_algorithm_oid: Result<String, ContentError>,
    /// SubjectPublicKeyInfo public-key algorithm OID.
    pub public_key_algorithm_oid: Result<String, ContentError>,
}

pub(super) fn parse(der: &[u8]) -> Prepared<Details> {
    let (remaining, certificate) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(der)
        .map_err(|error| ContentError::x509("invalid X.509 structure", error))?;
    if !remaining.is_empty() {
        return Err(ContentError::representation("trailing data after X.509 certificate").into());
    }
    let (dns_names, ip_addresses) = names(&certificate)?;
    Ok(Details {
        subject: field(distinguished_name(certificate.subject()))?,
        issuer: field(distinguished_name(certificate.issuer()))?,
        serial_number: BigInt::from_signed_bytes_be(certificate.raw_serial()),
        version: certificate.version().0.checked_add(1).ok_or_else(|| {
            ContentError::representation("X.509 version number overflows its representation")
        })?,
        not_before: UtcDateTime::from_asn1(certificate.validity().not_before),
        not_after: UtcDateTime::from_asn1(certificate.validity().not_after),
        dns_names,
        ip_addresses,
        is_ca: ca_flag(&certificate),
        signature_algorithm_oid: field(oid_text(&certificate.signature_algorithm.algorithm))?,
        public_key_algorithm_oid: field(oid_text(&certificate.public_key().algorithm.algorithm))?,
    })
}

fn distinguished_name(name: &X509Name<'_>) -> Prepared<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::new();
    for (rdn_index, rdn) in name.iter_rdn().enumerate() {
        if rdn_index != 0 {
            append(&mut output, ", ")?;
        }
        for (attr_index, attr) in rdn.iter().enumerate() {
            if attr_index != 0 {
                append(&mut output, " + ")?;
            }
            match oid2abbrev(attr.attr_type(), oid_registry()) {
                Ok(label) => append(&mut output, label)?,
                Err(_) => append(&mut output, &oid_text(attr.attr_type())?)?,
            }
            character(&mut output, '=')?;
            let any = attr.attr_value();
            // ASN.1 decoding remains in the mature component. Quoting is a
            // readable display convention, not a claim of canonical RFC4514.
            match any.tag() {
                Tag::NumericString
                | Tag::VisibleString
                | Tag::PrintableString
                | Tag::GeneralString
                | Tag::ObjectDescriptor
                | Tag::GraphicString
                | Tag::T61String
                | Tag::VideotexString
                | Tag::Utf8String
                | Tag::Ia5String => {
                    let text = std::str::from_utf8(any.data).map_err(|error| ContentError {
                        reason: "invalid DN string",
                        cause: Some(Cause::Utf8(error)),
                    })?;
                    quoted(&mut output, text)?;
                }
                Tag::BmpString => {
                    let text = BmpString::try_from(any).map_err(|error| ContentError {
                        reason: "invalid DN BMP string",
                        cause: Some(Cause::Asn1(error)),
                    })?;
                    quoted(&mut output, text.as_ref())?;
                }
                _ => {
                    character(&mut output, '#')?;
                    for byte in any.as_bytes() {
                        character(&mut output, char::from(HEX[usize::from(byte >> 4)]))?;
                        character(&mut output, char::from(HEX[usize::from(byte & 15)]))?;
                    }
                }
            }
        }
    }
    Ok(output)
}

type Names = Result<Vec<String>, ContentError>;
fn names(certificate: &X509Certificate<'_>) -> Prepared<(Names, Names)> {
    let extension = match certificate.get_extension_unique(&OID_X509_EXT_SUBJECT_ALT_NAME) {
        Ok(Some(extension)) => extension,
        Ok(None) => return Ok((Ok(Vec::new()), Ok(Vec::new()))),
        Err(error) => {
            let error = ContentError::x509(
                "invalid subjectAltName",
                x509_parser::nom::Err::Error(error),
            );
            return Ok((Err(error.clone()), Err(error)));
        }
    };
    let names = match SubjectAlternativeName::from_der(extension.value) {
        Ok(([], names)) => names,
        Err(error) => {
            let error = ContentError::x509("invalid subjectAltName encoding", error);
            return Ok((Err(error.clone()), Err(error)));
        }
        _ => {
            let error = ContentError::representation("trailing data after subjectAltName");
            return Ok((Err(error.clone()), Err(error)));
        }
    };
    let mut dns = Ok(Vec::new());
    let mut ips = Ok(Vec::new());
    for name in names.general_names {
        match name {
            GeneralName::DNSName(name) => {
                if let Ok(values) = &mut dns {
                    values.try_reserve(1)?;
                    values.push(copied(name)?);
                }
            }
            GeneralName::IPAddress(bytes) => {
                let mut address = String::new();
                address.try_reserve_exact(45)?;
                // Standard IP display is bounded by 45 ASCII bytes. The
                // reserved output never grows while Display writes it.
                let formatted = match bytes {
                    [a, b, c, d] => write!(&mut address, "{}", Ipv4Addr::new(*a, *b, *c, *d)),
                    bytes if bytes.len() == 16 => {
                        let mut value = [0; 16];
                        value.copy_from_slice(bytes);
                        write!(&mut address, "{}", Ipv6Addr::from(value))
                    }
                    _ => {
                        ips = Err(ContentError::representation(
                            "invalid subjectAltName IP address length",
                        ));
                        continue;
                    }
                };
                formatted
                    .map_err(|_| ContentError::representation("cannot format SAN IP address"))?;
                if let Ok(values) = &mut ips {
                    values.try_reserve(1)?;
                    values.push(address);
                }
            }
            GeneralName::Invalid(tag, _) if tag.0 == 2 => {
                dns = Err(ContentError::representation(
                    "invalid subjectAltName DNS encoding",
                ))
            }
            GeneralName::Invalid(tag, _) if tag.0 == 7 => {
                ips = Err(ContentError::representation(
                    "invalid subjectAltName IP encoding",
                ))
            }
            _ => (),
        }
    }
    Ok((dns, ips))
}

fn ca_flag(certificate: &X509Certificate<'_>) -> Result<bool, ContentError> {
    let Some(extension) = certificate
        .get_extension_unique(&OID_X509_EXT_BASIC_CONSTRAINTS)
        .map_err(|error| {
            ContentError::x509(
                "invalid basicConstraints",
                x509_parser::nom::Err::Error(error),
            )
        })?
    else {
        return Ok(false);
    };
    match BasicConstraints::from_der(extension.value) {
        Ok(([], constraints)) => Ok(constraints.ca),
        Err(error) => Err(ContentError::x509(
            "invalid basicConstraints encoding",
            error,
        )),
        _ => Err(ContentError::representation(
            "trailing data after basicConstraints",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oid_projection_is_exact_and_strict_without_fixed_integer_arcs() {
        let oid = |bytes: &'static [u8]| x509_parser::asn1_rs::Oid::new(Cow::Borrowed(bytes));
        assert_ne!(oid(&[0x88, 0x37]).to_id_string(), "2.999");
        assert_eq!(oid_text(&oid(&[0x88, 0x37])).unwrap(), "2.999");
        assert_eq!(
            oid_text(&oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 1, 1, 1])).unwrap(),
            "1.2.840.113549.1.1.1"
        );
        assert_eq!(
            oid_text(&oid(&[
                0x2a, 0x82, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0
            ]))
            .unwrap(),
            "1.2.18446744073709551616"
        );
        for bytes in [&[][..], &[0x2a, 0x86], &[0x2a, 0x80, 1], &[0x80, 0x50]] {
            assert!(matches!(
                oid_text(&x509_parser::asn1_rs::Oid::new(Cow::Borrowed(bytes))),
                Err(PreparationError::Content(_))
            ));
        }
    }

    #[test]
    fn der_validity_is_absolute_utc_and_preserves_fractional_seconds() {
        assert!(ASN1Time::from_der(b"\x18\x1320261105120000+0100").is_err());
        let (_, value) = ASN1Time::from_der(b"\x18\x0f20261105120000Z").unwrap();
        assert!(value.to_datetime().offset().is_utc());
        let value = UtcDateTime::from_asn1(value);
        assert_eq!(
            (value.year, value.month, value.day, value.hour, value.minute),
            (2026, 11, 5, 12, 0)
        );
        let (_, value) = ASN1Time::from_der(b"\x18\x1320261105120000.123Z").unwrap();
        assert_eq!(UtcDateTime::from_asn1(value).nanosecond, 123_000_000);
    }
}
