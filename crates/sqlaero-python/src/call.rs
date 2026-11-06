//! Operation-owned CALL output facts, separate from traversal and network I/O.

pub(crate) mod flow;

use crate::{call_output::Snapshot, errors, input::ScalarTypes, output};
use pyo3::prelude::*;
use sqlaero_core::{
    charset::output::OutputCodec,
    error::ClientErrorCode,
    memory::{Buffer, Pool, Reservation, Scope},
    result::{decode::Options, text::RowBatch},
};
use std::{
    mem::size_of,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// One lazily prepared physical-connection namespace, reused for later CALLs.
pub(crate) fn slot_prefix<'a>(
    py: Python<'_>,
    connection: &'a crate::connection::Connection,
) -> PyResult<&'a str> {
    if connection.call_slots.get().is_none() {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).map_err(|error| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                format!("Cannot prepare private CALL variable slots: {error}"),
            )
        })?;
        let mut name = String::new();
        name.try_reserve_exact(32)
            .map_err(|_| errors::memory_error(py))?;
        name.push_str(crate::binding::call::SLOT_NAMESPACE);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in random {
            name.push(char::from(HEX[(byte >> 4) as usize]));
            name.push(char::from(HEX[(byte & 15) as usize]));
        }
        name.push('_');
        let _existing = connection.call_slots.set(name);
    }
    connection
        .call_slots
        .get()
        .map(String::as_str)
        .ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "The private CALL slot namespace could not be retained",
            )
        })
}

/// Fixed control ownership is reserved before target SQL begins. Raw rows stay
/// available if later Python representation fails. No input, Connection or live
/// exception is retained; only a successfully created snapshot can hold types.
pub(crate) struct Output {
    positions: Buffer<usize>,
    options: Options,
    rows: OnceLock<Arc<RowBatch>>,
    codec: OnceLock<OutputCodec>,
    ready: AtomicBool,
    snapshot: OnceLock<Arc<Snapshot>>,
    _owner: Reservation,
}

impl Output {
    pub(crate) fn new(
        py: Python<'_>,
        positions: &[usize],
        options: Options,
        pool: Arc<Pool>,
        limit: usize,
    ) -> PyResult<Arc<Self>> {
        let scope = Scope::new(pool, limit).map_err(|error| output::memory(py, error))?;
        // Reserve this Arc and the future native RowBatch Arc control blocks
        // now; the RowBatch's existing reservation already owns its body.
        let arc_words = if positions.is_empty() { 2 } else { 4 };
        let owner = scope
            .charge(size_of::<Self>() + arc_words * size_of::<usize>())
            .map_err(|error| output::memory(py, error))?;
        let mut indices = Buffer::tracked(&scope);
        indices
            .extend_from_slice(positions)
            .map_err(|error| output::memory(py, error))?;
        Ok(Arc::new(Self {
            positions: indices,
            options,
            rows: OnceLock::new(),
            codec: OnceLock::new(),
            ready: AtomicBool::new(false),
            snapshot: OnceLock::new(),
            _owner: owner,
        }))
    }

    /// Commit one complete output only after Flow has a real successful CALL
    /// tail. Prepared output received before that tail stays with the native
    /// query/Flow; an ERR, truncated tail or timeout must not make it ready.
    pub(crate) fn save(
        &self,
        py: Python<'_>,
        rows: RowBatch,
        codec: Option<OutputCodec>,
    ) -> PyResult<()> {
        if rows.row_count != 1 || rows.columns.len() != self.positions.len() {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "Confirmed CALL output requires one row matching the declared original OUT/INOUT positions",
            ));
        }
        self.rows.set(Arc::new(rows)).map_err(|_| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "This CALL returned more than one output row set",
            )
        })?;
        if let Some(codec) = codec {
            let _same_output = self.codec.set(codec);
        }
        Ok(())
    }

    /// Normal cleanup, or a failed finish with fully received output, can make
    /// the saved values ready. Neither a partial row nor invented NULL is ready.
    pub(crate) fn finish(&self) {
        if self.positions.is_empty() || self.rows.get().is_some() {
            self.ready.store(true, Ordering::Release);
        }
    }
    pub(crate) fn fail(&self) {
        if self.rows.get().is_some() {
            self.ready.store(true, Ordering::Release);
        }
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    pub(crate) fn cached<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyAny>> {
        self.snapshot.get().and_then(|snapshot| snapshot.cached(py))
    }

    /// The caller holds its existing Cursor permit on cache misses. Construction
    /// and Decode are local and never hold native locks across Python calls.
    pub(crate) fn get<'py>(
        &self,
        py: Python<'py>,
        types: &Py<ScalarTypes>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.is_ready() {
            return Ok(py.None().into_bound(py));
        }
        if let Some(cached) = self.cached(py) {
            return Ok(cached);
        }
        if self.snapshot.get().is_none() {
            let prepared = if self.positions.is_empty() {
                Snapshot::empty(types.clone_ref(py))
            } else {
                let rows = self.rows.get().ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "The ready CALL output has no complete native row",
                    )
                })?;
                let codec=self.codec.get().copied().ok_or_else(||errors::client(py,
                    ClientErrorCode::UnsupportedCharset,
                    "The CALL output's actual metadata encoding is unknown or unsupported; no earlier encoding was substituted"))?;
                let mut indices = Buffer::tracked(rows.scope());
                indices
                    .extend_from_slice(&self.positions)
                    .map_err(|error| output::memory(py, error))?;
                Snapshot::new(
                    py,
                    Arc::clone(rows),
                    indices,
                    self.options,
                    codec,
                    types.clone_ref(py),
                )?
            };
            let _already_published = self.snapshot.set(Arc::new(prepared));
        }
        self.snapshot
            .get()
            .ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The CALL output snapshot could not be retained",
                )
            })?
            .get(py)
    }
    pub(crate) fn traverse(
        &self,
        visit: &pyo3::class::PyVisit<'_>,
    ) -> Result<(), pyo3::class::PyTraverseError> {
        if let Some(snapshot) = self.snapshot.get() {
            snapshot.traverse(visit)?;
        }
        Ok(())
    }
}
