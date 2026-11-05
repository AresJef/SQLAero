//! Owned TLS facts and lazily prepared certificate diagnostics.

use super::{Mode, Version};
use rustls_pki_types::CertificateDer;
use std::collections::TryReserveError;
use std::sync::{Arc, OnceLock};

mod details;
pub use details::{ContentError, Details, UtcDateTime};

/// A failure to copy complete established engine facts.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// A successful X.509 handshake did not expose a required engine fact.
    #[error("TLS snapshot is missing {0}")]
    Missing(&'static str),
    /// Native owned storage could not be reserved.
    #[error("cannot allocate TLS snapshot: {0}")]
    Allocation(#[from] TryReserveError),
}

/// Independent facts for one successfully established TLS connection.
/// No socket, active TLS state, borrowed session, or Python object is retained.
#[derive(Debug)]
pub struct Snapshot {
    /// The actual resolved connection verification policy.
    pub mode: Mode,
    /// The configured minimum protocol version.
    pub min_version: Version,
    /// The optional configured maximum; None remains unbounded by the caller.
    pub max_version: Option<Version>,
    /// The real configured target; absent for an unnamed Unix TLS endpoint.
    pub server_name: Option<String>,
    /// The negotiated protocol version.
    pub version: Version,
    /// The negotiated suite's canonical IANA name.
    pub cipher: &'static str,
    /// The exact 16-bit TLS suite identifier.
    pub cipher_id: u16,
    /// Whether rustls reported a resumed handshake.
    pub session_reused: bool,
    /// Owned peer identity material in the engine's original order.
    pub peer_certificates: Vec<Arc<Certificate>>,
}

impl Snapshot {
    /// Copy established facts while the caller still protects the wire/session.
    /// The returned snapshot remains valid after connection close or drop.
    ///
    /// # Errors
    /// Returns Missing for an incomplete/inconsistent handshake, unknown
    /// enabled suite, or absent X.509 identity, and Allocation for storage loss.
    pub fn capture(
        session: &rustls::ClientConnection,
        mode: Mode,
        min_version: Version,
        max_version: Option<Version>,
        server_name: Option<&str>,
    ) -> Result<Self, SnapshotError> {
        if session.is_handshaking() || mode == Mode::Disabled {
            return Err(SnapshotError::Missing("a completed TLS handshake"));
        }
        let version = match session.protocol_version() {
            Some(rustls::ProtocolVersion::TLSv1_2) => Version::Tls12,
            Some(rustls::ProtocolVersion::TLSv1_3) => Version::Tls13,
            _ => return Err(SnapshotError::Missing("a supported negotiated TLS version")),
        };
        let cipher_id = u16::from(
            session
                .negotiated_cipher_suite()
                .ok_or(SnapshotError::Missing("the negotiated cipher suite"))?
                .suite(),
        );
        let cipher = cipher_name(cipher_id)
            .ok_or(SnapshotError::Missing("the enabled cipher's IANA name"))?;
        let session_reused = match session.handshake_kind() {
            Some(rustls::HandshakeKind::Resumed) => true,
            Some(
                rustls::HandshakeKind::Full | rustls::HandshakeKind::FullWithHelloRetryRequest,
            ) => false,
            _ => return Err(SnapshotError::Missing("the completed handshake kind")),
        };
        let chain = session
            .peer_certificates()
            .filter(|chain| !chain.is_empty())
            .ok_or(SnapshotError::Missing("the peer X.509 certificate chain"))?;
        let mut peer_certificates = Vec::new();
        peer_certificates.try_reserve_exact(chain.len())?;
        for certificate in chain {
            let mut der = Vec::new();
            der.try_reserve_exact(certificate.as_ref().len())?;
            der.extend_from_slice(certificate.as_ref());
            peer_certificates.push(Arc::new(Certificate::new(der.into())));
        }
        let server_name = server_name
            .map(|name| {
                let mut owned = String::new();
                owned.try_reserve_exact(name.len())?;
                owned.push_str(name);
                Ok::<_, TryReserveError>(owned)
            })
            .transpose()?;
        Ok(Self {
            mode,
            min_version,
            max_version,
            server_name,
            version,
            cipher,
            cipher_id,
            session_reused,
            peer_certificates,
        })
    }
}

fn cipher_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x1301 => "TLS_AES_128_GCM_SHA256",
        0x1302 => "TLS_AES_256_GCM_SHA384",
        0x1303 => "TLS_CHACHA20_POLY1305_SHA256",
        0xc02b => "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        0xc02c => "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        0xcca9 => "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
        0xc02f => "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        0xc030 => "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        0xcca8 => "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        _ => return None,
    })
}

/// One independently owned peer certificate and its lazy native caches.
#[derive(Debug)]
pub struct Certificate {
    der: CertificateDer<'static>,
    fingerprint: OnceLock<[u8; 32]>,
    details: OnceLock<Result<Details, ContentError>>,
}

impl Certificate {
    /// Own exact DER; this constructor does not parse or validate its content.
    #[must_use]
    pub fn new(der: CertificateDer<'static>) -> Self {
        Self {
            der,
            fingerprint: OnceLock::new(),
            details: OnceLock::new(),
        }
    }
    /// Borrow the exact original DER under this certificate's owned lifetime.
    #[must_use]
    pub fn der(&self) -> &[u8] {
        self.der.as_ref()
    }
    /// Encode the original DER with LF, 64-character lines and a final newline.
    ///
    /// # Errors
    /// Returns the mature PEM encoder's error without requiring X.509 parsing.
    pub fn pem(&self) -> Result<String, pem_rfc7468::Error> {
        pem_rfc7468::encode_string("CERTIFICATE", pem_rfc7468::LineEnding::LF, self.der())
    }
    /// Compute SHA-256 of the original DER once, independent of parsed details.
    #[must_use]
    pub fn fingerprint(&self) -> &[u8; 32] {
        self.fingerprint.get_or_init(|| {
            let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, self.der());
            let mut bytes = [0; 32];
            bytes.copy_from_slice(digest.as_ref());
            bytes
        })
    }

    /// Prepare and reuse native metadata only when explicitly requested.
    /// Deterministic content failures are cached; no Python exception is stored.
    /// Individual optional fields preserve their own dependency failures.
    ///
    /// # Errors
    /// Returns a cached parse failure for malformed common X.509 structure.
    pub fn details(&self) -> Result<&Details, &ContentError> {
        self.details
            .get_or_init(|| details::parse(self.der()))
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_exports_and_fingerprint_do_not_parse_certificate_details() {
        let bytes = b"not an X.509 certificate".to_vec();
        let certificate = Certificate::new(bytes.clone().into());
        assert!(certificate.details.get().is_none());
        assert!(certificate.fingerprint.get().is_none());
        assert_eq!(certificate.der(), bytes);
        let pem = certificate.pem().unwrap();
        assert_eq!(pem_rfc7468::decode_vec(pem.as_bytes()).unwrap().1, bytes);
        assert!(pem.ends_with("-----END CERTIFICATE-----\n"));
        assert_eq!(certificate.fingerprint().len(), 32);
        assert!(certificate.details.get().is_none());
    }

    #[test]
    fn every_enabled_cipher_has_an_iana_name() {
        for suite in super::super::provider().cipher_suites.iter() {
            assert!(cipher_name(u16::from(suite.suite())).is_some());
        }
        assert_eq!(cipher_name(0x1302), Some("TLS_AES_256_GCM_SHA384"));
    }

    #[test]
    fn malformed_common_structure_is_cached_without_disabling_raw_facts() {
        let certificate = Certificate::new(vec![0x31, 0x00].into());
        assert!(certificate.details().is_err());
        assert!(std::ptr::eq(
            certificate.details().unwrap_err(),
            certificate.details().unwrap_err()
        ));
        assert_eq!(certificate.der(), &[0x31, 0]);
        assert_eq!(certificate.fingerprint().len(), 32);
        assert!(certificate.pem().unwrap().contains("CERTIFICATE"));
    }

    #[test]
    #[ignore = "requires the owned fixture certificate"]
    fn malformed_san_does_not_disable_unrelated_details_or_raw_exports() {
        use x509_parser::nom::Parser;
        use x509_parser::oid_registry::OID_X509_EXT_SUBJECT_ALT_NAME;
        use x509_parser::prelude::*;
        let fixture =
            std::path::PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").unwrap());
        let mut chain =
            super::super::certificates(&std::fs::read(fixture.join("tls/server.pem")).unwrap())
                .unwrap();
        let original = chain.remove(0);
        let (_, parsed) = X509CertificateParser::new()
            .with_deep_parse_extensions(false)
            .parse(original.as_ref())
            .unwrap();
        let san = parsed
            .get_extension_unique(&OID_X509_EXT_SUBJECT_ALT_NAME)
            .unwrap()
            .unwrap();
        let offset = san.value.as_ptr() as usize - original.as_ref().as_ptr() as usize;
        let mut der = original.as_ref().to_vec();
        der[offset] = 0x31;
        let certificate = Certificate::new(der.clone().into());
        let details = certificate.details().unwrap();
        assert!(details.subject.as_ref().unwrap().contains("SQLAero server"));
        assert!(details.dns_names.is_err());
        assert!(details.ip_addresses.is_err());
        assert!(!details.is_ca.as_ref().unwrap());
        assert_eq!(details.version, 3);
        assert_eq!(certificate.der(), der);
        assert_eq!(certificate.fingerprint().len(), 32);
    }

    #[test]
    #[ignore = "requires the owned fixture signing identity"]
    fn complete_and_resumed_engine_snapshots_own_original_peer_material() {
        use std::io::Cursor;
        let fixture =
            std::path::PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").unwrap());
        let chain =
            super::super::certificates(&std::fs::read(fixture.join("tls/server.pem")).unwrap())
                .unwrap();
        let key = super::super::private_key(
            &std::fs::read(fixture.join("tls/server-key.pem")).unwrap(),
            None,
        )
        .unwrap();
        let server = Arc::new(
            rustls::ServerConfig::builder_with_provider(super::super::provider())
                .with_protocol_versions(&[&rustls::version::TLS13])
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(chain.clone(), key)
                .unwrap(),
        );
        let client = super::super::client_config(
            Mode::Required,
            Version::Tls13,
            Some(Version::Tls13),
            super::super::CaMode::Replace,
            None,
            None,
        )
        .unwrap()
        .unwrap();
        for resumed in [false, true] {
            let mut client =
                rustls::ClientConnection::new(Arc::clone(&client), "localhost".try_into().unwrap())
                    .unwrap();
            assert!(
                Snapshot::capture(
                    &client,
                    Mode::Required,
                    Version::Tls13,
                    Some(Version::Tls13),
                    Some("localhost")
                )
                .is_err()
            );
            let mut server = rustls::ServerConnection::new(Arc::clone(&server)).unwrap();
            for _ in 0..32 {
                let mut outgoing = Vec::new();
                client.write_tls(&mut outgoing).unwrap();
                if !outgoing.is_empty() {
                    server.read_tls(&mut Cursor::new(outgoing)).unwrap();
                    server.process_new_packets().unwrap();
                }
                let mut incoming = Vec::new();
                server.write_tls(&mut incoming).unwrap();
                if !incoming.is_empty() {
                    client.read_tls(&mut Cursor::new(incoming)).unwrap();
                    client.process_new_packets().unwrap();
                }
                if !client.is_handshaking()
                    && !server.is_handshaking()
                    && !client.wants_write()
                    && !server.wants_write()
                {
                    break;
                }
            }
            let snapshot = Snapshot::capture(
                &client,
                Mode::Required,
                Version::Tls13,
                Some(Version::Tls13),
                Some("localhost"),
            )
            .unwrap();
            assert_eq!(snapshot.session_reused, resumed);
            assert_eq!(snapshot.cipher_id, 0x1302);
            assert_eq!(snapshot.version, Version::Tls13);
            drop(client);
            drop(server);
            assert_eq!(snapshot.peer_certificates[0].der(), chain[0].as_ref());
            let details = snapshot.peer_certificates[0].details().unwrap();
            assert_eq!(details.dns_names.as_ref().unwrap(), &["localhost"]);
            assert_eq!(details.ip_addresses.as_ref().unwrap(), &["127.0.0.1"]);
            assert_eq!(details.version, 3);
            assert_eq!(details.public_key_algorithm_oid, "1.2.840.113549.1.1.1");
            assert!(details.serial_number.bits() > 64);
        }
    }
}
