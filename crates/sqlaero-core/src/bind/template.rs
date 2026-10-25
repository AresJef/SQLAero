//! SQL lexical preparation with native ranges, not Python interpolation.

use super::writer;
use crate::charset::codec::Codec;
use crate::protocol::framing::WireError;
use std::ops::Range;

/// A positional-template failure, located in the original Unicode SQL source.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// Active named placeholders are outside the supported interface.
    #[error("named binding is not supported at SQL character {offset}")]
    Named {
        /// Zero-based original character of the unsupported directive.
        offset: usize,
    },
    /// A binding directive was found inside executable comment/hint code.
    #[error(
        "binding inside executable comments or optimizer hints is not supported at SQL character {offset}"
    )]
    CommentBinding {
        /// Zero-based original character of the prohibited directive.
        offset: usize,
    },
    /// A native allocation or final request-bound failure.
    #[error(transparent)]
    Wire(#[from] WireError),
}

enum Piece {
    Bytes(Range<usize>),
    Value(usize),
}

/// One encoded SQL template, shared by all rows under an unchanged context.
/// Fixed fragments borrow ranges of this same owned buffer; %% never triggers
/// a second scan, and there is no copied SQL string for each parameter.
pub struct Template {
    source: Vec<u8>,
    pieces: Vec<Piece>,
    slots: usize,
}

impl Template {
    /// Scan only the explicit-args path. No-args execution bypasses this module
    /// and sends the correctly encoded original SQL, including literal %%.
    ///
    /// # Errors
    /// Returns unsupported active binding syntax or actual native allocation.
    pub fn compile(
        source: Vec<u8>,
        codec: Codec,
        no_backslash: bool,
        ansi_quotes: bool,
    ) -> Result<Self, TemplateError> {
        let mut compiler = Compiler::new(source, codec, no_backslash, ansi_quotes);
        loop {
            if let Some(template) = compiler.advance()? {
                return Ok(template);
            }
        }
    }

    /// Return the native adapter width required for each current row.
    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// Append fixed bytes through the next position, or the end of the template.
    /// The caller writes one complete value directly into this same request
    /// before calling again. The cursor is local to one row, never shared.
    ///
    /// # Errors
    /// Returns the actual final request-bound or native allocation failure.
    pub fn append_until_slot(
        &self,
        cursor: &mut usize,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<Option<usize>, WireError> {
        while let Some(piece) = self.pieces.get(*cursor) {
            *cursor += 1;
            match piece {
                Piece::Bytes(range) => writer::append(output, &self.source[range.clone()], limit)?,
                Piece::Value(offset) => return Ok(Some(*offset)),
            }
        }
        Ok(None)
    }
}

/// A bounded native lexical preparation job. It owns only source bytes and
/// fragment ranges; the host checks signals/deadlines between advance calls.
pub struct Compiler {
    source: Vec<u8>,
    codec: Codec,
    no_backslash: bool,
    ansi_quotes: bool,
    position: usize,
    copied: usize,
    slots: usize,
    pieces: Vec<Piece>,
    state: Lex,
    special: bool,
    finished: bool,
}
#[derive(Clone, Copy)]
enum Lex {
    Code,
    Quote { delimiter: u8, identifier: bool },
    Line,
    Block,
    Named { start: usize },
}
impl Compiler {
    /// Retain the one encoded source buffer; no copied template or value tree.
    #[must_use]
    pub fn new(source: Vec<u8>, codec: Codec, no_backslash: bool, ansi_quotes: bool) -> Self {
        Self {
            source,
            codec,
            no_backslash,
            ansi_quotes,
            position: 0,
            copied: 0,
            slots: 0,
            pieces: Vec::new(),
            state: Lex::Code,
            special: false,
            finished: false,
        }
    }
    /// Scan at most 256 KiB of source plus one complete encoded character.
    /// None yields to the host; Some commits the finished immutable template.
    /// Unterminated quotes/comments do not create an extra SQL grammar check.
    ///
    /// # Errors
    /// Returns a prohibited active directive, consumed-job or allocation error.
    pub fn advance(&mut self) -> Result<Option<Template>, TemplateError> {
        let result = self.advance_inner();
        if result.is_err() {
            self.finished = true;
        }
        result
    }
    fn advance_inner(&mut self) -> Result<Option<Template>, TemplateError> {
        if self.finished {
            return Err(WireError::Malformed("template compiler already consumed").into());
        }
        let boundary = self
            .position
            .saturating_add(256 * 1024)
            .min(self.source.len());
        while self.position < boundary {
            let position = self.position;
            let byte = self.source[position];
            // Executable comments terminate at the original block boundary,
            // including when their interior quote or line comment is unfinished.
            if self.special && self.source.get(position..position + 2) == Some(b"*/") {
                self.special = false;
                self.state = Lex::Code;
                self.position += 2;
                continue;
            }
            match self.state {
                Lex::Quote {
                    delimiter,
                    identifier,
                } => {
                    if byte >= 128 {
                        self.position += self.character(position);
                    } else if byte == delimiter {
                        if self.source.get(position + 1) == Some(&delimiter) {
                            self.position += 2;
                        } else {
                            self.position += 1;
                            self.state = Lex::Code;
                        }
                    } else if byte == b'\\' && !identifier && !self.no_backslash {
                        self.position += 1;
                        if self.position < self.source.len()
                            && !(self.special
                                && self.source.get(self.position..self.position + 2) == Some(b"*/"))
                        {
                            self.position += self.character(self.position);
                        }
                    } else {
                        self.position += 1;
                    }
                }
                Lex::Line => {
                    if byte == b'\n' {
                        self.state = Lex::Code;
                    }
                    self.position += 1;
                }
                Lex::Block => {
                    if self.source.get(position..position + 2) == Some(b"*/") {
                        self.state = Lex::Code;
                        self.position += 2;
                    } else {
                        self.position += 1;
                    }
                }
                Lex::Named { start } => {
                    if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.') {
                        self.position += 1;
                    } else if position > start + 2
                        && self.source.get(position..position + 2) == Some(b")s")
                    {
                        let offset = self.source_position(start)?;
                        return Err(if self.special {
                            TemplateError::CommentBinding { offset }
                        } else {
                            TemplateError::Named { offset }
                        });
                    } else {
                        self.state = Lex::Code;
                    }
                }
                Lex::Code => {
                    if byte >= 128 {
                        self.position += self.character(position);
                        continue;
                    }
                    match byte {
                        b'\'' | b'"' | b'`' => {
                            self.state = Lex::Quote {
                                delimiter: byte,
                                identifier: byte == b'`' || (byte == b'"' && self.ansi_quotes),
                            };
                            self.position += 1;
                        }
                        b'#' => {
                            self.state = Lex::Line;
                            self.position += 1;
                        }
                        b'-' if self.source.get(position + 1) == Some(&b'-')
                            && self
                                .source
                                .get(position + 2)
                                .is_some_and(|next| self.codec.comment_space(*next)) =>
                        {
                            self.state = Lex::Line;
                            self.position += 2;
                        }
                        b'/' if self.source.get(position + 1) == Some(&b'*') => {
                            if !self.special
                                && self
                                    .source
                                    .get(position + 2)
                                    .is_some_and(|byte| matches!(byte, b'!' | b'+'))
                            {
                                self.special = true;
                                self.position += 3;
                            } else {
                                self.state = Lex::Block;
                                self.position += 2;
                            }
                        }
                        b'%' => {
                            if self.source.get(position + 1) == Some(&b's') {
                                if self.special {
                                    return Err(TemplateError::CommentBinding {
                                        offset: self.source_position(position)?,
                                    });
                                }
                                self.edit(position, true)?;
                                self.position += 2;
                            } else if self.source.get(position + 1) == Some(&b'%') {
                                if !self.special {
                                    self.edit(position, false)?;
                                }
                                self.position += 2;
                            } else if self.source.get(position + 1) == Some(&b'(') {
                                self.state = Lex::Named { start: position };
                                self.position += 2;
                            } else {
                                self.position += 1;
                            }
                        }
                        _ => self.position += 1,
                    }
                }
            }
        }
        if self.position < self.source.len() {
            return Ok(None);
        }
        if self.copied < self.source.len() {
            self.pieces.try_reserve(1).map_err(WireError::from)?;
            self.pieces
                .push(Piece::Bytes(self.copied..self.source.len()));
        }
        self.finished = true;
        Ok(Some(Template {
            source: std::mem::take(&mut self.source),
            pieces: std::mem::take(&mut self.pieces),
            slots: self.slots,
        }))
    }
    fn character(&self, position: usize) -> usize {
        self.codec
            .encoded_width(&self.source[position..])
            .max(1)
            .min(self.source.len() - position)
    }
    fn source_position(&self, byte: usize) -> Result<usize, WireError> {
        let text = self.codec.decode(&self.source[..byte]).map_err(|_| {
            WireError::Malformed("template source did not retain its validated encoding")
        })?;
        Ok(text.chars().count())
    }
    fn edit(&mut self, position: usize, value: bool) -> Result<(), WireError> {
        self.pieces.try_reserve(2)?;
        if position > self.copied {
            self.pieces.push(Piece::Bytes(self.copied..position));
        }
        if value {
            self.pieces.push(Piece::Value(position));
            self.slots += 1;
        } else {
            self.pieces.push(Piece::Bytes(position..position + 1));
        }
        self.copied = position + 2;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn codec() -> Codec {
        Codec::new(crate::charset::resolve(None, None).unwrap().charset)
    }
    #[test]
    fn active_positions_percent_and_protected_regions_are_distinct() {
        let source = b"SELECT '%s',\"%s\",`%s`,%%s,%%%s /* %s */ -- %s\n,%s".to_vec();
        let template = Template::compile(source, codec(), false, false).unwrap();
        assert_eq!(template.slots(), 2);
        let mut cursor = 0;
        let mut out = Vec::new();
        while template
            .append_until_slot(&mut cursor, &mut out, 1024)
            .unwrap()
            .is_some()
        {
            out.push(b'7');
        }
        assert_eq!(out, b"SELECT '%s',\"%s\",`%s`,%s,%7 /* %s */ -- %s\n,7");
    }
    #[test]
    fn special_comments_reject_only_actual_binding_and_do_not_rewrite_fixed_text() {
        assert!(matches!(
            Template::compile(b"SELECT /*! %s */ 1".to_vec(), codec(), false, false),
            Err(TemplateError::CommentBinding { .. })
        ));
        assert!(
            Template::compile(b"SELECT /*! '%s' */ %s".to_vec(), codec(), false, false).is_ok()
        );
        assert!(matches!(
            Template::compile(b"SELECT %(name)s".to_vec(), codec(), false, false),
            Err(TemplateError::Named { .. })
        ));
        assert!(Template::compile(b"SELECT 1%(2)".to_vec(), codec(), false, false).is_ok());
        assert_eq!(
            Template::compile(b"SELECT %s,'unfinished".to_vec(), codec(), false, false)
                .unwrap()
                .slots(),
            1
        );
    }

    #[test]
    fn long_quote_comment_and_directive_scans_yield_without_losing_context() {
        for (prefix, suffix, slots) in [
            ("SELECT '", "%s',%s", 1),
            ("SELECT /* ", "%s */ %s", 1),
            ("SELECT # ", "%s\n%s", 1),
            ("SELECT /*! '", "%s' */ %s", 1),
        ] {
            let source = format!("{prefix}{}{suffix}", "x".repeat(600_000)).into_bytes();
            let mut job = Compiler::new(source, codec(), false, false);
            let mut yields = 0;
            let template = loop {
                let before = job.position;
                let step = job.advance().unwrap();
                assert!(job.position - before <= 256 * 1024 + 4);
                match step {
                    Some(template) => break template,
                    None => yields += 1,
                }
            };
            assert!(yields >= 2);
            assert_eq!(template.slots(), slots);
        }
        let source = format!("SELECT %({})s", "x".repeat(600_000)).into_bytes();
        let mut job = Compiler::new(source, codec(), false, false);
        assert!(job.advance().unwrap().is_none());
        assert!(job.advance().unwrap().is_none());
        assert!(matches!(job.advance(), Err(TemplateError::Named { .. })));
    }
}
