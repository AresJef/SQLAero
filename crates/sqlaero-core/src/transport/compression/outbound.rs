//! Bounded compression and resumable writing of independent compressed frames.

use super::Algorithm;
use super::engine::Encoder;
use crate::memory::{Buffer, Scope};
use crate::protocol::framing::{Progress, WireError};
use std::io::{self, Write};
use std::sync::Arc;

pub(super) const BLOCK: usize = 64 * 1024;
pub(super) const STEP: usize = 256 * 1024;

pub(super) struct Outbound {
    encoder: Encoder,
    plain: Buffer<u8>,
    encoded: Buffer<u8>,
    header: [u8; 7],
    header_sent: usize,
    body_sent: usize,
    encoded_body: bool,
    pending: bool,
}

impl Outbound {
    pub(super) fn new(algorithm: Algorithm, level: u8) -> Result<Self, WireError> {
        Ok(Self {
            encoder: Encoder::new(algorithm, level)?,
            plain: Buffer::new(),
            encoded: Buffer::new(),
            header: [0; 7],
            header_sent: 0,
            body_sent: 0,
            encoded_body: false,
            pending: false,
        })
    }

    pub(super) fn is_idle(&self) -> bool {
        !self.pending && self.plain.is_empty()
    }

    pub(super) fn zstd_workspace_bytes(&self) -> Option<usize> {
        self.encoder.zstd_workspace_bytes()
    }

    pub(super) fn release(&mut self) {
        self.plain = Buffer::new();
        self.encoded = Buffer::new();
    }

    pub(super) fn write<W: Write>(
        &mut self,
        target: &mut W,
        sequence: &mut u8,
        input: &[u8],
        scope: &Arc<Scope>,
        work: &mut usize,
    ) -> Result<Progress<usize>, WireError> {
        if input.is_empty() {
            return Ok(Progress::Ready(0));
        }
        if *work >= STEP {
            return Ok(Progress::Yield);
        }
        if self.pending || self.plain.len() == BLOCK {
            match self.flush(target, sequence, work)? {
                Progress::Ready(()) => (),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
        }
        if self.plain.capacity() == 0 {
            self.plain = Buffer::tracked(scope);
            self.encoded = Buffer::tracked(scope);
        }
        let length = input.len().min(BLOCK - self.plain.len()).min(STEP - *work);
        self.plain.extend_from_slice(&input[..length])?;
        *work += length;
        Ok(Progress::Ready(length))
    }

    fn prepare(&mut self, sequence: u8, work: &mut usize) -> Result<(), WireError> {
        if self.plain.is_empty() || self.pending {
            return Ok(());
        }
        self.encoded_body = false;
        let mut length = self.plain.len();
        let mut expanded = 0;
        if length >= 50 {
            self.encoded.resize(Encoder::bound(length), 0)?;
            let compressed = self
                .encoder
                .encode(&self.plain, self.encoded.as_mut_slice())?;
            if compressed < length {
                self.encoded.truncate(compressed);
                expanded = length;
                length = compressed;
                self.encoded_body = true;
            }
        }
        self.header = [
            length as u8,
            (length >> 8) as u8,
            (length >> 16) as u8,
            sequence,
            expanded as u8,
            (expanded >> 8) as u8,
            (expanded >> 16) as u8,
        ];
        self.header_sent = 0;
        self.body_sent = 0;
        self.pending = true;
        *work = work.saturating_add(self.plain.len());
        Ok(())
    }

    pub(super) fn flush<W: Write>(
        &mut self,
        target: &mut W,
        sequence: &mut u8,
        work: &mut usize,
    ) -> Result<Progress<()>, WireError> {
        if self.is_idle() {
            return Ok(Progress::Ready(()));
        }
        if *work >= STEP {
            return Ok(Progress::Yield);
        }
        self.prepare(*sequence, work)?;
        loop {
            if *work >= STEP {
                return Ok(Progress::Yield);
            }
            let body = if self.encoded_body {
                self.encoded.as_slice()
            } else {
                self.plain.as_slice()
            };
            let bytes = if self.header_sent < 7 {
                &self.header[self.header_sent..]
            } else {
                &body[self.body_sent..]
            };
            if bytes.is_empty() {
                *sequence = sequence.wrapping_add(1);
                self.pending = false;
                self.plain.clear();
                self.encoded.clear();
                return Ok(Progress::Ready(()));
            }
            let bytes = &bytes[..bytes.len().min(STEP - *work)];
            match target.write(bytes) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                Ok(length) => {
                    if self.header_sent < 7 {
                        self.header_sent += length;
                    } else {
                        self.body_sent += length;
                    }
                    *work += length;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(Progress::Pending);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    return Ok(Progress::Yield);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}
