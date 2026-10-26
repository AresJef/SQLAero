//! TLS material preparation performed once before opening a connection.

use pkcs8::der::{Decode, SecretDocument};
use rustls::crypto::{CryptoProvider, aws_lc_rs};
use rustls::sign::CertifiedKey;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;

/// A non-secret explanation of invalid TLS configuration material.
#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    /// A certificate bundle was malformed or contained no certificates.
    #[error("invalid or empty X.509 PEM certificate bundle")]
    Certificates,
    /// The key was not a supported PEM private-key document.
    #[error("invalid private-key PEM; expected PKCS#8, RSA PKCS#1 or EC SEC1")]
    PrivateKey,
    /// An encrypted key requires an explicit password.
    #[error("encrypted PKCS#8 private key requires a password")]
    PasswordRequired,
    /// A password was supplied for an unencrypted key.
    #[error("key_password must be None for an unencrypted private key")]
    UnexpectedPassword,
    /// Decryption failed without exposing the password or private key.
    #[error("cannot decrypt PKCS#8 key: incorrect password or unsupported modern encryption")]
    Decryption,
    /// The certificate and key did not have positively confirmed consistency.
    #[error("client certificate and private key do not have a verified matching public key: {0}")]
    KeyPair(rustls::Error),
}

/// Return this project's single explicitly selected cryptographic provider.
#[must_use]
pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(aws_lc_rs::default_provider())
}

/// Parse a leaf-first certificate chain or a trust-anchor PEM bundle.
///
/// # Errors
/// Returns [`MaterialError::Certificates`] for malformed or empty material.
pub fn certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, MaterialError> {
    let result: Vec<_> = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<_, _>>()
        .map_err(|_| MaterialError::Certificates)?;
    if result.is_empty() {
        return Err(MaterialError::Certificates);
    }
    Ok(result)
}

/// Decode a PEM private key, decrypting modern PBES2 PKCS#8 when necessary.
///
/// SecretDocument and PrivateKeyDer own and erase their controlled plaintext
/// buffers. The caller remains responsible for its input and password storage.
/// No Python codec or external crypto process is used by this product path.
///
/// # Errors
/// Returns a material error for an unsupported format, missing password,
/// unsupported algorithm, malformed ciphertext or incorrect password.
pub fn private_key(
    pem: &[u8],
    password: Option<&[u8]>,
) -> Result<PrivateKeyDer<'static>, MaterialError> {
    let text = std::str::from_utf8(pem).map_err(|_| MaterialError::PrivateKey)?;
    let (label, document) =
        SecretDocument::from_pem(text).map_err(|_| MaterialError::PrivateKey)?;
    match label {
        "ENCRYPTED PRIVATE KEY" => {
            let password = password.ok_or(MaterialError::PasswordRequired)?;
            let encrypted = pkcs8::EncryptedPrivateKeyInfoRef::from_der(document.as_bytes())
                .map_err(|_| MaterialError::PrivateKey)?;
            let plaintext = encrypted
                .decrypt(password)
                .map_err(|_| MaterialError::Decryption)?;
            Ok(PrivatePkcs8KeyDer::from(plaintext.as_bytes().to_vec()).into())
        }
        "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY" => {
            if password.is_some() {
                return Err(MaterialError::UnexpectedPassword);
            }
            PrivateKeyDer::try_from(document.as_bytes().to_vec())
                .map_err(|_| MaterialError::PrivateKey)
        }
        _ => Err(MaterialError::PrivateKey),
    }
}

/// Build a signing identity only after positively checking its key pair.
///
/// rustls's convenience constructor permits an unknown consistency result.
/// SQLAero's configuration contract requires a successful explicit check.
///
/// # Errors
/// Returns [`MaterialError::KeyPair`] for invalid, inconsistent or unverifiable
/// signing material.
pub fn signing_identity(
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    provider: &CryptoProvider,
) -> Result<Arc<CertifiedKey>, MaterialError> {
    let identity = CertifiedKey::from_der(chain, key, provider).map_err(MaterialError::KeyPair)?;
    identity.keys_match().map_err(MaterialError::KeyPair)?;
    Ok(Arc::new(identity))
}

/// The four explicit SSL policies. Endpoint-dependent defaults are resolved
/// before opening a socket; there is no opportunistic downgrade policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Plain transport; no TLS negotiation.
    Disabled,
    /// Require TLS and valid handshake signatures, without authenticating a CA.
    Required,
    /// Require a trusted server certificate chain, without checking its name.
    VerifyCa,
    /// Require a trusted chain and the actual expected DNS or IP identity.
    VerifyIdentity,
}

/// The supported lower TLS protocol bound, parsed once from configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MinimumVersion {
    /// Permit TLS 1.2 and 1.3, preferring the highest mutually supported version.
    #[default]
    Tls12,
    /// Permit TLS 1.3 only.
    Tls13,
}

/// A TLS configuration failure, separate from a peer's later handshake error.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A declared configuration combination or textual token was invalid.
    #[error("{0}")]
    Invalid(&'static str),
    /// The selected provider or prepared trust material was invalid.
    #[error("cannot prepare TLS configuration: {0}")]
    Tls(#[from] rustls::Error),
}

impl std::str::FromStr for Mode {
    type Err = ConfigError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
        for (name, mode) in [
            ("DISABLED", Self::Disabled),
            ("REQUIRED", Self::Required),
            ("VERIFY_CA", Self::VerifyCa),
            ("VERIFY_IDENTITY", Self::VerifyIdentity),
        ] {
            if value.eq_ignore_ascii_case(name) {
                return Ok(mode);
            }
        }
        Err(ConfigError::Invalid(
            "mode must be DISABLED, REQUIRED, VERIFY_CA or VERIFY_IDENTITY",
        ))
    }
}

impl std::str::FromStr for MinimumVersion {
    type Err = ConfigError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
        if !value.is_ascii() {
            return Err(ConfigError::Invalid(
                "min_tls_version must be TLS 1.2 or TLS 1.3",
            ));
        }
        let value = if value
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("TLSv"))
        {
            &value[4..]
        } else if value
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("TLS"))
        {
            &value[3..]
        } else {
            value
        };
        match value.trim_start_matches([' ', '\t']) {
            "1.2" => Ok(Self::Tls12),
            "1.3" => Ok(Self::Tls13),
            _ => Err(ConfigError::Invalid(
                "min_tls_version must be TLS 1.2 or TLS 1.3",
            )),
        }
    }
}

/// Build one immutable TLS client configuration from prepared materials.
/// Explicit roots replace the system store. REQUIRED skips chain/name trust
/// only; all modes still verify the TLS handshake's cryptographic signatures.
///
/// # Errors
/// Returns invalid-combination, trust-material or provider failures. DISABLED
/// accepts no roots or client signing identity and returns None.
pub fn client_config(
    mode: Mode,
    minimum: MinimumVersion,
    roots: Option<Vec<CertificateDer<'static>>>,
    identity: Option<Arc<CertifiedKey>>,
) -> Result<Option<Arc<rustls::ClientConfig>>, ConfigError> {
    use rustls::client::danger::ServerCertVerifier;
    if mode == Mode::Disabled {
        if roots.is_some() || identity.is_some() {
            return Err(ConfigError::Invalid(
                "DISABLED conflicts with CA or client certificate material",
            ));
        }
        return Ok(None);
    }
    if mode == Mode::Required && roots.is_some() {
        return Err(ConfigError::Invalid("REQUIRED does not accept ca_file"));
    }
    let provider = provider();
    let verifier: Arc<dyn ServerCertVerifier> = match (mode, roots) {
        (Mode::VerifyIdentity, None) => Arc::new(rustls_platform_verifier::Verifier::new(
            Arc::clone(&provider),
        )?),
        (Mode::Required, None) => Arc::new(PolicyVerifier {
            provider: Arc::clone(&provider),
            policy: Policy::Required,
        }),
        (Mode::VerifyCa, None) => Arc::new(PolicyVerifier {
            provider: Arc::clone(&provider),
            policy: Policy::SystemCa,
        }),
        (_, Some(certificates)) => {
            let mut roots = rustls::RootCertStore::empty();
            for certificate in certificates {
                roots.add(certificate)?;
            }
            if roots.is_empty() {
                return Err(ConfigError::Invalid("ca_file contains no trust anchors"));
            }
            Arc::new(PolicyVerifier {
                provider: Arc::clone(&provider),
                policy: Policy::Explicit {
                    roots,
                    check_name: mode == Mode::VerifyIdentity,
                },
            })
        }
        _ => {
            return Err(ConfigError::Invalid(
                "invalid TLS mode/material combination",
            ));
        }
    };
    let versions = match minimum {
        MinimumVersion::Tls12 => &[&rustls::version::TLS13, &rustls::version::TLS12][..],
        MinimumVersion::Tls13 => &[&rustls::version::TLS13][..],
    };
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(versions)?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    let config = match identity {
        Some(identity) => builder
            .with_client_cert_resolver(Arc::new(rustls::sign::SingleCertAndKey::from(identity))),
        None => builder.with_no_client_auth(),
    };
    Ok(Some(Arc::new(config)))
}

#[derive(Debug)]
enum Policy {
    Required,
    Explicit {
        roots: rustls::RootCertStore,
        check_name: bool,
    },
    SystemCa,
}

#[derive(Debug)]
struct PolicyVerifier {
    provider: Arc<CryptoProvider>,
    policy: Policy,
}

impl rustls::client::danger::ServerCertVerifier for PolicyVerifier {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &rustls_pki_types::ServerName<'_>,
        ocsp: &[u8],
        now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use rustls::client::{verify_server_cert_signed_by_trust_anchor, verify_server_name};
        match &self.policy {
            Policy::Required => (),
            Policy::Explicit { roots, check_name } => {
                let parsed = rustls::server::ParsedCertificate::try_from(end)?;
                verify_server_cert_signed_by_trust_anchor(
                    &parsed,
                    roots,
                    intermediates,
                    now,
                    self.provider.signature_verification_algorithms.all,
                )?;
                if *check_name {
                    verify_server_name(&parsed, name)?;
                }
            }
            Policy::SystemCa => verify_system_ca(end, intermediates, ocsp, now)?,
        }
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(target_vendor = "apple")]
fn verify_system_ca(
    end: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    ocsp: &[u8],
    now: rustls_pki_types::UnixTime,
) -> Result<(), rustls::Error> {
    use core_foundation::date::CFDate;
    use core_foundation_sys::date::kCFAbsoluteTimeIntervalSince1970;
    use security_framework::{
        certificate::SecCertificate, policy::SecPolicy, secure_transport::SslProtocolSide,
        trust::SecTrust,
    };
    let certificates = std::iter::once(end)
        .chain(intermediates.iter())
        .map(|certificate| SecCertificate::from_der(certificate.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
    // SAFETY: CoreFoundation owns this immutable, exported epoch constant.
    let epoch = unsafe { kCFAbsoluteTimeIntervalSince1970 };
    let date = CFDate::new(now.as_secs() as f64 - epoch);
    let policy = SecPolicy::create_ssl(SslProtocolSide::SERVER, None);
    let mut trust = SecTrust::create_with_certificates(&certificates, &[policy])
        .map_err(|error| rustls::Error::General(error.to_string()))?;
    trust
        .set_trust_verify_date(&date)
        .map_err(|error| rustls::Error::General(error.to_string()))?;
    if !ocsp.is_empty() {
        trust
            .set_trust_ocsp_response(std::iter::once(ocsp))
            .map_err(|error| rustls::Error::General(error.to_string()))?;
    }
    // Preserve the system policy, including its possible synchronous work.
    // The connection driver checks the original deadline after this call; it
    // does not pretend that OS trust evaluation can be instantly preempted.
    trust
        .evaluate_with_error()
        .map_err(|error| rustls::Error::General(error.to_string()))
}

#[cfg(not(target_vendor = "apple"))]
fn verify_system_ca(
    _end: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _ocsp: &[u8],
    _now: rustls_pki_types::UnixTime,
) -> Result<(), rustls::Error> {
    Err(rustls::Error::General(
        "system CA-only validation requires the supported Apple transport adapter".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mode_and_minimum_version_tokens_are_parsed_once_and_strictly() {
        assert_eq!(
            " verify_identity\t".parse::<Mode>().unwrap(),
            Mode::VerifyIdentity
        );
        for text in ["TLS 1.3", "TLSv1.3", "tlsv\t1.3", "1.3"] {
            assert_eq!(
                text.parse::<MinimumVersion>().unwrap(),
                MinimumVersion::Tls13
            );
        }
        for text in ["TLS 1 .3", "TLS1.1", "1.4", "TLS\u{a0}1.3"] {
            assert!(text.parse::<MinimumVersion>().is_err());
        }
        assert!("PREFERRED".parse::<Mode>().is_err());
    }
}
