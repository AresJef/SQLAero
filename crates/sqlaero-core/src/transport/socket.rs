//! Mio-backed socket readiness, with platform handling confined to this module.

use super::deadline::{Direction, IoBudget, TimeoutKind};
use mio::event::Source;
use mio::{Events, Interest, Poll, Registry, Token};
use socket2::SockRef;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr};
#[cfg(unix)]
use std::path::Path;

/// Native socket or timeout failure, without a Python exception dependency.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A real socket, registration or polling failure.
    #[error("socket I/O failed: {0}")]
    Io(#[from] io::Error),
    /// One applicable monotonic budget expired.
    #[error(transparent)]
    Timeout(#[from] TimeoutKind),
}

/// A readiness return is either usable input/output or a control-check boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitStatus {
    /// Retry the nonblocking socket operation; readiness may be spurious.
    Ready,
    /// Recheck host interruption before continuing with the same native state.
    Control,
}

#[derive(Debug)]
enum Socket {
    Tcp(mio::net::TcpStream),
    #[cfg(unix)]
    Unix(mio::net::UnixStream),
}

impl Source for Socket {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.register(registry, token, interests),
            #[cfg(unix)]
            Self::Unix(stream) => stream.register(registry, token, interests),
        }
    }
    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.reregister(registry, token, interests),
            #[cfg(unix)]
            Self::Unix(stream) => stream.reregister(registry, token, interests),
        }
    }
    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.deregister(registry),
            #[cfg(unix)]
            Self::Unix(stream) => stream.deregister(registry),
        }
    }
}

/// One owned nonblocking socket and its local readiness poller. No background
/// worker, global dispatcher or platform-specific SQL path is introduced.
#[derive(Debug)]
pub struct Transport {
    socket: Socket,
    poll: Poll,
    events: Events,
    interest: Interest,
    connected: bool,
}

impl Transport {
    /// Start a TCP connection. Completion must be confirmed with connected().
    ///
    /// # Errors
    /// Returns actual socket-option, connection-start or registration failures.
    pub fn tcp(address: SocketAddr) -> Result<Self, TransportError> {
        let socket = mio::net::TcpStream::connect(address)?;
        Self::new(Socket::Tcp(socket))
    }

    /// Start a Unix-domain connection using a normalized native path.
    ///
    /// # Errors
    /// Returns actual connection-start or registration failures.
    #[cfg(unix)]
    pub fn unix(path: &Path) -> Result<Self, TransportError> {
        Self::new(Socket::Unix(mio::net::UnixStream::connect(path)?))
    }

    fn new(mut socket: Socket) -> Result<Self, TransportError> {
        let poll = Poll::new()?;
        let interest = Interest::READABLE.add(Interest::WRITABLE);
        poll.registry().register(&mut socket, Token(0), interest)?;
        Ok(Self {
            socket,
            poll,
            events: Events::with_capacity(4),
            interest,
            connected: false,
        })
    }

    /// Confirm completion using SO_ERROR and the actual connected peer.
    ///
    /// # Errors
    /// Returns the real connection failure; a readiness event alone is not success.
    pub fn connected(&mut self) -> Result<bool, TransportError> {
        if self.connected {
            return Ok(true);
        }
        let (failure, peer) = match &self.socket {
            Socket::Tcp(socket) => (socket.take_error()?, socket.peer_addr().map(|_| ())),
            #[cfg(unix)]
            Socket::Unix(socket) => (socket.take_error()?, socket.peer_addr().map(|_| ())),
        };
        if let Some(error) = failure {
            return Err(error.into());
        }
        match peer {
            Ok(()) => {
                if let Socket::Tcp(socket) = &self.socket {
                    socket.set_nodelay(true)?;
                    SockRef::from(socket).set_keepalive(true)?;
                }
                self.connected = true;
                Ok(true)
            }
            Err(error) if connection_pending(&error) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Borrow raw network I/O while accounting only actual positive transfers.
    pub fn io<'a>(&'a mut self, budget: &'a mut IoBudget) -> SocketIo<'a> {
        SocketIo {
            socket: &mut self.socket,
            budget,
        }
    }

    /// Wait for the needed direction within the existing budget. Timeout and
    /// OS interruption return control; neither restarts a deadline or interval.
    ///
    /// # Errors
    /// Returns a real poll/registration failure or the applicable timeout.
    pub fn wait(
        &mut self,
        direction: Direction,
        budget: &mut IoBudget,
    ) -> Result<WaitStatus, TransportError> {
        let interest = match direction {
            Direction::Read => Interest::READABLE,
            Direction::Write => Interest::WRITABLE,
        };
        if self.interest != interest {
            self.poll
                .registry()
                .reregister(&mut self.socket, Token(0), interest)?;
            self.interest = interest;
        }
        let duration = budget.wait_duration(direction)?;
        match self.poll.poll(&mut self.events, Some(duration)) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return Ok(WaitStatus::Control);
            }
            Err(error) => return Err(error.into()),
        }
        budget.check_io(direction)?;
        for event in self.events.iter() {
            let ready = match direction {
                Direction::Read => event.is_readable() || event.is_read_closed(),
                Direction::Write => event.is_writable() || event.is_write_closed(),
            };
            if ready || event.is_error() {
                return Ok(WaitStatus::Ready);
            }
        }
        budget.check()?;
        Ok(WaitStatus::Control)
    }

    /// Disable both directions locally. This does not prove the server canceled
    /// an in-flight statement or rolled back a transaction.
    ///
    /// # Errors
    /// Returns the actual local shutdown error.
    pub fn shutdown(&self) -> Result<(), TransportError> {
        match &self.socket {
            Socket::Tcp(socket) => socket.shutdown(Shutdown::Both)?,
            #[cfg(unix)]
            Socket::Unix(socket) => socket.shutdown(Shutdown::Both)?,
        }
        Ok(())
    }
}

fn connection_pending(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::NotConnected | io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    // The standard InProgress variant is still unstable. Use the mature
    // platform constant at this socket boundary, never a copied numeric errno.
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::EINPROGRESS) {
        return true;
    }
    false
}

/// A short-lived raw-socket borrow used by framing or TLS record processing.
pub struct SocketIo<'a> {
    socket: &'a mut Socket,
    budget: &'a mut IoBudget,
}

impl Read for SocketIo<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.budget
            .check_io(Direction::Read)
            .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
        let length = match self.socket {
            Socket::Tcp(socket) => socket.read(bytes),
            #[cfg(unix)]
            Socket::Unix(socket) => socket.read(bytes),
        }?;
        if length > 0 {
            self.budget.progress(Direction::Read);
        }
        Ok(length)
    }
}

impl Write for SocketIo<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.budget
            .check_io(Direction::Write)
            .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
        let length = match self.socket {
            Socket::Tcp(socket) => socket.write(bytes),
            #[cfg(unix)]
            Socket::Unix(socket) => socket.write(bytes),
        }?;
        if length > 0 {
            self.budget.progress(Direction::Write);
        }
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
