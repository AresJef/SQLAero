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
    /// Native MySQL markers are not part of the public binding interface.
    #[error("raw '?' binding is not supported at SQL character {offset}; use %s")]
    RawQuestion {
        /// Zero-based original Unicode character of the raw marker.
        offset: usize,
    },
    /// A preceding statement can change how later text values are parsed.
    #[error(
        "cannot safely bind the directive at SQL character {offset} after {setting} at SQL character {change_offset} may change the request context; execute the context change separately and wait for its response before binding another request"
    )]
    ContextChange {
        /// Zero-based original character of the later potential directive.
        offset: usize,
        /// Zero-based original character of the context-changing target.
        change_offset: usize,
        /// The lexical setting or opaque statement responsible for the barrier.
        setting: &'static str,
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
    pub(crate) source: Vec<u8>,
    pieces: Vec<Piece>,
    slots: usize,
    pub(crate) merge_tokens: Option<Vec<super::batch::Token>>,
    utc_overrides: Vec<UtcOverride>,
}

impl Template {
    /// Scan the explicit-args path. Text no-args execution bypasses this
    /// module; native no-args preparation uses its raw-marker guard mode.
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

    /// Whether the completed lexical scan found slot-specific UTC restrictions.
    #[must_use]
    pub fn has_utc_overrides(&self) -> bool {
        !self.utc_overrides.is_empty()
    }

    /// Encoded byte offset of a SET_VAR hint whose effective statement time
    /// zone cannot be proven UTC for this slot. This independent fact applies
    /// to text, native PREPARE and prepared display alike; it does not assert
    /// that a possibly invalid or ignored hint actually changed the server.
    #[must_use]
    pub fn utc_override(&self, slot: usize) -> Option<usize> {
        if slot >= self.slots {
            return None;
        }
        let index = self
            .utc_overrides
            .partition_point(|entry| entry.slots.end <= slot);
        self.utc_overrides
            .get(index)
            .filter(|entry| entry.slots.contains(&slot))
            .map(|entry| entry.hint)
    }

    /// Resolve a hint's original Unicode position only when an actual aware
    /// value needs a diagnostic. Ordinary and non-aware values pay no prefix
    /// decoding or allocation merely because the SQL contains a fixed hint.
    ///
    /// # Errors
    /// Returns an invalid encoded-source failure using the caller's real codec.
    pub fn utc_override_character(
        &self,
        slot: usize,
        codec: Codec,
    ) -> Result<Option<usize>, WireError> {
        self.utc_override(slot)
            .map(|offset| source_position(&self.source, codec, offset))
            .transpose()
    }

    /// Recognize one ordinary `LOAD DATA ... LOCAL INFILE` request using the
    /// retained lexical tokens. Call before `prepare_merge` consumes them.
    /// Missing or unsafe lexical metadata returns false; quoted/commented
    /// semicolons do not delimit a statement. This neither validates the rest
    /// of MySQL's LOAD grammar nor authorizes any local filesystem source.
    #[must_use]
    pub fn is_single_local_infile(&self) -> bool {
        let Some(tokens) = &self.merge_tokens else {
            return false;
        };
        let word = |index: usize, expected: &[u8]| {
            tokens.get(index).is_some_and(|token| {
                self.source
                    .get(token.start..token.end)
                    .is_some_and(|source| source.eq_ignore_ascii_case(expected))
            })
        };
        if !word(0, b"LOAD") || !word(1, b"DATA") {
            return false;
        }
        let local = if word(2, b"LOW_PRIORITY") || word(2, b"CONCURRENT") {
            3
        } else {
            2
        };
        word(local, b"LOCAL")
            && word(local + 1, b"INFILE")
            && tokens
                .iter()
                .enumerate()
                .all(|(index, token)| token.symbol != b';' || index + 1 == tokens.len())
    }

    /// Return the same encoded allocation after a read-only lexical check.
    #[must_use]
    pub fn into_source(self) -> Vec<u8> {
        self.source
    }

    /// Consume optional lexical metadata and recognize the approved repeated
    /// VALUES row. This preparation happens once, never once per input row.
    pub fn prepare_merge(&mut self) -> Option<super::batch::Layout> {
        self.merge_tokens
            .take()
            .and_then(|tokens| super::batch::recognize(&tokens, &self.source))
    }

    /// Start bounded native PREPARE assembly from the same lexical edits.
    /// Only actual value pieces become question marks; fixed bytes and quoted
    /// percent/question text are never scanned again.
    ///
    /// # Errors
    /// Returns a command-byte capacity or native allocation failure.
    pub fn prepare_request(&self, limit: usize) -> Result<PrepareWriter<'_>, WireError> {
        let mut output = Vec::new();
        writer::append(&mut output, &[0x16], limit)?;
        Ok(PrepareWriter {
            template: self,
            piece: 0,
            offset: 0,
            output,
            limit,
            finished: false,
            bound: true,
        })
    }

    /// Preserve the original SQL source for an omitted-arguments PREPARE.
    /// This uses the same bounded writer, without percent edits or parameter
    /// consumption. The caller still applies the prepared raw-marker guard.
    ///
    /// # Errors
    /// Returns command-byte capacity or native allocation failure.
    pub fn prepare_unbound_request(&self, limit: usize) -> Result<PrepareWriter<'_>, WireError> {
        let mut writer = self.prepare_request(limit)?;
        writer.bound = false;
        Ok(writer)
    }

    /// Encoded source length used by range-based request assembly.
    #[must_use]
    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// Whether the encoded template is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
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
        self.append_range_until_slot(&(0..self.source.len()), cursor, output, limit)
    }

    /// Append a selected source range through its next binding position.
    /// Percent normalization is shared with ordinary positional execution.
    ///
    /// # Errors
    /// Returns a request-bound or allocation failure.
    pub fn append_range_until_slot(
        &self,
        range: &Range<usize>,
        cursor: &mut usize,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<Option<usize>, WireError> {
        while let Some(piece) = self.pieces.get(*cursor) {
            *cursor += 1;
            match piece {
                Piece::Bytes(bytes) => {
                    let start = bytes.start.max(range.start);
                    let end = bytes.end.min(range.end);
                    if start < end {
                        writer::append(output, &self.source[start..end], limit)?;
                    }
                }
                Piece::Value(offset) if range.contains(offset) => return Ok(Some(*offset)),
                Piece::Value(_) => (),
            }
        }
        Ok(None)
    }
}

/// Bounded native template mapping. The host checks signals/deadlines between
/// advances, and receives one owned complete COM_STMT_PREPARE request at end.
pub struct PrepareWriter<'template> {
    template: &'template Template,
    piece: usize,
    offset: usize,
    output: Vec<u8>,
    limit: usize,
    finished: bool,
    bound: bool,
}
impl PrepareWriter<'_> {
    /// Copy at most 256 KiB of final bytes without exposing a partial request.
    /// Byte copies may stop within a character because all bytes remain owned
    /// and are not decoded or sent until the complete mapping is returned.
    ///
    /// # Errors
    /// Returns a consumed-job, complete request capacity or allocation failure.
    pub fn advance(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        if self.finished {
            return Err(WireError::Malformed("prepare writer already consumed"));
        }
        let result = self.advance_inner();
        if result.is_err() {
            self.finished = true;
        }
        result
    }
    fn advance_inner(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        let mut work = 0;
        while work < 256 * 1024 {
            let original = Piece::Bytes(0..self.template.source.len());
            let next = if self.bound {
                self.template.pieces.get(self.piece)
            } else {
                (self.piece == 0).then_some(&original)
            };
            let Some(piece) = next else {
                self.finished = true;
                return Ok(Some(std::mem::take(&mut self.output)));
            };
            match piece {
                Piece::Value(_) => {
                    writer::append(&mut self.output, b"?", self.limit)?;
                    self.piece += 1;
                    work += 1;
                }
                Piece::Bytes(range) => {
                    let start = range.start + self.offset;
                    let end = range.end.min(start.saturating_add(256 * 1024 - work));
                    writer::append(
                        &mut self.output,
                        &self.template.source[start..end],
                        self.limit,
                    )?;
                    work += end - start;
                    if end == range.end {
                        self.piece += 1;
                        self.offset = 0;
                    } else {
                        self.offset += end - start;
                    }
                }
            }
        }
        let pieces = if self.bound {
            self.template.pieces.len()
        } else {
            1
        };
        if self.piece == pieces {
            self.finished = true;
            Ok(Some(std::mem::take(&mut self.output)))
        } else {
            Ok(None)
        }
    }
}

/// One affected statement's ordered native slots and original hint location.
struct UtcOverride {
    slots: Range<usize>,
    hint: usize,
}

/// Statement-local facts, never a permanent request-context barrier. Ranges
/// are materialized only for affected statements with actual binding slots.
struct UtcHints {
    ranges: Vec<UtcOverride>,
    first_slot: usize,
    risk: Option<usize>,
    comment: HintUtc,
}
impl UtcHints {
    fn new() -> Self {
        Self {
            ranges: Vec::new(),
            first_slot: 0,
            risk: None,
            comment: HintUtc::new(),
        }
    }
    fn close_comment(&mut self) {
        self.comment.finish_candidate(false);
        self.risk = self.risk.or(self.comment.risk);
        self.comment = HintUtc::new();
    }
    fn finish_statement(&mut self, slots: usize) -> Result<(), WireError> {
        if let Some(hint) = self.risk.take()
            && self.first_slot < slots
        {
            self.ranges.try_reserve(1)?;
            self.ranges.push(UtcOverride {
                slots: self.first_slot..slots,
                hint,
            });
        }
        self.first_slot = slots;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum HintStage {
    Idle,
    Open,
    Name,
    Equals,
    Value,
}

/// Observe just SET_VAR's name/scalar/closing delimiter in the existing lexer.
/// No SQL AST, timezone evaluation, source copy or per-token allocation lives
/// here. A proven first UTC assignment suppresses duplicates only in this same
/// comment; a different comment has no such applicability proof.
struct HintUtc {
    depth: usize,
    stage: HintStage,
    start: usize,
    target: bool,
    known_target: bool,
    utc: bool,
    value_tokens: u8,
    first_utc: bool,
    risk: Option<usize>,
}
impl HintUtc {
    fn new() -> Self {
        Self {
            depth: 0,
            stage: HintStage::Idle,
            start: 0,
            target: false,
            known_target: false,
            utc: false,
            value_tokens: 0,
            first_utc: false,
            risk: None,
        }
    }
    fn finish_candidate(&mut self, closed: bool) {
        if self.target && !self.known_target {
            // The server resolves system names with its own collation. An
            // unknown name must not establish same-target duplicate identity.
            self.risk.get_or_insert(self.start);
        } else if self.target && !self.first_utc {
            if closed
                && matches!(self.stage, HintStage::Value)
                && self.value_tokens == 1
                && self.utc
                && self.depth == 1
            {
                self.first_utc = true;
            } else {
                self.risk.get_or_insert(self.start);
            }
        }
        self.target = false;
    }
    fn token(&mut self, symbol: u8, raw: &[u8], start: usize) {
        let depth = self.depth;
        if symbol == b'(' {
            self.depth = self.depth.saturating_add(1);
        }
        if symbol == b')' {
            if depth == 1 {
                self.finish_candidate(true);
                self.stage = HintStage::Idle;
            } else if self.target {
                self.value_tokens = 2;
                self.utc = false;
            }
            self.depth = self.depth.saturating_sub(1);
            return;
        }
        if depth == 0 {
            if matches!(self.stage, HintStage::Open) && symbol == b'(' {
                self.stage = HintStage::Name;
            } else if raw.eq_ignore_ascii_case(b"SET_VAR") {
                self.stage = HintStage::Open;
                self.start = start;
                self.target = false;
                self.known_target = false;
                self.value_tokens = 0;
                self.utc = false;
            } else {
                self.stage = HintStage::Idle;
            }
            return;
        }
        if depth > 1 {
            if self.target {
                self.value_tokens = 2;
                self.utc = false;
            }
            return;
        }
        match self.stage {
            HintStage::Name => {
                let name = if symbol == b'i' && raw.len() >= 2 && matches!(raw[0], b'`' | b'"') {
                    &raw[1..raw.len() - 1]
                } else {
                    raw
                };
                self.known_target = symbol == b'i' && name.eq_ignore_ascii_case(b"time_zone");
                self.target = self.known_target || (symbol == b'i' && !name.is_ascii());
                self.stage = if self.target {
                    HintStage::Equals
                } else {
                    HintStage::Idle
                };
            }
            HintStage::Equals => {
                self.stage = HintStage::Value;
                if symbol != b'=' {
                    self.value_tokens = 2;
                }
            }
            HintStage::Value => {
                self.value_tokens = self.value_tokens.saturating_add(1);
                self.utc = self.value_tokens == 1
                    && matches!(symbol, b'i' | b'x')
                    && raw.len() == 8
                    && matches!(raw[0], b'\'' | b'"' | b'`')
                    && raw[7] == raw[0]
                    && &raw[1..7] == b"+00:00";
            }
            _ => (),
        }
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
    tokens: Option<Vec<super::batch::Token>>,
    merge_safe: bool,
    native_markers: bool,
    binding: bool,
    local: LocalGate,
    context: ContextGuard,
    special_version: bool,
    hint: bool,
    utc: UtcHints,
}

#[derive(Clone, Copy)]
struct ContextChange {
    start: usize,
    setting: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextStatement {
    First,
    Other,
    Set,
}

/// Observe only statement prefixes and SET assignment targets, never SQL
/// expressions or Python values. This is a binding precondition, not a SQL
/// validator. A possible change remains unconfirmed for this whole request.
struct ContextGuard {
    enabled: bool,
    statement: ContextStatement,
    default_global: bool,
    target_global: bool,
    at_marks: u8,
    value: bool,
    depth: usize,
    character: bool,
    candidate: Option<ContextChange>,
    pending: Option<ContextChange>,
    barrier: Option<ContextChange>,
}

impl ContextGuard {
    fn new() -> Self {
        Self {
            enabled: true,
            statement: ContextStatement::First,
            default_global: false,
            target_global: false,
            at_marks: 0,
            value: false,
            depth: 0,
            character: false,
            candidate: None,
            pending: None,
            barrier: None,
        }
    }

    fn needs_words(&self) -> bool {
        self.enabled
            && self.barrier.is_none()
            && (self.statement == ContextStatement::First
                || (self.statement == ContextStatement::Set && !self.value))
    }

    fn word(&mut self, word: &[u8], start: usize) {
        if !self.needs_words() {
            return;
        }
        if self.statement == ContextStatement::First {
            self.statement = if word.eq_ignore_ascii_case(b"SET") {
                ContextStatement::Set
            } else {
                ContextStatement::Other
            };
            let setting = if word.eq_ignore_ascii_case(b"CALL") {
                Some("CALL")
            } else if word.eq_ignore_ascii_case(b"EXECUTE") {
                Some("EXECUTE")
            } else {
                None
            };
            self.pending = setting.map(|setting| ContextChange { start, setting });
            return;
        }
        if self.at_marks == 1 {
            return; // @user variables cannot change the input context.
        }
        if word.eq_ignore_ascii_case(b"GLOBAL")
            || word.eq_ignore_ascii_case(b"PERSIST")
            || word.eq_ignore_ascii_case(b"PERSIST_ONLY")
        {
            self.target_global = true;
            if self.at_marks == 0 {
                self.default_global = true;
            }
        } else if word.eq_ignore_ascii_case(b"SESSION") || word.eq_ignore_ascii_case(b"LOCAL") {
            self.target_global = false;
            if self.at_marks == 0 {
                self.default_global = false;
            }
        } else if !self.target_global {
            if self.at_marks == 0
                && (word.eq_ignore_ascii_case(b"NAMES")
                    || word.eq_ignore_ascii_case(b"CHARSET")
                    || (self.character && word.eq_ignore_ascii_case(b"SET")))
            {
                self.pending.get_or_insert(ContextChange {
                    start,
                    setting: "SET NAMES / CHARACTER SET / CHARSET",
                });
                self.value = true;
            } else {
                self.character = self.at_marks == 0 && word.eq_ignore_ascii_case(b"CHARACTER");
                self.candidate = if !word.is_ascii() {
                    Some(ContextChange {
                        start,
                        setting: "unconfirmed non-ASCII session SET target",
                    })
                } else {
                    [
                        "sql_mode",
                        "character_set_client",
                        "character_set_connection",
                        "collation_connection",
                        "time_zone",
                    ]
                    .into_iter()
                    .find(|name| word.eq_ignore_ascii_case(name.as_bytes()))
                    .map(|setting| ContextChange { start, setting })
                };
            }
        }
    }

    fn punctuation(&mut self, symbol: u8) {
        if !self.enabled {
            return;
        }
        if symbol == b';' {
            self.barrier = self.barrier.or(self.pending);
            self.statement = ContextStatement::First;
            self.default_global = false;
            self.reset_target();
        } else if self.statement == ContextStatement::Set {
            match symbol {
                b'@' if !self.value => {
                    if self.at_marks == 0 {
                        // @@ without an explicit qualifier means SESSION,
                        // even after a sticky GLOBAL keyword assignment.
                        self.target_global = false;
                    }
                    self.at_marks = self.at_marks.saturating_add(1);
                }
                b'=' if !self.value => {
                    if self.at_marks != 1 && !self.target_global {
                        self.pending = self.pending.or(self.candidate);
                    }
                    self.value = true;
                }
                b'(' => self.depth = self.depth.saturating_add(1),
                b')' => self.depth = self.depth.saturating_sub(1),
                b',' if self.depth == 0 => self.reset_target(),
                _ => (),
            }
        }
    }

    fn reset_target(&mut self) {
        self.target_global = self.default_global;
        self.at_marks = 0;
        self.value = false;
        self.depth = 0;
        self.character = false;
        self.candidate = None;
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalGate {
    Off,
    Prefix,
    Load,
    Rejected,
}
#[derive(Clone, Copy)]
enum Lex {
    Code,
    Quote {
        delimiter: u8,
        identifier: bool,
        start: usize,
    },
    Word {
        start: usize,
    },
    Line,
    Block,
    Named {
        start: usize,
    },
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
            tokens: None,
            merge_safe: true,
            native_markers: false,
            binding: true,
            local: LocalGate::Off,
            context: ContextGuard::new(),
            special_version: false,
            hint: false,
            utc: UtcHints::new(),
        }
    }

    /// Enable compact token spans during the same bounded lexical pass.
    /// Ordinary single-row binding does not pay for this optional metadata.
    #[must_use]
    pub fn for_batch(source: Vec<u8>, codec: Codec, no_backslash: bool, ansi_quotes: bool) -> Self {
        let mut compiler = Self::new(source, codec, no_backslash, ansi_quotes);
        compiler.tokens = Some(Vec::new());
        compiler
    }

    /// Inspect an already assembled COM_QUERY without activating binding or
    /// changing its bytes. A non-LOAD first keyword returns immediately without
    /// token/piece allocation or scanning the remaining SQL. Actual LOAD keeps
    /// bounded lexical checks. This read-only template is consumed with
    /// `is_single_local_infile` and `into_source`, never as a binding writer.
    /// A different/missing command prefix cannot authorize a LOCAL source.
    #[must_use]
    pub fn for_local_query(
        request: Vec<u8>,
        codec: Codec,
        no_backslash: bool,
        ansi_quotes: bool,
    ) -> Self {
        let valid = request.first() == Some(&0x03);
        let mut compiler = Self::new(request, codec, no_backslash, ansi_quotes);
        compiler.binding = false;
        compiler.context.enabled = false;
        compiler.merge_safe = valid;
        compiler.local = if valid {
            LocalGate::Prefix
        } else {
            LocalGate::Rejected
        };
        compiler.position = if valid { 1 } else { compiler.source.len() };
        compiler
    }

    /// Share the ordinary lexer while rejecting active raw native markers.
    /// Public positional directives remain %s and retain their exact ranges.
    #[must_use]
    pub fn for_prepared(
        source: Vec<u8>,
        codec: Codec,
        no_backslash: bool,
        ansi_quotes: bool,
    ) -> Self {
        let mut compiler = Self::new(source, codec, no_backslash, ansi_quotes);
        compiler.native_markers = true;
        compiler.context.enabled = false;
        compiler
    }

    /// Build a readable text preview with native-marker validation and the
    /// same context barrier as ordinary text interpolation. Actual PREPARE
    /// leaves multiple-statement validity to the server instead.
    #[must_use]
    pub fn for_prepared_display(
        source: Vec<u8>,
        codec: Codec,
        no_backslash: bool,
        ansi_quotes: bool,
    ) -> Self {
        let mut compiler = Self::for_prepared(source, codec, no_backslash, ansi_quotes);
        compiler.context.enabled = true;
        compiler
    }

    /// Guard native markers without activating the public binding language.
    /// Omitted arguments preserve all source bytes, including percent forms;
    /// only actual raw question marks outside protected text are rejected.
    #[must_use]
    pub fn for_unbound_prepared(
        source: Vec<u8>,
        codec: Codec,
        no_backslash: bool,
        ansi_quotes: bool,
    ) -> Self {
        let mut compiler = Self::for_prepared(source, codec, no_backslash, ansi_quotes);
        compiler.binding = false;
        compiler
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
            if let Some(change) = self.context.barrier {
                // The old lexer/codec can no longer prove which later bytes
                // are protected. Inspect the bounded suffix conservatively,
                // without rewriting it or guessing the new server context.
                if (self.native_markers && byte == b'?')
                    || (byte == b'%'
                        && self
                            .source
                            .get(position + 1)
                            .is_some_and(|next| matches!(next, b's' | b'%' | b'(')))
                {
                    return Err(TemplateError::ContextChange {
                        offset: self.source_position(position)?,
                        change_offset: self.source_position(change.start)?,
                        setting: change.setting,
                    });
                }
                self.position += 1;
                continue;
            }
            // Executable comments terminate at the original block boundary,
            // including when their interior quote or line comment is unfinished.
            if self.special && self.source.get(position..position + 2) == Some(b"*/") {
                if self.hint {
                    self.utc.close_comment();
                }
                self.special = false;
                self.special_version = false;
                self.hint = false;
                self.state = Lex::Code;
                self.position += 2;
                continue;
            }
            match self.state {
                Lex::Quote {
                    delimiter,
                    identifier,
                    start,
                } => {
                    if byte >= 128 {
                        self.position += self.character(position);
                    } else if byte == delimiter {
                        if self.source.get(position + 1) == Some(&delimiter) {
                            self.position += 2;
                        } else {
                            self.position += 1;
                            self.token(if identifier { b'i' } else { b'x' }, start, self.position)?;
                            self.state = Lex::Code;
                        }
                    } else if byte == b'\\' && !identifier && !self.no_backslash && !self.hint {
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
                Lex::Word { start } => {
                    if byte >= 128 {
                        self.position += self.character(position);
                    } else if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') {
                        self.position += 1;
                    } else {
                        self.word(start, position)?;
                        self.state = Lex::Code;
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
                    if (self.tokens.is_some()
                        || self.local == LocalGate::Prefix
                        || self.context.needs_words()
                        || self.hint)
                        && (byte >= 128
                            || byte.is_ascii_alphanumeric()
                            || matches!(byte, b'_' | b'$'))
                    {
                        self.state = Lex::Word { start: position };
                        self.position += self.character(position);
                        continue;
                    }
                    if byte >= 128 {
                        self.position += self.character(position);
                        continue;
                    }
                    match byte {
                        b'\'' | b'"' | b'`' => {
                            if self.local == LocalGate::Prefix {
                                self.reject_local();
                                continue;
                            }
                            self.state = Lex::Quote {
                                delimiter: byte,
                                identifier: byte == b'`' || (byte == b'"' && self.ansi_quotes),
                                start: position,
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
                                if self.local != LocalGate::Off {
                                    self.reject_local();
                                    continue;
                                }
                                self.merge_safe = false;
                                self.special = true;
                                self.special_version = self.source[position + 2] == b'!';
                                self.hint = self.source[position + 2] == b'+';
                                self.position += 3;
                            } else {
                                self.state = Lex::Block;
                                self.position += 2;
                            }
                        }
                        b'?' if self.native_markers => {
                            let offset = self.source_position(position)?;
                            return Err(if self.special {
                                TemplateError::CommentBinding { offset }
                            } else {
                                TemplateError::RawQuestion { offset }
                            });
                        }
                        b'%' => {
                            if self.local == LocalGate::Prefix {
                                self.reject_local();
                                continue;
                            }
                            if !self.binding {
                                self.position += 1;
                                continue;
                            }
                            if self.source.get(position + 1) == Some(&b's') {
                                if self.special {
                                    return Err(TemplateError::CommentBinding {
                                        offset: self.source_position(position)?,
                                    });
                                }
                                self.edit(position, true)?;
                                self.token(b'?', position, position + 2)?;
                                self.position += 2;
                            } else if self.source.get(position + 1) == Some(&b'%') {
                                if !self.special {
                                    self.edit(position, false)?;
                                }
                                self.token(b'x', position, position + 2)?;
                                self.position += 2;
                            } else if self.source.get(position + 1) == Some(&b'(') {
                                self.state = Lex::Named { start: position };
                                self.position += 2;
                            } else {
                                self.token(byte, position, position + 1)?;
                                self.position += 1;
                            }
                        }
                        _ => {
                            if !byte.is_ascii_whitespace() {
                                if self.local == LocalGate::Prefix {
                                    self.reject_local();
                                    continue;
                                }
                                self.token(byte, position, position + 1)?;
                            }
                            self.position += 1;
                        }
                    }
                }
            }
        }
        if self.position < self.source.len() {
            return Ok(None);
        }
        if let Lex::Word { start } = self.state {
            self.word(start, self.position)?;
        } else if !matches!(self.state, Lex::Code | Lex::Line) {
            self.merge_safe = false;
        }
        if self.local == LocalGate::Off && self.copied < self.source.len() {
            self.pieces.try_reserve(1).map_err(WireError::from)?;
            self.pieces
                .push(Piece::Bytes(self.copied..self.source.len()));
        }
        if self.hint {
            self.utc.close_comment();
        }
        self.utc.finish_statement(self.slots)?;
        self.finished = true;
        Ok(Some(Template {
            source: std::mem::take(&mut self.source),
            pieces: std::mem::take(&mut self.pieces),
            slots: self.slots,
            utc_overrides: std::mem::take(&mut self.utc.ranges),
            merge_tokens: if self.merge_safe {
                self.tokens.take()
            } else {
                None
            },
        }))
    }
    fn token(&mut self, symbol: u8, start: usize, end: usize) -> Result<(), WireError> {
        if self.hint {
            self.utc
                .comment
                .token(symbol, &self.source[start..end], start);
        } else if symbol == b';' {
            self.utc.finish_statement(self.slots)?;
        }
        if !self.hint
            && symbol == b'i'
            && self
                .source
                .get(start)
                .is_some_and(|byte| matches!(byte, b'`' | b'\'' | b'"'))
        {
            if self.context.statement == ContextStatement::First {
                self.context.statement = ContextStatement::Other;
            } else {
                self.context.word(&self.source[start + 1..end - 1], start);
            }
        } else if !self.hint {
            self.context.punctuation(symbol);
        }
        if self.context.barrier.is_some() {
            self.merge_safe = false;
        }
        if let Some(tokens) = &mut self.tokens {
            tokens.try_reserve(1)?;
            tokens.push(super::batch::Token { symbol, start, end });
        }
        Ok(())
    }
    fn word(&mut self, start: usize, end: usize) -> Result<(), WireError> {
        if self.local == LocalGate::Prefix {
            if !self.source[start..end].eq_ignore_ascii_case(b"LOAD") {
                self.reject_local();
                return Ok(());
            }
            self.local = LocalGate::Load;
            self.tokens = Some(Vec::new());
        }
        let mut context_start = start;
        if self.special_version {
            // MySQL recognizes five- or six-digit executable-comment version
            // prefixes. Keep this work bounded even for a giant numeric word.
            for _ in 0..6 {
                if context_start == end || !self.source[context_start].is_ascii_digit() {
                    break;
                }
                context_start += 1;
            }
            self.special_version = false;
        }
        if context_start < end && !self.hint {
            self.context
                .word(&self.source[context_start..end], context_start);
        }
        let symbol = super::batch::word(&self.source[start..end]);
        self.token(symbol, start, end)
    }
    fn reject_local(&mut self) {
        self.local = LocalGate::Rejected;
        self.merge_safe = false;
        self.tokens = None;
        self.position = self.source.len();
        self.state = Lex::Code;
    }
    fn character(&self, position: usize) -> usize {
        self.codec
            .encoded_width(&self.source[position..])
            .max(1)
            .min(self.source.len() - position)
    }
    fn source_position(&self, byte: usize) -> Result<usize, WireError> {
        source_position(&self.source, self.codec, byte)
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

fn source_position(source: &[u8], codec: Codec, byte: usize) -> Result<usize, WireError> {
    let text = codec.decode(&source[..byte]).map_err(|_| {
        WireError::Malformed("template source did not retain its validated encoding")
    })?;
    Ok(text.chars().count())
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
    fn context_changes_before_later_text_directives_are_rejected() {
        for prefix in [
            "SET sql_mode='NO_BACKSLASH_ESCAPES'",
            "SET SESSION `sql_mode`=''",
            "SET @@SESSION.sql_mode=''",
            "SET @@local.`character_set_client`=latin1",
            "SET @@`sql_mode`=''",
            "SET LOCAL character_set_connection=latin1",
            "SET NAMES latin1",
            "SET CHARACTER SET latin1",
            "SET CHARSET latin1",
            "SET collation_connection=latin1_swedish_ci",
            "SET time_zone='+01:00'",
            "SET @label=CONCAT('a', 'b'), sql_mode=''",
            "SET GLOBAL net_read_timeout=30, @@sql_mode=''",
            "SET @@GLOBAL.net_read_timeout=30, sql_mode=''",
            "SET GLOBAL net_read_timeout=30, SESSION sql_mode=''",
            "/*!80000 SET sql_mode='' */",
            "/*!80000SET sql_mode='' */",
            "/*+ fixed_hint */ SET sql_mode=''",
            "SET /*!80000 SESSION */ `sql_mode`=''",
            "CALL application_routine()",
            "EXECUTE application_statement",
        ] {
            let source = format!("{prefix}; SELECT %s");
            assert!(
                Template::compile(source.clone().into_bytes(), codec(), false, false).is_err(),
                "an unconfirmed request context must not bind later values: {source}"
            );
        }
    }

    #[test]
    fn unrelated_settings_and_values_before_a_change_keep_their_contract() {
        for source in [
            "SET @label=@@sql_mode; SELECT %s",
            "SET @sql_mode='text'; SELECT %s",
            "SET character_set_results=latin1; SELECT %s",
            "SET net_read_timeout=30; SELECT %s",
            "SET GLOBAL sql_mode=''; SELECT %s",
            "SET GLOBAL net_read_timeout=30, sql_mode=''; SELECT %s",
            "SET @@GLOBAL.sql_mode=''; SELECT %s",
            "SET PERSIST sql_mode=''; SELECT %s",
            "SET @@PERSIST_ONLY.sql_mode=''; SELECT %s",
            "SET @sql_mode=@@sql_mode, @name=CONCAT('a', 'b'); SELECT %s",
            "/*+ fixed_hint */ SELECT %s",
            "SELECT 'SET NAMES latin1;'; SELECT %s",
            "/* SET sql_mode=''; */ SELECT %s",
            "SELECT %s; SET sql_mode=''; SELECT 1",
            "SET sql_mode=%s",
        ] {
            assert_eq!(
                Template::compile(source.as_bytes().to_vec(), codec(), false, false)
                    .unwrap()
                    .slots(),
                1,
                "{source}"
            );
        }
    }

    #[test]
    fn changed_context_suffix_does_not_reuse_old_quote_rules_or_rewrite_percent() {
        for suffix in [
            "SELECT %s",
            "SELECT '%s'",
            "SELECT \"%s\"",
            "SELECT %%",
            "SELECT %(name)s",
        ] {
            let source = format!("SET sql_mode='ANSI_QUOTES'; {suffix}");
            assert!(matches!(
                Template::compile(source.into_bytes(), codec(), false, false),
                Err(TemplateError::ContextChange {
                    setting: "sql_mode",
                    ..
                })
            ));
        }
        let source = "SELECT '中'; SET sql_mode=''; SELECT %s";
        let error = match Template::compile(source.as_bytes().to_vec(), codec(), false, false) {
            Err(error) => error,
            Ok(_) => panic!("the later directive must not use an unconfirmed context"),
        };
        assert!(matches!(error, TemplateError::ContextChange {
            offset, change_offset, setting: "sql_mode",
        } if offset == source.chars().position(|ch| ch == '%').unwrap()
             && change_offset == 16));
    }

    #[test]
    fn prepared_protocol_keeps_server_validation_while_text_preview_uses_guard() {
        let source = b"SET NAMES latin1; SELECT %s".to_vec();
        let mut native = Compiler::for_prepared(source.clone(), codec(), false, false);
        let native = native.advance().unwrap().unwrap();
        assert_eq!(native.slots(), 1);
        let mut preview = Compiler::for_prepared_display(source, codec(), false, false);
        assert!(matches!(
            preview.advance(),
            Err(TemplateError::ContextChange { .. })
        ));
    }

    #[test]
    fn prepared_preview_cannot_lose_its_raw_marker_guard_after_a_context_barrier() {
        let mut preview = Compiler::for_prepared_display(
            b"SET NAMES latin1; SELECT ?".to_vec(),
            codec(),
            false,
            false,
        );
        assert!(matches!(
            preview.advance(),
            Err(TemplateError::ContextChange { .. })
        ));
    }

    #[test]
    fn context_barrier_survives_bounded_yields_without_a_suffix_allocation() {
        let source = format!("SET sql_mode=''; {}%s", " ".repeat(600_000)).into_bytes();
        let mut compiler = Compiler::new(source, codec(), false, false);
        assert!(compiler.advance().unwrap().is_none());
        assert!(compiler.advance().unwrap().is_none());
        assert!(matches!(
            compiler.advance(),
            Err(TemplateError::ContextChange { .. })
        ));
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
    #[test]
    fn time_zone_hint_records_all_slots_of_only_its_statement() {
        let source = "SELECT %s; WITH c AS (SELECT %s) SELECT /*+ SET_VAR(time_zone='+01:00') */ %s; SELECT %s";
        let template =
            Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
        let offset = source.find("SET_VAR").unwrap();
        assert_eq!(template.slots(), 4);
        assert_eq!(template.utc_override(0), None);
        assert_eq!(template.utc_override(1), Some(offset));
        assert_eq!(template.utc_override(2), Some(offset));
        assert_eq!(template.utc_override(3), None);
        assert_eq!(template.utc_override(4), None);
        assert_eq!(template.source, source.as_bytes());
    }

    #[test]
    fn time_zone_hint_facts_are_independent_of_prepared_context_barriers() {
        for preview in [false, true] {
            let source = "WITH c AS (SELECT %s) SELECT /*+ SET_VAR(`time_zone`='+08:00') */ %s";
            let mut compiler = if preview {
                Compiler::for_prepared_display(source.as_bytes().to_vec(), codec(), false, false)
            } else {
                Compiler::for_prepared(source.as_bytes().to_vec(), codec(), false, false)
            };
            let template = compiler.advance().unwrap().unwrap();
            assert_eq!(template.utc_override(0), source.find("SET_VAR"));
            assert_eq!(template.utc_override(1), source.find("SET_VAR"));
            let mut writer = template.prepare_request(1024).unwrap();
            assert_eq!(
                writer.advance().unwrap().unwrap(),
                [vec![0x16], source.replace("%s", "?").into_bytes()].concat()
            );
        }
    }

    #[test]
    fn hint_offsets_are_encoded_bytes_until_unicode_diagnostics_are_requested() {
        let source = "SELECT '中😀',%s /*+ SET_VAR(time_zone=SYSTEM) */";
        let template =
            Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
        let offset = source.find("SET_VAR").unwrap();
        assert_eq!(template.utc_override(0), Some(offset));
        assert_eq!(
            template.utc_override_character(0, codec()).unwrap(),
            Some(source[..offset].chars().count())
        );
    }
    #[test]
    fn exact_utc_and_same_comment_first_utc_duplicates_keep_no_fact_allocation() {
        for source in [
            "SELECT %s",
            "SELECT /*+ SET_VAR(time_zone='+00:00') */ %s",
            "SELECT /*+ SET_VAR(`time_zone`=`+00:00`) */ %s",
            "SELECT /*+ SET_VAR(time_zone=\"+00:00\") */ %s",
            "SELECT /*+ SET_VAR(time_zone='+00:00') SET_VAR(time_zone='+08:00') */ %s",
            "SELECT /*+ SET_VAR(sort_buffer_size=16M) SET_VAR(time_zone='+00:00') SET_VAR(time_zone=SYSTEM) */ %s",
            "SELECT /*+ SET_VAR(time_zone='+08:00') */ 1",
        ] {
            for ansi in [false, true] {
                let template =
                    Template::compile(source.as_bytes().to_vec(), codec(), false, ansi).unwrap();
                assert!(
                    (0..template.slots()).all(|slot| template.utc_override(slot).is_none()),
                    "{source}"
                );
                assert_eq!(template.utc_overrides.capacity(), 0, "{source}");
                assert_eq!(template.source, source.as_bytes());
            }
        }
    }

    #[test]
    fn non_utc_or_unknown_values_do_not_gain_a_guessed_utc_guarantee() {
        for value in [
            "'+01:00'",
            "'UTC'",
            "SYSTEM",
            "0",
            "NULL",
            "CONCAT('+00:00')",
            "('+00:00')",
            "'+00:00' extra",
            "'+00:00'+0",
            "'+00:00''x'",
            "' +00:00'",
        ] {
            let source = format!("SELECT /*+ SET_VAR(time_zone={value}) */ %s");
            let template =
                Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
            assert_eq!(template.utc_override(0), source.find("SET_VAR"), "{source}");
        }
        for body in [
            "SET_VAR(time_zone='+01:00') SET_VAR(time_zone='+00:00')",
            "SET_VAR(time_zone=unknown) SET_VAR(time_zone='+00:00')",
        ] {
            let source = format!("SELECT /*+ {body} */ %s");
            let template =
                Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
            assert_eq!(template.utc_override(0), source.find("SET_VAR"));
        }
        let source = "SELECT /*+ SET_VAR(time_zone='+00:00' */ %s";
        let template =
            Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
        assert_eq!(
            template.utc_override(0),
            source.find("SET_VAR"),
            "missing ')' is not a proven UTC hint"
        );
    }

    #[test]
    fn different_comments_do_not_inherit_a_first_comments_applicability() {
        let source = "SELECT /*+ SET_VAR(time_zone='+00:00') */ %s /*+ SET_VAR(time_zone='+02:00') */; SELECT %s";
        let template =
            Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
        assert_eq!(template.utc_override(0), source.rfind("SET_VAR"));
        assert_eq!(template.utc_override(1), None);
    }

    #[test]
    fn hint_quoted_names_follow_the_official_identifier_rule() {
        for name in ["time_zone", "`TIME_ZONE`", "\"time_zone\""] {
            for ansi in [false, true] {
                let source = format!("SELECT /*+ SET_VAR({name}='+01:00') */ %s");
                let template =
                    Template::compile(source.as_bytes().to_vec(), codec(), false, ansi).unwrap();
                assert_eq!(
                    template.utc_override(0),
                    if name.starts_with('"') && !ansi {
                        None
                    } else {
                        source.find("SET_VAR")
                    },
                    "{source}, ansi={ansi}"
                );
            }
        }
        for source in [
            "SELECT /*+ SET_VAR('time_zone'='+01:00') */ %s",
            "SELECT /*+ `SET_VAR`(time_zone='+01:00') */ %s",
            "SELECT /*+ SET_VAR(`time``_zone`='+01:00') */ %s",
        ] {
            assert_eq!(
                Template::compile(source.as_bytes().to_vec(), codec(), false, false)
                    .unwrap()
                    .utc_override(0),
                None,
                "{source}"
            );
        }
    }

    #[test]
    fn protected_hint_text_and_ordinary_sql_rules_remain_distinct() {
        for source in [
            "SELECT '/*+ SET_VAR(time_zone=SYSTEM) */',%s",
            "SELECT /* SET_VAR(time_zone=SYSTEM) */ %s",
            "SELECT -- /*+ SET_VAR(time_zone=SYSTEM) */\n%s",
            "SELECT # /*+ SET_VAR(time_zone=SYSTEM) */\n%s",
            "SELECT /*+ QB_NAME(`SET_VAR(time_zone=SYSTEM)`) */ %s",
            "SELECT /*+ SET_VAR(sort_buffer_size=16M) */ %s",
            "SELECT /*+ SET_VAR(sql_mode='NO_BACKSLASH_ESCAPES') */ %s; SELECT %s",
        ] {
            let template =
                Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
            assert!(
                (0..template.slots()).all(|slot| template.utc_override(slot).is_none()),
                "{source}"
            );
            assert_eq!(template.utc_overrides.capacity(), 0);
        }
        let source = r"SELECT /*+ SET_VAR(sort_buffer_size='x\') SET_VAR(time_zone='+01:00') */ %s";
        for no_backslash in [false, true] {
            let template =
                Template::compile(source.as_bytes().to_vec(), codec(), no_backslash, false)
                    .unwrap();
            assert_eq!(template.utc_override(0), source.find("SET_VAR(time_zone"));
            assert_eq!(template.source, source.as_bytes());
        }
        assert_eq!(
            Template::compile(br"SELECT 'x\'%s',%s".to_vec(), codec(), false, false)
                .unwrap()
                .slots(),
            1,
            "ordinary SQL still recognizes backslash escapes"
        );
    }

    #[test]
    fn late_hint_ranges_survive_bounded_scans_and_remain_after_merge_metadata_is_taken() {
        let source = format!(
            "WITH c AS (SELECT %s) SELECT /*+ SET_VAR(sort_buffer_size='{}') SET_VAR(time_zone='+01:00') */ %s; SELECT %s",
            "x".repeat(600_000)
        );
        let mut compiler = Compiler::for_batch(source.as_bytes().to_vec(), codec(), false, false);
        let mut yields = 0;
        let mut template = loop {
            let before = compiler.position;
            let value = compiler.advance().unwrap();
            assert!(compiler.position - before <= 256 * 1024 + 4);
            if let Some(template) = value {
                break template;
            }
            yields += 1;
        };
        assert!(yields >= 2);
        let offset = source.find("SET_VAR(time_zone").unwrap();
        template.prepare_merge();
        assert_eq!(template.utc_override(0), Some(offset));
        assert_eq!(template.utc_override(1), Some(offset));
        assert_eq!(template.utc_override(2), None);
        assert_eq!(template.source, source.as_bytes());
    }
    #[test]
    fn non_ascii_set_var_names_have_no_unproved_target_or_duplicate_identity() {
        for (source, last) in [
            ("SELECT /*+ SET_VAR(timé_zone='+02:00') */ %s", false),
            (
                "SELECT /*+ SET_VAR(`timé_zone`='+00:00') SET_VAR(time_zone='+01:00') */ %s",
                false,
            ),
            (
                "SELECT /*+ SET_VAR(time_zone='+00:00') SET_VAR(timé_zone='+01:00') */ %s",
                true,
            ),
            (
                "SELECT /*+ SET_VAR(sórt_buffer_size='+00:00') SET_VAR(time_zone='+01:00') */ %s",
                false,
            ),
        ] {
            let template =
                Template::compile(source.as_bytes().to_vec(), codec(), false, false).unwrap();
            assert_eq!(
                template.utc_override(0),
                if last {
                    source.rfind("SET_VAR")
                } else {
                    source.find("SET_VAR")
                },
                "{source}"
            );
        }
    }

    #[test]
    fn non_ascii_session_set_targets_keep_the_existing_scope_and_value_rules() {
        for source in [
            "SET sql_móde='NO_BACKSLASH_ESCAPES'; SELECT %s",
            "SET SESSION `time_zóne`='+00:00'; SELECT %s",
            "SET @@SESSION.`cháracter_set_client`=latin1; SELECT %s",
            "SET @label=1, sql_móde=''; SELECT %s",
            "SET @@GLOBAL.net_read_timeout=30, sql_móde=''; SELECT %s",
        ] {
            assert!(
                matches!(
                    Template::compile(source.as_bytes().to_vec(), codec(), false, false),
                    Err(TemplateError::ContextChange { .. })
                ),
                "{source}"
            );
        }
        for source in [
            "SET GLOBAL sql_móde=''; SELECT %s",
            "SET @@GLOBAL.`time_zóne`=''; SELECT %s",
            "SET GLOBAL net_read_timeout=30, sql_móde=''; SELECT %s",
            "SET @sql_móde='text'; SELECT %s",
            "SET @x=@@session.sql_móde; SELECT %s",
        ] {
            assert!(
                Template::compile(source.as_bytes().to_vec(), codec(), false, false).is_ok(),
                "{source}"
            );
        }
    }
}
