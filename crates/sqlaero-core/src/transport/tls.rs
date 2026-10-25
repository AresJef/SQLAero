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
