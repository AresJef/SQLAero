//! Ordered, owned native result sets, independent of host output objects.

use super::{decode::Options, text::Response};
use crate::charset::output::OutputCodec;
use crate::memory::{Buffer, MemoryError};
use crate::session::query::QueryError;
use std::sync::Arc;

/// A complete trusted set and the output policy fixed for its execution.
pub struct Set {
    /// Actual native response, including independent no-row states.
    pub response: Response,
    /// The Cursor's selected decoding policy when this operation began.
    pub options: Options,
    /// Confirmed metadata-label encoding for this response.
    pub names_codec: OutputCodec,
    /// Zero-based actual request index; result count does not imply SQL count.
    pub request_index: usize,
    /// Zero-based actual set index within that request.
    pub set_index: usize,
}

/// An actual native error at the boundary after a trusted result prefix.
/// No Python exception, traceback, callback, or execution replay is retained.
pub struct Failure {
    /// The original typed server, transport, protocol or resource failure.
    pub error: Arc<QueryError>,
    /// A codec only when it is actually certified for the ERR bytes.
    pub codec: Option<OutputCodec>,
}

/// Current operation's ordered native owners. The ordinary one-set case keeps
/// its first set inline and allocates no list or registry. Later entries are
/// capacity-charged to the same result pool as their actual native response.
#[derive(Default)]
pub struct Chain {
    first: Option<Set>,
    later: Buffer<Option<Set>>,
    count: usize,
    next: usize,
    later_base: usize,
    failure: Option<Failure>,
}

impl Chain {
    /// Append a fully verified set without copying rows or preparing Python.
    ///
    /// # Errors
    /// Preserves the actual allocation or quota failure without discarding the
    /// already-owned prefix. The caller still owns this operation's main error.
    pub fn push(&mut self, set: Set) -> Result<(), MemoryError> {
        if self.count == 0 {
            self.first = Some(set);
        } else {
            if self.later.capacity() == 0 {
                self.later = Buffer::tracked(set.response.scope());
                self.later_base = self.count;
            }
            self.later.push(Some(set))?;
        }
        self.count += 1;
        Ok(())
    }

    /// Take the next complete set in source/request order. Previously taken
    /// entries retain no row owner; their consumer controls its own lifetime.
    pub fn take_next(&mut self) -> Option<Set> {
        let set = if self.next == 0 {
            self.first.take()
        } else {
            self.later
                .as_mut_slice()
                .get_mut(self.next.checked_sub(self.later_base)?)
                .and_then(Option::take)
        };
        if set.is_some() {
            self.next += 1;
            if self.next == self.count {
                self.later = Buffer::new();
            }
        }
        set
    }

    /// Save only one terminal native cause; repeated reads do not append an
    /// exception history or alter a previously returned host snapshot.
    pub fn fail(&mut self, failure: Failure) {
        self.failure = Some(failure);
    }

    /// Borrow the terminal failure only after all complete sets were consumed.
    #[must_use]
    pub fn failure(&self) -> Option<&Failure> {
        (self.next == self.count)
            .then_some(self.failure.as_ref())
            .flatten()
    }
}
