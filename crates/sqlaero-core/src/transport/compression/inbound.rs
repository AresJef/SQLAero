//! Streaming decompression into the shared framing reader's charged target.

use super::Algorithm;
use super::engine::{Decoded, Decoder};
use super::outbound::STEP;
use crate::memory::{Buffer, Scope};
use crate::protocol::framing::{Progress, WireError};
use std::io::{self, Read};
use std::sync::Arc;

const INPUT: usize = 16 * 1024;

pub(super) struct Inbound {
    decoder: Decoder,
    header: [u8; 7],
    header_read: usize,
    active: bool,
    remaining: usize,
    expected: usize,
    expanded: usize,
    input: Buffer<u8>,
    position: usize,
}

impl Inbound {
    pub(super) fn new(algorithm: Algorithm) -> Result<Self, WireError> {
        Ok(Self {
            decoder: Decoder::new(algorithm)?,
            header: [0; 7],
            header_read: 0,
            active: false,
            remaining: 0,
            expected: 0,
            expanded: 0,
            input: Buffer::new(),
            position: 0,
        })
    }

    pub(super) fn is_idle(&self) -> bool {
        !self.active && self.header_read == 0
    }

    pub(super) fn zstd_workspace_bytes(&self) -> Option<usize> {
        self.decoder.zstd_workspace_bytes()
    }

    pub(super) fn needs_footer(&self) -> bool {
        self.active && self.expected != 0 && self.expanded == self.expected
    }

    fn header<R: Read>(
        &mut self,
        source: &mut R,
        sequence: &mut u8,
        scope: &Arc<Scope>,
        work: &mut usize,
    ) -> Result<Progress<()>, WireError> {
        while self.header_read < 7 {
            if *work >= STEP {
                return Ok(Progress::Yield);
            }
            let end = (self.header_read + STEP - *work).min(7);
            match source.read(&mut self.header[self.header_read..end]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                Ok(length) => {
                    self.header_read += length;
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
        if self.header[3] != *sequence {
            return Err(WireError::Sequence {
                expected: *sequence,
                received: self.header[3],
            });
        }
        *sequence = sequence.wrapping_add(1);
        self.remaining = size(&self.header[..3]);
        self.expected = size(&self.header[4..]);
        if self.expected != 0 && self.remaining == 0 {
            return Err(WireError::Malformed(
                "empty compressed body declares expanded bytes",
            ));
        }
        self.expanded = 0;
        self.position = 0;
        self.input = Buffer::tracked(scope);
        self.active = true;
        if self.expected != 0 {
            self.decoder.reset()?;
        }
        Ok(Progress::Ready(()))
    }

    fn fill<R: Read>(
        &mut self,
        source: &mut R,
        work: &mut usize,
    ) -> Result<Progress<()>, WireError> {
        if self.position != self.input.len() || self.remaining == 0 {
            return Ok(Progress::Ready(()));
        }
        if *work >= STEP {
            return Ok(Progress::Yield);
        }
        self.input
            .resize(self.remaining.min(INPUT).min(STEP - *work), 0)?;
        self.position = 0;
        match source.read(self.input.as_mut_slice()) {
            Ok(0) => {
                self.input.clear();
                Err(io::Error::from(io::ErrorKind::UnexpectedEof).into())
            }
            Ok(length) => {
                self.input.truncate(length);
                self.remaining -= length;
                *work += length;
                Ok(Progress::Ready(()))
            }
            Err(error) => {
                self.input.clear();
                match error.kind() {
                    io::ErrorKind::WouldBlock => Ok(Progress::Pending),
                    io::ErrorKind::Interrupted => Ok(Progress::Yield),
                    _ => Err(error.into()),
                }
            }
        }
    }

    fn decoded(&mut self, decoded: &Decoded, work: &mut usize) -> Result<(), WireError> {
        self.position += decoded.consumed;
        self.expanded = self
            .expanded
            .checked_add(decoded.produced)
            .filter(|expanded| *expanded <= self.expected)
            .ok_or(WireError::Malformed(
                "compressed payload exceeds declared expanded length",
            ))?;
        *work = work.saturating_add(decoded.produced);
        if decoded.finished {
            if self.expanded != self.expected {
                return Err(WireError::Malformed(
                    "compressed payload has wrong expanded length",
                ));
            }
            if self.remaining != 0 || self.position != self.input.len() {
                return Err(WireError::Malformed("trailing compressed payload bytes"));
            }
            self.end_frame();
        }
        Ok(())
    }

    fn end_frame(&mut self) {
        self.active = false;
        self.header_read = 0;
        self.input = Buffer::new();
        self.position = 0;
    }

    pub(super) fn finish_footer<R: Read>(
        &mut self,
        source: &mut R,
        work: &mut usize,
    ) -> Result<Progress<()>, WireError> {
        while self.needs_footer() {
            if *work >= STEP {
                return Ok(Progress::Yield);
            }
            match self.fill(source, work)? {
                Progress::Ready(()) => (),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
            let mut extra = [0; 1];
            let decoded = self
                .decoder
                .decode(&self.input[self.position..], &mut extra)?;
            self.decoded(&decoded, work)?;
            if decoded.consumed == 0 && decoded.produced == 0 && !decoded.finished {
                return Err(WireError::Malformed("truncated compressed payload footer"));
            }
        }
        Ok(Progress::Ready(()))
    }

    pub(super) fn read<R: Read>(
        &mut self,
        source: &mut R,
        sequence: &mut u8,
        target: &mut [u8],
        scope: &Arc<Scope>,
        work: &mut usize,
    ) -> Result<Progress<usize>, WireError> {
        if target.is_empty() {
            return Ok(Progress::Ready(0));
        }
        loop {
            if *work >= STEP {
                return Ok(Progress::Yield);
            }
            if !self.active {
                match self.header(source, sequence, scope, work)? {
                    Progress::Ready(()) => (),
                    Progress::Pending => return Ok(Progress::Pending),
                    Progress::Yield => return Ok(Progress::Yield),
                }
            }
            if *work >= STEP {
                return Ok(Progress::Yield);
            }
            if self.expected == 0 {
                if self.remaining == 0 {
                    self.end_frame();
                    continue;
                }
                let length = target.len().min(self.remaining).min(STEP - *work);
                match source.read(&mut target[..length]) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                    Ok(length) => {
                        self.remaining -= length;
                        *work += length;
                        if self.remaining == 0 {
                            self.end_frame();
                        }
                        return Ok(Progress::Ready(length));
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
            if self.needs_footer() {
                match self.finish_footer(source, work)? {
                    Progress::Ready(()) => continue,
                    Progress::Pending => return Ok(Progress::Pending),
                    Progress::Yield => return Ok(Progress::Yield),
                }
            }
            match self.fill(source, work)? {
                Progress::Ready(()) => (),
                Progress::Pending => return Ok(Progress::Pending),
                Progress::Yield => return Ok(Progress::Yield),
            }
            let length = target
                .len()
                .min(self.expected - self.expanded)
                .min(STEP - *work);
            if length == 0 {
                return Ok(Progress::Yield);
            }
            let decoded = self
                .decoder
                .decode(&self.input[self.position..], &mut target[..length])?;
            self.decoded(&decoded, work)?;
            if decoded.produced != 0 {
                return Ok(Progress::Ready(decoded.produced));
            }
            if decoded.consumed == 0 && !decoded.finished {
                return Err(WireError::Malformed(
                    "compressed payload cannot make progress",
                ));
            }
        }
    }
}

fn size(bytes: &[u8]) -> usize {
    usize::from(bytes[0]) | usize::from(bytes[1]) << 8 | usize::from(bytes[2]) << 16
}
