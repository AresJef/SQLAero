//! Bounded successful SQL text labels, owned by one MySQL row operation.
//!
//! Readers supply validated native coordinates. This target interprets no
//! Python types, dictionary keys or source conversion protocols.
use crate::errors;
use crate::input::arrow::consume_text_reference;
use crate::input::cell::{TextIdentity, TextReference};
use crate::input::control::Checkpoint;
use crate::input::source::CellWriter;
use pyo3::prelude::*;
use sqlaero_core::bind::{Context, writer};

const DOMAINS: usize = 8;
const LABELS: usize = 64;
const BYTES: usize = 16 * 1024;
struct Labels {
    identity: TextIdentity,
    encoded: Vec<Option<Vec<u8>>>,
    bytes: usize,
}
/// Only owned Rust bytes and nonreused native identities survive a write.
/// No source buffer, Python reference or release callback is retained here.
pub(super) struct Cache {
    context: Option<(&'static str, bool)>,
    domains: Vec<Option<Labels>>,
    bytes: usize,
}
impl Cache {
    pub(super) fn new() -> Self {
        Self {
            context: None,
            domains: Vec::new(),
            bytes: 0,
        }
    }
    pub(super) fn eligible(reference: &TextReference<'_>) -> bool {
        reference.identity.is_some()
            && reference.channel < DOMAINS
            && reference.values.len() <= LABELS
    }
    pub(super) fn write(
        &mut self,
        py: Python<'_>,
        reference: TextReference<'_>,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let Some(identity) = reference.identity else {
            return consume_text_reference(
                py,
                reference,
                &mut crate::input::cell::Mysql {
                    output,
                    context,
                    visitor,
                },
                budget,
            );
        };
        let key = (context.codec.name(), context.no_backslash_escapes);
        if self.context != Some(key) {
            self.domains.clear();
            self.bytes = 0;
            self.context = Some(key);
        }
        if self.domains.is_empty() {
            self.domains
                .try_reserve_exact(DOMAINS)
                .map_err(|_| errors::memory_error(py))?;
            self.domains.resize_with(DOMAINS, || None);
        }
        // Tables retain a direct positional lookup. Independent provider
        // columns can also reuse their own retained native value domain.
        let preferred = reference.channel;
        let owns = |slot: &Option<Labels>| {
            slot.as_ref()
                .is_some_and(|labels| labels.identity == identity)
        };
        let slot = if owns(&self.domains[preferred]) {
            preferred
        } else if let Some(index) = self.domains.iter().position(owns) {
            index
        } else {
            let index = self
                .domains
                .iter()
                .position(Option::is_none)
                .unwrap_or(preferred);
            let mut encoded = Vec::new();
            encoded
                .try_reserve_exact(reference.values.len())
                .map_err(|_| errors::memory_error(py))?;
            encoded.resize_with(reference.values.len(), || None);
            if let Some(old) = self.domains[index].take() {
                self.bytes -= old.bytes;
            }
            self.domains[index] = Some(Labels {
                identity,
                encoded,
                bytes: 0,
            });
            index
        };
        let labels = self.domains[slot].as_mut().ok_or_else(|| {
            errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::InternalState,
                "SQL native label domain is unavailable",
            )
        })?;
        if let Some(encoded) = &labels.encoded[reference.index] {
            visitor.checkpoint(py, encoded.len())?;
            return writer::append(output, encoded, context.limit)
                .map_err(|error| errors::wire(py, error));
        }
        let start = output.len();
        consume_text_reference(
            py,
            reference,
            &mut crate::input::cell::Mysql {
                output,
                context,
                visitor,
            },
            budget,
        )?;
        let length = output.len() - start;
        if length <= BYTES - self.bytes {
            let mut encoded = Vec::new();
            encoded
                .try_reserve_exact(length)
                .map_err(|_| errors::memory_error(py))?;
            encoded.extend_from_slice(&output[start..]);
            labels.encoded[reference.index] = Some(encoded);
            labels.bytes += length;
            self.bytes += length;
        }
        Ok(())
    }
}
