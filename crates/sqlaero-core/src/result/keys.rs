//! Deterministic unique result keys, independent of Python object construction.

use crate::memory::{Buffer, MemoryError, Scope};
use std::sync::Arc;

/// Canonical label/source bytes. The boundary supplies lossless UTF-8/WTF-8,
/// so exact equality remains meaningful even for surrogateescaped labels.
pub struct Name<'a> {
    /// The actual query-visible label, including AS.
    pub label: &'a [u8],
    /// Actual schema identity; empty means no source.
    pub database: &'a [u8],
    /// Query-visible table/alias identity; empty means no source table.
    pub table: &'a [u8],
}

/// A capacity or actual internal uniqueness failure; no silent renaming reset.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// Real controlled capacity/allocation failure.
    #[error(transparent)]
    Memory(#[from] MemoryError),
    /// A native invariant failed instead of producing ambiguous public keys.
    #[error("column key construction failed: {0}")]
    Invariant(&'static str),
}

#[derive(Clone, Copy)]
struct Rule {
    unique: bool,
    keep: bool,
    anchor: usize,
    next: usize,
}

/// All keys in column order, plus a sorted lookup index. Sorting uses the
/// in-place standard algorithm, not hidden hash-table allocation estimates.
pub struct Keys {
    values: Buffer<Buffer<u8>>,
    order: Buffer<usize>,
}
impl Keys {
    /// Derive the approved global collision rules exactly once per snapshot.
    /// All growing native storage, including temporary indexes, uses its scope.
    ///
    /// # Errors
    /// Returns actual capacity/overflow or an unexpected uniqueness failure.
    pub fn new(names: &[Name<'_>], scope: Option<&Arc<Scope>>) -> Result<Self, KeyError> {
        let mut order = buffer(scope);
        let mut values = buffer(scope);
        let mut rules = buffer(scope);
        order.reserve(names.len())?;
        values.reserve(names.len())?;
        rules.reserve(names.len())?;
        for index in 0..names.len() {
            order.push(index)?;
            values.push(buffer(scope))?;
            rules.push(Rule {
                unique: false,
                keep: false,
                anchor: 0,
                next: 1,
            })?;
        }
        order.as_mut_slice().sort_unstable_by(|a, b| {
            names[*a]
                .label
                .cmp(names[*b].label)
                .then_with(|| names[*a].database.cmp(names[*b].database))
                .then_with(|| names[*a].table.cmp(names[*b].table))
                .then(a.cmp(b))
        });
        let mut start = 0;
        while start < order.len() {
            let first = order[start];
            let mut end = start + 1;
            while end < order.len() && names[order[end]].label == names[first].label {
                end += 1;
            }
            let multiple_sources = order[start + 1..end].iter().any(|index| {
                names[*index].database != names[first].database
                    || names[*index].table != names[first].table
            });
            for index in &order[start..end] {
                let name = &names[*index];
                let candidate = &mut values.as_mut_slice()[*index];
                if multiple_sources && !name.table.is_empty() {
                    candidate.extend_from_slice(name.table)?;
                    candidate.push(b'.')?;
                }
                candidate.extend_from_slice(name.label)?;
                rules.as_mut_slice()[*index].unique = end == start + 1;
            }
            start = end;
        }

        order.as_mut_slice().sort_unstable_by(|a, b| {
            values[*a]
                .as_slice()
                .cmp(values[*b].as_slice())
                .then(a.cmp(b))
        });
        let mut reserved = buffer(scope);
        reserved.reserve(names.len())?;
        start = 0;
        while start < order.len() {
            let first = order[start];
            let mut end = start + 1;
            while end < order.len() && values[order[end]].as_slice() == values[first].as_slice() {
                end += 1;
            }
            for index in &order[start..end] {
                let rule = &mut rules.as_mut_slice()[*index];
                rule.keep = end == start + 1 || rule.unique;
                rule.anchor = first;
                if rule.keep {
                    reserved.push(*index)?;
                }
            }
            start = end;
        }
        // Reserved indexes are already sorted by their unchanged candidates.
        // Only numbered entries mutate below. A generated suffix's final dot
        // uniquely identifies its candidate prefix, so different candidates
        // cannot collide with one another's monotonically increasing numbers.
        for index in 0..names.len() {
            if rules[index].keep {
                continue;
            }
            let anchor = rules[index].anchor;
            let base = values[index].len();
            loop {
                let number = rules[anchor].next;
                rules.as_mut_slice()[anchor].next = number
                    .checked_add(1)
                    .ok_or(KeyError::Invariant("suffix counter overflow"))?;
                let candidate = &mut values.as_mut_slice()[index];
                candidate.truncate(base);
                candidate.push(b'.')?;
                number_into(candidate, number)?;
                let occupied = reserved
                    .binary_search_by(|reserved| {
                        values[*reserved].as_slice().cmp(values[index].as_slice())
                    })
                    .is_ok();
                if !occupied {
                    break;
                }
            }
        }
        order
            .as_mut_slice()
            .sort_unstable_by(|a, b| values[*a].as_slice().cmp(values[*b].as_slice()));
        if order
            .windows(2)
            .any(|pair| values[pair[0]].as_slice() == values[pair[1]].as_slice())
        {
            return Err(KeyError::Invariant("derived keys are not globally unique"));
        }
        Ok(Self { values, order })
    }
    /// Borrow the final key for one actual column.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        self.values.get(index).map(Buffer::as_slice)
    }
    /// Find only an exact final key; ambiguous raw-label fallback is forbidden.
    #[must_use]
    pub fn find(&self, key: &[u8]) -> Option<usize> {
        self.order
            .binary_search_by(|index| self.values[*index].as_slice().cmp(key))
            .ok()
            .map(|position| self.order[position])
    }
}
fn buffer<T>(scope: Option<&Arc<Scope>>) -> Buffer<T> {
    scope.map_or_else(Buffer::new, Buffer::tracked)
}
fn number_into(output: &mut Buffer<u8>, mut number: usize) -> Result<(), MemoryError> {
    let mut bytes = [0; usize::BITS as usize];
    let mut position = bytes.len();
    loop {
        position -= 1;
        bytes[position] = b'0' + (number % 10) as u8;
        number /= 10;
        if number == 0 {
            break;
        }
    }
    output.extend_from_slice(&bytes[position..])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(labels: &[(&str, &str)], expected: &[&str]) {
        let names: Vec<_> = labels
            .iter()
            .map(|(name, table)| Name {
                label: name.as_bytes(),
                table: table.as_bytes(),
                database: b"db",
            })
            .collect();
        let keys = Keys::new(&names, None).unwrap();
        for (index, expected) in expected.iter().enumerate() {
            assert_eq!(keys.get(index), Some(expected.as_bytes()));
            assert_eq!(keys.find(expected.as_bytes()), Some(index));
        }
    }
    #[test]
    fn labels_sources_and_global_reserved_collisions_follow_the_spec() {
        check(&[("user_id", "u")], &["user_id"]);
        check(&[("id", "u"), ("id", "u")], &["id.1", "id.2"]);
        check(&[("id", "u"), ("id", "o")], &["u.id", "o.id"]);
        check(
            &[("id", "u"), ("id", "o"), ("id", "o")],
            &["u.id", "o.id.1", "o.id.2"],
        );
        check(&[("id", "u"), ("id", "")], &["u.id", "id"]);
        check(
            &[("id", "u"), ("id", "u"), ("id", "")],
            &["u.id.1", "u.id.2", "id"],
        );
        check(&[("x", ""), ("x", "")], &["x.1", "x.2"]);
        check(
            &[("id", "u"), ("id", "u"), ("id.1", "")],
            &["id.2", "id.3", "id.1"],
        );
        check(
            &[("id", "u"), ("id", "o"), ("u.id", "")],
            &["u.id.1", "o.id", "u.id"],
        );
        check(&[("", ""), ("", "")], &[".1", ".2"]);
    }
}
