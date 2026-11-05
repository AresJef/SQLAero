//! One prepare acknowledgement and its required metadata blocks.

use crate::memory::Scope;
use crate::protocol::framing::WireError;
use crate::protocol::packets::{self, OkKind};
use crate::result::text::{Column, ResultError, ServerFailure};
use mysql_common::constants::CapabilityFlags;
use mysql_common::io::ParseBuf;
use mysql_common::packets::StmtPacket;
use std::sync::Arc;

/// A fully synchronized successful PREPARE; this does not mean SQL executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statement {
    /// Actual server-assigned connection-local statement handle.
    pub id: u32,
    /// Actual parameter position count declared by the server.
    pub parameters: u16,
    /// Prepare-time column count; execute-time metadata remains authoritative.
    pub columns: u16,
    /// Warning count of preparing this statement, independent of execution.
    pub warnings: u16,
}

enum Phase {
    First,
    Parameters(u16),
    ParameterEnd,
    Columns(u16),
    ColumnEnd,
    Finished,
}

/// Validate all prepare metadata without retaining a second result collection.
pub struct PrepareReceiver {
    capabilities: CapabilityFlags,
    statement: Option<Statement>,
    phase: Phase,
    server_err: bool,
    scope: Arc<Scope>,
}

impl PrepareReceiver {
    /// Start one prepare response using the command's existing capacity scope.
    #[must_use]
    pub fn new(capabilities: CapabilityFlags, scope: Arc<Scope>) -> Self {
        Self {
            capabilities,
            statement: None,
            phase: Phase::First,
            server_err: false,
            scope,
        }
    }

    /// A received provisional ID, including when later metadata is incomplete.
    /// Callers must retire or release it safely after a later failure.
    #[must_use]
    pub fn statement_id(&self) -> Option<u32> {
        self.statement.map(|statement| statement.id)
    }

    /// Whether a complete server error was actually received.
    #[must_use]
    pub const fn received_server_err(&self) -> bool {
        self.server_err
    }

    pub(crate) fn invalidate_terminal_facts(&mut self) {
        self.server_err = false;
        self.statement = None;
    }

    /// Consume one complete classic packet payload, with no guessed tail reads.
    ///
    /// # Errors
    /// Preserves the shared server diagnostic or actual wire/resource failure.
    pub fn push(&mut self, data: &[u8]) -> Result<Option<Statement>, ResultError> {
        if matches!(self.phase, Phase::Finished) {
            return Err(WireError::Malformed("prepare response already ended").into());
        }
        let result = self.push_inner(data);
        if result.is_err() {
            self.phase = Phase::Finished;
        }
        result
    }

    fn push_inner(&mut self, data: &[u8]) -> Result<Option<Statement>, ResultError> {
        if data.first() == Some(&0xff) {
            let packet = packets::server_error(data, self.capabilities)?;
            self.server_err = true;
            return Err(ResultError::Server(ServerFailure::new(
                &packet,
                &self.scope,
            )?));
        }
        let no_eof = self
            .capabilities
            .contains(CapabilityFlags::CLIENT_DEPRECATE_EOF);
        match self.phase {
            Phase::First => {
                if self
                    .capabilities
                    .contains(CapabilityFlags::CLIENT_OPTIONAL_RESULTSET_METADATA)
                {
                    return Err(ResultError::Unsupported(
                        "optional prepare metadata was not enabled",
                    ));
                }
                if data.len() != 12 || data[9] != 0 {
                    return Err(
                        WireError::Malformed("invalid prepare acknowledgement fields").into(),
                    );
                }
                let mut buffer = ParseBuf(data);
                let packet: StmtPacket = buffer.parse(()).map_err(WireError::Decode)?;
                self.statement = Some(Statement {
                    id: packet.statement_id(),
                    parameters: packet.num_params(),
                    columns: packet.num_columns(),
                    warnings: packet.warning_count(),
                });
                if packet.num_params() != 0 {
                    self.phase = Phase::Parameters(packet.num_params());
                    Ok(None)
                } else {
                    self.after_parameters()
                }
            }
            Phase::Parameters(left) => {
                Column::parse(data, 0)?;
                if left > 1 {
                    self.phase = Phase::Parameters(left - 1);
                    Ok(None)
                } else if no_eof {
                    self.after_parameters()
                } else {
                    self.phase = Phase::ParameterEnd;
                    Ok(None)
                }
            }
            Phase::ParameterEnd => {
                packets::ok(data, self.capabilities, OkKind::LegacyEnd)?;
                self.after_parameters()
            }
            Phase::Columns(left) => {
                Column::parse(data, 0)?;
                if left > 1 {
                    self.phase = Phase::Columns(left - 1);
                    Ok(None)
                } else if no_eof {
                    self.finish()
                } else {
                    self.phase = Phase::ColumnEnd;
                    Ok(None)
                }
            }
            Phase::ColumnEnd => {
                packets::ok(data, self.capabilities, OkKind::LegacyEnd)?;
                self.finish()
            }
            Phase::Finished => Err(WireError::Malformed("prepare response already ended").into()),
        }
    }

    fn after_parameters(&mut self) -> Result<Option<Statement>, ResultError> {
        let statement = self
            .statement
            .ok_or(WireError::Malformed("prepare metadata has no statement"))?;
        if statement.columns != 0 {
            self.phase = Phase::Columns(statement.columns);
            Ok(None)
        } else {
            self.finish()
        }
    }

    fn finish(&mut self) -> Result<Option<Statement>, ResultError> {
        self.phase = Phase::Finished;
        self.statement
            .map(Some)
            .ok_or_else(|| WireError::Malformed("completed prepare has no statement").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Pool;

    fn receiver(deprecate: bool) -> PrepareReceiver {
        PrepareReceiver::new(
            CapabilityFlags::CLIENT_PROTOCOL_41
                | if deprecate {
                    CapabilityFlags::CLIENT_DEPRECATE_EOF
                } else {
                    CapabilityFlags::empty()
                },
            Scope::new(Pool::new(1_000_000), 1_000_000).unwrap(),
        )
    }

    fn acknowledgement(parameters: u16, columns: u16) -> Vec<u8> {
        let mut bytes = vec![0, 17, 0, 0, 0];
        bytes.extend_from_slice(&columns.to_le_bytes());
        bytes.extend_from_slice(&parameters.to_le_bytes());
        bytes.extend_from_slice(&[0, 7, 0]);
        bytes
    }

    fn metadata() -> Vec<u8> {
        let mut packet = Vec::new();
        for name in [b"def".as_slice(), b"", b"", b"", b"?", b""] {
            packet.push(name.len() as u8);
            packet.extend_from_slice(name);
        }
        packet.extend_from_slice(&[12, 63, 0, 0, 0, 0, 0, 253, 128, 0, 0, 0, 0]);
        packet
    }

    #[test]
    fn prepare_waits_for_both_metadata_blocks_and_only_negotiated_eof() {
        for deprecate in [false, true] {
            let mut receiver = receiver(deprecate);
            assert!(receiver.push(&acknowledgement(1, 1)).unwrap().is_none());
            assert_eq!(receiver.statement_id(), Some(17));
            assert!(receiver.push(&metadata()).unwrap().is_none());
            if !deprecate {
                assert!(receiver.push(&[0xfe, 0, 0, 2, 0]).unwrap().is_none());
            }
            let mut result = receiver.push(&metadata()).unwrap();
            if !deprecate {
                assert!(result.is_none());
                result = receiver.push(&[0xfe, 0, 0, 2, 0]).unwrap();
            }
            assert_eq!(
                result,
                Some(Statement {
                    id: 17,
                    parameters: 1,
                    columns: 1,
                    warnings: 7
                })
            );
            assert!(receiver.push(&metadata()).is_err());
        }
    }

    #[test]
    fn zero_metadata_prepare_finishes_on_its_acknowledgement() {
        assert_eq!(
            receiver(true)
                .push(&acknowledgement(0, 0))
                .unwrap()
                .unwrap()
                .id,
            17
        );
    }

    #[test]
    fn malformed_metadata_and_server_errors_keep_real_phase_and_id() {
        let mut receiver = receiver(true);
        receiver.push(&acknowledgement(1, 0)).unwrap();
        assert!(receiver.push(b"bad column").is_err());
        assert_eq!(receiver.statement_id(), Some(17));
        let mut receiver = self::receiver(true);
        let Err(ResultError::Server(error)) = receiver.push(b"\xff\x28\x04#42000syntax error")
        else {
            panic!("expected server failure");
        };
        assert_eq!(error.error_code(), 1064);
        assert!(receiver.received_server_err());
        for bytes in [vec![], vec![0; 11], vec![0; 13], vec![0xfb]] {
            assert!(self::receiver(true).push(&bytes).is_err());
        }
    }
}
