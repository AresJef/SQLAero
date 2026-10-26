//! Uncompressed MySQL logical messages, independent of the transport backend.

use crate::memory::{Buffer, MemoryError, Scope};
use std::collections::TryReserveError;
use std::io::{self, Read, Write};
use std::sync::Arc;

/// Maximum payload in one three-byte-length classic-protocol frame.
pub const MAX_FRAME_PAYLOAD: usize = 0x00ff_ffff;
/// Maximum logical payload permitted by MySQL's packet-size contract.
pub const MAX_MESSAGE_PAYLOAD: usize = 1 << 30;

/// A framing, resource or underlying I/O failure. Callers decide connection
/// invalidation from the actual send/receive boundary, not from its text.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// The transport failed or ended before a complete message was read.
    #[error("MySQL transport I/O failed: {0}")]
    Io(#[from] io::Error),
    /// A frame did not continue the current command's shared sequence.
    #[error("MySQL packet sequence mismatch: expected {expected}, received {received}")]
    Sequence {
        /// Expected modulo-256 sequence number.
        expected: u8,
        /// Sequence number found in the frame header.
        received: u8,
    },
    /// A logical payload exceeded the configured or protocol limit.
    #[error("MySQL logical message requires {required} bytes, exceeding the {limit}-byte limit")]
    Limit {
        /// Observed or requested logical payload length, excluding frame headers.
        required: usize,
        /// Effective logical-message limit.
        limit: usize,
    },
    /// Native storage could not be reserved.
    #[error("cannot reserve MySQL message storage: {0}")]
    Allocation(#[from] TryReserveError),
    /// A result-owned message exhausted the actual native capacity budget.
    #[error(transparent)]
    Memory(MemoryError),
    /// Packet fields did not satisfy the negotiated protocol contract.
    #[error("invalid MySQL packet: {0}")]
    Malformed(&'static str),
    /// A selected public packet parser rejected the payload.
    #[error("invalid MySQL packet fields: {0}")]
    Decode(io::Error),
}
impl From<MemoryError> for WireError {
    fn from(error: MemoryError) -> Self {
        match error {
            MemoryError::Allocation(error) => Self::Allocation(error),
            error => Self::Memory(error),
        }
    }
}

/// The next shared sequence number in a handshake or command exchange.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sequence(u8);

impl Sequence {
    /// Start an exchange with an explicit first sequence number.
    #[must_use]
    pub const fn new(next: u8) -> Self {
        Self(next)
    }

    /// Return the next expected or emitted frame number.
    #[must_use]
    pub const fn next(self) -> u8 {
        self.0
    }

    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

/// Result of bounded nonblocking framing work. A yield permits the binding
/// boundary to check signals without a Python callback inside the native core.
#[derive(Debug)]
pub enum Progress<T> {
    /// The entire logical message was transferred.
    Ready(T),
    /// The socket needs an appropriate readiness event.
    Pending,
    /// The bounded work slice ended or an OS interruption requires a check.
    Yield,
}

const STEP_BYTES: usize = 256 * 1024;

/// Resumable message reception. Partial headers and bodies remain owned here
/// across readiness waits and interpreter signal checks.
#[derive(Debug)]
pub struct MessageReader {
    header: [u8; 4],
    header_read: usize,
    frame_remaining: usize,
    frame_length: usize,
    payload: Buffer<u8>,
    limit: usize,
    complete: bool,
}

impl MessageReader {
    /// Prepare one logical message under a fixed receive limit.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            header: [0; 4],
            header_read: 0,
            frame_remaining: 0,
            frame_length: 0,
            payload: Buffer::new(),
            limit: limit.min(MAX_MESSAGE_PAYLOAD),
            complete: false,
        }
    }

    /// Receive into the same result/connection quota as the collector. A
    /// complete message retains its charge until parsing/copying releases it.
    #[must_use]
    pub fn tracked(limit: usize, scope: &Arc<Scope>) -> Self {
        let mut reader = Self::new(limit);
        reader.payload = Buffer::tracked(scope);
        reader
    }

    /// Consume available bytes without blocking internally. At most a bounded
    /// byte slice is processed before yielding. A completed reader is not reused.
    ///
    /// # Errors
    /// Returns a framing, allocation or transport error. The caller must not
    /// infer non-execution from an incomplete response.
    pub fn step<R: Read>(
        &mut self,
        reader: &mut R,
        sequence: &mut Sequence,
    ) -> Result<Progress<Buffer<u8>>, WireError> {
        if self.complete {
            return Err(WireError::Malformed("message reader already consumed"));
        }
        let mut transferred = 0;
        loop {
            if transferred >= STEP_BYTES {
                return Ok(Progress::Yield);
            }
            if self.header_read < 4 {
                let end = 4.min(self.header_read + STEP_BYTES - transferred);
                match reader.read(&mut self.header[self.header_read..end]) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                    Ok(length) => {
                        self.header_read += length;
                        transferred += length;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(Progress::Pending);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        return Ok(Progress::Yield);
                    }
                    Err(error) => return Err(error.into()),
                }
                if self.header_read < 4 {
                    continue;
                }
                if self.header[3] != sequence.0 {
                    return Err(WireError::Sequence {
                        expected: sequence.0,
                        received: self.header[3],
                    });
                }
                sequence.advance();
                self.frame_length = usize::from(self.header[0])
                    | (usize::from(self.header[1]) << 8)
                    | (usize::from(self.header[2]) << 16);
                let total =
                    self.payload
                        .len()
                        .checked_add(self.frame_length)
                        .ok_or(WireError::Limit {
                            required: usize::MAX,
                            limit: self.limit,
                        })?;
                if total > self.limit {
                    return Err(WireError::Limit {
                        required: total,
                        limit: self.limit,
                    });
                }
                self.frame_remaining = self.frame_length;
            }
            if self.frame_remaining > 0 {
                let length = self
                    .frame_remaining
                    .min(16 * 1024)
                    .min(STEP_BYTES - transferred);
                if length == 0 {
                    return Ok(Progress::Yield);
                }
                let start = self.payload.len();
                self.payload.resize(start + length, 0)?;
                let received = reader.read(&mut self.payload.as_mut_slice()[start..]);
                match received {
                    Ok(0) => {
                        self.payload.truncate(start);
                        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                    }
                    Ok(length) => {
                        self.payload.truncate(start + length);
                        self.frame_remaining -= length;
                        transferred += length;
                    }
                    Err(error) => {
                        self.payload.truncate(start);
                        return match error.kind() {
                            io::ErrorKind::WouldBlock => Ok(Progress::Pending),
                            io::ErrorKind::Interrupted => Ok(Progress::Yield),
                            _ => Err(error.into()),
                        };
                    }
                }
                if self.frame_remaining > 0 {
                    continue;
                }
            }
            if self.frame_length < MAX_FRAME_PAYLOAD {
                self.complete = true;
                return Ok(Progress::Ready(std::mem::take(&mut self.payload)));
            }
            self.header_read = 0;
        }
    }
}

/// Resumable framing state for an immutable native request. The caller keeps
/// the same payload alive throughout transfer; no self-referential borrow or
/// second copy of the request is needed.
#[derive(Debug)]
pub struct MessageWriter {
    length: usize,
    offset: usize,
    frame_start: usize,
    frame_end: usize,
    header: [u8; 4],
    header_sent: usize,
    complete: bool,
}

impl MessageWriter {
    /// Return whether every frame has been accepted by the byte writer. A TLS
    /// channel separately flushes its encrypted records before reporting sent.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Validate the full logical length before any byte can be sent.
    ///
    /// # Errors
    /// Returns a size-limit failure, including the command byte when present.
    pub fn new(length: usize, limit: usize) -> Result<Self, WireError> {
        let limit = limit.min(MAX_MESSAGE_PAYLOAD);
        if length > limit {
            return Err(WireError::Limit {
                required: length,
                limit,
            });
        }
        Ok(Self {
            length,
            offset: 0,
            frame_start: 0,
            frame_end: length.min(MAX_FRAME_PAYLOAD),
            header: [0; 4],
            header_sent: 0,
            complete: false,
        })
    }

    /// Write a bounded slice of available request bytes without waiting.
    ///
    /// # Errors
    /// Returns an I/O failure or a changed-payload contract violation. Partial
    /// progress remains real even if the transfer subsequently fails.
    pub fn step<W: Write>(
        &mut self,
        writer: &mut W,
        payload: &[u8],
        sequence: &mut Sequence,
    ) -> Result<Progress<()>, WireError> {
        if payload.len() != self.length {
            return Err(WireError::Malformed(
                "request length changed during transfer",
            ));
        }
        if self.complete {
            return Err(WireError::Malformed("message writer already consumed"));
        }
        let mut transferred = 0;
        loop {
            if transferred >= STEP_BYTES {
                return Ok(Progress::Yield);
            }
            if self.header_sent == 0 {
                let length = self.frame_end - self.frame_start;
                self.header = [
                    length as u8,
                    (length >> 8) as u8,
                    (length >> 16) as u8,
                    sequence.0,
                ];
            }
            if self.header_sent < 4 {
                let end = 4.min(self.header_sent + STEP_BYTES - transferred);
                match writer.write(&self.header[self.header_sent..end]) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                    Ok(length) => {
                        self.header_sent += length;
                        transferred += length;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(Progress::Pending);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        return Ok(Progress::Yield);
                    }
                    Err(error) => return Err(error.into()),
                }
                if self.header_sent < 4 {
                    continue;
                }
            }
            if self.offset < self.frame_end {
                let end = self.frame_end.min(self.offset + STEP_BYTES - transferred);
                if end == self.offset {
                    return Ok(Progress::Yield);
                }
                match writer.write(&payload[self.offset..end]) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                    Ok(length) => {
                        self.offset += length;
                        transferred += length;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(Progress::Pending);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        return Ok(Progress::Yield);
                    }
                    Err(error) => return Err(error.into()),
                }
                if self.offset < self.frame_end {
                    continue;
                }
            }
            sequence.advance();
            if self.frame_end - self.frame_start < MAX_FRAME_PAYLOAD {
                self.complete = true;
                return Ok(Progress::Ready(()));
            }
            self.frame_start = self.offset;
            self.frame_end = self.length.min(self.offset + MAX_FRAME_PAYLOAD);
            self.header_sent = 0;
        }
    }
}

/// Read one complete logical message from a blocking reader.
///
/// Nonblocking production transports use [`MessageReader::step`] so a wait
/// never loses partial state. This convenience function never polls a socket.
///
/// # Errors
/// Returns an I/O, sequence, size-limit or allocation failure.
pub fn read_message<R: Read>(
    reader: &mut R,
    sequence: &mut Sequence,
    limit: usize,
) -> Result<Buffer<u8>, WireError> {
    let mut message = MessageReader::new(limit);
    loop {
        match message.step(reader, sequence)? {
            Progress::Ready(payload) => return Ok(payload),
            Progress::Yield => (),
            Progress::Pending => return Err(io::Error::from(io::ErrorKind::WouldBlock).into()),
        }
    }
}

/// Write one complete logical payload to a blocking writer.
///
/// # Errors
/// Returns a size-limit failure before writing, or an actual transport failure.
pub fn write_message<W: Write>(
    writer: &mut W,
    payload: &[u8],
    sequence: &mut Sequence,
    limit: usize,
) -> Result<(), WireError> {
    let mut message = MessageWriter::new(payload.len(), limit)?;
    loop {
        match message.step(writer, payload, sequence)? {
            Progress::Ready(()) => return Ok(()),
            Progress::Yield => (),
            Progress::Pending => return Err(io::Error::from(io::ErrorKind::WouldBlock).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct ShortIo {
        bytes: Cursor<Vec<u8>>,
    }
    impl Read for ShortIo {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let length = output.len().min(3);
            self.bytes.read(&mut output[..length])
        }
    }
    impl Write for ShortIo {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            self.bytes.write(&input[..input.len().min(2)])
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn short_io_preserves_complete_payload_and_shared_sequence() {
        let mut stream = ShortIo {
            bytes: Cursor::new(Vec::new()),
        };
        let mut write_sequence = Sequence::new(9);
        write_message(&mut stream, b"\x03SELECT 1", &mut write_sequence, 64).unwrap();
        stream.bytes.set_position(0);
        let mut read_sequence = Sequence::new(9);
        assert_eq!(
            read_message(&mut stream, &mut read_sequence, 64)
                .unwrap()
                .as_slice(),
            b"\x03SELECT 1"
        );
        assert_eq!(read_sequence.next(), write_sequence.next());
    }

    #[test]
    fn exact_frame_boundary_emits_empty_terminator_and_wraps_sequence() {
        let input = vec![0xa5; MAX_FRAME_PAYLOAD];
        let mut bytes = Vec::new();
        let mut sequence = Sequence::new(255);
        write_message(&mut bytes, &input, &mut sequence, MAX_MESSAGE_PAYLOAD).unwrap();
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
        assert_eq!(sequence.next(), 1);
        assert_eq!(
            read_message(
                &mut Cursor::new(bytes),
                &mut Sequence::new(255),
                input.len()
            )
            .unwrap(),
            input
        );
    }

    #[test]
    fn limits_reject_before_sending_or_reading_a_declared_body() {
        let mut bytes = Vec::new();
        assert!(matches!(
            write_message(&mut bytes, b"12345", &mut Sequence::default(), 4),
            Err(WireError::Limit { .. })
        ));
        assert!(bytes.is_empty());
        let mut declared = Cursor::new([9, 0, 0, 0]);
        assert!(matches!(
            read_message(&mut declared, &mut Sequence::default(), 8),
            Err(WireError::Limit {
                required: 9,
                limit: 8
            })
        ));
        assert_eq!(declared.position(), 4);
    }

    #[test]
    fn sequence_and_truncation_are_not_treated_as_valid_empty_messages() {
        assert!(matches!(
            read_message(&mut Cursor::new([0, 0, 0, 1]), &mut Sequence::default(), 64),
            Err(WireError::Sequence { .. })
        ));
        for bytes in [vec![1, 0], vec![2, 0, 0, 0, 1]] {
            assert!(matches!(
                read_message(&mut Cursor::new(bytes), &mut Sequence::default(), 64),
                Err(WireError::Io(_))
            ));
        }
    }

    #[test]
    fn pending_at_partial_headers_and_bodies_preserves_exact_progress() {
        struct Intermittent {
            bytes: Cursor<Vec<u8>>,
            block: bool,
        }
        impl Read for Intermittent {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                self.block = !self.block;
                if self.block {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                let end = output.len().min(1);
                self.bytes.read(&mut output[..end])
            }
        }
        impl Write for Intermittent {
            fn write(&mut self, input: &[u8]) -> io::Result<usize> {
                self.block = !self.block;
                if self.block {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                self.bytes.write(&input[..input.len().min(1)])
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let input = b"\x03SELECT 12345";
        let mut stream = Intermittent {
            bytes: Cursor::new(Vec::new()),
            block: false,
        };
        let mut send = MessageWriter::new(input.len(), 64).unwrap();
        let mut sequence = Sequence::new(255);
        for _ in 0..128 {
            if matches!(
                send.step(&mut stream, input, &mut sequence).unwrap(),
                Progress::Ready(())
            ) {
                break;
            }
        }
        assert_eq!(sequence.next(), 0);
        stream.bytes.set_position(0);
        let mut receive = MessageReader::new(64);
        let mut sequence = Sequence::new(255);
        let mut output = None;
        for _ in 0..128 {
            if let Progress::Ready(data) = receive.step(&mut stream, &mut sequence).unwrap() {
                output = Some(data);
                break;
            }
        }
        assert_eq!(output.as_deref(), Some(input.as_slice()));
        assert_eq!(sequence.next(), 0);
    }
}
