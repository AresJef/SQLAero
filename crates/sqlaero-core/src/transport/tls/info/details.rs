//! X.509 metadata preparation without signature verification or borrowed caches.

use num_bigint::BigInt;
use std::net::{Ipv4Addr, Ipv6Addr};
use x509_parser::asn1_rs::{BmpString, Tag};
use x509_parser::nom::Parser;
use x509_parser::oid_registry::{OID_X509_EXT_BASIC_CONSTRAINTS, OID_X509_EXT_SUBJECT_ALT_NAME};
use x509_parser::prelude::*;

/// A deterministic certificate-content failure, never a Python exception.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ContentError(String);

impl ContentError {
    /// Explain an exact native field which cannot be represented at the host boundary.
    #[must_use]
    pub fn representation(reason: &'static str) -> Self {
        Self(reason.to_owned())
    }
}

fn content(message: impl Into<String>) -> ContentError {
    ContentError(message.into())
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
    pub signature_algorithm_oid: String,
    /// SubjectPublicKeyInfo public-key algorithm OID.
    pub public_key_algorithm_oid: String,
}

pub(super) fn parse(der: &[u8]) -> Result<Details, ContentError> {
    let (remaining, certificate) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(der)
        .map_err(|error| content(format!("invalid X.509 structure: {error:?}")))?;
    if !remaining.is_empty() {
        return Err(content("trailing data after X.509 certificate"));
    }
    let (dns_names, ip_addresses) = names(&certificate);
    Ok(Details {
        subject: distinguished_name(certificate.subject()),
        issuer: distinguished_name(certificate.issuer()),
        serial_number: BigInt::from_signed_bytes_be(certificate.raw_serial()),
        version: certificate
            .version()
            .0
            .checked_add(1)
            .ok_or_else(|| content("X.509 version number overflows its representation"))?,
        not_before: UtcDateTime::from_asn1(certificate.validity().not_before),
        not_after: UtcDateTime::from_asn1(certificate.validity().not_after),
        dns_names,
        ip_addresses,
        is_ca: ca_flag(&certificate),
        signature_algorithm_oid: certificate.signature_algorithm.algorithm.to_id_string(),
        public_key_algorithm_oid: certificate.public_key().algorithm.algorithm.to_id_string(),
    })
}

fn distinguished_name(name: &X509Name<'_>) -> Result<String, ContentError> {
    use std::fmt::Write;
    let mut output = String::new();
    for (rdn_index, rdn) in name.iter_rdn().enumerate() {
        if rdn_index != 0 {
            output.push_str(", ");
        }
        for (attr_index, attr) in rdn.iter().enumerate() {
            if attr_index != 0 {
                output.push_str(" + ");
            }
            let oid = attr.attr_type().to_id_string();
            let label = oid2abbrev(attr.attr_type(), oid_registry()).unwrap_or(&oid);
            output.push_str(label);
            output.push('=');
            let any = attr.attr_value();
            // ASN.1 decoding remains in the mature component. This display
            // quotes each decoded value; it does not claim canonical RFC4514.
            let text = match any.tag() {
                Tag::NumericString
                | Tag::VisibleString
                | Tag::PrintableString
                | Tag::GeneralString
                | Tag::ObjectDescriptor
                | Tag::GraphicString
                | Tag::T61String
                | Tag::VideotexString
                | Tag::Utf8String
                | Tag::Ia5String => std::str::from_utf8(any.data)
                    .map_err(|error| content(format!("invalid DN string: {error}")))?
                    .to_owned(),
                Tag::BmpString => BmpString::try_from(any.clone())
                    .map_err(|error| content(format!("invalid DN BMP string: {error:?}")))?
                    .string(),
                _ => {
                    output.push('#');
                    for byte in any.as_bytes() {
                        let _ = write!(&mut output, "{byte:02x}");
                    }
                    continue;
                }
            };
            let _ = write!(&mut output, "{text:?}");
        }
    }
    Ok(output)
}

type Names = Result<Vec<String>, ContentError>;

fn names(certificate: &X509Certificate<'_>) -> (Names, Names) {
    let extension = match certificate.get_extension_unique(&OID_X509_EXT_SUBJECT_ALT_NAME) {
        Ok(Some(extension)) => extension,
        Ok(None) => return (Ok(Vec::new()), Ok(Vec::new())),
        Err(error) => {
            let error = content(format!("invalid subjectAltName: {error:?}"));
            return (Err(error.clone()), Err(error));
        }
    };
    let names = match SubjectAlternativeName::from_der(extension.value) {
        Ok(([], names)) => names,
        Err(error) => {
            let error = content(format!("invalid subjectAltName encoding: {error:?}"));
            return (Err(error.clone()), Err(error));
        }
        _ => {
            let error = content("trailing data after subjectAltName");
            return (Err(error.clone()), Err(error));
        }
    };
    let mut dns = Ok(Vec::new());
    let mut ips = Ok(Vec::new());
    for name in names.general_names {
        match name {
            GeneralName::DNSName(name) => {
                if let Ok(values) = &mut dns {
                    values.push(name.to_owned());
                }
            }
            GeneralName::IPAddress(bytes) => {
                let address = match bytes {
                    [a, b, c, d] => Ipv4Addr::new(*a, *b, *c, *d).to_string(),
                    bytes if bytes.len() == 16 => {
                        let mut address = [0; 16];
                        address.copy_from_slice(bytes);
                        Ipv6Addr::from(address).to_string()
                    }
                    _ => {
                        ips = Err(content("invalid subjectAltName IP address length"));
                        continue;
                    }
                };
                if let Ok(values) = &mut ips {
                    values.push(address);
                }
            }
            GeneralName::Invalid(tag, _) if tag.0 == 2 => {
                dns = Err(content("invalid subjectAltName DNS encoding"))
            }
            GeneralName::Invalid(tag, _) if tag.0 == 7 => {
                ips = Err(content("invalid subjectAltName IP encoding"))
            }
            _ => (),
        }
    }
    (dns, ips)
}

fn ca_flag(certificate: &X509Certificate<'_>) -> Result<bool, ContentError> {
    let Some(extension) = certificate
        .get_extension_unique(&OID_X509_EXT_BASIC_CONSTRAINTS)
        .map_err(|error| content(format!("invalid basicConstraints: {error:?}")))?
    else {
        return Ok(false);
    };
    match BasicConstraints::from_der(extension.value) {
        Ok(([], constraints)) => Ok(constraints.ca),
        Err(error) => Err(content(format!(
            "invalid basicConstraints encoding: {error:?}"
        ))),
        _ => Err(content("trailing data after basicConstraints")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
