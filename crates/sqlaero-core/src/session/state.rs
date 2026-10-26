//! Confirmed session facts updated only at complete protocol boundaries.

use super::initialize::Confirmed;
use crate::charset::{self, codec::Codec, output::OutputCodec};
use crate::protocol::framing::WireError;
use mysql_common::constants::{SessionStateType, StatusFlags};
use mysql_common::packets::{OkPacket, session_state_change::SessionStateChange};

/// One actual, independently known session role. None means unconfirmed;
/// a known NULL character_set_results is represented by Some(None).
#[derive(Debug)]
pub struct Facts {
    /// Latest applicable confirmed status; absence does not mean no transaction.
    pub status: Option<StatusFlags>,
    /// Confirmed schema bytes, known no schema, or unconfirmed.
    pub database: Option<Option<Vec<u8>>>,
    /// Actual time-zone token, never inferred from the client's local zone.
    pub time_zone: Option<String>,
    /// Actual SQL mode, including the meaningful empty mode string.
    pub sql_mode: Option<String>,
    /// Sending role; changing result conversion does not change this field.
    pub client_charset: Option<String>,
    /// Server interpretation role, independent of sending and results.
    pub connection_charset: Option<String>,
    /// Actual result conversion role; Some(None) means no conversion.
    pub results_charset: Option<Option<String>>,
    /// Actual connection collation, without aliasing server utf8 to utf8mb4.
    pub connection_collation: Option<String>,
    /// Actual server timeout values; None denotes invalidated knowledge.
    pub server_timeouts: [Option<u64>; 4],
    /// Server packet limit confirmed at connect, not a requested substitute.
    pub server_packet_limit: usize,
}
impl From<Confirmed> for Facts {
    fn from(value: Confirmed) -> Self {
        let charset = value.encoding.charset.name;
        Self {
            status: Some(value.status),
            database: Some(value.database.map(String::into_bytes)),
            time_zone: Some(value.time_zone),
            sql_mode: Some(value.sql_mode),
            client_charset: Some(charset.to_owned()),
            connection_charset: Some(charset.to_owned()),
            results_charset: Some(Some(charset.to_owned())),
            connection_collation: Some(value.encoding.collation.name.to_owned()),
            server_timeouts: value.server_timeouts.map(Some),
            server_packet_limit: value.server_packet_limit,
        }
    }
}
impl Facts {
    /// Apply only a fully received OK/terminator. Unmentioned facts retain their
    /// independently confirmed values; a general change hint invents no value.
    ///
    /// # Errors
    /// Malformed tracking payloads or non-ASCII system identities invalidate
    /// safe reuse at the caller, instead of performing lossy normalization.
    pub fn apply(&mut self, completion: &OkPacket<'_>) -> Result<(), WireError> {
        self.status = Some(completion.status_flags());
        let entries = completion
            .session_state_info()
            .map_err(|_| WireError::Malformed("invalid session tracking envelope"))?;
        for entry in entries {
            // Only these payloads supply facts used by this client. In
            // particular STATE_CHANGE is a raw ASCII hint in real MySQL
            // replies, not the length-encoded value assumed by the dependency's
            // generic decoder; the hint supplies no variable values to apply.
            if !matches!(
                entry.data_type(),
                SessionStateType::SESSION_TRACK_SYSTEM_VARIABLES
                    | SessionStateType::SESSION_TRACK_SCHEMA
            ) {
                continue;
            }
            match entry.decode().map_err(WireError::Decode)? {
                SessionStateChange::SystemVariables(values) => {
                    for value in values {
                        self.variable(value.name_bytes(), value.value_bytes())?;
                    }
                }
                SessionStateChange::Schema(schema) => {
                    self.database = Some(if schema.as_bytes().is_empty() {
                        None
                    } else {
                        Some(schema.as_bytes().to_vec())
                    });
                }
                _ => (),
            }
        }
        Ok(())
    }
    fn variable(&mut self, name: &[u8], value: &[u8]) -> Result<(), WireError> {
        let text = || {
            if !value.is_ascii() {
                return Err(WireError::Malformed("non-ASCII tracked system identity"));
            }
            std::str::from_utf8(value)
                .map(str::to_owned)
                .map_err(|_| WireError::Malformed("invalid tracked system identity"))
        };
        match name {
            b"time_zone" => self.time_zone = Some(text()?),
            b"sql_mode" => self.sql_mode = Some(text()?),
            b"character_set_client" => self.client_charset = Some(text()?),
            b"character_set_connection" => self.connection_charset = Some(text()?),
            b"character_set_results" => {
                self.results_charset =
                    Some(if value.eq_ignore_ascii_case(b"NULL") || value.is_empty() {
                        None
                    } else {
                        Some(text()?)
                    });
            }
            b"collation_connection" => self.connection_collation = Some(text()?),
            b"net_read_timeout"
            | b"net_write_timeout"
            | b"wait_timeout"
            | b"innodb_lock_wait_timeout" => {
                let index = match name {
                    b"net_read_timeout" => 0,
                    b"net_write_timeout" => 1,
                    b"wait_timeout" => 2,
                    _ => 3,
                };
                let number = std::str::from_utf8(value)
                    .ok()
                    .filter(|value| {
                        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or(WireError::Malformed("invalid tracked server timeout"))?;
                self.server_timeouts[index] = Some(number);
            }
            // The protocol status is the authoritative autocommit fact. A
            // textual variable value cannot override its actual status bits.
            b"autocommit" => (),
            _ => (),
        }
        Ok(())
    }
    /// Borrow the precise supported sending codec. Configuration aliases must
    /// not reinterpret an actual unsupported server charset such as utf8mb3.
    #[must_use]
    pub fn client_codec(&self) -> Option<Codec> {
        let name = self.client_charset.as_deref()?;
        let resolved = charset::resolve(Some(name), None).ok()?;
        (resolved.charset.name == name).then(|| Codec::new(resolved.charset))
    }
    /// Name metadata follows actual result conversion; without conversion the
    /// supported MySQL lines report system metadata using utf8mb3.
    #[must_use]
    pub fn metadata_codec(&self) -> Option<OutputCodec> {
        match self.results_charset.as_ref()? {
            None => Some(OutputCodec::Utf8mb3),
            Some(name) => charset::by_charset_name(name).and_then(|charset| {
                Some(OutputCodec::for_collation(
                    charset::by_collation_name(charset.default_collation)?.id,
                ))
            }),
        }
    }
    /// Exact UTC policy for automatic aware-datetime conversion. SYSTEM and
    /// zone names currently at offset zero are deliberately not accepted.
    #[must_use]
    pub fn utc_confirmed(&self) -> bool {
        self.time_zone.as_deref() == Some("+00:00")
    }
    /// Mark only transaction/autocommit knowledge unavailable after a complete
    /// ERR whose SQL effects cannot be inferred. It does not retire the wire.
    pub fn invalidate_status(&mut self) {
        self.status = None;
    }
    /// Invalidate dependencies after evidence of a partial settings operation
    /// or broken tracking. This does not rewrite their old values as defaults.
    pub fn invalidate_settings(&mut self) {
        self.time_zone = None;
        self.sql_mode = None;
        self.client_charset = None;
        self.connection_charset = None;
        self.results_charset = None;
        self.connection_collation = None;
        self.server_timeouts = [None; 4];
    }
}

impl Facts {
    /// Restore only the execution context after evidence of uncertain settings.
    /// The fixed six-field SELECT is decoded using its own actual metadata,
    /// including server-only result encodings, never the stale session cache.
    ///
    /// # Errors
    /// Rejects missing fields, unknown encodings, non-ASCII system identities
    /// or invalid ownership; no partial settings combination is published.
    pub fn confirm_context(
        &mut self,
        rows: &crate::result::text::Buffered,
    ) -> Result<(), WireError> {
        if rows.row_count != 1 || rows.columns.len() != 6 {
            return Err(WireError::Malformed(
                "execution context confirmation must contain one row of six fields",
            ));
        }
        let fields = rows
            .row(0)
            .ok_or(WireError::Malformed("missing context confirmation row"))?;
        let field = |index: usize| -> Result<Option<String>, WireError> {
            let Some(data) = fields[index].get(&rows.bytes)? else {
                return Ok(None);
            };
            let column = rows
                .columns
                .get(index)
                .ok_or(WireError::Malformed("missing context confirmation column"))?;
            let codec = OutputCodec::for_collation(column.collation_id);
            let mut value = String::new();
            value
                .try_reserve(data.len())
                .map_err(WireError::Allocation)?;
            let mut position = 0;
            while position < data.len() {
                let (point, end) = codec.point(data, position).map_err(|_| {
                    WireError::Malformed(
                        "context confirmation uses an unsupported or invalid text encoding",
                    )
                })?;
                if !point.is_ascii() {
                    return Err(WireError::Malformed("context system identity is not ASCII"));
                }
                value.push(point);
                position = end;
            }
            Ok(Some(value))
        };
        let client =
            field(0)?.ok_or(WireError::Malformed("client charset confirmation is NULL"))?;
        let connection = field(1)?.ok_or(WireError::Malformed(
            "connection charset confirmation is NULL",
        ))?;
        let results = field(2)?;
        let collation = field(3)?.ok_or(WireError::Malformed(
            "connection collation confirmation is NULL",
        ))?;
        let mode = field(4)?.ok_or(WireError::Malformed("SQL mode confirmation is NULL"))?;
        let zone = field(5)?.ok_or(WireError::Malformed("time zone confirmation is NULL"))?;
        self.client_charset = Some(client);
        self.connection_charset = Some(connection);
        self.results_charset = Some(results);
        self.connection_collation = Some(collation);
        self.sql_mode = Some(mode);
        self.time_zone = Some(zone);
        Ok(())
    }
    /// Whether a future user command lacks a fact required by its native
    /// input/metadata preparation. Known NULL results are not missing facts.
    #[must_use]
    pub fn needs_context(&self) -> bool {
        self.client_charset.is_none()
            || self.connection_charset.is_none()
            || self.results_charset.is_none()
            || self.connection_collation.is_none()
            || self.sql_mode.is_none()
            || self.time_zone.is_none()
    }
}
