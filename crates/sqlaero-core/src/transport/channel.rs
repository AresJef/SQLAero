//! One concrete plain/TLS byte channel with resumable MySQL message transfer.

use super::deadline::{Direction, IoBudget, TimeoutKind};
use super::socket::{Transport, TransportError, WaitStatus};
use crate::protocol::framing::{MessageReader, MessageWriter, Progress, Sequence, WireError};
use rustls::{ClientConfig, ClientConnection};
use rustls_pki_types::ServerName;
use std::io;
use std::sync::Arc;

/// An actual channel failure, preserving its native source category.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// A classic-protocol or native storage failure.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// A raw socket or readiness failure.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The TLS engine rejected the peer or its records.
    #[error("TLS connection failed: {0}")]
    Tls(#[from] rustls::Error),
    /// The original total or no-progress interval expired.
    #[error(transparent)]
    Timeout(#[from] TimeoutKind),
}

/// Outcome of concrete channel work; waiting never discards message progress.
#[derive(Debug)]
pub enum Step<T> {
    /// The requested channel work completed.
    Ready(T),
    /// Wait for this specific network direction.
    Wait(Direction),
    /// Return to the host boundary for a signal check, retaining native state.
    Control,
}

/// An owned socket and optional TLS state, shared by authentication and commands.
pub struct Channel {
    transport: Transport,
    tls: Option<ClientConnection>,
    sequence: Sequence,
    local: bool,
}

impl Channel {
    /// Wrap the selected endpoint. `local` is true only for the explicit Unix
    /// channel, not for a TCP loopback address.
    #[must_use]
    pub fn new(transport: Transport, local: bool) -> Self {
        Self {
            transport,
            tls: None,
            sequence: Sequence::default(),
            local,
        }
    }

    /// Confirm completion of the originally selected endpoint.
    ///
    /// # Errors
    /// Returns its actual connection failure.
    pub fn connected(&mut self) -> Result<bool, ChannelError> {
        Ok(self.transport.connected()?)
    }

    /// Begin a new command's packet sequence after the preceding exchange has
    /// been fully consumed. This never creates a new operation timeout budget.
    pub fn new_command(&mut self) {
        self.sequence = Sequence::default();
    }

    /// Return whether full authentication may use this established channel.
    #[must_use]
    pub fn secure(&self) -> bool {
        match &self.tls {
            Some(tls) => !tls.is_handshaking(),
            None => self.local,
        }
    }

    /// Install TLS only after the complete SSLRequest has been sent.
    ///
    /// # Errors
    /// Returns invalid upgrade state or invalid TLS configuration/name failures.
    pub fn start_tls(
        &mut self,
        config: Arc<ClientConfig>,
        name: ServerName<'static>,
    ) -> Result<(), ChannelError> {
        if self.tls.is_some() {
            return Err(WireError::Malformed("TLS was already started").into());
        }
        self.tls = Some(ClientConnection::new(config, name)?);
        Ok(())
    }

    /// Advance the TLS handshake, including its final outgoing flight.
    ///
    /// # Errors
    /// Returns real TLS, transport or timeout failures; no downgrade is attempted.
    pub fn handshake_step(&mut self, budget: &mut IoBudget) -> Result<Step<()>, ChannelError> {
        budget.check()?;
        for _ in 0..16 {
            match self.flush_tls(budget)? {
                Step::Ready(()) => (),
                other => return Ok(other),
            }
            let tls = self
                .tls
                .as_mut()
                .ok_or(WireError::Malformed("TLS handshake was not started"))?;
            if !tls.is_handshaking() {
                return Ok(Step::Ready(()));
            }
            match tls.read_tls(&mut self.transport.io(budget)) {
                Ok(0) => return Err(WireError::Io(io::ErrorKind::UnexpectedEof.into()).into()),
                Ok(_) => {
                    tls.process_new_packets()?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(Step::Wait(Direction::Read));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    return Ok(Step::Control);
                }
                Err(error) => return Err(WireError::Io(error).into()),
            }
        }
        Ok(Step::Control)
    }

    /// Advance a native request, keeping plaintext storage alive until every
    /// TLS record has also been flushed. Framing and network completion differ.
    ///
    /// # Errors
    /// Returns real framing, transport, TLS or timeout failures. Partial progress
    /// does not establish whether MySQL executed the command.
    pub fn send_step(
        &mut self,
        message: &mut MessageWriter,
        bytes: &[u8],
        budget: &mut IoBudget,
    ) -> Result<Step<()>, ChannelError> {
        budget.check()?;
        if self.tls.is_none() {
            return message
                .step(&mut self.transport.io(budget), bytes, &mut self.sequence)
                .map(|step| convert(step, Direction::Write))
                .map_err(ChannelError::from);
        }
        for _ in 0..16 {
            if !message.is_complete() {
                let tls = self
                    .tls
                    .as_mut()
                    .ok_or(WireError::Malformed("missing TLS state"))?;
                if tls.is_handshaking() {
                    return Err(
                        WireError::Malformed("application data before TLS completion").into(),
                    );
                }
                match message.step(&mut tls.writer(), bytes, &mut self.sequence)? {
                    Progress::Yield => return Ok(Step::Control),
                    Progress::Ready(()) | Progress::Pending => (),
                }
            }
            match self.flush_tls(budget)? {
                Step::Ready(()) => (),
                other => return Ok(other),
            }
            if message.is_complete() {
                return Ok(Step::Ready(()));
            }
        }
        Ok(Step::Control)
    }

    /// Advance a response without losing partial headers across TLS records or
    /// host signal checks. Final Python values are not constructed here.
    ///
    /// # Errors
    /// Returns real framing, transport, TLS or timeout failures.
    pub fn receive_step(
        &mut self,
        message: &mut MessageReader,
        budget: &mut IoBudget,
    ) -> Result<Step<Vec<u8>>, ChannelError> {
        budget.check()?;
        if self.tls.is_none() {
            return message
                .step(&mut self.transport.io(budget), &mut self.sequence)
                .map(|step| convert(step, Direction::Read))
                .map_err(ChannelError::from);
        }
        for _ in 0..16 {
            let tls = self
                .tls
                .as_mut()
                .ok_or(WireError::Malformed("missing TLS state"))?;
            match message.step(&mut tls.reader(), &mut self.sequence)? {
                Progress::Ready(bytes) => return Ok(Step::Ready(bytes)),
                Progress::Yield => return Ok(Step::Control),
                Progress::Pending => (),
            }
            match self.flush_tls(budget)? {
                Step::Ready(()) => (),
                Step::Wait(direction) => return Ok(Step::Wait(direction)),
                Step::Control => return Ok(Step::Control),
            }
            let tls = self
                .tls
                .as_mut()
                .ok_or(WireError::Malformed("missing TLS state"))?;
            match tls.read_tls(&mut self.transport.io(budget)) {
                Ok(0) => return Err(WireError::Io(io::ErrorKind::UnexpectedEof.into()).into()),
                Ok(_) => {
                    tls.process_new_packets()?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(Step::Wait(Direction::Read));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    return Ok(Step::Control);
                }
                Err(error) => return Err(WireError::Io(error).into()),
            }
        }
        Ok(Step::Control)
    }

    fn flush_tls(&mut self, budget: &mut IoBudget) -> Result<Step<()>, ChannelError> {
        let Some(tls) = self.tls.as_mut() else {
            return Ok(Step::Ready(()));
        };
        let mut transferred = 0;
        while tls.wants_write() {
            if transferred >= 256 * 1024 {
                return Ok(Step::Control);
            }
            match tls.write_tls(&mut self.transport.io(budget)) {
                Ok(0) => return Err(WireError::Io(io::ErrorKind::WriteZero.into()).into()),
                Ok(length) => transferred += length,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(Step::Wait(Direction::Write));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    return Ok(Step::Control);
                }
                Err(error) => return Err(WireError::Io(error).into()),
            }
        }
        Ok(Step::Ready(()))
    }

    /// Wait only as long as the existing monotonic budget permits.
    ///
    /// # Errors
    /// Returns the actual readiness or timeout failure.
    pub fn wait(
        &mut self,
        direction: Direction,
        budget: &mut IoBudget,
    ) -> Result<WaitStatus, ChannelError> {
        Ok(self.transport.wait(direction, budget)?)
    }

    /// Return the actual negotiated TLS protocol version, if enabled.
    #[must_use]
    pub fn tls_version(&self) -> Option<rustls::ProtocolVersion> {
        self.tls.as_ref()?.protocol_version()
    }

    /// Return the actual negotiated cipher suite, if enabled.
    #[must_use]
    pub fn tls_cipher(&self) -> Option<rustls::CipherSuite> {
        self.tls
            .as_ref()?
            .negotiated_cipher_suite()
            .map(|suite| suite.suite())
    }

    /// Disable the local transport without claiming server-side cancellation.
    ///
    /// # Errors
    /// Returns the actual shutdown failure.
    pub fn shutdown(&self) -> Result<(), ChannelError> {
        Ok(self.transport.shutdown()?)
    }
}

fn convert<T>(step: Progress<T>, direction: Direction) -> Step<T> {
    match step {
        Progress::Ready(value) => Step::Ready(value),
        Progress::Pending => Step::Wait(direction),
        Progress::Yield => Step::Control,
    }
}
