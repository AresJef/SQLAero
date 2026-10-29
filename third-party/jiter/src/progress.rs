//! Fallible, cooperative work checkpoints for strings and object keys.

use std::collections::TryReserveError;

use crate::JiterError;
use crate::errors::{JsonError, json_error};
use crate::simd::decode_string_chunk;
use crate::string_decoder::{StringChunk, StringOutput, Tape, parse_escape};

/// Maximum input window passed to the unchanged platform string scanner.
pub const WORK_WINDOW: usize = 64 * 1024;

/// Keep syntax, allocation and caller cancellation as distinct failures.
#[derive(Debug)]
pub enum CheckedError<E> {
    /// A normal JSON syntax/type error, retaining its original byte position.
    Json(JiterError),
    /// The exact caller-owned cancellation value, never converted to text.
    Interrupted(E),
    /// Failure to grow the temporary decoded-string storage.
    Allocation(TryReserveError),
}

impl<E> From<JsonError> for CheckedError<E> {
    fn from(error: JsonError) -> Self {
        Self::Json(error.into())
    }
}

/// A call-local, cumulative work budget. The callback is never stored globally.
pub struct Progress<F> {
    callback: F,
    completed: usize,
    remaining: usize,
}

impl<F> Progress<F> {
    /// Create a budget shared by every string/key in one document.
    pub fn new(callback: F) -> Self {
        Self {
            callback,
            completed: 0,
            remaining: WORK_WINDOW,
        }
    }

    /// Account for actual scanning, validation or copying work.
    #[inline]
    pub fn charge<E>(&mut self, bytes: usize) -> Result<(), CheckedError<E>>
    where
        F: FnMut(usize) -> Result<(), E>,
    {
        self.checkpoint(bytes).map_err(CheckedError::Interrupted)
    }

    /// Share this budget with the caller's final representation conversion.
    #[inline]
    pub fn checkpoint<E>(&mut self, bytes: usize) -> Result<(), E>
    where
        F: FnMut(usize) -> Result<(), E>,
    {
        if bytes < self.remaining {
            self.remaining -= bytes;
        } else {
            self.completed = self
                .completed
                .saturating_add(WORK_WINDOW - self.remaining)
                .saturating_add(bytes);
            (self.callback)(self.completed)?;
            self.remaining = WORK_WINDOW;
        }
        Ok(())
    }
}

fn scan<E, F>(
    data: &[u8],
    mut index: usize,
    mut ascii: bool,
    partial: bool,
    progress: &mut Progress<F>,
) -> Result<(StringChunk, bool, usize), CheckedError<E>>
where
    F: FnMut(usize) -> Result<(), E>,
{
    loop {
        let end = index.saturating_add(WORK_WINDOW).min(data.len());
        let (kind, ascii_next, position) =
            decode_string_chunk(&data[..end], index, ascii, end < data.len() || partial)?;
        if position > index {
            progress.charge(position - index)?;
        }
        ascii = ascii_next;
        // A synthetic window end is neither a closing quote nor real EOF.
        if position == end && end < data.len() {
            index = end;
        } else {
            return Ok((kind, ascii, position));
        }
    }
}

fn copy<E, F>(
    tape: &mut Tape,
    data: &[u8],
    progress: &mut Progress<F>,
) -> Result<(), CheckedError<E>>
where
    F: FnMut(usize) -> Result<(), E>,
{
    for chunk in data.chunks(WORK_WINDOW) {
        tape.try_reserve(chunk.len())
            .map_err(CheckedError::Allocation)?;
        tape.extend_from_slice(chunk);
        progress.charge(chunk.len())?;
    }
    Ok(())
}

fn text<'a, E, F>(
    bytes: &'a [u8],
    ascii: bool,
    start: usize,
    partial: bool,
    progress: &mut Progress<F>,
) -> Result<&'a str, CheckedError<E>>
where
    F: FnMut(usize) -> Result<(), E>,
{
    if ascii {
        // SAFETY: the platform scanner and escape decoder proved ASCII.
        return Ok(unsafe { std::str::from_utf8_unchecked(bytes) });
    }
    let mut position = 0;
    while position < bytes.len() {
        let end = position.saturating_add(WORK_WINDOW).min(bytes.len());
        let consumed = match std::str::from_utf8(&bytes[position..end]) {
            Ok(_) => end - position,
            Err(error) if error.error_len().is_none() && end < bytes.len() => error.valid_up_to(),
            Err(error) if error.error_len().is_none() && partial => {
                let valid = position + error.valid_up_to();
                progress.charge(error.valid_up_to())?;
                // SAFETY: prior chunks and the reported prefix are valid UTF-8.
                return Ok(unsafe { std::str::from_utf8_unchecked(&bytes[..valid]) });
            }
            Err(error) => {
                return Err(json_error!(
                    InvalidUnicodeCodePoint,
                    start + position + error.valid_up_to() + 1
                )
                .into());
            }
        };
        // Non-final windows exceed the maximum UTF-8 sequence length, so an
        // incomplete suffix always follows a nonempty validated prefix.
        position += consumed;
        progress.charge(consumed)?;
    }
    // SAFETY: the complete slice was validated, carrying split sequences.
    Ok(unsafe { std::str::from_utf8_unchecked(bytes) })
}

pub(crate) fn decode<'t, 'j, E, F>(
    data: &'j [u8],
    index: usize,
    tape: &'t mut Tape,
    partial: bool,
    progress: &mut Progress<F>,
) -> Result<(StringOutput<'t, 'j>, usize), CheckedError<E>>
where
    'j: 't,
    F: FnMut(usize) -> Result<(), E>,
{
    let start = index + 1;
    let (kind, mut ascii, mut index) = scan(data, start, true, partial, progress)?;
    if matches!(kind, StringChunk::StringEnd) {
        let value = text(&data[start..index], ascii, start, partial, progress)?;
        // SAFETY: text validated UTF-8 and the scanner established ascii.
        return Ok((unsafe { StringOutput::data(value, ascii) }, index + 1));
    }
    tape.clear();
    let mut chunk_start = start;
    loop {
        copy(tape, &data[chunk_start..index], progress)?;
        let escape_start = index;
        index += 1;
        let next = *data
            .get(index)
            .ok_or_else(|| json_error!(EofWhileParsingString, index))?;
        let point = match next {
            b'"' | b'\\' | b'/' => next as char,
            b'b' => '\u{0008}',
            b'f' => '\u{000c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let (point, end) = parse_escape(data, index)?;
                index = end;
                ascii &= point.is_ascii();
                point
            }
            _ => return Err(json_error!(InvalidEscape, index).into()),
        };
        // One escaped scalar writes at most four bytes. Keep its fallible
        // reserve, then charge the consumed source and decoded bytes together.
        let decoded_len = if point.is_ascii() {
            tape.try_reserve(1).map_err(CheckedError::Allocation)?;
            tape.push(point as u8);
            1
        } else {
            let mut encoded = [0; 4];
            let bytes = point.encode_utf8(&mut encoded).as_bytes();
            tape.try_reserve(bytes.len())
                .map_err(CheckedError::Allocation)?;
            tape.extend_from_slice(bytes);
            bytes.len()
        };
        progress.charge(index - escape_start + 1 + decoded_len)?;
        index += 1;
        let (kind, ascii_next, end) = scan(data, index, ascii, partial, progress)?;
        ascii = ascii_next;
        if matches!(kind, StringChunk::StringEnd) {
            copy(tape, &data[index..end], progress)?;
            let value = text(tape, ascii, start, partial, progress)?;
            // SAFETY: text validated the decoded tape; escapes preserve ascii.
            return Ok((unsafe { StringOutput::tape(value, ascii) }, end + 1));
        }
        chunk_start = index;
        index = end;
    }
}
