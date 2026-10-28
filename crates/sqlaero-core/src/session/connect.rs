//! Concrete synchronous establishment steps sharing native protocol primitives.

use crate::protocol::auth::{self, AuthError, AuthPath, AuthResponse, CachingSha2};
use crate::protocol::framing::{MAX_MESSAGE_PAYLOAD, MessageReader, MessageWriter, WireError};
use crate::protocol::packets;
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::{Direction, IoBudget, TimeoutKind};
use crate::transport::socket::{Transport, WaitStatus};
use mysql_common::constants::{CapabilityFlags, StatusFlags};
use mysql_common::packets::{AuthPlugin, HandshakeResponse, SslRequest};
use mysql_common::proto::MySerialize;
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use std::collections::VecDeque;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use zeroize::{Zeroize, Zeroizing};

/// An explicitly selected native endpoint; Unix never falls back to TCP.
#[derive(Debug)]
pub enum Endpoint {
    /// Resolve the original host for dialing; TLS identity remains separate.
    Tcp {
        /// Original host, without SQL quoting or normalization.
        host: String,
        /// TCP port already validated at the binding boundary.
        port: u16,
    },
    /// A normalized, lossless native path.
    Unix(PathBuf),
}

/// Prepared TLS configuration and an already validated expected identity.
pub struct ConnectTls {
    /// Immutable provider, verifier and optional client signing identity.
    pub config: Arc<ClientConfig>,
    /// Expected DNS/IP identity or the non-SNI internal name for unnamed Unix.
    pub name: ServerName<'static>,
}

/// Establishment-only inputs. Python objects and callbacks are not permitted.
/// This internal Rust API receives a resolved TLS policy, not Python defaults.
pub struct ConnectParams {
    /// Exactly one selected endpoint.
    pub endpoint: Endpoint,
    /// User text, strictly UTF-8 in the fixed utf8mb4 handshake context.
    pub user: String,
    /// UTF-8 password storage, erased as soon as authentication no longer needs it.
    pub password: Zeroizing<Vec<u8>>,
    /// Optional initial database, selected in the authentication response.
    pub database: Option<String>,
    /// Explicitly resolved TLS policy; None means no TLS on this internal path.
    pub tls: Option<ConnectTls>,
    /// Optional trusted local RSA SPKI public-key file, validated even if unused.
    pub server_public_key: Option<PathBuf>,
    /// Original establishment budget, shared with later session initialization.
    pub connect_timeout: Option<Duration>,
    /// Maximum continuous read wait without actual progress.
    pub read_timeout: Option<Duration>,
    /// Maximum continuous write wait without actual progress.
    pub write_timeout: Option<Duration>,
    /// Client logical-send limit, including command bytes when present.
    pub max_send: usize,
    /// Client logical-receive limit, excluding physical frame headers.
    pub max_receive: usize,
}

/// Metadata actually received from the greeting and final authentication OK.
#[derive(Debug)]
pub struct ServerMetadata {
    /// Real protocol-reported server version text.
    pub version: String,
    /// Parsed target version used only for supported-line checks.
    pub version_tuple: (u16, u16, u16),
    /// Server connection identifier.
    pub connection_id: u32,
    /// Status flags from the latest complete authentication confirmation.
    pub status: StatusFlags,
    /// Direct, fast or full authentication actually observed on the wire.
    pub auth_path: AuthPath,
}

/// Establishment failure with no automatic retry or security downgrade.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// An invalid native configuration, before opening a socket where possible.
    #[error("invalid connection configuration: {0}")]
    Configuration(&'static str),
    /// A source file or host resolution operation failed.
    #[error("connection preparation failed: {0}")]
    Preparation(#[from] std::io::Error),
    /// The server did not provide the required modern MySQL protocol.
    #[error("unsupported MySQL server or capability: {0}")]
    Unsupported(&'static str),
    /// A complete packet was malformed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// A real transport, TLS or timeout failure occurred.
    #[error(transparent)]
    Channel(#[from] ChannelError),
    /// The actual authentication result failed.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// The original establishment deadline expired during preparation.
    #[error(transparent)]
    Timeout(#[from] TimeoutKind),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Socket,
    Greeting,
    SslRequest,
    Tls,
    Login,
    Authentication,
    Reply,
    Finished,
    Failed,
}

struct Outgoing {
    bytes: Zeroizing<Vec<u8>>,
    framing: MessageWriter,
}

/// A successfully authenticated transport. It is deliberately not a Ready
/// SQLAero session: initialization must consume the same original budget.
pub struct Authenticated {
    /// The same endpoint, TLS state and packet-sequence context.
    pub channel: Channel,
    /// Actual negotiated capabilities.
    pub capabilities: CapabilityFlags,
    /// Confirmed server metadata.
    pub metadata: ServerMetadata,
    /// Remaining original establishment budget, never reset after login.
    pub budget: IoBudget,
    /// Effective client send limit, before the server's value is read.
    pub max_send: usize,
    /// Effective client receive limit.
    pub max_receive: usize,
}

/// One concrete connect continuation. Only real TCP address attempts and the
/// protocol's own steps are represented; there is no general action engine.
pub struct Establish {
    params: ConnectParams,
    budget: Option<IoBudget>,
    addresses: VecDeque<SocketAddr>,
    channel: Option<Channel>,
    phase: Phase,
    incoming: MessageReader,
    outgoing: Option<Outgoing>,
    auth: Option<CachingSha2>,
    key: Option<Arc<aws_lc_rs::rsa::OaepPublicEncryptingKey>>,
    metadata: Option<ServerMetadata>,
    capabilities: CapabilityFlags,
    nonce: [u8; 20],
}

impl Establish {
    /// Validate native fields, fix the original deadline, prepare explicit
    /// trust material and resolve TCP addresses once. System DNS may block;
    /// its elapsed time is included and checked immediately on return.
    ///
    /// # Errors
    /// Returns invalid configuration, file/key, DNS or elapsed-budget failures.
    pub fn new(params: ConnectParams) -> Result<Self, ConnectError> {
        let budget = IoBudget::new(
            TimeoutKind::Connect,
            params.connect_timeout,
            params.read_timeout,
            params.write_timeout,
        );
        Self::with_budget(params, budget)
    }

    /// Continue with the original boundary-owned establishment deadline, so
    /// configuration/TLS preparation is not granted a second timeout window.
    ///
    /// # Errors
    /// Returns the same validation/preparation failures as [`Self::new`].
    pub fn with_budget(params: ConnectParams, budget: IoBudget) -> Result<Self, ConnectError> {
        if params.user.is_empty()
            || params.user.contains('\0')
            || params.password.contains(&0)
            || params
                .database
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.contains('\0'))
        {
            return Err(ConnectError::Configuration(
                "user/database must be nonempty when provided; identity fields cannot contain NUL",
            ));
        }
        if params.max_send == 0
            || params.max_receive == 0
            || params.max_send > MAX_MESSAGE_PAYLOAD
            || params.max_receive > MAX_MESSAGE_PAYLOAD
        {
            return Err(ConnectError::Configuration(
                "logical message limits must be within 1..=1GiB",
            ));
        }
        let key = params
            .server_public_key
            .as_ref()
            .map(|path| {
                std::fs::read(path)
                    .map_err(ConnectError::from)
                    .and_then(|bytes| auth::server_key(&bytes).map_err(ConnectError::from))
            })
            .transpose()?;
        let addresses = match &params.endpoint {
            Endpoint::Tcp { host, port } => (host.as_str(), *port).to_socket_addrs()?.collect(),
            Endpoint::Unix(_) => VecDeque::new(),
        };
        budget.check()?;
        Ok(Self {
            incoming: MessageReader::new(params.max_receive),
            params,
            budget: Some(budget),
            addresses,
            channel: None,
            phase: Phase::Socket,
            outgoing: None,
            auth: None,
            key,
            metadata: None,
            capabilities: CapabilityFlags::empty(),
            nonce: [0; 20],
        })
    }

    /// Advance native establishment until complete or a host control boundary.
    /// The binding releases the GIL for this call and checks signals on Control.
    ///
    /// # Errors
    /// Returns a real establishment failure. Controlled secrets and transport
    /// are released immediately on failure; no alternate TLS policy is tried.
    pub fn advance(&mut self) -> Result<Step<Authenticated>, ConnectError> {
        let result = self.advance_inner();
        if result.is_err() {
            self.params.password.zeroize();
            self.auth = None;
            self.outgoing = None;
            self.channel = None;
            self.phase = Phase::Failed;
        }
        result
    }

    fn advance_inner(&mut self) -> Result<Step<Authenticated>, ConnectError> {
        loop {
            let budget = self.budget.as_mut().ok_or(ConnectError::Configuration(
                "establishment already consumed",
            ))?;
            budget.check()?;
            if self.phase == Phase::Socket {
                if self.channel.is_none() {
                    let transport = match &self.params.endpoint {
                        #[cfg(unix)]
                        Endpoint::Unix(path) => Transport::unix(path),
                        #[cfg(not(unix))]
                        Endpoint::Unix(_) => {
                            return Err(ConnectError::Unsupported(
                                "unix_socket is not supported on this platform; use host and port for TCP",
                            ));
                        }
                        Endpoint::Tcp { .. } => {
                            let address =
                                self.addresses
                                    .pop_front()
                                    .ok_or(ConnectError::Configuration(
                                        "host resolved to no usable address",
                                    ))?;
                            match Transport::tcp(address) {
                                Ok(transport) => Ok(transport),
                                Err(_) if !self.addresses.is_empty() => continue,
                                Err(error) => Err(error),
                            }
                        }
                    }
                    .map_err(ChannelError::from)?;
                    self.channel = Some(Channel::new(
                        transport,
                        matches!(self.params.endpoint, Endpoint::Unix(_)),
                    ));
                }
                let channel = self
                    .channel
                    .as_mut()
                    .ok_or(ConnectError::Configuration("missing endpoint"))?;
                match channel.connected() {
                    Ok(true) => self.phase = Phase::Greeting,
                    Ok(false) => {
                        if channel.wait(Direction::Write, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                    }
                    Err(_) if !self.addresses.is_empty() => self.channel = None,
                    Err(error) => return Err(error.into()),
                }
                continue;
            }
            let channel = self
                .channel
                .as_mut()
                .ok_or(ConnectError::Configuration("missing channel"))?;
            if self.phase == Phase::Tls {
                match channel.handshake_step(budget)? {
                    Step::Ready(()) => {
                        self.phase = Phase::Login;
                        self.prepare_login()?;
                    }
                    Step::Wait(direction) => {
                        if channel.wait(direction, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                    }
                    Step::Control => return Ok(Step::Control),
                }
                continue;
            }
            if matches!(self.phase, Phase::SslRequest | Phase::Login | Phase::Reply) {
                let outgoing = self.outgoing.as_mut().ok_or(ConnectError::Configuration(
                    "missing authentication response",
                ))?;
                match channel.send_step(&mut outgoing.framing, &outgoing.bytes, budget)? {
                    Step::Ready(()) => {
                        self.outgoing = None;
                        if self.phase == Phase::SslRequest {
                            let tls =
                                self.params.tls.as_ref().ok_or(ConnectError::Configuration(
                                    "missing TLS configuration",
                                ))?;
                            channel.start_tls(Arc::clone(&tls.config), tls.name.clone())?;
                            self.phase = Phase::Tls;
                        } else {
                            self.phase = Phase::Authentication;
                        }
                    }
                    Step::Wait(direction) => {
                        if channel.wait(direction, budget)? == WaitStatus::Control {
                            return Ok(Step::Control);
                        }
                    }
                    Step::Control => return Ok(Step::Control),
                }
                continue;
            }
            if !matches!(self.phase, Phase::Greeting | Phase::Authentication) {
                return Err(ConnectError::Configuration("establishment already ended"));
            }
            let data = match channel.receive_step(&mut self.incoming, budget)? {
                Step::Ready(data) => data,
                Step::Wait(direction) => {
                    if channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                    continue;
                }
                Step::Control => return Ok(Step::Control),
            };
            self.incoming = MessageReader::new(self.params.max_receive);
            if self.phase == Phase::Greeting {
                if data.first() == Some(&0xff) {
                    return Err(AuthError::Server(
                        packets::server_error(&data, CapabilityFlags::empty())?.into_owned(),
                    )
                    .into());
                }
                let greeting = packets::handshake(&data)?;
                let version = greeting
                    .server_version_parsed()
                    .ok_or(ConnectError::Unsupported("unrecognized version"))?;
                if !matches!((version.0, version.1), (8, 4) | (9, 7)) {
                    return Err(ConnectError::Unsupported("expected MySQL 8.4 or 9.7"));
                }
                self.metadata = Some(ServerMetadata {
                    version: std::str::from_utf8(greeting.server_version_ref())
                        .map_err(|_| WireError::Malformed("invalid server version text"))?
                        .to_owned(),
                    version_tuple: version,
                    connection_id: greeting.connection_id(),
                    status: greeting.status_flags(),
                    auth_path: AuthPath::Direct,
                });
                self.nonce = greeting
                    .nonce()
                    .try_into()
                    .map_err(|_| WireError::Malformed("invalid greeting nonce"))?;
                // The greeting name is a hint, not this account's final
                // authentication requirement. Declare our supported plugin in
                // login and judge any subsequent AuthSwitch independently.
                greeting
                    .auth_plugin_name_ref()
                    .filter(|name| !name.is_empty())
                    .ok_or(ConnectError::Unsupported("authentication plugin missing"))?;
                let required = CapabilityFlags::CLIENT_PROTOCOL_41
                    | CapabilityFlags::CLIENT_SECURE_CONNECTION
                    | CapabilityFlags::CLIENT_PLUGIN_AUTH
                    | CapabilityFlags::CLIENT_SESSION_TRACK;
                if !greeting.capabilities().contains(required) {
                    return Err(ConnectError::Unsupported(
                        "required protocol-41/auth/session-track capabilities missing",
                    ));
                }
                self.capabilities = required
                    | ((CapabilityFlags::CLIENT_LONG_PASSWORD
                        | CapabilityFlags::CLIENT_LONG_FLAG
                        | CapabilityFlags::CLIENT_TRANSACTIONS
                        | CapabilityFlags::CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA
                        | CapabilityFlags::CLIENT_DEPRECATE_EOF
                        | CapabilityFlags::CLIENT_MULTI_RESULTS)
                        & greeting.capabilities());
                if self.params.database.is_some() {
                    self.capabilities
                        .insert(CapabilityFlags::CLIENT_CONNECT_WITH_DB);
                }
                if self.params.tls.is_some() {
                    if !greeting
                        .capabilities()
                        .contains(CapabilityFlags::CLIENT_SSL)
                    {
                        return Err(ConnectError::Unsupported(
                            "server does not support required TLS",
                        ));
                    }
                    self.capabilities.insert(CapabilityFlags::CLIENT_SSL);
                    let request =
                        SslRequest::new(self.capabilities, self.params.max_receive as u32, 45);
                    let mut bytes = Zeroizing::new(Vec::new());
                    request.serialize(&mut bytes);
                    self.outgoing = Some(Outgoing {
                        framing: MessageWriter::new(bytes.len(), self.params.max_send)?,
                        bytes,
                    });
                    self.phase = Phase::SslRequest;
                } else {
                    self.phase = Phase::Login;
                    self.prepare_login()?;
                }
            } else {
                let auth = self
                    .auth
                    .as_mut()
                    .ok_or(ConnectError::Configuration("missing authentication state"))?;
                match auth.advance(&data)? {
                    AuthResponse::AwaitConfirmation => (),
                    AuthResponse::Reply(bytes) => {
                        self.outgoing = Some(Outgoing {
                            framing: MessageWriter::new(bytes.len(), self.params.max_send)?,
                            bytes,
                        });
                        self.phase = Phase::Reply;
                    }
                    AuthResponse::Complete(ok) => {
                        let path = auth.path();
                        self.auth = None;
                        let mut metadata = self
                            .metadata
                            .take()
                            .ok_or(ConnectError::Configuration("missing server metadata"))?;
                        metadata.status = ok.status_flags();
                        metadata.auth_path = path;
                        self.phase = Phase::Finished;
                        return Ok(Step::Ready(Authenticated {
                            channel: self
                                .channel
                                .take()
                                .ok_or(ConnectError::Configuration("missing channel"))?,
                            capabilities: self.capabilities,
                            metadata,
                            budget: self
                                .budget
                                .take()
                                .ok_or(ConnectError::Configuration("missing original budget"))?,
                            max_send: self.params.max_send,
                            max_receive: self.params.max_receive,
                        }));
                    }
                }
            }
        }
    }

    fn prepare_login(&mut self) -> Result<(), ConnectError> {
        let secure = self.channel.as_ref().is_some_and(Channel::secure);
        let auth = CachingSha2::new(
            b"caching_sha2_password",
            std::mem::take(&mut self.params.password),
            self.nonce,
            secure,
            self.key.clone(),
            self.capabilities,
            self.params.max_send,
        )?;
        let token = auth.initial_response();
        let metadata = self
            .metadata
            .as_ref()
            .ok_or(ConnectError::Configuration("missing greeting"))?;
        // Both negotiated auth encodings use one length byte for a 0/32-byte
        // token. Account for every real field, rather than rejecting on an
        // inflated allocation estimate or a historical statement-size limit.
        let lengths = [
            32,
            self.params.user.len(),
            1,
            token.len(),
            1,
            self.params.database.as_ref().map_or(0, String::len),
            usize::from(self.params.database.is_some()),
            b"caching_sha2_password".len(),
            1,
        ];
        let length = lengths
            .into_iter()
            .try_fold(0usize, usize::checked_add)
            .ok_or(WireError::Limit {
                required: usize::MAX,
                limit: self.params.max_send,
            })?;
        if length > self.params.max_send {
            return Err(WireError::Limit {
                required: length,
                limit: self.params.max_send,
            }
            .into());
        }
        let response = HandshakeResponse::new(
            Some(token.as_slice()),
            metadata.version_tuple,
            Some(self.params.user.as_bytes()),
            self.params.database.as_ref().map(|name| name.as_bytes()),
            Some(AuthPlugin::CachingSha2Password),
            self.capabilities,
            None,
            self.params.max_receive as u32,
        );
        let mut bytes = Zeroizing::new(Vec::new());
        bytes
            .try_reserve_exact(length)
            .map_err(WireError::Allocation)?;
        response.serialize(&mut bytes);
        debug_assert_eq!(bytes.len(), length);
        debug_assert_eq!(response.collation(), 45);
        debug_assert_eq!(response.capabilities(), self.capabilities);
        self.outgoing = Some(Outgoing {
            framing: MessageWriter::new(bytes.len(), self.params.max_send)?,
            bytes,
        });
        self.auth = Some(auth);
        Ok(())
    }
}
