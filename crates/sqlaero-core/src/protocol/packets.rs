//! Packet contexts and length validation around the selected public parser API.

use super::framing::WireError;
use mysql_common::constants::CapabilityFlags;
use mysql_common::io::ParseBuf;
use mysql_common::packets::{
    CommonOkPacket, ErrPacket, HandshakePacket, OkPacket, OkPacketDeserializer, OldEofPacket,
    ResultSetTerminator, ServerError,
};
use mysql_common::proto::MyDeserialize;

/// The negotiated context in which an OK/EOF marker is being read.
#[derive(Debug, Clone, Copy)]
pub enum OkKind {
    /// A command or authentication success beginning with 0x00.
    Command,
    /// A result terminator when CLIENT_DEPRECATE_EOF was negotiated.
    ResultEnd,
    /// A five-byte legacy EOF without CLIENT_DEPRECATE_EOF.
    LegacyEnd,
}

fn exact<'a, T: MyDeserialize<'a>>(data: &'a [u8], context: T::Ctx) -> Result<T, WireError> {
    let mut buffer = ParseBuf(data);
    let packet = buffer.parse(context).map_err(WireError::Decode)?;
    if !buffer.is_empty() {
        return Err(WireError::Malformed("unexpected trailing packet fields"));
    }
    Ok(packet)
}

/// Read a protocol-10 greeting without accepting truncated nonce material.
///
/// # Errors
/// Returns a malformed/decode failure for an unsupported protocol or invalid
/// greeting. A guard protects the upstream signed auth-length calculation from
/// hostile lengths before the parser is called.
pub fn handshake(data: &[u8]) -> Result<HandshakePacket<'_>, WireError> {
    if data.first() != Some(&10) {
        return Err(WireError::Malformed("expected protocol-10 greeting"));
    }
    let fields = data[1..]
        .iter()
        .position(|byte| *byte == 0)
        .and_then(|position| position.checked_add(2))
        .ok_or(WireError::Malformed("unterminated server version"))?;
    let auth_length = fields
        .checked_add(20)
        .and_then(|index| data.get(index))
        .copied()
        .ok_or(WireError::Malformed("truncated greeting fields"))?;
    if auth_length > 127 {
        return Err(WireError::Malformed("invalid authentication data length"));
    }
    let packet: HandshakePacket<'_> = exact(data, ())?;
    let second = packet
        .scramble_2_ref()
        .ok_or(WireError::Malformed("missing authentication nonce"))?;
    let second = second.strip_suffix(&[0]).unwrap_or(second);
    if packet.scramble_1_ref().len() + second.len() != 20 {
        return Err(WireError::Malformed(
            "authentication nonce must contain 20 bytes",
        ));
    }
    Ok(packet)
}

/// Parse an OK or EOF using its actual negotiated context.
///
/// # Errors
/// Returns a decode/malformed failure for incorrect markers or field lengths.
pub fn ok(
    data: &[u8],
    capabilities: CapabilityFlags,
    kind: OkKind,
) -> Result<OkPacket<'_>, WireError> {
    match kind {
        OkKind::Command => {
            exact::<OkPacketDeserializer<CommonOkPacket>>(data, capabilities).map(Into::into)
        }
        OkKind::ResultEnd => {
            exact::<OkPacketDeserializer<ResultSetTerminator>>(data, capabilities).map(Into::into)
        }
        OkKind::LegacyEnd => {
            exact::<OkPacketDeserializer<OldEofPacket>>(data, capabilities).map(Into::into)
        }
    }
}

/// Parse a MySQL ERR while preserving the actual code, SQLSTATE and raw text.
///
/// # Errors
/// Returns a decode/malformed failure for invalid fields or a MariaDB-only
/// progress response, which is outside the selected MySQL protocol.
pub fn server_error(
    data: &[u8],
    capabilities: CapabilityFlags,
) -> Result<ServerError<'_>, WireError> {
    match exact::<ErrPacket<'_>>(data, capabilities)? {
        ErrPacket::Error(error) => Ok(error),
        ErrPacket::Progress(_) => Err(WireError::Malformed("unexpected progress-report response")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mysql_common::constants::StatusFlags;

    #[test]
    fn command_ok_and_both_eof_forms_use_distinct_contexts() {
        let caps = CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_SESSION_TRACK;
        let packet = ok(&[0, 1, 2, 2, 0, 3, 0, 0], caps, OkKind::Command).unwrap();
        assert_eq!(packet.affected_rows(), 1);
        assert_eq!(packet.last_insert_id(), Some(2));
        assert!(
            packet
                .status_flags()
                .contains(StatusFlags::SERVER_STATUS_AUTOCOMMIT)
        );
        assert_eq!(packet.warnings(), 3);
        assert_eq!(
            ok(&[0xfe, 3, 0, 2, 0], caps, OkKind::LegacyEnd)
                .unwrap()
                .warnings(),
            3
        );
        assert_eq!(
            ok(&[0xfe, 0, 0, 2, 0, 3, 0, 0], caps, OkKind::ResultEnd)
                .unwrap()
                .warnings(),
            3
        );
        assert!(ok(&[0, 1], caps, OkKind::Command).is_err());
    }

    #[test]
    fn server_error_keeps_real_number_and_rejects_truncation() {
        let caps = CapabilityFlags::CLIENT_PROTOCOL_41;
        let error = server_error(b"\xff\x26\x04#23000Duplicate entry", caps).unwrap();
        assert_eq!(error.error_code(), 1062);
        assert_eq!(error.message_ref(), b"Duplicate entry");
        assert!(server_error(b"\xff\x26", caps).is_err());
        assert!(server_error(b"\xff\x26\x04#230", caps).is_err());
    }

    #[test]
    fn hostile_greeting_length_is_rejected_without_upstream_signed_overflow() {
        let mut greeting = b"\x0a8.4.13\0".to_vec();
        greeting.extend_from_slice(&[0; 31]);
        greeting[8 + 20] = 128;
        assert!(handshake(&greeting).is_err());
        assert!(handshake(b"\x0aunterminated").is_err());
    }
}
