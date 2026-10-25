//! Checked text-protocol response collection with flat native field locations.

use crate::protocol::framing::{MAX_FRAME_PAYLOAD, WireError};
use crate::protocol::packets::{self, OkKind};
use mysql_common::constants::{CapabilityFlags, StatusFlags};
use mysql_common::io::ParseBuf;
use mysql_common::packets::{Column, OkPacket, ServerError};
use std::mem::size_of;

/// NULL or an absolute byte range in an owned result. usize::MAX cannot be a
/// valid Vec offset; its use avoids a third machine word per nullable field.
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    start: usize,
    end: usize,
}

impl Cell {
    /// Obtain a borrowed field, or None for protocol NULL.
    pub fn get(self, storage: &[u8]) -> Result<Option<&[u8]>, WireError> {
        if self.start == usize::MAX {
            Ok(None)
        } else {
            storage
                .get(self.start..self.end)
                .map(Some)
                .ok_or(WireError::Malformed(
                    "field range does not belong to this result storage",
                ))
        }
    }
}

/// Buffered bytes and field locations, with no Python values or generic Value
/// tree. Column metadata is independent of the row storage.
pub struct Buffered {
    /// Raw metadata parsed once per column.
    pub columns: Vec<Column>,
    /// Contiguous native row payload storage.
    pub bytes: Vec<u8>,
    /// Row-major nullable field locations.
    pub fields: Vec<Cell>,
    /// Number of complete rows, including rows containing only NULLs.
    pub row_count: usize,
    /// Actual final response status, warnings and session-state information.
    pub completion: OkPacket<'static>,
}

impl Buffered {
    /// Return a row's native fields without decoding or allocating.
    #[must_use]
    pub fn row(&self, index: usize) -> Option<&[Cell]> {
        if index >= self.row_count {
            return None;
        }
        let start = index.checked_mul(self.columns.len())?;
        self.fields
            .get(start..start.checked_add(self.columns.len())?)
    }
}

/// One fully consumed command response.
pub enum Response {
    /// A non-row OK response.
    Command(OkPacket<'static>),
    /// A buffered row result, including an empty row result with real columns.
    Rows(Buffered),
}

impl Response {
    /// Return actual completion metadata, never a guessed success state.
    #[must_use]
    pub fn completion(&self) -> &OkPacket<'static> {
        match self {
            Self::Command(ok) => ok,
            Self::Rows(rows) => &rows.completion,
        }
    }

    /// Retain confirmation metadata while discarding an unsupported row result.
    pub fn into_completion(self) -> OkPacket<'static> {
        match self {
            Self::Command(ok) => ok,
            Self::Rows(rows) => rows.completion,
        }
    }
}

/// A complete server failure, unsupported response or invalid resource shape.
#[derive(Debug, thiserror::Error)]
pub enum ResultError {
    /// Protocol fields were malformed or native reservation failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// The server actually returned ERR; its number and message remain intact.
    #[error("MySQL command failed: {0}")]
    Server(ServerError<'static>),
    /// A response requires a capability outside the declared client scope.
    #[error("unsupported MySQL response: {0}")]
    Unsupported(&'static str),
    /// Native result storage exceeded its configured bound.
    #[error(
        "native result requires {required} bytes, exceeding the {limit}-byte result memory limit"
    )]
    Memory {
        /// Observed/requested controlled storage.
        required: usize,
        /// Configured native result limit.
        limit: usize,
    },
}

enum Phase {
    First,
    Columns(usize),
    MetadataEnd,
    Rows,
    Finished,
}

/// A single-result collector. The command driver handles any unexpected next
/// result using the same collector and original deadline before reporting it.
pub struct Receiver {
    capabilities: CapabilityFlags,
    phase: Phase,
    columns: Vec<Column>,
    bytes: Vec<u8>,
    fields: Vec<Cell>,
    row_count: usize,
    metadata_bytes: usize,
    limit: usize,
}

impl Receiver {
    /// Start a result without reserving storage from a remote column count.
    #[must_use]
    pub fn new(capabilities: CapabilityFlags, memory_limit: usize) -> Self {
        Self {
            capabilities,
            phase: Phase::First,
            columns: Vec::new(),
            bytes: Vec::new(),
            fields: Vec::new(),
            row_count: 0,
            metadata_bytes: 0,
            limit: memory_limit,
        }
    }

    /// Consume a complete logical message. None means more protocol messages
    /// are required; Some is returned only after a complete OK/EOF boundary.
    ///
    /// # Errors
    /// Returns real ERR, malformed fields, unsupported LOCAL INFILE or storage
    /// failures. No incomplete result is exposed to the caller.
    pub fn push(&mut self, data: &[u8]) -> Result<Option<Response>, ResultError> {
        if data.first() == Some(&0xff) {
            return Err(ResultError::Server(
                packets::server_error(data, self.capabilities)?.into_owned(),
            ));
        }
        match self.phase {
            Phase::First => {
                if data.first() == Some(&0) {
                    self.phase = Phase::Finished;
                    return Ok(Some(Response::Command(
                        packets::ok(data, self.capabilities, OkKind::Command)?.into_owned(),
                    )));
                }
                if data.first() == Some(&0xfb) {
                    return Err(ResultError::Unsupported("LOCAL INFILE is disabled"));
                }
                let mut position = 0;
                let count = length(data, &mut position)?
                    .ok_or(WireError::Malformed("NULL column count"))?;
                if position != data.len() || count == 0 {
                    return Err(WireError::Malformed("invalid result column count").into());
                }
                self.phase = Phase::Columns(count);
            }
            Phase::Columns(remaining) => {
                self.check_growth(data.len(), 0, 1)?;
                let mut buffer = ParseBuf(data);
                let column: Column = buffer.parse(()).map_err(WireError::Decode)?;
                if !buffer.is_empty() {
                    return Err(WireError::Malformed("trailing column definition bytes").into());
                }
                self.columns
                    .try_reserve_exact(1)
                    .map_err(WireError::Allocation)?;
                self.metadata_bytes =
                    self.metadata_bytes
                        .checked_add(data.len())
                        .ok_or(ResultError::Memory {
                            required: usize::MAX,
                            limit: self.limit,
                        })?;
                self.columns.push(column);
                if remaining == 1 {
                    self.phase = if self
                        .capabilities
                        .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                    {
                        Phase::Rows
                    } else {
                        Phase::MetadataEnd
                    };
                } else {
                    self.phase = Phase::Columns(remaining - 1);
                }
            }
            Phase::MetadataEnd => {
                packets::ok(data, self.capabilities, OkKind::LegacyEnd)?;
                self.phase = Phase::Rows;
            }
            Phase::Rows => {
                // A canonical text row with an 0xFE length prefix requires at
                // least 2^24 data bytes. Deprecated-EOF OK packets are shorter.
                let end = data.first() == Some(&0xfe)
                    && if self
                        .capabilities
                        .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                    {
                        data.len() < MAX_FRAME_PAYLOAD
                    } else {
                        data.len() < 9
                    };
                if end {
                    let kind = if self
                        .capabilities
                        .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                    {
                        OkKind::ResultEnd
                    } else {
                        OkKind::LegacyEnd
                    };
                    let completion = packets::ok(data, self.capabilities, kind)?.into_owned();
                    self.phase = Phase::Finished;
                    return Ok(Some(Response::Rows(Buffered {
                        columns: std::mem::take(&mut self.columns),
                        bytes: std::mem::take(&mut self.bytes),
                        fields: std::mem::take(&mut self.fields),
                        row_count: self.row_count,
                        completion,
                    })));
                }
                self.check_growth(data.len(), self.columns.len(), 0)?;
                self.fields
                    .try_reserve_exact(self.columns.len())
                    .map_err(WireError::Allocation)?;
                let base = self.bytes.len();
                let mut position = 0;
                for _ in 0..self.columns.len() {
                    let cell = match length(data, &mut position)? {
                        None => Cell {
                            start: usize::MAX,
                            end: 0,
                        },
                        Some(length) => {
                            let end = position
                                .checked_add(length)
                                .filter(|end| *end <= data.len())
                                .ok_or(WireError::Malformed("truncated text field"))?;
                            let cell = Cell {
                                start: base + position,
                                end: base + end,
                            };
                            position = end;
                            cell
                        }
                    };
                    self.fields.push(cell);
                }
                if position != data.len() {
                    return Err(WireError::Malformed("text row has trailing fields").into());
                }
                self.bytes
                    .try_reserve_exact(data.len())
                    .map_err(WireError::Allocation)?;
                self.bytes.extend_from_slice(data);
                self.row_count = self
                    .row_count
                    .checked_add(1)
                    .ok_or(WireError::Malformed("row count overflow"))?;
            }
            Phase::Finished => {
                return Err(WireError::Malformed("result collector already consumed").into());
            }
        }
        Ok(None)
    }

    fn check_growth(&self, bytes: usize, fields: usize, columns: usize) -> Result<(), ResultError> {
        let required = self
            .bytes
            .len()
            .checked_add(self.metadata_bytes)
            .and_then(|size| size.checked_add(bytes))
            .and_then(|size| {
                (self.fields.len().checked_add(fields)?)
                    .checked_mul(size_of::<Cell>())?
                    .checked_add(size)
            })
            .and_then(|size| {
                (self.columns.len().checked_add(columns)?)
                    .checked_mul(size_of::<Column>())?
                    .checked_add(size)
            })
            .ok_or(ResultError::Memory {
                required: usize::MAX,
                limit: self.limit,
            })?;
        if required > self.limit {
            return Err(ResultError::Memory {
                required,
                limit: self.limit,
            });
        }
        Ok(())
    }
}

/// Read a checked length-encoded integer/NULL without constructing field values.
fn length(data: &[u8], position: &mut usize) -> Result<Option<usize>, WireError> {
    let first = *data
        .get(*position)
        .ok_or(WireError::Malformed("missing length-encoded field"))?;
    *position += 1;
    let count = match first {
        0..=250 => return Ok(Some(usize::from(first))),
        251 => return Ok(None),
        252 => 2,
        253 => 3,
        254 => 8,
        _ => return Err(WireError::Malformed("invalid length-encoded marker")),
    };
    let end = position
        .checked_add(count)
        .filter(|end| *end <= data.len())
        .ok_or(WireError::Malformed("truncated length-encoded integer"))?;
    let mut bytes = [0; 8];
    bytes[..count].copy_from_slice(&data[*position..end]);
    *position = end;
    usize::try_from(u64::from_le_bytes(bytes))
        .map(Some)
        .map_err(|_| WireError::Malformed("field length does not fit native storage"))
}

/// Return whether the server declared another result, without treating it as a
/// supported public nextset capability.
#[must_use]
pub fn has_more(response: &Response) -> bool {
    response
        .completion()
        .status_flags()
        .contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS)
}
