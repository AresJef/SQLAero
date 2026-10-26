//! Checked text results with owned flat storage and allocation-bound quotas.

use crate::memory::{Buffer, MemoryError, Pool, Reservation, Scope};
use crate::protocol::framing::{MAX_FRAME_PAYLOAD, WireError};
use crate::protocol::packets::{self, OkKind};
use mysql_common::constants::{CapabilityFlags, StatusFlags};
use mysql_common::packets::{OkPacket, ServerError, SqlState};
use std::mem::size_of;
use std::ops::Deref;
use std::sync::Arc;

/// NULL or an absolute range in an owned native buffer.
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    start: usize,
    end: usize,
}
impl Cell {
    /// Borrow a checked field; protocol NULL remains distinct from empty bytes.
    ///
    /// # Errors
    /// Returns an invalid ownership/range failure, never an unchecked pointer.
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

/// One actual protocol column, using spans of its metadata owner's bytes.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    labels: [Cell; 6],
    /// Raw MySQL type byte; unknown future types remain observable.
    pub type_code: u8,
    /// Actual protocol collation ID, not a guessed client codec.
    pub collation_id: u16,
    /// Server-declared display byte length, not a measured value length.
    pub column_length: u32,
    /// Raw MySQL column flags.
    pub flags: u16,
    /// Raw protocol decimals/fractional-second marker.
    pub decimals: u8,
}

/// The six protocol labels, in their actual wire order.
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Label {
    /// Protocol catalog, normally def.
    Catalog,
    /// Source schema/database.
    Database,
    /// Query-visible table name or alias.
    Table,
    /// Original physical table name.
    OriginalTable,
    /// Query-visible column name or AS label.
    Name,
    /// Original source column name.
    OriginalName,
}

impl Column {
    /// Actual known protocol type name, without guessing the original DDL.
    #[must_use]
    pub fn type_name(&self) -> Option<&'static str> {
        Some(match self.type_code {
            0 => "DECIMAL",
            1 => "TINY",
            2 => "SHORT",
            3 => "LONG",
            4 => "FLOAT",
            5 => "DOUBLE",
            6 => "NULL",
            7 => "TIMESTAMP",
            8 => "LONGLONG",
            9 => "INT24",
            10 => "DATE",
            11 => "TIME",
            12 => "DATETIME",
            13 => "YEAR",
            14 => "NEWDATE",
            15 => "VARCHAR",
            16 => "BIT",
            17 => "TIMESTAMP2",
            18 => "DATETIME2",
            19 => "TIME2",
            20 => "TYPED_ARRAY",
            242 => "VECTOR",
            245 => "JSON",
            246 => "NEWDECIMAL",
            247 => "ENUM",
            248 => "SET",
            249 => "TINY_BLOB",
            250 => "MEDIUM_BLOB",
            251 => "LONG_BLOB",
            252 => "BLOB",
            253 => "VAR_STRING",
            254 => "STRING",
            255 => "GEOMETRY",
            _ => return None,
        })
    }
    /// Whether the protocol type has numeric signedness/display semantics.
    #[must_use]
    pub fn numeric(&self) -> bool {
        matches!(self.type_code, 0..=5 | 8 | 9 | 13 | 246)
    }
    /// Applicable unsigned flag; the raw flags remain separately observable.
    #[must_use]
    pub fn is_unsigned(&self) -> bool {
        self.numeric() && self.flags & 32 != 0
    }
    /// Applicable legacy zero-fill flag.
    #[must_use]
    pub fn is_zerofill(&self) -> bool {
        self.numeric() && self.flags & 64 != 0
    }
    /// Actual ENUM type/flag, without assuming one-character values.
    #[must_use]
    pub fn is_enum(&self) -> bool {
        self.type_code == 247 || self.flags & 256 != 0
    }
    /// Actual SET type/flag, without inferring its schema members.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.type_code == 248 || self.flags & 2048 != 0
    }
    /// Binary string family under binary encoding, excluding ENUM/SET and the
    /// unrelated BINARY_FLAG on numeric or textual binary-collation columns.
    #[must_use]
    pub fn is_binary_string(&self) -> bool {
        matches!(self.type_code, 15 | 249..=254)
            && self.collation_id == 63
            && !self.is_enum()
            && !self.is_set()
    }
    /// Reliable numeric scale; special floating markers are not decimal digits.
    #[must_use]
    pub fn scale(&self) -> Option<u8> {
        match self.type_code {
            1..=3 | 8 | 9 | 13 => Some(0),
            0 | 4 | 5 | 246 if self.decimals <= 30 => Some(self.decimals),
            _ => None,
        }
    }
    /// Decimal precision after subtracting actual sign and decimal-point slots.
    /// Integer display widths and FLOAT/DOUBLE do not establish precision.
    #[must_use]
    pub fn precision(&self) -> Option<u32> {
        if !matches!(self.type_code, 0 | 246) {
            return None;
        }
        let scale = u32::from(self.scale()?);
        let precision = self
            .column_length
            .checked_sub(u32::from(!self.is_unsigned()) + u32::from(scale != 0))?;
        (precision > 0 && precision <= 65 && scale <= precision).then_some(precision)
    }
    /// Declared fractional-second precision, independent of current value text.
    #[must_use]
    pub fn fractional_seconds_precision(&self) -> Option<u8> {
        (matches!(self.type_code, 7 | 11 | 12 | 17..=19) && self.decimals <= 6)
            .then_some(self.decimals)
    }

    fn parse(data: &[u8], base: usize) -> Result<Self, WireError> {
        let mut position = 0;
        let mut labels = [Cell { start: 0, end: 0 }; 6];
        for label in &mut labels {
            let size = length(data, &mut position)?
                .ok_or(WireError::Malformed("NULL column metadata label"))?;
            let end = position
                .checked_add(size)
                .filter(|end| *end <= data.len())
                .ok_or(WireError::Malformed("truncated column metadata label"))?;
            *label = Cell {
                start: base
                    .checked_add(position)
                    .ok_or(WireError::Malformed("column offset overflow"))?,
                end: base
                    .checked_add(end)
                    .ok_or(WireError::Malformed("column offset overflow"))?,
            };
            position = end;
        }
        let catalog = labels[0].start - base..labels[0].end - base;
        // character_set_results also converts metadata labels, including the
        // constant catalog. Every admitted legacy charset preserves ASCII;
        // server-only UTF-16/UCS-2/UTF-32 use these exact forms. This validates
        // the catalog without guessing the encoding of any other label.
        if !matches!(
            data.get(catalog),
            Some(b"def" | b"\0d\0e\0f" | b"d\0e\0f\0" | b"\0\0\0d\0\0\0e\0\0\0f")
        ) {
            return Err(WireError::Malformed("invalid column catalog"));
        }
        if length(data, &mut position)? != Some(12) || data.len().saturating_sub(position) != 12 {
            return Err(WireError::Malformed("invalid fixed column metadata length"));
        }
        let fixed = &data[position..];
        Ok(Self {
            labels,
            collation_id: u16::from_le_bytes([fixed[0], fixed[1]]),
            column_length: u32::from_le_bytes([fixed[2], fixed[3], fixed[4], fixed[5]]),
            type_code: fixed[6],
            flags: u16::from_le_bytes([fixed[7], fixed[8]]),
            decimals: fixed[9],
        })
    }
}

/// Immutable column metadata independent of all row bytes and Cursor state.
pub struct Columns {
    items: Buffer<Column>,
    bytes: Buffer<u8>,
    _owner: Reservation,
}
impl Columns {
    /// Keep necessary lazy native metadata work under its original quota.
    #[must_use]
    pub fn scope(&self) -> &Arc<Scope> {
        self._owner.scope()
    }
    /// Borrow one protocol label without allocating or decoding Python text.
    #[must_use]
    pub fn label(&self, index: usize, label: Label) -> Option<&[u8]> {
        self.items.get(index)?.labels[label as usize]
            .get(&self.bytes)
            .ok()
            .flatten()
    }
}
impl Deref for Columns {
    type Target = [Column];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

/// Owned confirmation metadata, charged until it leaves result ownership.
/// Public packet parsing remains delegated to mysql_common.
pub struct Completion {
    packet: OkPacket<'static>,
    _owner: Reservation,
}
impl Completion {
    /// Keep the exact native packet and cover its owned diagnostic/state bytes.
    ///
    /// # Errors
    /// Returns the actual result or connection capacity failure.
    pub fn new(packet: OkPacket<'_>, scope: &Arc<Scope>) -> Result<Self, MemoryError> {
        let payload = packet
            .info_ref()
            .map_or(0, <[u8]>::len)
            .checked_add(packet.session_state_info_ref().map_or(0, <[u8]>::len))
            .and_then(|size| size.checked_add(size_of::<Self>()))
            .ok_or(MemoryError::Result {
                required: usize::MAX,
                limit: usize::MAX,
            })?;
        let owner = scope.charge(payload)?;
        Ok(Self {
            packet: packet.into_owned(),
            _owner: owner,
        })
    }
    /// Transfer confirmation from result ownership into the session-state
    /// parser. It no longer represents a Cursor-owned native row result.
    #[must_use]
    pub fn into_packet(self) -> OkPacket<'static> {
        self.packet
    }
}
impl Deref for Completion {
    type Target = OkPacket<'static>;
    fn deref(&self) -> &Self::Target {
        &self.packet
    }
}

/// Native buffered rows. Keeping a Columns snapshot never keeps these fields
/// or bytes alive. Python materialization is intentionally outside this type.
pub struct Buffered {
    /// Immutable metadata with its own shared lifetime and capacity charge.
    pub columns: Arc<Columns>,
    /// Owned row payload storage; capacity remains charged after fetch.
    pub bytes: Buffer<u8>,
    /// Row-major field locations; no generic value tree.
    pub fields: Buffer<Cell>,
    /// Actual complete row count.
    pub row_count: usize,
    /// Final received OK/EOF facts.
    pub completion: Completion,
    _owner: Reservation,
}
impl Buffered {
    /// Share only the allocation scope while preparing this result's native
    /// output strategies. It does not expose or lock the connection transport.
    #[must_use]
    pub fn scope(&self) -> &Arc<Scope> {
        self._owner.scope()
    }
    /// Release fully consumed row allocations at their natural owner boundary.
    /// Call only after the binding has atomically committed its final fetch;
    /// metadata, total count, confirmation and delivered Python values remain.
    pub fn release_rows(&mut self) {
        self.bytes = Buffer::new();
        self.fields = Buffer::new();
    }

    /// Borrow one row's locations without allocation or connection locking.
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

/// A fully consumed response. A counted intermediate result deliberately
/// owns no complete row buffer or externally publishable metadata.
pub enum Response {
    /// A non-row statement's actual OK.
    Command(Completion),
    /// The final, buffered row result.
    Rows(Buffered),
    /// An intermediate row result consumed with bounded storage.
    Counted {
        /// Actual returned row count.
        rows: usize,
        /// Actual final response metadata.
        completion: Completion,
    },
}
impl Response {
    /// Actual final OK/EOF metadata shared by counters and state updates.
    #[must_use]
    pub fn completion(&self) -> &OkPacket<'static> {
        match self {
            Self::Command(ok) => ok,
            Self::Rows(rows) => &rows.completion,
            Self::Counted { completion, .. } => completion,
        }
    }
    /// Move real completion facts into session-state ownership.
    #[must_use]
    pub fn into_completion(self) -> OkPacket<'static> {
        self.into_owned_completion().into_packet()
    }

    /// Keep completion capacity owned across a bounded control yield between
    /// unexpected result sets, without retaining their discarded row data.
    #[must_use]
    pub fn into_owned_completion(self) -> Completion {
        match self {
            Self::Command(ok) => ok,
            Self::Rows(rows) => rows.completion,
            Self::Counted { completion, .. } => completion,
        }
    }
}

/// A complete ERR whose raw diagnostic allocation shares the result quota.
/// Parsing remains delegated to mysql_common; only ownership is adapted here.
pub struct ServerFailure {
    code: u16,
    state: Option<SqlState>,
    message: Buffer<u8>,
    _owner: Reservation,
}
impl ServerFailure {
    fn new(packet: &ServerError<'_>, scope: &Arc<Scope>) -> Result<Self, MemoryError> {
        // The incoming packet remains charged while this copy is allocated.
        // Buffer charges real capacity and any growth peak, not payload length.
        let owner = scope.charge(size_of::<Self>())?;
        let mut message = Buffer::tracked(scope);
        message.extend_from_slice(packet.message_ref())?;
        Ok(Self {
            code: packet.error_code(),
            state: packet.sql_state_ref().copied(),
            message,
            _owner: owner,
        })
    }
    /// Actual unsigned 16-bit server number, without client inference.
    #[must_use]
    pub fn error_code(&self) -> u16 {
        self.code
    }
    /// Actual optional protocol SQLSTATE.
    #[must_use]
    pub fn sql_state_ref(&self) -> Option<&SqlState> {
        self.state.as_ref()
    }
    /// Received bytes, including NUL or invalid diagnostic encoding.
    #[must_use]
    pub fn message_ref(&self) -> &[u8] {
        &self.message
    }
}
impl std::fmt::Debug for ServerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerFailure")
            .field("code", &self.code)
            .field("state", &self.state)
            .field("message", &self.message)
            .finish()
    }
}
impl std::fmt::Display for ServerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "server error {}: {}",
            self.code,
            String::from_utf8_lossy(&self.message)
        )
    }
}
impl std::error::Error for ServerFailure {}

/// Native collection errors with their actual origin preserved.
#[derive(Debug, thiserror::Error)]
pub enum ResultError {
    /// Malformed wire data or framing storage failure.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// A complete server ERR packet.
    #[error("MySQL command failed: {0}")]
    Server(ServerFailure),
    /// A response outside the negotiated/public scope.
    #[error("unsupported MySQL response: {0}")]
    Unsupported(&'static str),
    /// Actual capacity, transient allocation or aggregate quota failure.
    #[error(transparent)]
    Memory(#[from] MemoryError),
}

#[derive(Clone, Copy)]
enum Phase {
    First,
    Columns(usize),
    MetadataEnd,
    Rows,
    Finished,
}

/// Collector for one result under a shared allocation scope. Intermediate
/// results use the same parser while retaining only counts and completion.
pub struct Receiver {
    capabilities: CapabilityFlags,
    phase: Phase,
    columns: Buffer<Column>,
    metadata: Buffer<u8>,
    bytes: Buffer<u8>,
    fields: Buffer<Cell>,
    column_count: usize,
    row_count: usize,
    discard: bool,
    scope: Arc<Scope>,
    _owner: Reservation,
}
impl Receiver {
    /// Start with owned capacity bookkeeping, without trusting remote counts.
    ///
    /// # Errors
    /// Returns a fixed native-owner quota failure.
    pub fn new(
        capabilities: CapabilityFlags,
        scope: Arc<Scope>,
        discard: bool,
    ) -> Result<Self, ResultError> {
        let owner = scope.charge(size_of::<Self>())?;
        Ok(Self {
            capabilities,
            phase: Phase::First,
            columns: Buffer::tracked(&scope),
            metadata: Buffer::tracked(&scope),
            bytes: Buffer::tracked(&scope),
            fields: Buffer::tracked(&scope),
            column_count: 0,
            row_count: 0,
            discard,
            scope,
            _owner: owner,
        })
    }
    /// Convenience scope for independent native callers. Connection execution
    /// passes its shared pool to Query instead of creating an isolated pool.
    ///
    /// # Errors
    /// Returns actual fixed owner/metadata capacity failures.
    pub fn isolated(capabilities: CapabilityFlags, limit: usize) -> Result<Self, ResultError> {
        Self::new(
            capabilities,
            Scope::new(Pool::new(usize::MAX), limit)?,
            false,
        )
    }
    /// Consume a complete logical message under the original command deadline.
    ///
    /// # Errors
    /// Returns real server, unsupported, malformed or capacity failures. An
    /// incomplete or rejected result is never exposed as successful rows.
    pub fn push(&mut self, data: &[u8]) -> Result<Option<Response>, ResultError> {
        if matches!(self.phase, Phase::Finished) {
            return Err(WireError::Malformed("result collector already consumed").into());
        }
        let result = self.push_inner(data);
        if result.is_err() {
            self.phase = Phase::Finished;
        }
        result
    }
    fn push_inner(&mut self, data: &[u8]) -> Result<Option<Response>, ResultError> {
        if data.first() == Some(&0xff) {
            return Err(ResultError::Server(ServerFailure::new(
                &packets::server_error(data, self.capabilities)?,
                &self.scope,
            )?));
        }
        match self.phase {
            Phase::First => {
                if data.first() == Some(&0) {
                    let completion = Completion::new(
                        packets::ok(data, self.capabilities, OkKind::Command)?,
                        &self.scope,
                    )?;
                    self.phase = Phase::Finished;
                    return Ok(Some(Response::Command(completion)));
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
                self.column_count = count;
                self.phase = Phase::Columns(count);
            }
            Phase::Columns(remaining) => {
                let base = if self.discard { 0 } else { self.metadata.len() };
                let column = Column::parse(data, base)?;
                if !self.discard {
                    self.metadata.extend_from_slice(data)?;
                    self.columns.push(column)?;
                }
                self.phase = if remaining == 1 {
                    if self
                        .capabilities
                        .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                    {
                        Phase::Rows
                    } else {
                        Phase::MetadataEnd
                    }
                } else {
                    Phase::Columns(remaining - 1)
                };
            }
            Phase::MetadataEnd => {
                packets::ok(data, self.capabilities, OkKind::LegacyEnd)?;
                self.phase = Phase::Rows;
            }
            Phase::Rows => {
                // Canonical 0xFE field lengths need at least 2^24 data bytes.
                // Only a shorter negotiated end packet can terminate rows.
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
                    let completion =
                        Completion::new(packets::ok(data, self.capabilities, kind)?, &self.scope)?;
                    let response = if self.discard {
                        Response::Counted {
                            rows: self.row_count,
                            completion,
                        }
                    } else {
                        let metadata_owner = self
                            .scope
                            .charge(size_of::<Columns>() + 2 * size_of::<usize>())?;
                        let row_owner = self
                            .scope
                            .charge(size_of::<Buffered>() - size_of::<Completion>())?;
                        let columns = Arc::new(Columns {
                            items: std::mem::take(&mut self.columns),
                            bytes: std::mem::take(&mut self.metadata),
                            _owner: metadata_owner,
                        });
                        Response::Rows(Buffered {
                            columns,
                            bytes: std::mem::take(&mut self.bytes),
                            fields: std::mem::take(&mut self.fields),
                            row_count: self.row_count,
                            completion,
                            _owner: row_owner,
                        })
                    };
                    self.phase = Phase::Finished;
                    return Ok(Some(response));
                }
                if !self.discard {
                    self.fields.reserve(self.column_count)?;
                }
                let base = self.bytes.len();
                let mut position = 0;
                for _ in 0..self.column_count {
                    let cell = match length(data, &mut position)? {
                        None => Cell {
                            start: usize::MAX,
                            end: 0,
                        },
                        Some(size) => {
                            let end = position
                                .checked_add(size)
                                .filter(|end| *end <= data.len())
                                .ok_or(WireError::Malformed("truncated text field"))?;
                            let cell = Cell {
                                start: base
                                    .checked_add(position)
                                    .ok_or(WireError::Malformed("row offset overflow"))?,
                                end: base
                                    .checked_add(end)
                                    .ok_or(WireError::Malformed("row offset overflow"))?,
                            };
                            position = end;
                            cell
                        }
                    };
                    if !self.discard {
                        self.fields.push(cell)?;
                    }
                }
                if position != data.len() {
                    return Err(WireError::Malformed("text row has trailing fields").into());
                }
                if !self.discard {
                    self.bytes.extend_from_slice(data)?;
                }
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
}

/// Checked length-encoded integer/NULL with no per-field allocation.
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

/// Whether the actual final status declares another result.
#[must_use]
pub fn has_more(response: &Response) -> bool {
    response
        .completion()
        .status_flags()
        .contains(StatusFlags::SERVER_MORE_RESULTS_EXISTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_error_copy_charges_the_live_packet_peak_and_releases_ownership() {
        let caps = CapabilityFlags::CLIENT_PROTOCOL_41;
        let pool = Pool::new(100_000);
        let scope = Scope::new(Arc::clone(&pool), 10_000).unwrap();
        let mut input = Buffer::tracked(&scope);
        input.extend_from_slice(b"\xff\x26\x04#23000").unwrap();
        input.extend_from_slice(&[b'x'; 8000]).unwrap();
        let mut collector = Receiver::new(caps, Arc::clone(&scope), false).unwrap();
        assert!(matches!(
            collector.push(&input),
            Err(ResultError::Memory(MemoryError::Result { .. }))
        ));
        drop(collector);
        drop(input);
        drop(scope);
        assert_eq!(pool.used(), 0);

        let scope = Scope::new(Arc::clone(&pool), 30_000).unwrap();
        let mut input = Buffer::tracked(&scope);
        input.extend_from_slice(b"\xff\x26\x04#23000a\0b").unwrap();
        let mut collector = Receiver::new(caps, Arc::clone(&scope), false).unwrap();
        let Err(ResultError::Server(error)) = collector.push(&input) else {
            panic!("expected a complete ERR")
        };
        assert_eq!(error.error_code(), 1062);
        assert_eq!(error.sql_state_ref().unwrap().as_bytes(), *b"23000");
        assert_eq!(error.message_ref(), b"a\0b");
        drop(input);
        drop(collector);
        drop(scope);
        assert_ne!(pool.used(), 0);
        drop(error);
        assert_eq!(pool.used(), 0);
    }
    fn packet(type_code: u8) -> Vec<u8> {
        let mut data = Vec::new();
        for value in [
            b"def".as_slice(),
            b"db",
            b"alias",
            b"original",
            "名字".as_bytes(),
            b"name",
        ] {
            data.push(value.len() as u8);
            data.extend_from_slice(value);
        }
        data.extend_from_slice(&[12, 45, 0, 255, 0, 0, 0, type_code, 1, 0, 31, 0, 0]);
        data
    }
    fn receiver(pool: &Arc<Pool>, discard: bool) -> Receiver {
        Receiver::new(
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            Scope::new(Arc::clone(pool), 1_000_000).unwrap(),
            discard,
        )
        .unwrap()
    }
    #[test]
    fn metadata_matches_public_packet_parser_and_has_separate_ownership() {
        let data = packet(253);
        let native = Column::parse(&data, 0).unwrap();
        let mut input = mysql_common::io::ParseBuf(&data);
        let reference: mysql_common::packets::Column = input.parse(()).unwrap();
        assert_eq!(native.type_code, reference.column_type() as u8);
        assert_eq!(native.collation_id, reference.character_set());
        assert_eq!(native.column_length, reference.column_length());
        assert_eq!(native.flags, reference.flags().bits());
        let pool = Pool::new(1_000_000);
        let mut collector = receiver(&pool, false);
        collector.push(&[1]).unwrap();
        collector.push(&data).unwrap();
        collector.push(b"\x03abc").unwrap();
        let Response::Rows(mut rows) = collector.push(&[0xfe, 0, 0, 2, 0, 0, 0]).unwrap().unwrap()
        else {
            panic!("expected rows")
        };
        assert_eq!(
            rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(),
            Some(b"abc".as_slice())
        );
        let columns = Arc::clone(&rows.columns);
        assert_eq!(columns.label(0, Label::Name), Some("名字".as_bytes()));
        drop(collector);
        let before = pool.used();
        rows.release_rows();
        assert!(pool.used() < before);
        assert_eq!(rows.row_count, 1);
        drop(rows);
        assert!(pool.used() < before);
        assert_eq!(columns.label(0, Label::Table), Some(b"alias".as_slice()));
        assert_ne!(pool.used(), 0);
        drop(columns);
        assert_eq!(pool.used(), 0);
    }
    #[test]
    fn intermediate_rows_are_validated_and_counted_without_full_storage() {
        let pool = Pool::new(1_000_000);
        let mut collector = receiver(&pool, true);
        collector.push(&[1]).unwrap();
        collector.push(&packet(253)).unwrap();
        let baseline = pool.used();
        for _ in 0..10_000 {
            collector.push(b"\x03abc").unwrap();
        }
        assert_eq!(pool.used(), baseline);
        let Response::Counted { rows, .. } =
            collector.push(&[0xfe, 0, 0, 2, 0, 0, 0]).unwrap().unwrap()
        else {
            panic!("expected counted rows")
        };
        assert_eq!(rows, 10_000);
        drop(collector);
        assert_eq!(pool.used(), 0);
    }
    #[test]
    fn unknown_type_metadata_is_preserved_but_bad_row_shape_is_terminal() {
        let pool = Pool::new(1_000_000);
        let mut collector = receiver(&pool, false);
        collector.push(&[1]).unwrap();
        collector.push(&packet(0xaa)).unwrap();
        collector.push(&[0xfb]).unwrap();
        let Response::Rows(rows) = collector.push(&[0xfe, 0, 0, 2, 0, 0, 0]).unwrap().unwrap()
        else {
            panic!("expected rows")
        };
        assert_eq!(rows.columns[0].type_code, 0xaa);
        assert_eq!(rows.row(0).unwrap()[0].get(&rows.bytes).unwrap(), None);
        let mut invalid = receiver(&pool, true);
        invalid.push(&[1]).unwrap();
        invalid.push(&packet(253)).unwrap();
        assert!(invalid.push(b"\x05abc").is_err());
        assert!(invalid.push(b"\x03abc").is_err());
    }
}
