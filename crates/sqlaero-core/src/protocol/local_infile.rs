//! Explicit one-operation LOCAL source and bounded upload on the shared channel.

use crate::memory::{Buffer, Reservation, Scope};
use crate::protocol::framing::{MessageWriter, WireError};
use crate::transport::channel::{Channel, ChannelError, Step};
use crate::transport::deadline::IoBudget;
use crate::transport::socket::WaitStatus;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

/// Local file failure stays separate from wire/channel failures.
#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    /// The same authorized file failed or ended prematurely during reading.
    #[error("LOCAL source read failed: {0}")]
    Source(#[from] io::Error),
    /// Actual framing, capacity or invalid upload-state failure.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// The shared channel or original operation deadline failed.
    #[error(transparent)]
    Channel(#[from] ChannelError),
}

/// Native upload transfer observations, never an SQL/transaction confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadProgress {
    /// Fixed length observed on the actual opened file.
    pub opening_length: u64,
    /// Bytes actually read from that same file.
    pub read_bytes: u64,
    /// Complete data payload bytes flushed through the selected channel.
    pub flushed_bytes: u64,
    /// Whether the normal final empty upload packet was flushed.
    pub eof_flushed: bool,
}

/// Prepared one-operation upload. Construction may reserve its bounded native
/// work buffer before old results are handed off or any SQL request is sent.
pub struct Upload {
    source: Source,
    buffer: Buffer<u8>,
    chunk_size: usize,
    sender: Option<MessageWriter>,
    scope: Arc<Scope>,
    max_send: usize,
    requested: bool,
    eof: bool,
    eof_flushed: bool,
    finished: bool,
    flushed: u64,
    _owner: Reservation,
}

impl Upload {
    /// Prepare bounded upload storage from an explicitly authorized source.
    ///
    /// # Errors
    /// Returns request-bound or actual scope/allocation failures before reading.
    pub fn new(source: Source, max_send: usize, scope: Arc<Scope>) -> Result<Self, LocalError> {
        // Even an empty file uses an empty packet, but a non-empty file must
        // make progress under the configured logical-message bound.
        if max_send == 0 && source.remaining() != 0 {
            return Err(WireError::Limit {
                required: 1,
                limit: 0,
            }
            .into());
        }
        let owner = scope
            .charge(std::mem::size_of::<Self>())
            .map_err(WireError::from)?;
        let chunk_size = (64 * 1024)
            .min(max_send)
            .min(usize::try_from(source.remaining()).unwrap_or(usize::MAX));
        let mut buffer = Buffer::tracked(&scope);
        buffer.resize(chunk_size, 0).map_err(WireError::from)?;
        buffer.clear();
        Ok(Self {
            source,
            buffer,
            chunk_size,
            sender: None,
            scope,
            max_send,
            requested: false,
            eof: false,
            eof_flushed: false,
            finished: false,
            flushed: 0,
            _owner: owner,
        })
    }

    /// Consume the one expected LOCAL request. A server filename is deliberately
    /// absent from this API: it cannot select or authorize a different source.
    ///
    /// # Errors
    /// Rejects an additional request after this operation's permission was used.
    pub fn accept_request(&mut self) -> Result<(), LocalError> {
        if self.requested {
            return Err(WireError::Malformed("unexpected additional LOCAL request").into());
        }
        self.requested = true;
        Ok(())
    }

    /// Current actual transfer observations, without claiming server receipt.
    #[must_use]
    pub fn progress(&self) -> UploadProgress {
        UploadProgress {
            opening_length: self.source.length(),
            read_bytes: self.source.read_bytes(),
            flushed_bytes: self.flushed,
            eof_flushed: self.eof_flushed,
        }
    }

    /// Continue the current command, without resetting either packet sequence
    /// or operation deadline. Ready means EOF was flushed; the shared response
    /// receiver must still consume the actual final OK/ERR.
    ///
    /// # Errors
    /// Retains the main local read, wire or channel error; never sends a normal
    /// empty EOF as a substitute for canceling an incomplete upload.
    pub fn advance(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<UploadProgress>, LocalError> {
        let result = self.advance_inner(channel, budget);
        if result.is_err() {
            self.finished = true;
        }
        result
    }

    fn advance_inner(
        &mut self,
        channel: &mut Channel,
        budget: &mut IoBudget,
    ) -> Result<Step<UploadProgress>, LocalError> {
        if self.finished || !self.requested {
            return Err(WireError::Malformed("LOCAL upload is not active").into());
        }
        budget.check().map_err(ChannelError::from)?;
        if self.sender.is_none() {
            self.eof = self.source.remaining() == 0;
            self.buffer.clear();
            if !self.eof {
                self.buffer
                    .resize(self.chunk_size, 0)
                    .map_err(WireError::from)?;
                let read = match self.source.read_chunk(self.buffer.as_mut_slice()) {
                    Ok(read) => read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        return Ok(Step::Control);
                    }
                    Err(error) => return Err(error.into()),
                };
                self.buffer.truncate(read);
                // Filesystem calls remain ordinary blocking calls. Their
                // elapsed work belongs to the same operation deadline.
                budget.check().map_err(ChannelError::from)?;
            }
            self.sender = Some(MessageWriter::new(self.buffer.len(), self.max_send)?);
        }
        loop {
            match channel.send_step(
                self.sender.as_mut().expect("prepared LOCAL message"),
                &self.buffer,
                budget,
                Some(&self.scope),
            )? {
                Step::Ready(()) => {
                    self.sender = None;
                    if self.eof {
                        self.eof_flushed = true;
                        self.finished = true;
                        return Ok(Step::Ready(self.progress()));
                    }
                    self.flushed += self.buffer.len() as u64;
                    // At most one bounded file read and complete data packet
                    // per host signal checkpoint. No sequence/deadline reset.
                    return Ok(Step::Control);
                }
                Step::Control => return Ok(Step::Control),
                Step::Wait(direction) => {
                    if channel.wait(direction, budget)? == WaitStatus::Control {
                        return Ok(Step::Control);
                    }
                }
            }
        }
    }
}

/// The one actual opened regular-file object authorized for this operation.
/// Its opening length is fixed; no server filename enters this type's API.
pub struct Source {
    file: File,
    length: u64,
    read: u64,
}

impl Source {
    /// Open one ordinary file once and retain that same object after path changes.
    /// The caller owns the existing operation deadline before entering this
    /// blocking filesystem call and checks it again immediately afterwards.
    ///
    /// # Errors
    /// Preserves actual open/metadata failures or rejects a non-regular object.
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // A FIFO must not block waiting for a writer before the actual
            // opened object can be rejected. Regular-file reads keep their
            // normal filesystem semantics; this does not promise hard timeout.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LOCAL source must be a regular file",
            ));
        }
        Ok(Self {
            file,
            length: metadata.len(),
            read: 0,
        })
    }

    /// Actual length captured from the opened file, without a new path lookup.
    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// Bytes actually read from the same file, independent of network receipt.
    #[must_use]
    pub const fn read_bytes(&self) -> u64 {
        self.read
    }

    /// Remaining bytes under the original fixed-length authorization.
    #[must_use]
    pub const fn remaining(&self) -> u64 {
        self.length - self.read
    }

    /// Read at most one caller-bounded chunk and never follow later appends.
    /// Zero means the fixed opening length is complete, including empty files.
    ///
    /// # Errors
    /// Returns actual local I/O or premature EOF; EOF is never used to disguise
    /// a truncated upload as normal completion.
    pub fn read_chunk(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.remaining() == 0 {
            return Ok(0);
        }
        if output.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LOCAL read chunk must not be empty",
            ));
        }
        let length = output
            .len()
            .min(usize::try_from(self.remaining()).unwrap_or(usize::MAX));
        let read = self.file.read(&mut output[..length])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LOCAL source ended before its opening length",
            ));
        }
        self.read += read as u64;
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Pool;
    use crate::protocol::framing::{self, MessageReader, Sequence};
    use crate::transport::deadline::{Direction, TimeoutKind};
    use crate::transport::socket::Transport;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Case {
        directory: PathBuf,
    }
    impl Case {
        fn new(bytes: &[u8]) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("sqlaero-c-local-{}-{nonce}", std::process::id()));
            fs::create_dir(&directory).unwrap();
            let case = Self { directory };
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(case.path())
                .unwrap();
            file.write_all(bytes).unwrap();
            case
        }
        fn path(&self) -> PathBuf {
            self.directory.join("导入😀.csv")
        }
    }
    impl Drop for Case {
        fn drop(&mut self) {
            for name in ["导入😀.csv", "renamed.csv", "link.csv", "fifo"] {
                let _ = fs::remove_file(self.directory.join(name));
            }
            let _ = fs::remove_dir(&self.directory);
        }
    }

    #[test]
    fn source_keeps_same_open_object_when_path_is_renamed_and_replaced() {
        let case = Case::new(b"original");
        let mut source = Source::open(&case.path()).unwrap();
        fs::rename(case.path(), case.directory.join("renamed.csv")).unwrap();
        fs::write(case.path(), b"unrelated").unwrap();
        assert_eq!(source.length(), 8);
        let mut bytes = [0; 32];
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 8);
        assert_eq!(&bytes[..8], b"original");
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 0);
    }

    #[test]
    fn later_append_is_ignored_and_early_eof_is_an_actual_error() {
        let case = Case::new(b"abc");
        let mut source = Source::open(&case.path()).unwrap();
        OpenOptions::new()
            .append(true)
            .open(case.path())
            .unwrap()
            .write_all(b"append")
            .unwrap();
        let mut bytes = [0; 32];
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 3);
        assert_eq!(&bytes[..3], b"abc");
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 0);
        let mut source = Source::open(&case.path()).unwrap();
        OpenOptions::new()
            .write(true)
            .open(case.path())
            .unwrap()
            .set_len(2)
            .unwrap();
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 2);
        assert_eq!(
            source.read_chunk(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(source.read_bytes(), 2);
    }

    #[test]
    fn empty_regular_files_finish_without_reading_and_directories_are_rejected() {
        let case = Case::new(b"");
        let mut source = Source::open(&case.path()).unwrap();
        assert_eq!(source.length(), 0);
        assert_eq!(source.read_chunk(&mut []).unwrap(), 0);
        assert!(Source::open(&case.directory).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn ordinary_symlink_resolution_uses_the_original_opened_handle() {
        let case = Case::new(b"linked");
        let link = case.directory.join("link.csv");
        std::os::unix::fs::symlink(case.path(), &link).unwrap();
        let mut source = Source::open(&link).unwrap();
        fs::remove_file(link).unwrap();
        let mut bytes = [0; 32];
        assert_eq!(source.read_chunk(&mut bytes).unwrap(), 6);
        assert_eq!(&bytes[..6], b"linked");
    }

    #[test]
    #[cfg(unix)]
    fn fifo_is_rejected_without_waiting_for_a_writer() {
        use std::os::unix::fs::OpenOptionsExt;
        use std::sync::mpsc;
        use std::time::Duration;
        let case = Case::new(b"");
        let path = case.directory.join("fifo");
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: The owned NUL-terminated CString remains alive for this
        // synchronous libc call, and mode has no pointer or lifetime contract.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = mpsc::channel();
        let thread_path = path.clone();
        let reader = std::thread::spawn(move || {
            sender.send(Source::open(&thread_path).map(|_| ())).unwrap();
        });
        let immediate = receiver.recv_timeout(Duration::from_secs(1));
        if immediate.is_err() {
            // A regressed blocking open gets a writer so the test can fail
            // without leaving a permanently stuck suite or background thread.
            let _writer = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap();
            let _ = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        }
        reader.join().unwrap();
        assert!(matches!(immediate, Ok(Err(_))));
    }

    fn peer(expected: Vec<u8>, complete: bool) -> (Channel, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut sequence = Sequence::default();
            let request = framing::read_message(&mut socket, &mut sequence, 4096).unwrap();
            assert_eq!(request.first(), Some(&3));
            framing::write_message(
                &mut socket,
                b"\xfb/does/not/authorize/a/source",
                &mut sequence,
                4096,
            )
            .unwrap();
            let mut received = Vec::new();
            loop {
                match framing::read_message(&mut socket, &mut sequence, 64 * 1024) {
                    Ok(bytes) if bytes.is_empty() => {
                        assert!(complete, "a failed upload sent a false normal EOF");
                        break;
                    }
                    Ok(bytes) => received.extend_from_slice(&bytes),
                    Err(WireError::Io(error))
                        if !complete && error.kind() == io::ErrorKind::UnexpectedEof =>
                    {
                        break;
                    }
                    Err(error) => panic!("unexpected upload peer failure: {error}"),
                }
            }
            assert_eq!(received, expected);
            if complete {
                framing::write_message(&mut socket, &[0, 3, 0, 2, 0, 0, 0], &mut sequence, 4096)
                    .unwrap();
            }
        });
        (Channel::new(Transport::tcp(address).unwrap(), false), peer)
    }

    fn request_file(channel: &mut Channel, budget: &mut IoBudget, scope: &Arc<Scope>) {
        while !channel.connected().unwrap() {
            channel.wait(Direction::Write, budget).unwrap();
        }
        channel.new_command().unwrap();
        let mut message = MessageWriter::new(5, 4096).unwrap();
        loop {
            match channel
                .send_step(&mut message, b"\x03LOAD", budget, Some(scope))
                .unwrap()
            {
                Step::Ready(()) => break,
                Step::Wait(direction) => {
                    channel.wait(direction, budget).unwrap();
                }
                Step::Control => (),
            }
        }
        let mut incoming = MessageReader::tracked(4096, scope);
        loop {
            match channel
                .receive_step(&mut incoming, budget, Some(scope))
                .unwrap()
            {
                Step::Ready(bytes) => {
                    assert_eq!(bytes.first(), Some(&0xfb));
                    break;
                }
                Step::Wait(direction) => {
                    channel.wait(direction, budget).unwrap();
                }
                Step::Control => (),
            }
        }
    }

    #[test]
    fn upload_keeps_request_sequence_and_reads_final_response_only_after_empty_eof() {
        for (bytes, maximum) in [
            (Vec::new(), 64 * 1024),
            (vec![b'x'; 200_000], 64 * 1024),
            (vec![b'y'; 9_000], 127),
        ] {
            let case = Case::new(&bytes);
            let scope = Scope::new(Pool::new(2_000_000), 2_000_000).unwrap();
            let mut upload = Upload::new(
                Source::open(&case.path()).unwrap(),
                maximum,
                Arc::clone(&scope),
            )
            .unwrap();
            let (mut channel, peer) = peer(bytes.clone(), true);
            let mut budget = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_secs(3)),
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(2)),
            );
            request_file(&mut channel, &mut budget, &scope);
            upload.accept_request().unwrap();
            let progress = loop {
                match upload.advance(&mut channel, &mut budget).unwrap() {
                    Step::Ready(progress) => break progress,
                    Step::Control => (),
                    Step::Wait(direction) => {
                        channel.wait(direction, &mut budget).unwrap();
                    }
                }
            };
            assert_eq!(
                progress,
                UploadProgress {
                    opening_length: bytes.len() as u64,
                    read_bytes: bytes.len() as u64,
                    flushed_bytes: bytes.len() as u64,
                    eof_flushed: true
                }
            );
            assert!(upload.accept_request().is_err());
            let mut incoming = MessageReader::tracked(4096, &scope);
            loop {
                match channel
                    .receive_step(&mut incoming, &mut budget, Some(&scope))
                    .unwrap()
                {
                    Step::Ready(bytes) => {
                        assert_eq!(&*bytes, &[0, 3, 0, 2, 0, 0, 0]);
                        break;
                    }
                    Step::Control => (),
                    Step::Wait(direction) => {
                        channel.wait(direction, &mut budget).unwrap();
                    }
                }
            }
            peer.join().unwrap();
        }
    }

    #[test]
    fn truncated_upload_keeps_actual_partial_facts_without_sending_normal_eof() {
        let case = Case::new(&vec![b'x'; 100_000]);
        let source = Source::open(&case.path()).unwrap();
        let scope = Scope::new(Pool::new(2_000_000), 2_000_000).unwrap();
        let mut upload = Upload::new(source, 64 * 1024, Arc::clone(&scope)).unwrap();
        OpenOptions::new()
            .write(true)
            .open(case.path())
            .unwrap()
            .set_len(50_000)
            .unwrap();
        let (mut channel, peer) = peer(vec![b'x'; 50_000], false);
        let mut budget = IoBudget::new(
            TimeoutKind::Operation,
            Some(Duration::from_secs(3)),
            Some(Duration::from_secs(2)),
            Some(Duration::from_secs(2)),
        );
        request_file(&mut channel, &mut budget, &scope);
        upload.accept_request().unwrap();
        let error = loop {
            match upload.advance(&mut channel, &mut budget) {
                Ok(Step::Control) => (),
                Ok(step) => panic!("unexpected truncated upload result: {step:?}"),
                Err(error) => break error,
            }
        };
        assert!(
            matches!(error, LocalError::Source(error) if error.kind() == io::ErrorKind::UnexpectedEof)
        );
        assert_eq!(
            upload.progress(),
            UploadProgress {
                opening_length: 100_000,
                read_bytes: 50_000,
                flushed_bytes: 50_000,
                eof_flushed: false,
            }
        );
        assert!(upload.advance(&mut channel, &mut budget).is_err());
        // The command owner retires this uncertain connection. A local read
        // error cannot be repaired by pretending to send normal upload EOF.
        channel.shutdown().unwrap();
        peer.join().unwrap();
    }

    #[test]
    fn expired_original_budget_prevents_the_next_file_read() {
        let case = Case::new(b"unread");
        let scope = Scope::new(Pool::new(2_000_000), 2_000_000).unwrap();
        let mut upload =
            Upload::new(Source::open(&case.path()).unwrap(), 64 * 1024, scope).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut channel = Channel::new(
            Transport::tcp(listener.local_addr().unwrap()).unwrap(),
            false,
        );
        let mut budget = IoBudget::new(TimeoutKind::Operation, Some(Duration::ZERO), None, None);
        upload.accept_request().unwrap();
        assert!(matches!(
            upload.advance(&mut channel, &mut budget),
            Err(LocalError::Channel(ChannelError::Timeout(
                TimeoutKind::Operation
            )))
        ));
        assert_eq!(upload.progress().read_bytes, 0);
        assert!(!upload.progress().eof_flushed);
    }

    #[test]
    fn bounded_work_storage_is_charged_before_any_file_read_or_request() {
        let case = Case::new(&vec![0; 100_000]);
        let pool = Pool::new(2_000_000);
        let scope = Scope::new(Arc::clone(&pool), 4096).unwrap();
        let before = scope.used();
        assert!(matches!(
            Upload::new(
                Source::open(&case.path()).unwrap(),
                64 * 1024,
                Arc::clone(&scope)
            ),
            Err(LocalError::Wire(WireError::Memory(_)))
        ));
        assert_eq!(scope.used(), before);
        assert_eq!(pool.used(), before);
        assert!(matches!(
            Upload::new(Source::open(&case.path()).unwrap(), 0, scope),
            Err(LocalError::Wire(WireError::Limit {
                required: 1,
                limit: 0
            }))
        ));
    }
}
