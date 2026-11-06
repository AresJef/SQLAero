//! Frozen safe compression engines with bounded protocol-sized input/output.

use super::Algorithm;
use crate::protocol::framing::WireError;
use flate2::{Compress, Decompress, FlushCompress, FlushDecompress, Status};
use std::io;
use zstd::zstd_safe::zstd_sys::{ZSTD_ErrorCode, ZSTD_getErrorCode};
use zstd::zstd_safe::{CCtx, CParameter, DCtx, DParameter, InBuffer, OutBuffer, ResetDirective};

pub(super) enum Encoder {
    Zlib(Compress),
    Zstd(CCtx<'static>),
}

impl Encoder {
    pub(super) fn zstd_workspace_bytes(&self) -> Option<usize> {
        match self {
            Self::Zstd(context) => Some(context.sizeof()),
            Self::Zlib(_) => None,
        }
    }
    pub(super) fn new(algorithm: Algorithm, level: u8) -> Result<Self, WireError> {
        Ok(match algorithm {
            Algorithm::Zlib => Self::Zlib(Compress::new(flate2::Compression::default(), true)),
            Algorithm::Zstd => {
                let mut context = CCtx::try_create().ok_or_else(out_of_memory)?;
                context
                    .set_parameter(CParameter::CompressionLevel(i32::from(level)))
                    .map_err(zstd_error)?;
                Self::Zstd(context)
            }
        })
    }

    pub(super) fn bound(length: usize) -> usize {
        zstd::zstd_safe::compress_bound(length)
            .max(length + (length >> 12) + (length >> 14) + (length >> 25) + 13)
    }

    pub(super) fn encode(&mut self, source: &[u8], target: &mut [u8]) -> Result<usize, WireError> {
        match self {
            Self::Zlib(context) => {
                context.reset();
                let status = context
                    .compress(source, target, FlushCompress::Finish)
                    .map_err(|error| {
                        WireError::Decode(io::Error::new(io::ErrorKind::InvalidData, error))
                    })?;
                if status != Status::StreamEnd || context.total_in() != source.len() as u64 {
                    return Err(WireError::Malformed("incomplete zlib compression"));
                }
                Ok(context.total_out() as usize)
            }
            Self::Zstd(context) => {
                context
                    .reset(ResetDirective::SessionOnly)
                    .map_err(zstd_error)?;
                context.compress2(target, source).map_err(zstd_error)
            }
        }
    }
}

pub(super) enum Decoder {
    Zlib(Decompress),
    Zstd(DCtx<'static>),
}

pub(super) struct Decoded {
    pub(super) consumed: usize,
    pub(super) produced: usize,
    pub(super) finished: bool,
}

impl Decoder {
    pub(super) fn zstd_workspace_bytes(&self) -> Option<usize> {
        match self {
            Self::Zstd(context) => Some(context.sizeof()),
            Self::Zlib(_) => None,
        }
    }
    pub(super) fn new(algorithm: Algorithm) -> Result<Self, WireError> {
        Ok(match algorithm {
            Algorithm::Zlib => Self::Zlib(Decompress::new(true)),
            Algorithm::Zstd => {
                let mut context = DCtx::try_create().ok_or_else(out_of_memory)?;
                // A classic compressed payload expands to at most 2^24-1
                // bytes. Do not admit an attacker-selected larger history.
                context
                    .set_parameter(DParameter::WindowLogMax(24))
                    .map_err(zstd_error)?;
                Self::Zstd(context)
            }
        })
    }

    pub(super) fn reset(&mut self) -> Result<(), WireError> {
        match self {
            Self::Zlib(context) => {
                context.reset(true);
                Ok(())
            }
            Self::Zstd(context) => {
                context
                    .reset(ResetDirective::SessionOnly)
                    .map_err(zstd_error)?;
                Ok(())
            }
        }
    }

    pub(super) fn decode(
        &mut self,
        source: &[u8],
        target: &mut [u8],
    ) -> Result<Decoded, WireError> {
        match self {
            Self::Zlib(context) => {
                let input = context.total_in();
                let output = context.total_out();
                let status = context
                    .decompress(source, target, FlushDecompress::None)
                    .map_err(|error| {
                        WireError::Decode(io::Error::new(io::ErrorKind::InvalidData, error))
                    })?;
                Ok(Decoded {
                    consumed: (context.total_in() - input) as usize,
                    produced: (context.total_out() - output) as usize,
                    finished: status == Status::StreamEnd,
                })
            }
            Self::Zstd(context) => {
                let mut input = InBuffer {
                    src: source,
                    pos: 0,
                };
                let mut output = OutBuffer::around(target);
                let left = context
                    .decompress_stream(&mut output, &mut input)
                    .map_err(zstd_error)?;
                Ok(Decoded {
                    consumed: input.pos,
                    produced: output.pos(),
                    finished: left == 0,
                })
            }
        }
    }
}

fn out_of_memory() -> WireError {
    WireError::Io(io::Error::from(io::ErrorKind::OutOfMemory))
}

fn zstd_error(code: usize) -> WireError {
    // SAFETY: this status comes from the frozen Zstandard API. The read-only
    // conversion takes no pointers and does not access a compression context.
    let native = unsafe { ZSTD_getErrorCode(code) };
    if native == ZSTD_ErrorCode::ZSTD_error_memory_allocation {
        // Do not allocate an owned diagnostic payload while reporting OOM.
        return WireError::Decode(io::ErrorKind::OutOfMemory.into());
    }
    WireError::Decode(io::Error::new(
        io::ErrorKind::InvalidData,
        zstd::zstd_safe::get_error_name(code),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: ZSTD_ErrorCode) -> usize {
        // Zstandard returns its typed error number negated as size_t.
        (code as usize).wrapping_neg()
    }

    #[test]
    fn native_allocation_error_code_remains_typed_out_of_memory() {
        let error = zstd_error(status(ZSTD_ErrorCode::ZSTD_error_memory_allocation));
        assert!(matches!(
            error,
            WireError::Decode(source) if source.kind() == io::ErrorKind::OutOfMemory
        ));
    }

    #[test]
    fn absent_contexts_keep_the_actual_creation_failure_mapper() {
        // Exercise the same None -> out_of_memory callback as both constructors;
        // this is controlled adapter coverage, not an induced process-wide OOM.
        let encoder: Option<CCtx<'static>> = None;
        let decoder: Option<DCtx<'static>> = None;
        for result in [
            encoder.ok_or_else(out_of_memory).map(|_| ()),
            decoder.ok_or_else(out_of_memory).map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(WireError::Io(source)) if source.kind() == io::ErrorKind::OutOfMemory
            ));
        }
    }

    #[test]
    fn non_allocation_native_codes_remain_invalid_data() {
        for code in [
            ZSTD_ErrorCode::ZSTD_error_corruption_detected,
            ZSTD_ErrorCode::ZSTD_error_checksum_wrong,
            ZSTD_ErrorCode::ZSTD_error_parameter_unsupported,
            ZSTD_ErrorCode::ZSTD_error_dstSize_tooSmall,
        ] {
            assert!(matches!(
                zstd_error(status(code)),
                WireError::Decode(source) if source.kind() == io::ErrorKind::InvalidData
            ));
        }
    }

    #[test]
    fn actual_native_bad_frame_remains_invalid_data() {
        let mut decoder = Decoder::new(Algorithm::Zstd).unwrap();
        assert!(matches!(
            decoder.decode(b"not a Zstandard frame", &mut [0; 64]),
            Err(WireError::Decode(source)) if source.kind() == io::ErrorKind::InvalidData
        ));
    }
}
