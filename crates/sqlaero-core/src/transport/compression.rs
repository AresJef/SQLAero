//! The classic compression layer, independent of socket and TLS ownership.

use crate::memory::{Buffer, Scope};
use crate::protocol::CapabilityFlags;
use crate::protocol::framing::{MessageReader, MessageWriter, Progress, Sequence, WireError};
use std::io::{self, Read, Write};
use std::sync::Arc;

mod engine;
mod inbound;
mod outbound;

use inbound::Inbound;
use outbound::Outbound;

/// One connection's compression state, above plain/TLS I/O and below normal
/// classic framing. Its sequence is independent of the normal packet sequence.
pub struct Codec {
    algorithm: Algorithm,
    zstd_level: u8,
    sequence: u8,
    inbound: Inbound,
    outbound: Outbound,
    ready: Option<Buffer<u8>>,
    failed: bool,
}

impl Codec {
    /// Initialize the actually negotiated algorithm after authentication OK.
    /// No dictionary, legacy format or background worker is enabled.
    ///
    /// # Errors
    /// Returns an invalid level or native engine allocation/configuration error.
    pub fn new(algorithm: Algorithm, zstd_level: u8) -> Result<Self, WireError> {
        if !(1..=22).contains(&zstd_level) {
            return Err(WireError::Malformed("invalid Zstandard compression level"));
        }
        Ok(Self {
            algorithm,
            zstd_level,
            sequence: 0,
            inbound: Inbound::new(algorithm)?,
            outbound: Outbound::new(algorithm, zstd_level)?,
            ready: None,
            failed: false,
        })
    }

    /// Actual configured wire algorithm, without any Python object dependency.
    #[must_use]
    pub const fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// Actual immutable single-algorithm configuration, without reparsing user
    /// input or constructing a Python object. The level applies only to zstd.
    #[must_use]
    pub const fn snapshot(&self) -> Options {
        Options {
            algorithms: Algorithms(match self.algorithm {
                Algorithm::Zlib => 1,
                Algorithm::Zstd => 2,
            }),
            zstd_level: self.zstd_level,
        }
    }

    /// Current Zstandard encoder/decoder context sizes reported by the frozen
    /// safe library, excluding result buffers. This is a point-in-time native
    /// workspace observation, not a peak or process-RSS bound. The decoder's
    /// history window is separately capped at log2(16 MiB).
    #[must_use]
    pub fn zstd_workspace_bytes(&self) -> Option<(usize, usize)> {
        Some((
            self.outbound.zstd_workspace_bytes()?,
            self.inbound.zstd_workspace_bytes()?,
        ))
    }

    /// Reset independent framing only at a fully synchronized command boundary.
    /// This does not discard unread bytes or silently repair a failed exchange.
    ///
    /// # Errors
    /// Rejects unconsumed compressed input/output or a previous framing failure.
    pub fn start_command(&mut self) -> Result<(), WireError> {
        if self.failed
            || self.ready.is_some()
            || !self.inbound.is_idle()
            || !self.outbound.is_idle()
        {
            self.failed = true;
            return Err(WireError::Malformed(
                "compression has unfinished command bytes",
            ));
        }
        self.sequence = 0;
        Ok(())
    }

    /// Feed a normal framing reader from bounded streaming decompression. The
    /// final logical message remains owned until its compressed checksum/footer
    /// is verified. Actual temporary buffer capacity shares the result scope.
    ///
    /// # Errors
    /// Preserves actual compressed lengths, independent sequence, engine,
    /// allocation and transport failures. Never retries as raw byte framing.
    pub fn read_message_step<R: Read>(
        &mut self,
        message: &mut MessageReader,
        source: &mut R,
        ordinary: &mut Sequence,
        scope: &Arc<Scope>,
    ) -> Result<Progress<Buffer<u8>>, WireError> {
        if self.failed {
            return Err(WireError::Malformed("compression exchange already failed"));
        }
        let result = self.read_inner(message, source, ordinary, scope);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn read_inner<R: Read>(
        &mut self,
        message: &mut MessageReader,
        source: &mut R,
        ordinary: &mut Sequence,
        scope: &Arc<Scope>,
    ) -> Result<Progress<Buffer<u8>>, WireError> {
        let mut work = 0;
        if self.ready.is_none() {
            let mut adapter = Reader {
                state: &mut self.inbound,
                source,
                sequence: &mut self.sequence,
                scope,
                work: &mut work,
                error: None,
            };
            let result = message.step(&mut adapter, ordinary);
            if let Some(error) = adapter.error {
                return Err(error);
            }
            match result? {
                Progress::Ready(bytes) => self.ready = Some(bytes),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
        }
        if self.inbound.needs_footer() {
            match self.inbound.finish_footer(source, &mut work)? {
                Progress::Ready(()) => (),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
        }
        self.ready
            .take()
            .map(Progress::Ready)
            .ok_or(WireError::Malformed(
                "compression lost completed message ownership",
            ))
    }

    /// Frame and compress bounded chunks, preserving offsets across short
    /// writes and host-control returns. `message.is_complete()` alone does not
    /// mean this method has flushed its last compressed block; wait for Ready.
    ///
    /// # Errors
    /// Returns actual scope/quota, compression, framing or transport failures.
    pub fn write_message_step<W: Write>(
        &mut self,
        message: &mut MessageWriter,
        bytes: &[u8],
        target: &mut W,
        ordinary: &mut Sequence,
        scope: &Arc<Scope>,
    ) -> Result<Progress<()>, WireError> {
        if self.failed {
            return Err(WireError::Malformed("compression exchange already failed"));
        }
        let result = self.write_inner(message, bytes, target, ordinary, scope);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn write_inner<W: Write>(
        &mut self,
        message: &mut MessageWriter,
        bytes: &[u8],
        target: &mut W,
        ordinary: &mut Sequence,
        scope: &Arc<Scope>,
    ) -> Result<Progress<()>, WireError> {
        let mut work = 0;
        if !message.is_complete() {
            let mut adapter = Writer {
                state: &mut self.outbound,
                target,
                sequence: &mut self.sequence,
                scope,
                work: &mut work,
                error: None,
            };
            let result = message.step(&mut adapter, bytes, ordinary);
            if let Some(error) = adapter.error {
                return Err(error);
            }
            match result? {
                Progress::Ready(()) => (),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
        }
        match self.outbound.flush(target, &mut self.sequence, &mut work)? {
            Progress::Ready(()) => {
                self.outbound.release();
                Ok(Progress::Ready(()))
            }
            Progress::Pending => Ok(Progress::Pending),
            Progress::Yield => Ok(Progress::Yield),
        }
    }
}

struct Reader<'a, R> {
    state: &'a mut Inbound,
    source: &'a mut R,
    sequence: &'a mut u8,
    scope: &'a Arc<Scope>,
    work: &'a mut usize,
    error: Option<WireError>,
}

impl<R: Read> Read for Reader<'_, R> {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        io_progress(
            self.state
                .read(self.source, self.sequence, target, self.scope, self.work),
            &mut self.error,
        )
    }
}

struct Writer<'a, W> {
    state: &'a mut Outbound,
    target: &'a mut W,
    sequence: &'a mut u8,
    scope: &'a Arc<Scope>,
    work: &'a mut usize,
    error: Option<WireError>,
}

impl<W: Write> Write for Writer<'_, W> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        io_progress(
            self.state
                .write(self.target, self.sequence, input, self.scope, self.work),
            &mut self.error,
        )
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn io_progress(
    result: Result<Progress<usize>, WireError>,
    error: &mut Option<WireError>,
) -> io::Result<usize> {
    match result {
        Ok(Progress::Ready(length)) => Ok(length),
        Ok(Progress::Pending) => Err(io::ErrorKind::WouldBlock.into()),
        Ok(Progress::Yield) => Err(io::ErrorKind::Interrupted.into()),
        Err(actual) => {
            *error = Some(actual);
            Err(io::Error::other("compression layer failed"))
        }
    }
}

/// An actual negotiated compression algorithm; no compression is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// The classic zlib-wrapped deflate format.
    Zlib,
    /// A separate Zstandard frame for each compressed protocol payload.
    Zstd,
}

impl Algorithm {
    /// Canonical public algorithm name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Zlib => "zlib",
            Self::Zstd => "zstd",
        }
    }
}

/// A normalized permission set. Input order never becomes an algorithm priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Algorithms(u8);

impl Algorithms {
    /// Default permission: only an uncompressed connection is allowed.
    pub const UNCOMPRESSED: Self = Self(4);

    /// Parse one root CSV string; every empty item is an actual input error.
    ///
    /// # Errors
    /// Returns a strict unknown/empty algorithm error without arbitrary coercion.
    pub fn from_csv(names: &str) -> Result<Self, &'static str> {
        Self::from_names(names.split(','))
    }

    /// Parse list/tuple element names. Elements never recursively expand CSV.
    ///
    /// # Errors
    /// Rejects empty input, empty items, unknown names and compound elements.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, &'static str> {
        let mut bits = 0;
        for name in names {
            let name = name.trim_matches(|character: char| character.is_ascii_whitespace());
            bits |= if name.eq_ignore_ascii_case("zlib") {
                1
            } else if name.eq_ignore_ascii_case("zstd") {
                2
            } else if name.eq_ignore_ascii_case("uncompressed") {
                4
            } else {
                return Err("compression algorithm must be zlib, zstd or uncompressed");
            };
        }
        if bits == 0 {
            return Err("compression algorithms must not be empty");
        }
        Ok(Self(bits))
    }

    /// Canonical names in stable order, independent of source ordering.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        ["zlib", "zstd", "uncompressed"]
            .into_iter()
            .enumerate()
            .filter_map(move |(index, name)| (self.0 & (1 << index) != 0).then_some(name))
    }

    /// Whether a compressed algorithm is explicitly permitted.
    #[must_use]
    pub const fn permits(self, algorithm: Algorithm) -> bool {
        self.0
            & match algorithm {
                Algorithm::Zlib => 1,
                Algorithm::Zstd => 2,
            }
            != 0
    }

    /// Whether a raw connection is explicitly permitted.
    #[must_use]
    pub const fn permits_uncompressed(self) -> bool {
        self.0 & 4 != 0
    }
}

/// Immutable native configuration shared by the shorthand and public class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    algorithms: Algorithms,
    zstd_level: u8,
}

/// The exact flag offer and deterministic algorithm implied by a greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Negotiated {
    /// Only allowed compression flags that the greeting actually provided.
    pub capabilities: CapabilityFlags,
    /// Actual compression mode implied by those flags, or allowed raw mode.
    pub algorithm: Option<Algorithm>,
    /// Requested Zstandard level. It applies only if Zstandard is selected.
    pub zstd_level: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            algorithms: Algorithms::UNCOMPRESSED,
            zstd_level: 3,
        }
    }
}

impl Options {
    /// Prepare one already type-checked native level in the approved 1..=22 range.
    ///
    /// # Errors
    /// Returns an out-of-range level; Python exact-int validation stays upstream.
    pub fn new(algorithms: Algorithms, zstd_level: u8) -> Result<Self, &'static str> {
        if !(1..=22).contains(&zstd_level) {
            return Err("zstd_compression_level must be in 1..=22");
        }
        Ok(Self {
            algorithms,
            zstd_level,
        })
    }

    /// Prepared immutable permissions, without a Python input object's identity.
    #[must_use]
    pub const fn algorithms(self) -> Algorithms {
        self.algorithms
    }

    /// Configured native Zstandard level, applicable only to Zstandard.
    #[must_use]
    pub const fn zstd_level(self) -> u8 {
        self.zstd_level
    }

    /// Select the official priority zlib, then zstd, then explicitly allowed raw.
    /// A later authentication failure still keeps its actual server origin;
    /// greeting flags do not reveal whether the server permits raw connections.
    ///
    /// # Errors
    /// Rejects a greeting with no mutually permitted compressed capability when
    /// raw fallback was not explicitly allowed. Never reconnects or probes SQL.
    pub fn negotiate(self, server: CapabilityFlags) -> Result<Negotiated, &'static str> {
        let mut capabilities = CapabilityFlags::empty();
        if self.algorithms.permits(Algorithm::Zlib) {
            capabilities |= server & CapabilityFlags::CLIENT_COMPRESS;
        }
        if self.algorithms.permits(Algorithm::Zstd) {
            capabilities |= server & CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM;
        }
        let algorithm = if capabilities.contains(CapabilityFlags::CLIENT_COMPRESS) {
            Some(Algorithm::Zlib)
        } else if capabilities.contains(CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM) {
            Some(Algorithm::Zstd)
        } else if self.algorithms.permits_uncompressed() {
            None
        } else {
            return Err("server provides no permitted compression algorithm");
        };
        Ok(Negotiated {
            capabilities,
            algorithm,
            zstd_level: self.zstd_level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Pool, Scope};
    use crate::protocol::framing;
    use std::io::Cursor;

    fn scope() -> Arc<Scope> {
        Scope::new(Pool::new(64 * 1024 * 1024), 64 * 1024 * 1024).unwrap()
    }

    fn send<W: Write>(
        codec: &mut Codec,
        bytes: &[u8],
        target: &mut W,
        sequence: &mut Sequence,
        scope: &Arc<Scope>,
    ) {
        let mut message = MessageWriter::new(bytes.len(), bytes.len()).unwrap();
        for _ in 0..100_000 {
            if matches!(
                codec
                    .write_message_step(&mut message, bytes, target, sequence, scope)
                    .unwrap(),
                Progress::Ready(())
            ) {
                return;
            }
        }
        panic!("bounded writer did not finish");
    }

    fn receive<R: Read>(
        codec: &mut Codec,
        source: &mut R,
        sequence: &mut Sequence,
        scope: &Arc<Scope>,
    ) -> Result<Buffer<u8>, WireError> {
        let mut message = MessageReader::tracked(8 * 1024 * 1024, scope);
        for _ in 0..100_000 {
            if let Progress::Ready(bytes) =
                codec.read_message_step(&mut message, source, sequence, scope)?
            {
                return Ok(bytes);
            }
        }
        panic!("bounded reader did not finish");
    }

    fn frame(algorithm: Algorithm, plain: &[u8]) -> Vec<u8> {
        let mut encoder = engine::Encoder::new(algorithm, 3).unwrap();
        let mut compressed = vec![0; engine::Encoder::bound(plain.len())];
        let length = encoder.encode(plain, &mut compressed).unwrap();
        compressed.truncate(length);
        let expanded = plain.len();
        let mut bytes = vec![
            length as u8,
            (length >> 8) as u8,
            (length >> 16) as u8,
            0,
            expanded as u8,
            (expanded >> 8) as u8,
            (expanded >> 16) as u8,
        ];
        bytes.extend_from_slice(&compressed);
        bytes
    }

    #[test]
    fn permission_parsers_normalize_ascii_names_without_expanding_sequence_csv() {
        let expected = Algorithms::from_csv(" ZSTD, zlib,zstd,uncompressed ").unwrap();
        assert_eq!(
            expected,
            Algorithms::from_names(["uncompressed", " ZLIB ", "zstd"]).unwrap()
        );
        assert_eq!(
            expected.names().collect::<Vec<_>>(),
            ["zlib", "zstd", "uncompressed"]
        );
        for names in [
            "",
            " ",
            "zstd,",
            ",zlib",
            "zstd,,zlib",
            "off",
            "none",
            "auto",
            "\u{a0}zstd",
        ] {
            assert!(Algorithms::from_csv(names).is_err());
        }
        assert!(Algorithms::from_names([]).is_err());
        assert!(Algorithms::from_names(["zlib,zstd"]).is_err());
    }

    #[test]
    fn negotiation_follows_mysql_priority_and_explicit_raw_permission() {
        let both =
            CapabilityFlags::CLIENT_COMPRESS | CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM;
        let configured = Options::new(Algorithms::from_csv("zstd,zlib").unwrap(), 10).unwrap();
        assert_eq!(
            configured.negotiate(both).unwrap(),
            Negotiated {
                capabilities: both,
                algorithm: Some(Algorithm::Zlib),
                zstd_level: 10
            }
        );
        assert_eq!(
            configured
                .negotiate(CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM)
                .unwrap()
                .algorithm,
            Some(Algorithm::Zstd)
        );
        assert!(configured.negotiate(CapabilityFlags::empty()).is_err());
        assert_eq!(Options::default().negotiate(both).unwrap().algorithm, None);
        assert_eq!(
            Options::new(Algorithms::from_csv("zstd,uncompressed").unwrap(), 3)
                .unwrap()
                .negotiate(CapabilityFlags::CLIENT_COMPRESS)
                .unwrap()
                .algorithm,
            None
        );
        assert!(Options::new(Algorithms::UNCOMPRESSED, 0).is_err());
        assert!(Options::new(Algorithms::UNCOMPRESSED, 23).is_err());
    }

    #[test]
    fn both_algorithms_roundtrip_fragmented_messages_with_independent_sequences() {
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let baseline = scope.used();
            let input = vec![0x5a; 1_000_000];
            let mut wire = Vec::new();
            let mut sender = Codec::new(algorithm, 3).unwrap();
            assert_eq!(
                sender.snapshot().algorithms().names().collect::<Vec<_>>(),
                [algorithm.name()]
            );
            assert_eq!(sender.snapshot().zstd_level(), 3);
            sender.start_command().unwrap();
            let mut ordinary = Sequence::default();
            send(&mut sender, &input, &mut wire, &mut ordinary, &scope);
            assert_eq!(
                scope.used(),
                baseline,
                "outbound work buffers must release at send completion"
            );
            assert_eq!(ordinary.next(), 1);
            assert!(sender.sequence > 1);
            let mut receiver = Codec::new(algorithm, 3).unwrap();
            let mut ordinary = Sequence::default();
            assert_eq!(
                receive(&mut receiver, &mut Cursor::new(wire), &mut ordinary, &scope).unwrap(),
                input
            );
            assert_eq!(ordinary.next(), 1);
            assert_eq!(receiver.sequence, sender.sequence);
            receiver.start_command().unwrap();
            assert_eq!(scope.used(), baseline);
        }
    }

    #[test]
    fn one_compressed_frame_can_hold_several_logical_messages() {
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let mut plain = Vec::new();
            let mut sequence = Sequence::default();
            for input in [vec![b'a'; 2000], vec![b'b'; 3000]] {
                framing::write_message(&mut plain, &input, &mut sequence, 4096).unwrap();
            }
            let mut wire = Cursor::new(frame(algorithm, &plain));
            let mut codec = Codec::new(algorithm, 3).unwrap();
            let mut sequence = Sequence::default();
            assert_eq!(
                receive(&mut codec, &mut wire, &mut sequence, &scope).unwrap(),
                vec![b'a'; 2000]
            );
            assert_eq!(
                receive(&mut codec, &mut wire, &mut sequence, &scope).unwrap(),
                vec![b'b'; 3000]
            );
            codec.start_command().unwrap();
        }
    }

    #[test]
    fn compressed_and_ordinary_sequences_wrap_separately() {
        let scope = scope();
        let mut sender = Codec::new(Algorithm::Zlib, 3).unwrap();
        sender.sequence = 255;
        let mut sequence = Sequence::new(254);
        let mut wire = Vec::new();
        for input in [b"one", b"two", b"end"] {
            send(&mut sender, input, &mut wire, &mut sequence, &scope);
        }
        assert_eq!((sender.sequence, sequence.next()), (2, 1));
        let mut receiver = Codec::new(Algorithm::Zlib, 3).unwrap();
        receiver.sequence = 255;
        let mut sequence = Sequence::new(254);
        let mut wire = Cursor::new(wire);
        for expected in [b"one", b"two", b"end"] {
            assert_eq!(
                receive(&mut receiver, &mut wire, &mut sequence, &scope)
                    .unwrap()
                    .as_slice(),
                expected
            );
        }
        assert_eq!((receiver.sequence, sequence.next()), (2, 1));
    }

    #[test]
    fn malformed_lengths_sequences_checksums_and_trailing_bytes_never_publish_a_message() {
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let mut plain = Vec::new();
            framing::write_message(&mut plain, &vec![b'a'; 512], &mut Sequence::default(), 4096)
                .unwrap();
            let wire = frame(algorithm, &plain);
            let mut wrong_sequence = wire.clone();
            wrong_sequence[3] = 7;
            let mut too_short = wire.clone();
            too_short[4] -= 1;
            let mut too_long = wire.clone();
            too_long[4] += 1;
            let mut trailing = wire.clone();
            trailing[0] += 1;
            trailing.push(0);
            let mut invalid = wire.clone();
            invalid[7] ^= 0xff;
            for bytes in [
                wrong_sequence,
                too_short,
                too_long,
                trailing,
                invalid,
                vec![0, 0, 0, 0, 1, 0, 0],
            ] {
                assert!(
                    receive(
                        &mut Codec::new(algorithm, 3).unwrap(),
                        &mut Cursor::new(bytes),
                        &mut Sequence::default(),
                        &scope
                    )
                    .is_err()
                );
            }
            if algorithm == Algorithm::Zlib {
                let mut checksum = wire;
                *checksum.last_mut().unwrap() ^= 1;
                assert!(
                    receive(
                        &mut Codec::new(algorithm, 3).unwrap(),
                        &mut Cursor::new(checksum),
                        &mut Sequence::default(),
                        &scope
                    )
                    .is_err()
                );
            }
        }
    }

    #[test]
    fn compression_buffers_obey_actual_shared_scope_capacity() {
        let scope = Scope::new(Pool::new(4096), 4096).unwrap();
        let input = vec![b'a'; 100_000];
        let mut codec = Codec::new(Algorithm::Zlib, 3).unwrap();
        let mut message = MessageWriter::new(input.len(), input.len()).unwrap();
        assert!(matches!(
            codec.write_message_step(
                &mut message,
                &input,
                &mut Vec::new(),
                &mut Sequence::default(),
                &scope
            ),
            Err(WireError::Memory(_))
        ));
        assert!(codec.start_command().is_err());
    }

    struct Intermittent {
        cursor: Cursor<Vec<u8>>,
        blocked: bool,
    }
    impl Read for Intermittent {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.blocked = !self.blocked;
            if self.blocked {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let length = bytes.len().min(1);
            self.cursor.read(&mut bytes[..length])
        }
    }
    impl Write for Intermittent {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.blocked = !self.blocked;
            if self.blocked {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.cursor.write(&bytes[..bytes.len().min(2)])
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn partial_headers_payloads_and_footer_waits_keep_owned_progress() {
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let input = vec![b'a'; 512];
            let mut stream = Intermittent {
                cursor: Cursor::new(Vec::new()),
                blocked: false,
            };
            let mut sender = Codec::new(algorithm, 3).unwrap();
            send(
                &mut sender,
                &input,
                &mut stream,
                &mut Sequence::default(),
                &scope,
            );
            stream.cursor.set_position(0);
            let mut receiver = Codec::new(algorithm, 3).unwrap();
            let mut message = MessageReader::tracked(4096, &scope);
            let mut sequence = Sequence::default();
            let mut saw_footer_wait = false;
            let mut finished = false;
            for _ in 0..10_000 {
                match receiver
                    .read_message_step(&mut message, &mut stream, &mut sequence, &scope)
                    .unwrap()
                {
                    Progress::Ready(bytes) => {
                        assert_eq!(bytes, input);
                        finished = true;
                        break;
                    }
                    Progress::Pending | Progress::Yield => {
                        saw_footer_wait |= receiver.ready.is_some()
                    }
                }
            }
            assert!(finished);
            if algorithm == Algorithm::Zlib {
                assert!(
                    saw_footer_wait,
                    "zlib checksum must be validated before publishing the message"
                );
            }
            receiver.start_command().unwrap();
        }
    }

    #[test]
    fn new_command_rejects_unread_bytes_without_sending_or_resetting() {
        let scope = scope();
        let mut plain = Vec::new();
        let mut sequence = Sequence::default();
        framing::write_message(&mut plain, b"first", &mut sequence, 4096).unwrap();
        framing::write_message(&mut plain, b"extra", &mut sequence, 4096).unwrap();
        let mut codec = Codec::new(Algorithm::Zlib, 3).unwrap();
        receive(
            &mut codec,
            &mut Cursor::new(frame(Algorithm::Zlib, &plain)),
            &mut Sequence::default(),
            &scope,
        )
        .unwrap();
        assert!(codec.start_command().is_err());
        let mut wire = Vec::new();
        let mut message = MessageWriter::new(1, 1).unwrap();
        assert!(
            codec
                .write_message_step(
                    &mut message,
                    b"x",
                    &mut wire,
                    &mut Sequence::default(),
                    &scope
                )
                .is_err()
        );
        assert!(wire.is_empty());
    }

    #[test]
    fn configured_zstd_levels_keep_bounded_frames_and_report_workspace_separately() {
        for level in [1, 3, 22] {
            let scope = scope();
            let input = vec![b'a'; 64 * 1024];
            let mut codec = Codec::new(Algorithm::Zstd, level).unwrap();
            let before = codec.zstd_workspace_bytes().unwrap();
            let mut wire = Vec::new();
            send(
                &mut codec,
                &input,
                &mut wire,
                &mut Sequence::default(),
                &scope,
            );
            let mut receiver = Codec::new(Algorithm::Zstd, level).unwrap();
            assert_eq!(
                receive(
                    &mut receiver,
                    &mut Cursor::new(wire),
                    &mut Sequence::default(),
                    &scope
                )
                .unwrap(),
                input
            );
            println!(
                "zstd level={level} fixed_output_block=65536 decoder_window_log_max=24 initial_contexts={before:?} encoder_current={} decoder_current={}",
                codec.zstd_workspace_bytes().unwrap().0,
                receiver.zstd_workspace_bytes().unwrap().1
            );
        }
    }

    #[test]
    fn logical_receive_limit_is_enforced_on_decompressed_framing_header() {
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let plain = b"\x09\0\0\0ninebytes";
            let mut codec = Codec::new(algorithm, 3).unwrap();
            let mut message = MessageReader::tracked(8, &scope);
            assert!(matches!(
                codec.read_message_step(
                    &mut message,
                    &mut Cursor::new(frame(algorithm, plain)),
                    &mut Sequence::default(),
                    &scope
                ),
                Err(WireError::Limit {
                    required: 9,
                    limit: 8
                })
            ));
        }
    }

    #[test]
    fn maximum_expanded_frame_streams_without_an_extra_full_expansion_buffer() {
        let length = framing::MAX_FRAME_PAYLOAD - 4;
        for algorithm in [Algorithm::Zlib, Algorithm::Zstd] {
            let scope = scope();
            let mut plain = Vec::new();
            framing::write_message(
                &mut plain,
                &vec![b'a'; length],
                &mut Sequence::default(),
                length,
            )
            .unwrap();
            assert_eq!(plain.len(), framing::MAX_FRAME_PAYLOAD);
            let mut source = Cursor::new(frame(algorithm, &plain));
            drop(plain);
            let mut codec = Codec::new(algorithm, 3).unwrap();
            let mut message = MessageReader::tracked(length, &scope);
            let mut sequence = Sequence::default();
            loop {
                if let Progress::Ready(bytes) = codec
                    .read_message_step(&mut message, &mut source, &mut sequence, &scope)
                    .unwrap()
                {
                    assert_eq!(bytes.len(), length);
                    assert!(bytes.iter().all(|byte| *byte == b'a'));
                    break;
                }
            }
            codec.start_command().unwrap();
        }
    }
}
