//! One-time VALUES recognition and complete-row, byte-bounded request assembly.

use super::{template::Template, writer};
use crate::protocol::framing::WireError;
use std::ops::Range;

pub(crate) struct Token {
    pub(crate) symbol: u8,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

const INSERT: u8 = 128;
const REPLACE: u8 = 129;
const LOW: u8 = 130;
const HIGH: u8 = 131;
const DELAYED: u8 = 132;
const IGNORE: u8 = 133;
const INTO: u8 = 134;
const VALUES: u8 = 135;
const AS: u8 = 136;
const ON: u8 = 137;
const DUPLICATE: u8 = 138;
const KEY: u8 = 139;
const UPDATE: u8 = 140;

pub(crate) fn word(bytes: &[u8]) -> u8 {
    for (name, symbol) in [
        (b"INSERT".as_slice(), INSERT),
        (b"REPLACE", REPLACE),
        (b"LOW_PRIORITY", LOW),
        (b"HIGH_PRIORITY", HIGH),
        (b"DELAYED", DELAYED),
        (b"IGNORE", IGNORE),
        (b"INTO", INTO),
        (b"VALUE", VALUES),
        (b"VALUES", VALUES),
        (b"AS", AS),
        (b"ON", ON),
        (b"DUPLICATE", DUPLICATE),
        (b"KEY", KEY),
        (b"UPDATE", UPDATE),
    ] {
        if bytes.eq_ignore_ascii_case(name) {
            return symbol;
        }
    }
    if bytes.first().is_some_and(u8::is_ascii_digit) {
        b'x'
    } else {
        b'i'
    }
}

fn identifier(symbol: Option<u8>) -> bool {
    symbol.is_some_and(|symbol| symbol == b'i' || (INSERT..=UPDATE).contains(&symbol))
}

/// Source ranges of a repeatable, placeholder-only VALUES row. Prefix and
/// suffix stay in the original encoded template; no SQL AST or copied text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Encoded source range including this row's opening and closing brackets.
    pub row: Range<usize>,
}

pub(crate) fn recognize(tokens: &[Token]) -> Option<Layout> {
    let at = |index: usize| tokens.get(index).map(|token| token.symbol);
    let replace = match at(0)? {
        INSERT => false,
        REPLACE => true,
        _ => return None,
    };
    let mut i = 1;
    if matches!(at(i), Some(LOW | DELAYED)) || (!replace && at(i) == Some(HIGH)) {
        i += 1;
    }
    if !replace && at(i) == Some(IGNORE) {
        i += 1;
    }
    if at(i) == Some(INTO) {
        i += 1;
    }
    if !identifier(at(i)) {
        return None;
    }
    i += 1;
    if at(i) == Some(b'.') {
        if !identifier(at(i + 1)) {
            return None;
        }
        i += 2;
    }
    if at(i) == Some(b'(') {
        i = names(tokens, i, true)?;
    }
    if at(i) != Some(VALUES) || at(i + 1) != Some(b'(') {
        return None;
    }
    i += 1;
    let start = tokens[i].start;
    i += 1;
    if at(i) != Some(b')') {
        if at(i) != Some(b'?') {
            return None;
        }
        i += 1;
        while at(i) == Some(b',') {
            if at(i + 1) != Some(b'?') {
                return None;
            }
            i += 2;
        }
    }
    if at(i) != Some(b')') {
        return None;
    }
    let end = tokens[i].end;
    i += 1;
    if at(i) == Some(AS) {
        if !identifier(at(i + 1)) {
            return None;
        }
        i += 2;
        if at(i) == Some(b'(') {
            i = names(tokens, i, false)?;
        }
    }
    if at(i) == Some(ON) {
        if at(i + 1) != Some(DUPLICATE) || at(i + 2) != Some(KEY) || at(i + 3) != Some(UPDATE) {
            return None;
        }
        i += 4;
        let first = i;
        while at(i).is_some() && at(i) != Some(b';') {
            if at(i) == Some(b'?') {
                return None;
            }
            i += 1;
        }
        if i == first {
            return None;
        }
    }
    if at(i) == Some(b';') {
        i += 1;
    }
    (i == tokens.len()).then_some(Layout { row: start..end })
}

fn names(tokens: &[Token], start: usize, empty: bool) -> Option<usize> {
    let at = |index: usize| tokens.get(index).map(|token| token.symbol);
    let mut i = start + 1;
    if empty && at(i) == Some(b')') {
        return Some(i + 1);
    }
    if !identifier(at(i)) {
        return None;
    }
    i += 1;
    while at(i) == Some(b',') {
        if !identifier(at(i + 1)) {
            return None;
        }
        i += 2;
    }
    (at(i) == Some(b')')).then_some(i + 1)
}

/// Reusable, native complete-row request builder. An overflowing but legal
/// encoded row remains owned by the caller, so normal rollover never converts
/// the same input again. A real row error must discard this unsent builder.
pub struct Blocks {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    current: Vec<u8>,
    rows: usize,
    limit: usize,
    target: usize,
}
impl Blocks {
    /// Prepare fixed fragments once. The target is only a soft batching goal;
    /// a larger legal single row can be sent intact up to the hard limit.
    ///
    /// # Errors
    /// Returns a fixed-fragment size, unexpected slot or allocation failure.
    pub fn new(
        template: &Template,
        layout: &Layout,
        limit: usize,
        target: usize,
    ) -> Result<Self, WireError> {
        let mut prefix = Vec::new();
        writer::append(&mut prefix, &[3], limit)?;
        let mut cursor = 0;
        if template
            .append_range_until_slot(&(0..layout.row.start), &mut cursor, &mut prefix, limit)?
            .is_some()
        {
            return Err(WireError::Malformed("VALUES prefix contains a binding"));
        }
        let mut suffix = Vec::new();
        cursor = 0;
        if template
            .append_range_until_slot(
                &(layout.row.end..template.len()),
                &mut cursor,
                &mut suffix,
                limit,
            )?
            .is_some()
        {
            return Err(WireError::Malformed("VALUES suffix contains a binding"));
        }
        Ok(Self {
            prefix,
            suffix,
            current: Vec::new(),
            rows: 0,
            limit,
            target: target.min(limit),
        })
    }

    /// Append one already-encoded complete row. False means seal and send the
    /// current block, then offer these same row bytes again under the confirmed
    /// context. No row is partly appended or silently skipped.
    ///
    /// # Errors
    /// Returns a complete single-row hard-limit or allocation failure, before
    /// the current block is sent. Checked arithmetic never wraps a size.
    pub fn push(&mut self, row: &[u8]) -> Result<bool, WireError> {
        let fixed = self.prefix.len().checked_add(self.suffix.len());
        let single = fixed
            .and_then(|fixed| fixed.checked_add(row.len()))
            .unwrap_or(usize::MAX);
        if single > self.limit {
            return Err(WireError::Limit {
                required: single,
                limit: self.limit,
            });
        }
        let required = self
            .current
            .len()
            .checked_add(usize::from(self.rows != 0))
            .and_then(|length| length.checked_add(row.len()))
            .and_then(|length| length.checked_add(self.suffix.len()))
            .unwrap_or(usize::MAX);
        if self.rows != 0 && required > self.target {
            return Ok(false);
        }
        if self.rows == 0 {
            writer::append(&mut self.current, &self.prefix, self.limit)?;
        }
        if self.rows != 0 {
            writer::append(&mut self.current, b",", self.limit)?;
        }
        writer::append(&mut self.current, row, self.limit - self.suffix.len())?;
        self.rows += 1;
        Ok(true)
    }

    /// Number of complete source rows in the current unsent command.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Seal the current complete request, or return None for an empty builder.
    ///
    /// # Errors
    /// Returns allocation failure while appending the already-budgeted suffix.
    pub fn take(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        if self.rows == 0 {
            return Ok(None);
        }
        writer::append(&mut self.current, &self.suffix, self.limit)?;
        self.rows = 0;
        Ok(Some(std::mem::take(&mut self.current)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bind::template::Compiler;
    use crate::charset::{codec::Codec, resolve};
    fn prepared(sql: &str) -> Template {
        let codec = Codec::new(resolve(None, None).unwrap().charset);
        let mut job = Compiler::for_batch(sql.as_bytes().to_vec(), codec, false, false);
        loop {
            if let Some(template) = job.advance().unwrap() {
                return template;
            }
        }
    }

    #[test]
    fn approved_shapes_share_the_original_lexical_boundaries() {
        for sql in [
            "insert into t values (%s)",
            "REPLACE LOW_PRIORITY INTO `db`.`t` (`a`,`b`) VALUE (%s, %s);",
            "INSERT HIGH_PRIORITY IGNORE t(a,b) VALUES (%s /* value */, %s) AS new(x,y) ON DUPLICATE KEY UPDATE a=new.x,b=IF(b='VALUES(%s)',10%%3,b); # tail",
            "INSERT /* header */ INTO t VALUES /* row */ (%s) AS `new` ON DUPLICATE KEY UPDATE a=VALUES(a)",
            "INSERT INTO t () VALUES ()",
            "INSERT INTO t VALUES (# protected %s\n%s)",
        ] {
            let mut template = prepared(sql);
            assert!(template.prepare_merge().is_some(), "{sql}");
        }
        for sql in [
            "UPDATE t SET a=%s",
            "INSERT INTO t SELECT %s",
            "INSERT INTO t SET a=%s",
            "INSERT INTO t VALUES %s",
            "INSERT INTO t VALUES (1,%s)",
            "INSERT INTO t VALUES (DEFAULT,%s)",
            "INSERT INTO t VALUES (ABS(%s))",
            "INSERT INTO t VALUES (%s),(%s)",
            "INSERT INTO t VALUES (%s) ON DUPLICATE KEY UPDATE a=%s",
            "INSERT INTO t VALUES (%s); SELECT 1",
            "INSERT /*! IGNORE */ INTO t VALUES (%s)",
            "INSERT INTO t VALUES (%s) AS new()",
            "INSERT INTO t VALUES (%s) 'unfinished",
        ] {
            assert!(prepared(sql).prepare_merge().is_none(), "{sql}");
        }
    }

    #[test]
    fn complete_request_boundaries_include_command_suffix_and_delimiters() {
        let mut template = prepared("INSERT t VALUES (%s) ON DUPLICATE KEY UPDATE a=10%%3;");
        let layout = template.prepare_merge().unwrap();
        let expected = b"\x03INSERT t VALUES (7),(8) ON DUPLICATE KEY UPDATE a=10%3;";
        let mut blocks = Blocks::new(&template, &layout, expected.len(), expected.len()).unwrap();
        assert!(blocks.push(b"(7)").unwrap());
        assert!(blocks.push(b"(8)").unwrap());
        assert!(!blocks.push(b"(9)").unwrap());
        assert_eq!(blocks.rows(), 2);
        assert_eq!(blocks.take().unwrap().unwrap(), expected);
        assert!(blocks.push(b"(9)").unwrap());
        assert!(blocks.take().unwrap().is_some());
        assert!(blocks.take().unwrap().is_none());
        let mut soft = Blocks::new(&template, &layout, expected.len(), 1).unwrap();
        assert!(soft.push(b"(1234)").unwrap());
        assert!(!soft.push(b"(1)").unwrap());
        assert!(soft.push(&vec![b'x'; expected.len()]).is_err());
        assert_eq!(soft.rows(), 1);
    }

    fn regex() -> &'static regex::bytes::Regex {
        static REGEX: std::sync::OnceLock<regex::bytes::Regex> = std::sync::OnceLock::new();
        REGEX.get_or_init(|| regex::bytes::RegexBuilder::new(
            r"\A(?:\x80[\x82-\x84]?\x85?|\x81[\x82\x84]?)\x86?[i\x80-\x8c](?:\.[i\x80-\x8c])?(?:\((?:[i\x80-\x8c](?:,[i\x80-\x8c])*)?\))?\x87(?P<row>\((?:\?(?:,\?)*)?\))(?:\x88[i\x80-\x8c](?:\([i\x80-\x8c](?:,[i\x80-\x8c])*\))?)?(?:\x89\x8a\x8b\x8c[^;?]+)?;?\z"
        ).unicode(false).build().unwrap())
    }
    fn recognize_regex(tokens: &[Token]) -> Option<Layout> {
        let symbols: Vec<u8> = tokens.iter().map(|token| token.symbol).collect();
        let matched = regex().captures(&symbols)?.name("row")?;
        Some(Layout {
            row: tokens[matched.start()].start..tokens[matched.end() - 1].end,
        })
    }

    const PROBE_SQL: &[&str] = &[
        "INSERT INTO t VALUES (%s,%s)",
        "REPLACE LOW_PRIORITY INTO `db`.`t` (`a`,`b`) VALUE (%s, %s);",
        "INSERT HIGH_PRIORITY IGNORE t(a,b) VALUES (%s /* value */, %s) AS new(x,y) ON DUPLICATE KEY UPDATE a=new.x,b=IF(b='VALUES(%s)',10%%3,b); # tail",
        "INSERT INTO t () VALUES ()",
        "SELECT %s, '%s', 10%%3 /* fixed */",
        "INSERT INTO t VALUES (%s) ON DUPLICATE KEY UPDATE a=%s",
    ];

    #[test]
    fn scanner_and_precompiled_regex_recognize_the_same_lexical_inputs() {
        for sql in PROBE_SQL.iter().copied().chain([
            "INSERT INTO t VALUES (%s),(%s)",
            "INSERT INTO t VALUES (DEFAULT,%s)",
            "INSERT INTO t VALUES (ABS(%s))",
            "INSERT INTO t VALUES (%s) AS new()",
            "INSERT INTO t VALUES (%s); SELECT 1",
            "INSERT INTO t VALUES (# %s\n%s)",
        ]) {
            let template = prepared(sql);
            let tokens = template.merge_tokens.unwrap();
            assert_eq!(recognize(&tokens), recognize_regex(&tokens), "{sql}");
        }
    }

    #[test]
    fn confirmed_prefix_uses_actual_responses_without_counting_unsent_rows() {
        use crate::protocol::packets::{self, OkKind};
        use crate::result::text::Response;
        use mysql_common::constants::CapabilityFlags;
        let caps = CapabilityFlags::CLIENT_PROTOCOL_41;
        let scope = crate::memory::Scope::new(crate::memory::Pool::new(10000), 10000).unwrap();
        let first = Response::Command(
            crate::result::text::Completion::new(
                packets::ok(&[0, 2, 8, 2, 0, 3, 0], caps, OkKind::Command).unwrap(),
                &scope,
            )
            .unwrap(),
        );
        let second = Response::Command(
            crate::result::text::Completion::new(
                packets::ok(&[0, 0, 0, 2, 0, 1, 0], caps, OkKind::Command).unwrap(),
                &scope,
            )
            .unwrap(),
        );
        let mut prefix = crate::session::query::ConfirmedPrefix::default();
        prefix.record(0..100, &first).unwrap();
        assert_eq!(
            (
                prefix.commands,
                prefix.source_rows,
                prefix.row_count,
                prefix.warnings,
                prefix.last_insert_id
            ),
            (1, 100, 2, 3, Some(8))
        );
        assert!(prefix.record(101..110, &second).is_err());
        assert_eq!(prefix.commands, 1);
        prefix.record(100..120, &second).unwrap();
        assert_eq!(
            (
                prefix.commands,
                prefix.source_rows,
                prefix.row_count,
                prefix.warnings,
                prefix.last_insert_id
            ),
            (2, 120, 2, 4, None)
        );
        assert!(
            prefix
                .diagnostic()
                .contains("does not imply transaction commit")
        );
    }

    // This bounded development comparison is not a public performance gate.
    // Both alternatives include exactly the same lexical preparation and SQL
    // ownership, and regex initialization is outside every timed call.
    #[test]
    #[cfg(unix)]
    #[ignore = "bounded release-only implementation-choice measurement"]
    fn compare_recognition_paths() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };
        fn call(sql: &str, regular: bool, assembled: bool) {
            let mut template = prepared(black_box(sql));
            let tokens = template.merge_tokens.take().unwrap();
            let layout = if regular {
                recognize_regex(&tokens)
            } else {
                recognize(&tokens)
            };
            if assembled {
                if let Some(layout) = layout {
                    let mut blocks = Blocks::new(&template, &layout, 67108864, 67108864).unwrap();
                    let row = if template.slots() == 0 {
                        b"()".as_slice()
                    } else {
                        b"(7,'Ada')".as_slice()
                    };
                    for _ in 0..100 {
                        assert!(blocks.push(black_box(row)).unwrap());
                    }
                    black_box(blocks.take().unwrap());
                }
            } else {
                black_box(layout);
            }
        }
        fn thread_ns() -> u64 {
            let mut time = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            assert_eq!(
                // SAFETY: the destination is a valid writable timespec and
                // the OS counter belongs to this synchronous test thread.
                unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) },
                0
            );
            u64::try_from(time.tv_sec).unwrap() * 1_000_000_000
                + u64::try_from(time.tv_nsec).unwrap()
        }
        fn elapsed(sql: &str, regular: bool, assembled: bool, n: usize) -> (u64, u64) {
            let cpu = thread_ns();
            let start = Instant::now();
            for _ in 0..n {
                call(sql, regular, assembled);
            }
            let wall = start.elapsed().as_nanos() as u64;
            (wall, thread_ns() - cpu)
        }
        regex();
        let mut read_margin = 0_u64;
        for _ in 0..256 {
            let (wall, cpu) = elapsed(PROBE_SQL[0], false, false, 0);
            read_margin = read_margin.max(wall.abs_diff(cpu));
        }
        // Empirical interface allowance, not an absolute bound on interference.
        let read_margin = read_margin.saturating_mul(2).saturating_add(1000);
        println!(
            "{{\"phase\":\"clock_probe\",\"empirical_read_margin_ns\":{read_margin},\"off_cpu_limit\":0.1,\"target_ns\":2000000,\"group_limit_ns\":250000000,\"pairs_per_scale\":6}}"
        );
        let until = Instant::now() + Duration::from_secs(30);
        for (scene, sql) in PROBE_SQL.iter().enumerate() {
            for assembled in [false, true] {
                for _ in 0..32 {
                    call(sql, false, assembled);
                    call(sql, true, assembled);
                }
                let mut n = 16;
                while elapsed(sql, false, assembled, n).0 < 2_000_000 && n < 1_048_576 {
                    n *= 2;
                }
                for scale in [1, 2] {
                    let mut group = 0;
                    while group < 6 {
                        assert!(
                            Instant::now() < until,
                            "comparison exhausted its 30-second bound"
                        );
                        let order = if group % 2 == 0 {
                            [false, true]
                        } else {
                            [true, false]
                        };
                        let mut samples = [(0_u64, 0_u64); 2];
                        let started = Instant::now();
                        for regular in order {
                            samples[usize::from(regular)] =
                                elapsed(sql, regular, assembled, n * scale);
                        }
                        let valid = started.elapsed() <= Duration::from_millis(250)
                            && samples.iter().all(|(wall, cpu)| {
                                *wall >= 1_000_000
                                    && cpu.saturating_sub(*wall) <= read_margin
                                    && (*wall as f64 - *cpu as f64 + read_margin as f64)
                                        / *wall as f64
                                        <= 0.1
                            });
                        let values = samples.map(|(wall, _)| wall as f64 / (n * scale) as f64);
                        println!(
                            "{{\"scene\":{scene},\"assembled100\":{assembled},\"scale\":{scale},\"group\":{group},\"n\":{},\"valid\":{valid},\"scanner_wall\":{},\"scanner_cpu\":{},\"regex_wall\":{},\"regex_cpu\":{},\"scanner_ns\":{},\"regex_ns\":{},\"ratio\":{}}}",
                            n * scale,
                            samples[0].0,
                            samples[0].1,
                            samples[1].0,
                            samples[1].1,
                            values[0],
                            values[1],
                            values[1] / values[0]
                        );
                        if valid {
                            group += 1;
                        }
                    }
                }
            }
        }
    }
}
