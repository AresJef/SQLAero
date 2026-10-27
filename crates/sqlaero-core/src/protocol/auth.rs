//! caching_sha2_password steps without server-key fetching or Python callbacks.

use super::framing::WireError;
use super::packets::{self, OkKind};
use aws_lc_rs::rsa::{OAEP_SHA1_MGF1SHA1, OaepPublicEncryptingKey, PublicEncryptingKey};
use mysql_common::auth::plugins::caching_sha2_password::scramble_sha256;
use mysql_common::constants::CapabilityFlags;
use mysql_common::io::ParseBuf;
use mysql_common::packets::{AuthPlugin, AuthSwitchRequest, OkPacket, ServerError};
use rustls_pki_types::SubjectPublicKeyInfoDer;
use rustls_pki_types::pem::PemObject;
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

/// Authentication errors retain real server fields and never print credentials.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// A malformed packet or sequence-related validation failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// MySQL returned a complete ERR.
    #[error("MySQL authentication failed: {0}")]
    Server(ServerError<'static>),
    /// The requested plugin is outside this client's declared support.
    #[error("unsupported authentication plugin '{0}'; only caching_sha2_password is supported")]
    UnsupportedPlugin(String),
    /// Bare TCP full authentication requires a separately trusted local key.
    #[error(
        "bare TCP full authentication requires server_public_key_path; automatic server-key fetching is disabled"
    )]
    TrustedKeyRequired,
    /// The explicit local key was not a supported RSA SPKI key.
    #[error(
        "server_public_key_path must contain a supported RSA SubjectPublicKeyInfo PEM public key"
    )]
    PublicKey,
    /// The password cannot fit, or the selected crypto provider rejected it.
    #[error(
        "RSA-OAEP password encryption failed; check key size or use verified TLS or a Unix socket"
    )]
    Encryption,
}

fn unsupported_plugin(plugin: &[u8]) -> AuthError {
    let mut name: String = plugin
        .iter()
        .take(32)
        .flat_map(|byte| std::ascii::escape_default(*byte))
        .map(char::from)
        .collect();
    if plugin.len() > 32 {
        name.push_str(" [truncated]");
    }
    AuthError::UnsupportedPlugin(name)
}

/// Load the caller's explicit local trust material. This is also checked when
/// fast authentication or TLS makes RSA unnecessary; bad config is not ignored.
///
/// # Errors
/// Returns [`AuthError::PublicKey`] for unsupported or malformed key material.
pub fn server_key(pem: &[u8]) -> Result<Arc<OaepPublicEncryptingKey>, AuthError> {
    let key = SubjectPublicKeyInfoDer::from_pem_slice(pem).map_err(|_| AuthError::PublicKey)?;
    let key = PublicEncryptingKey::from_der(key.as_ref()).map_err(|_| AuthError::PublicKey)?;
    OaepPublicEncryptingKey::new(key)
        .map(Arc::new)
        .map_err(|_| AuthError::PublicKey)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Challenge,
    FinalConfirmation,
    Finished,
    Failed,
}

/// Authentication path actually signaled by the server, for truthful
/// diagnostics and distinct cold/warm establishment measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuthPath {
    /// Final OK arrived directly, with no fast/full intermediate signal.
    #[default]
    Direct,
    /// Fast-auth success was followed by its required final confirmation.
    Fast,
    /// The server requested the complete authentication exchange.
    Full,
}

/// One concrete authentication continuation, not a generic driver action.
pub enum AuthResponse {
    /// Send this exact owned native response, then read the next server packet.
    Reply(Zeroizing<Vec<u8>>),
    /// Fast success is provisional: wait for the required final OK or ERR.
    AwaitConfirmation,
    /// A complete final OK has actually been parsed.
    Complete(OkPacket<'static>),
}

/// Authentication-only secrets and state. Controlled password storage is erased
/// at final confirmation or failure, and is never retained for reconnection.
pub struct CachingSha2 {
    password: Zeroizing<Vec<u8>>,
    nonce: [u8; 20],
    secure: bool,
    key: Option<Arc<OaepPublicEncryptingKey>>,
    capabilities: CapabilityFlags,
    phase: Phase,
    switched: bool,
    send_limit: usize,
    path: AuthPath,
}

impl CachingSha2 {
    /// Start the declared plugin using a checked 20-byte challenge. `secure`
    /// means an established TLS stream or the explicit local Unix channel.
    ///
    /// # Errors
    /// Returns UnsupportedPlugin for any undeclared plugin, without trying it.
    pub fn new(
        plugin: &[u8],
        password: Zeroizing<Vec<u8>>,
        nonce: [u8; 20],
        secure: bool,
        key: Option<Arc<OaepPublicEncryptingKey>>,
        capabilities: CapabilityFlags,
        send_limit: usize,
    ) -> Result<Self, AuthError> {
        if plugin != b"caching_sha2_password" {
            return Err(unsupported_plugin(plugin));
        }
        Ok(Self {
            password,
            nonce,
            secure,
            key,
            capabilities,
            phase: Phase::Challenge,
            switched: false,
            send_limit,
            path: AuthPath::Direct,
        })
    }

    /// Compute the initial challenge response using mysql_common's public
    /// SHA-256 primitive. Empty passwords produce the protocol's empty response.
    #[must_use]
    pub fn initial_response(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(
            scramble_sha256(&self.nonce, &self.password)
                .map(Vec::from)
                .unwrap_or_default(),
        )
    }

    /// Return the protocol path observed so far, without claiming final success.
    #[must_use]
    pub fn path(&self) -> AuthPath {
        self.path
    }

    /// Process one complete server authentication packet in the current phase.
    ///
    /// # Errors
    /// Returns real ERR, invalid phase/packet or unsafe/missing key failures.
    /// No branch emits REQUEST_SERVER_KEY or trusts unsolicited key material.
    pub fn advance(&mut self, data: &[u8]) -> Result<AuthResponse, AuthError> {
        let result = self.advance_inner(data);
        if result.is_err() {
            self.phase = Phase::Failed;
            self.password.zeroize();
        }
        result
    }

    fn advance_inner(&mut self, data: &[u8]) -> Result<AuthResponse, AuthError> {
        if matches!(self.phase, Phase::Finished | Phase::Failed) {
            return Err(WireError::Malformed("authentication exchange already ended").into());
        }
        if data.first() == Some(&0xff) {
            return Err(AuthError::Server(
                packets::server_error(data, self.capabilities)?.into_owned(),
            ));
        }
        if data.first() == Some(&0) {
            let ok = packets::ok(data, self.capabilities, OkKind::Command)?.into_owned();
            self.phase = Phase::Finished;
            self.password.zeroize();
            return Ok(AuthResponse::Complete(ok));
        }
        if self.phase != Phase::Challenge {
            return Err(WireError::Malformed("expected final authentication OK or ERR").into());
        }
        match data {
            [1, 3] => {
                self.path = AuthPath::Fast;
                self.phase = Phase::FinalConfirmation;
                Ok(AuthResponse::AwaitConfirmation)
            }
            [1, 4] => {
                self.path = AuthPath::Full;
                let reply = self.full_response()?;
                self.phase = Phase::FinalConfirmation;
                Ok(AuthResponse::Reply(reply))
            }
            [0xfe, ..] if !self.switched => {
                let mut buffer = ParseBuf(data);
                let request: AuthSwitchRequest<'_> = buffer.parse(()).map_err(WireError::Decode)?;
                if !buffer.is_empty() {
                    return Err(WireError::Malformed("trailing AuthSwitch fields").into());
                }
                if request.auth_plugin() != AuthPlugin::CachingSha2Password {
                    return Err(unsupported_plugin(request.auth_plugin().as_bytes()));
                }
                self.nonce = request.plugin_data().try_into().map_err(|_| {
                    WireError::Malformed("AuthSwitch challenge must contain 20 bytes")
                })?;
                self.switched = true;
                Ok(AuthResponse::Reply(self.initial_response()))
            }
            _ => Err(WireError::Malformed("unexpected authentication signal or phase").into()),
        }
    }

    fn full_response(&mut self) -> Result<Zeroizing<Vec<u8>>, AuthError> {
        let length = self.password.len().checked_add(1).ok_or(WireError::Limit {
            required: usize::MAX,
            limit: self.send_limit,
        })?;
        if self.secure {
            if length > self.send_limit {
                return Err(WireError::Limit {
                    required: length,
                    limit: self.send_limit,
                }
                .into());
            }
            self.password
                .try_reserve(1)
                .map_err(WireError::Allocation)?;
            self.password.push(0);
            return Ok(std::mem::take(&mut self.password));
        }
        let key = self.key.as_ref().ok_or(AuthError::TrustedKeyRequired)?;
        if length > key.max_plaintext_size(&OAEP_SHA1_MGF1SHA1) {
            return Err(AuthError::Encryption);
        }
        let size = key.ciphertext_size();
        if size > self.send_limit {
            return Err(WireError::Limit {
                required: size,
                limit: self.send_limit,
            }
            .into());
        }
        self.password
            .try_reserve(1)
            .map_err(WireError::Allocation)?;
        self.password.push(0);
        for (index, byte) in self.password.iter_mut().enumerate() {
            *byte ^= self.nonce[index % self.nonce.len()];
        }
        // MySQL's caching_sha2_password wire contract specifies OAEP with SHA-1
        // and MGF1-SHA1. This is not a configurable password-storage algorithm.
        let mut ciphertext = Zeroizing::new(Vec::new());
        ciphertext
            .try_reserve_exact(size)
            .map_err(WireError::Allocation)?;
        ciphertext.resize(size, 0);
        key.encrypt(&OAEP_SHA1_MGF1SHA1, &self.password, &mut ciphertext, None)
            .map_err(|_| AuthError::Encryption)?;
        self.password.zeroize();
        Ok(ciphertext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn auth(secure: bool) -> CachingSha2 {
        CachingSha2::new(
            b"caching_sha2_password",
            Zeroizing::new(b"private-test-value".to_vec()),
            [0x41; 20],
            secure,
            None,
            CapabilityFlags::CLIENT_PROTOCOL_41,
            64 * 1024,
        )
        .unwrap()
    }

    #[test]
    fn provisional_fast_success_still_requires_final_confirmation() {
        let mut auth = auth(true);
        assert!(matches!(
            auth.advance(&[1, 3]).unwrap(),
            AuthResponse::AwaitConfirmation
        ));
        assert_eq!(auth.phase, Phase::FinalConfirmation);
        assert!(!auth.password.is_empty());
        assert!(matches!(
            auth.advance(&[0, 0, 0, 2, 0, 0, 0]).unwrap(),
            AuthResponse::Complete(_)
        ));
        assert!(auth.password.is_empty());
    }

    #[test]
    fn a_valid_auth_switch_updates_only_the_challenge_and_keeps_final_confirmation() {
        let mut auth = auth(true);
        let original = auth.initial_response();
        let mut packet = b"\xfecaching_sha2_password\0".to_vec();
        packet.extend_from_slice(&[0x42; 20]);
        packet.push(0);
        match auth.advance(&packet).unwrap() {
            AuthResponse::Reply(response) => assert_ne!(&response[..], &original[..]),
            _ => panic!("expected switched challenge response"),
        }
        assert_eq!(auth.phase, Phase::Challenge);
        assert!(matches!(
            auth.advance(&[1, 3]).unwrap(),
            AuthResponse::AwaitConfirmation
        ));
        assert!(matches!(
            auth.advance(&[0, 0, 0, 2, 0, 0, 0]).unwrap(),
            AuthResponse::Complete(_)
        ));
    }

    #[test]
    fn unsupported_actual_switch_is_rejected_with_bounded_escaped_name() {
        for name in [b"mysql_native_password".to_vec(), b"unknown\n".repeat(1000)] {
            let mut auth = auth(true);
            let mut packet = vec![0xfe];
            packet.extend_from_slice(&name);
            packet.push(0);
            packet.extend_from_slice(&[0x42; 20]);
            packet.push(0);
            let error = auth
                .advance(&packet)
                .err()
                .expect("unsupported actual plugin");
            assert!(matches!(error, AuthError::UnsupportedPlugin(_)));
            let display = error.to_string();
            assert!(!display.contains('\n') && !display.contains("private-test-value"));
            assert!(display.len() < 256);
            assert_eq!(auth.phase, Phase::Failed);
            assert!(auth.password.is_empty());
        }
    }

    #[test]
    fn authentication_err_still_requires_the_negotiated_sqlstate_marker() {
        let mut auth = auth(true);
        assert!(matches!(
            auth.advance(b"\xff\x10\x04Too many connections"),
            Err(AuthError::Wire(_))
        ));
    }

    #[test]
    fn full_response_uses_only_a_secure_channel_or_explicit_trusted_key() {
        let mut secure = auth(true);
        match secure.advance(&[1, 4]).unwrap() {
            AuthResponse::Reply(response) => assert_eq!(&response[..], b"private-test-value\0"),
            _ => panic!("expected the full response"),
        }
        let mut bare = auth(false);
        assert!(matches!(
            bare.advance(&[1, 4]),
            Err(AuthError::TrustedKeyRequired)
        ));
        assert!(bare.password.is_empty());
    }

    #[test]
    fn final_confirmation_rejects_switching_back_and_preserves_server_errors() {
        let mut auth = auth(true);
        auth.advance(&[1, 3]).unwrap();
        assert!(
            auth.advance(b"\xfecaching_sha2_password\0new-challenge\0")
                .is_err()
        );
        let mut auth = super::tests::auth(true);
        match auth.advance(b"\xff\x15\x04#28000Access denied") {
            Err(AuthError::Server(error)) => assert_eq!(error.error_code(), 1045),
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("expected the server error"),
        }
    }
}
