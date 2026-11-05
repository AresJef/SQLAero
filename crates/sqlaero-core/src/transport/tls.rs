//! TLS material preparation performed once before opening a connection.

use pkcs8::der::{Decode, SecretDocument};
use rustls::crypto::{CryptoProvider, aws_lc_rs};
use rustls::sign::CertifiedKey;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::TryReserveError;
use std::sync::Arc;

pub mod info;
pub mod materials;

// Keep native platform trust objects in this transport boundary. The supported
// Apple wrapper owns its certificates and implements Send + Sync upstream.
#[cfg(target_vendor = "apple")]
type TrustAnchor = security_framework::certificate::SecCertificate;
#[cfg(target_vendor = "apple")]
type PreparedRoots = Vec<TrustAnchor>;
#[cfg(windows)]
type PreparedRoots = Vec<CertificateDer<'static>>;
#[cfg(not(any(target_vendor = "apple", windows)))]
type PreparedRoots = rustls::RootCertStore;

#[cfg(windows)]
mod windows;

#[cfg(target_vendor = "apple")]
type ValidatedRoots = PreparedRoots;
#[cfg(not(target_vendor = "apple"))]
type ValidatedRoots = rustls::RootCertStore;

fn validate_anchors(
    certificates: &[CertificateDer<'static>],
) -> Result<ValidatedRoots, ConfigError> {
    if certificates.is_empty() {
        return Err(ConfigError::Invalid("ca_file contains no trust anchors"));
    }
    #[cfg(target_vendor = "apple")]
    {
        let mut roots = Vec::new();
        roots.try_reserve_exact(certificates.len())?;
        for certificate in certificates {
            roots.push(TrustAnchor::from_der(certificate.as_ref()).map_err(|_| {
                ConfigError::Invalid("ca_file contains an invalid X.509 certificate")
            })?);
        }
        Ok(roots)
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        // Validate and prepare DER at the configuration boundary. The Linux
        // verifier borrows this store; it does not reload or reparse CA files
        // while advancing the connection state machine.
        let mut store = rustls::RootCertStore::empty();
        store.roots.try_reserve_exact(certificates.len())?;
        for certificate in certificates {
            store
                .add(CertificateDer::from(certificate.as_ref()))
                .map_err(|_| {
                    ConfigError::Invalid("ca_file contains an invalid X.509 certificate")
                })?;
        }
        Ok(store)
    }
}

fn prepare_anchors(
    certificates: Vec<CertificateDer<'static>>,
) -> Result<PreparedRoots, ConfigError> {
    let roots = validate_anchors(&certificates)?;
    #[cfg(windows)]
    {
        let _ = roots;
        Ok(certificates)
    }
    #[cfg(not(windows))]
    {
        Ok(roots)
    }
}

fn system_ca_policy() -> Result<Policy, ConfigError> {
    #[cfg(any(target_vendor = "apple", windows))]
    {
        Ok(Policy::SystemCa)
    }
    #[cfg(not(any(target_vendor = "apple", windows)))]
    {
        // This is the same native-root loader used by the existing default
        // rustls-platform-verifier on these targets. Partial loader errors do
        // not discard other usable roots, but an empty store is never trusted.
        let loaded = rustls_native_certs::load_native_certs();
        let mut roots = rustls::RootCertStore::empty();
        roots.roots.try_reserve_exact(loaded.certs.len())?;
        roots.add_parsable_certificates(loaded.certs);
        if roots.is_empty() {
            return Err(ConfigError::Invalid(
                "the system trust store contains no usable trust anchors",
            ));
        }
        Ok(Policy::Explicit {
            roots,
            check_name: false,
            include_system: false,
        })
    }
}

/// A non-secret explanation of invalid TLS configuration material.
#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    /// Native material storage could not be reserved.
    #[error("cannot allocate TLS material storage: {0}")]
    Allocation(#[from] TryReserveError),
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
    let mut result = Vec::new();
    for certificate in CertificateDer::pem_slice_iter(pem) {
        let certificate = certificate.map_err(|_| MaterialError::Certificates)?;
        result.try_reserve(1)?;
        result.push(certificate);
    }
    if result.is_empty() {
        return Err(MaterialError::Certificates);
    }
    Ok(result)
}

/// Parse a complete CA-only PEM bundle, allowing surrounding explanation text.
///
/// # Errors
/// Returns a non-secret material failure for a noncertificate PEM section,
/// malformed section, or empty bundle. Client identity chains use the existing
/// [`certificates`] entry point independently of this CA source policy.
pub fn ca_certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, MaterialError> {
    let mut inside = false;
    // The mature reader silently skips unknown PEM labels and permits a new
    // BEGIN to replace an unfinished section. Only CA source policy is stricter:
    // gate labels and complete, unnested sections before delegating decoding.
    // Like that reader, recognize CR and LF and allow trailing ASCII spaces.
    // No decoded body or input-derived label is copied into an error.
    for line in pem.split(|byte| matches!(byte, b'\r' | b'\n')) {
        let line = line.trim_ascii_end();
        if line.trim_ascii_start().starts_with(b"-----BEGIN") {
            if inside || line != b"-----BEGIN CERTIFICATE-----" {
                return Err(MaterialError::Certificates);
            }
            inside = true;
        } else if line.trim_ascii_start().starts_with(b"-----END") {
            if !inside || line != b"-----END CERTIFICATE-----" {
                return Err(MaterialError::Certificates);
            }
            inside = false;
        }
    }
    if inside {
        return Err(MaterialError::Certificates);
    }
    let certificates = certificates(pem)?;
    if certificates
        .iter()
        .any(|certificate| certificate.as_ref().is_empty())
    {
        return Err(MaterialError::Certificates);
    }
    Ok(certificates)
}

fn owned_bytes(bytes: &[u8]) -> Result<Vec<u8>, TryReserveError> {
    let mut owned = Vec::new();
    owned.try_reserve_exact(bytes.len())?;
    owned.extend_from_slice(bytes);
    Ok(owned)
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
            Ok(PrivatePkcs8KeyDer::from(owned_bytes(plaintext.as_bytes())?).into())
        }
        "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY" => {
            if password.is_some() {
                return Err(MaterialError::UnexpectedPassword);
            }
            PrivateKeyDer::try_from(owned_bytes(document.as_bytes())?)
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

/// The supported TLS protocol version, parsed once from configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    /// TLS protocol version 1.2; the default configured minimum.
    #[default]
    Tls12,
    /// TLS protocol version 1.3.
    Tls13,
}

impl Version {
    /// The normalized public spelling of this configured protocol version.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tls12 => "TLSv1.2",
            Self::Tls13 => "TLSv1.3",
        }
    }
}

/// How explicit CA material relates to native system trust.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaMode {
    /// Explicit roots replace system roots; no material means system trust.
    #[default]
    Replace,
    /// Retain native system trust and add the explicit roots.
    Append,
}

impl std::str::FromStr for CaMode {
    type Err = ConfigError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
        if value.eq_ignore_ascii_case("replace") {
            Ok(Self::Replace)
        } else if value.eq_ignore_ascii_case("append") {
            Ok(Self::Append)
        } else {
            Err(ConfigError::Invalid("ca_mode must be replace or append"))
        }
    }
}

fn protocol_versions(
    minimum: Version,
    maximum: Option<Version>,
) -> Result<&'static [&'static rustls::SupportedProtocolVersion], ConfigError> {
    static TLS12: [&rustls::SupportedProtocolVersion; 1] = [&rustls::version::TLS12];
    static TLS13: [&rustls::SupportedProtocolVersion; 1] = [&rustls::version::TLS13];
    static BOTH: [&rustls::SupportedProtocolVersion; 2] =
        [&rustls::version::TLS13, &rustls::version::TLS12];
    if maximum.is_some_and(|maximum| maximum < minimum) {
        return Err(ConfigError::Invalid(
            "min_tls_version exceeds max_tls_version",
        ));
    }
    Ok(match (minimum, maximum) {
        (Version::Tls12, Some(Version::Tls12)) => &TLS12,
        (Version::Tls12, _) => &BOTH,
        (Version::Tls13, _) => &TLS13,
    })
}

/// Validate the configured range without enabling TLS or loading trust.
///
/// # Errors
/// Returns an invalid configuration when the minimum exceeds the maximum.
pub fn validate_range(minimum: Version, maximum: Option<Version>) -> Result<(), ConfigError> {
    protocol_versions(minimum, maximum).map(|_| ())
}

/// A TLS configuration failure, separate from a peer's later handshake error.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Native configuration storage could not be reserved.
    #[error("cannot allocate TLS configuration storage: {0}")]
    Allocation(#[from] TryReserveError),
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

impl std::str::FromStr for Version {
    type Err = ConfigError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
        if !value.is_ascii() {
            return Err(ConfigError::Invalid(
                "TLS version must be TLS 1.2 or TLS 1.3",
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
                "TLS version must be TLS 1.2 or TLS 1.3",
            )),
        }
    }
}

/// Build one immutable TLS client configuration from prepared materials.
/// Explicit roots replace the system store unless append is selected. REQUIRED
/// skips chain/name trust only; all modes verify handshake signatures.
///
/// # Errors
/// Returns invalid-combination, trust-material or provider failures. DISABLED
/// accepts no roots or client signing identity and returns None.
pub fn client_config(
    mode: Mode,
    minimum: Version,
    maximum: Option<Version>,
    ca_mode: CaMode,
    roots: Option<Vec<CertificateDer<'static>>>,
    identity: Option<Arc<CertifiedKey>>,
) -> Result<Option<Arc<rustls::ClientConfig>>, ConfigError> {
    use rustls::client::danger::ServerCertVerifier;
    let versions = protocol_versions(minimum, maximum)?;
    if mode == Mode::Disabled {
        if roots.is_some() || identity.is_some() {
            return Err(ConfigError::Invalid(
                "DISABLED conflicts with CA or client certificate material",
            ));
        }
        return Ok(None);
    }
    if mode == Mode::Required && roots.is_some() {
        return Err(ConfigError::Invalid(
            "REQUIRED does not accept custom CA material",
        ));
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
            policy: system_ca_policy()?,
        }),
        (_, Some(certificates)) => Arc::new(PolicyVerifier {
            provider: Arc::clone(&provider),
            policy: explicit_policy(certificates, mode == Mode::VerifyIdentity, ca_mode)?,
        }),
        _ => {
            return Err(ConfigError::Invalid(
                "invalid TLS mode/material combination",
            ));
        }
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

fn explicit_policy(
    certificates: Vec<CertificateDer<'static>>,
    check_name: bool,
    ca_mode: CaMode,
) -> Result<Policy, ConfigError> {
    let roots = prepare_anchors(certificates)?;
    #[cfg(not(any(target_vendor = "apple", windows)))]
    let roots = if ca_mode == CaMode::Append {
        let loaded = rustls_native_certs::load_native_certs();
        let mut system = rustls::RootCertStore::empty();
        system.roots.try_reserve_exact(loaded.certs.len())?;
        system.add_parsable_certificates(loaded.certs);
        if system.is_empty() {
            return Err(ConfigError::Invalid(
                "ca_mode append requires usable system trust anchors; the system trust source is unavailable",
            ));
        }
        let mut roots = roots;
        roots.roots.try_reserve(system.roots.len())?;
        roots.extend(system.roots);
        roots
    } else {
        roots
    };
    Ok(Policy::Explicit {
        roots,
        check_name,
        include_system: ca_mode == CaMode::Append,
    })
}

#[derive(Debug)]
enum Policy {
    Required,
    Explicit {
        roots: PreparedRoots,
        check_name: bool,
        include_system: bool,
    },
    #[cfg(any(target_vendor = "apple", windows))]
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
        match &self.policy {
            Policy::Required => (),
            Policy::Explicit {
                roots,
                check_name,
                include_system,
            } => {
                verify_platform_trust(
                    end,
                    intermediates,
                    check_name.then_some(name),
                    Some(roots),
                    *include_system,
                    ocsp,
                    now,
                )?;
            }
            #[cfg(any(target_vendor = "apple", windows))]
            Policy::SystemCa => {
                verify_platform_trust(end, intermediates, None, None, true, ocsp, now)?;
            }
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
fn verify_platform_trust(
    end: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    name: Option<&rustls_pki_types::ServerName<'_>>,
    anchors: Option<&PreparedRoots>,
    include_system: bool,
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
    let hostname = name.map(rustls_pki_types::ServerName::to_str);
    let policy = SecPolicy::create_ssl(SslProtocolSide::SERVER, hostname.as_deref());
    let mut trust = SecTrust::create_with_certificates(&certificates, &[policy])
        .map_err(|error| rustls::Error::General(error.to_string()))?;
    if let Some(anchors) = anchors {
        trust
            .set_anchor_certificates(anchors)
            .map_err(|error| rustls::Error::General(error.to_string()))?;
        // Root-source composition does not replace the native SSL policy,
        // including its applicable revocation rules.
        trust
            .set_trust_anchor_certificates_only(!include_system)
            .map_err(|error| rustls::Error::General(error.to_string()))?;
    }
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

#[cfg(not(any(target_vendor = "apple", windows)))]
fn verify_platform_trust(
    end: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    name: Option<&rustls_pki_types::ServerName<'_>>,
    anchors: Option<&PreparedRoots>,
    _include_system: bool,
    _ocsp: &[u8],
    now: rustls_pki_types::UnixTime,
) -> Result<(), rustls::Error> {
    let roots = anchors.ok_or_else(|| {
        rustls::Error::General("system trust anchors were not prepared".to_owned())
    })?;
    let certificate = rustls::server::ParsedCertificate::try_from(end)?;
    rustls::client::verify_server_cert_signed_by_trust_anchor(
        &certificate,
        roots,
        intermediates,
        now,
        provider().signature_verification_algorithms.all,
    )?;
    if let Some(name) = name {
        rustls::client::verify_server_name(&certificate, name)?;
    }
    Ok(())
}

#[cfg(windows)]
fn verify_platform_trust(
    end: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    name: Option<&rustls_pki_types::ServerName<'_>>,
    anchors: Option<&PreparedRoots>,
    include_system: bool,
    ocsp: &[u8],
    now: rustls_pki_types::UnixTime,
) -> Result<(), rustls::Error> {
    windows::verify(
        end,
        intermediates,
        name,
        anchors.map(Vec::as_slice),
        include_system,
        ocsp,
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_bounds_validate_even_when_tls_is_disabled() {
        assert!(
            client_config(
                Mode::Disabled,
                Version::Tls13,
                Some(Version::Tls12),
                CaMode::Replace,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            client_config(
                Mode::Disabled,
                Version::Tls12,
                Some(Version::Tls13),
                CaMode::Append,
                None,
                None,
            )
            .unwrap()
            .is_none()
        );
        for (minimum, maximum, expected) in [
            (
                Version::Tls12,
                None,
                vec![
                    rustls::ProtocolVersion::TLSv1_3,
                    rustls::ProtocolVersion::TLSv1_2,
                ],
            ),
            (
                Version::Tls12,
                Some(Version::Tls12),
                vec![rustls::ProtocolVersion::TLSv1_2],
            ),
            (
                Version::Tls13,
                Some(Version::Tls13),
                vec![rustls::ProtocolVersion::TLSv1_3],
            ),
        ] {
            assert_eq!(
                protocol_versions(minimum, maximum)
                    .unwrap()
                    .iter()
                    .map(|v| v.version)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn ca_policy_tokens_are_native_and_strict() {
        assert_eq!(" APPEND\t".parse::<CaMode>().unwrap(), CaMode::Append);
        assert_eq!("replace".parse::<CaMode>().unwrap(), CaMode::Replace);
        assert!("add".parse::<CaMode>().is_err());
        assert!("\u{a0}append".parse::<CaMode>().is_err());
    }

    #[test]
    #[ignore = "requires the owned MySQL fixture certificate material"]
    fn explicit_roots_keep_platform_ssl_name_usage_and_time_policy() {
        use rustls::client::danger::ServerCertVerifier;
        use std::time::Duration;

        let root = std::path::PathBuf::from(
            std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").expect("owned fixture root"),
        );
        let chain = certificates(&std::fs::read(root.join("tls/server.pem")).unwrap()).unwrap();
        let ca = certificates(&std::fs::read(root.join("tls/ca.pem")).unwrap()).unwrap();
        let client = certificates(&std::fs::read(root.join("tls/client.pem")).unwrap()).unwrap();
        let now = rustls_pki_types::UnixTime::now();
        let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
        let wrong = rustls_pki_types::ServerName::try_from("unrelated.invalid").unwrap();
        let identity = PolicyVerifier {
            provider: provider(),
            policy: Policy::Explicit {
                roots: prepare_anchors(ca.clone()).unwrap(),
                check_name: true,
                include_system: false,
            },
        };
        let verification = identity.verify_server_cert(&chain[0], &[], &name, &[], now);
        assert!(
            verification.is_ok(),
            "native identity verification: {verification:?}"
        );
        let absolute = rustls_pki_types::ServerName::try_from("localhost.").unwrap();
        assert!(
            identity
                .verify_server_cert(&chain[0], &[], &absolute, &[], now)
                .is_ok()
        );
        assert!(
            identity
                .verify_server_cert(&chain[0], &[], &wrong, &[], now)
                .is_err()
        );
        assert!(
            identity
                .verify_server_cert(&client[0], &[], &name, &[], now)
                .is_err()
        );
        let future = rustls_pki_types::UnixTime::since_unix_epoch(Duration::from_secs(
            now.as_secs() + 30 * 86400,
        ));
        assert!(
            identity
                .verify_server_cert(&chain[0], &[], &name, &[], future)
                .is_err()
        );
        let ca_only = PolicyVerifier {
            provider: provider(),
            policy: Policy::Explicit {
                roots: prepare_anchors(ca).unwrap(),
                check_name: false,
                include_system: false,
            },
        };
        assert!(
            ca_only
                .verify_server_cert(&chain[0], &[], &wrong, &[], now)
                .is_ok()
        );
        let wrong_root = PolicyVerifier {
            provider: provider(),
            policy: Policy::Explicit {
                roots: prepare_anchors(client).unwrap(),
                check_name: false,
                include_system: false,
            },
        };
        assert!(
            wrong_root
                .verify_server_cert(&chain[0], &[], &name, &[], now)
                .is_err()
        );
    }

    #[test]
    fn mode_and_minimum_version_tokens_are_parsed_once_and_strictly() {
        assert_eq!(
            " verify_identity\t".parse::<Mode>().unwrap(),
            Mode::VerifyIdentity
        );
        for text in ["TLS 1.3", "TLSv1.3", "tlsv\t1.3", "1.3"] {
            assert_eq!(text.parse::<Version>().unwrap(), Version::Tls13);
        }
        for text in ["TLS 1 .3", "TLS1.1", "1.4", "TLS\u{a0}1.3"] {
            assert!(text.parse::<Version>().is_err());
        }
        assert!("PREFERRED".parse::<Mode>().is_err());
    }
}
