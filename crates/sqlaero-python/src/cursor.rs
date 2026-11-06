//! Thin public Cursor over one native incremental request and buffered Reader.

use crate::{
    binding,
    config::{self, Argument},
    connection::{CommandRole, Connection},
    errors,
    input::{self, ScalarTypes},
    metadata, output,
    reader::Reader,
};
use num_bigint::BigUint;
use pyo3::exceptions::PyStopIteration;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::{
    charset::output::OutputCodec,
    error::ClientErrorCode,
    result::{
        chain::{Chain, Set as NativeSet},
        decode::Options,
        text::Response,
    },
    session::query::ConfirmedPrefix,
    session::{
        query::{CommandConfirmation, Query},
        shared::CommandLease,
    },
    transport::deadline::IoBudget,
};
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
struct Results {
    pending: Chain,
    batch: Option<u128>,
    failed: bool,
    started: bool,
    failure_note: Option<std::sync::Arc<errors::SavedFailure>>,
    stream: Option<crate::stream::Stream>,
    call_output: Option<std::sync::Arc<crate::call::Output>>,
}
#[derive(Default)]
struct Collected {
    chain: Chain,
    prefix: ConfirmedPrefix,
    total: u128,
    total_unknown: bool,
    final_first_known: bool,
    statement_invalidated: bool,
}
impl Collected {
    /// Both ordinary requests and managed CALL count only their first public
    /// set. Private protocol/variable stages never enter this boundary.
    fn first_response(
        &mut self,
        py: Python<'_>,
        response: &Response,
        final_request: bool,
    ) -> PyResult<CommandConfirmation> {
        let confirmation = CommandConfirmation::from_response(response);
        self.first_confirmation(py, confirmation, final_request)?;
        Ok(confirmation)
    }

    fn first_confirmation(
        &mut self,
        py: Python<'_>,
        confirmation: CommandConfirmation,
        final_request: bool,
    ) -> PyResult<()> {
        self.total = self
            .total
            .checked_add(confirmation.count())
            .ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "batch first-result row count overflow",
                )
            })?;
        self.final_first_known |= final_request;
        Ok(())
    }
}

struct Publication {
    collected: Collected,
    live: Option<crate::stream::Stream>,
    result: PyResult<()>,
    call_output: Option<std::sync::Arc<crate::call::Output>>,
}

/// Restore the native stream at every ordinary exit. Unwinding retires its
/// Cursor and lets the unfinished native token disable the wire without I/O.
struct HeldStream<'a> {
    cursor: &'a CursorCore,
    value: Option<crate::stream::Stream>,
}

/// Keep the one current prepared handle outside state locks during Python
/// checkpoints and wire work. A mandatory concurrent retirement prevents it
/// from being restored and lets its native Drop record safe release instead.
struct HeldStatement<'a> {
    cursor: &'a CursorCore,
    value: Option<crate::statement::Cached>,
}
impl Drop for HeldStatement<'_> {
    fn drop(&mut self) {
        let Some(value) = self.value.take() else {
            return;
        };
        if self.cursor.reader.is_closed() {
            return;
        }
        let old = self
            .cursor
            .statement
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(value);
        drop(old);
    }
}
impl HeldStream<'_> {
    fn get(&mut self, py: Python<'_>) -> PyResult<&mut crate::stream::Stream> {
        self.value.as_mut().ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "stream owner was already released",
            )
        })
    }
}
impl Drop for HeldStream<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.cursor.reader.retire();
        }
        let Some(mut value) = self.value.take() else {
            return;
        };
        value.returned();
        if self.cursor.reader.is_closed() {
            return;
        }
        let old = {
            let mut state = self
                .cursor
                .results
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.stream.replace(value)
        };
        drop(old);
    }
}

struct Settings {
    options: Options,
    arraysize: BigUint,
}
#[derive(Clone, Copy)]
struct Counts {
    rows: u128,
    last_id: Option<u64>,
    warnings: u128,
}
/// Independent buffered result and read position. Commands use their owning
/// Connection; local fetches never acquire the connection's network lease.
pub(crate) struct CursorCore {
    pub(crate) connection: Py<Connection>,
    reader: Reader,
    settings: Mutex<Settings>,
    counts: Mutex<Option<Counts>>,
    results: Mutex<Results>,
    streaming: bool,
    prepared: bool,
    statement: Mutex<Option<crate::statement::Cached>>,
}
/// Buffered tuple rows over the common native result implementation.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Cursor {
    core: CursorCore,
}
/// Buffered dictionary rows over the same native result implementation.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct DictCursor {
    core: CursorCore,
}
/// Tuple rows received incrementally from the original native response.
/// Prepared callproc with explicit OUT/INOUT may look ahead at most two native
/// rows to identify the actual output flag; ordinary queries return at headers.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct SSCursor {
    core: CursorCore,
}
/// Dictionary rows received incrementally through the same native response.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct SSDictCursor {
    core: CursorCore,
}
impl std::ops::Deref for SSCursor {
    type Target = CursorCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
impl std::ops::Deref for SSDictCursor {
    type Target = CursorCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
impl std::ops::Deref for Cursor {
    type Target = CursorCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
impl std::ops::Deref for DictCursor {
    type Target = CursorCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

/// Typed owners let transaction cleanup use Rust dispatch rather than Python
/// method lookup, arbitrary subclasses or a second result implementation.
pub(crate) enum CursorHandle {
    Tuple(Py<Cursor>),
    Dictionary(Py<DictCursor>),
    StreamingTuple(Py<SSCursor>),
    StreamingDictionary(Py<SSDictCursor>),
}
impl CursorHandle {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            Self::Tuple(value) => Self::Tuple(value.clone_ref(py)),
            Self::Dictionary(value) => Self::Dictionary(value.clone_ref(py)),
            Self::StreamingTuple(value) => Self::StreamingTuple(value.clone_ref(py)),
            Self::StreamingDictionary(value) => Self::StreamingDictionary(value.clone_ref(py)),
        }
    }
    pub(crate) fn object(&self, py: Python<'_>) -> Py<PyAny> {
        match self {
            Self::Tuple(value) => value.clone_ref(py).into_any(),
            Self::Dictionary(value) => value.clone_ref(py).into_any(),
            Self::StreamingTuple(value) => value.clone_ref(py).into_any(),
            Self::StreamingDictionary(value) => value.clone_ref(py).into_any(),
        }
    }
    pub(crate) fn close(&self, py: Python<'_>) -> PyResult<()> {
        match self {
            Self::Tuple(value) => value.borrow(py).close(py),
            Self::Dictionary(value) => value.borrow(py).close(py),
            Self::StreamingTuple(value) => value.borrow(py).close(py),
            Self::StreamingDictionary(value) => value.borrow(py).close(py),
        }
    }
    pub(crate) fn retire(&self, py: Python<'_>) {
        match self {
            Self::Tuple(value) => value.borrow(py).retire(),
            Self::Dictionary(value) => value.borrow(py).retire(),
            Self::StreamingTuple(value) => value.borrow(py).retire(),
            Self::StreamingDictionary(value) => value.borrow(py).retire(),
        }
    }
    pub(crate) fn finish_response(&self, py: Python<'_>, budget: &IoBudget) -> PyResult<()> {
        match self {
            Self::Tuple(value) => value.borrow(py).finish_response(py, budget),
            Self::Dictionary(value) => value.borrow(py).finish_response(py, budget),
            Self::StreamingTuple(value) => value.borrow(py).finish_response(py, budget),
            Self::StreamingDictionary(value) => value.borrow(py).finish_response(py, budget),
        }
    }
}
impl CursorCore {
    fn hold_stream(&self, py: Python<'_>) -> PyResult<Option<HeldStream<'_>>> {
        let value = self.results(py)?.stream.take();
        let Some(value) = value else {
            return Ok(None);
        };
        let mut held = HeldStream {
            cursor: self,
            value: Some(value),
        };
        held.get(py)?.enter(py)?;
        Ok(Some(held))
    }

    fn hold_statement(&self, py: Python<'_>) -> PyResult<HeldStatement<'_>> {
        let value = self
            .statement
            .lock()
            .map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The Cursor's prepared handle slot is unavailable",
                )
            })?
            .take();
        Ok(HeldStatement {
            cursor: self,
            value,
        })
    }

    fn stream_event(
        &self,
        py: Python<'_>,
        stream: &mut crate::stream::Stream,
        event: crate::stream::Event,
    ) -> PyResult<()> {
        use sqlaero_core::result::text::StreamEvent;
        match event.value {
            StreamEvent::Header(header) => {
                let codec=event.codec.ok_or_else(||errors::client(py,ClientErrorCode::UnsupportedCharset,"The current streaming result's metadata encoding is unknown or unsupported; no older encoding was substituted"))?;
                stream.rows = Some(
                    self.reader
                        .stream_header(py, header, stream.options, codec)?,
                );
                *self.counters(py)? = None;
            }
            StreamEvent::Rows(batch) => {
                let rows = stream.rows.as_mut().ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "stream rows have no current header",
                    )
                })?;
                rows.push(batch)
                    .map_err(|error| output::memory(py, error))?;
            }
            StreamEvent::End {
                tail,
                completion,
                total_rows,
                ..
            } => {
                let count = total_rows.map_or_else(
                    || u128::from(completion.affected_rows()),
                    |count| count as u128,
                );
                *self.counters(py)? = Some(Counts {
                    rows: count,
                    last_id: completion.last_insert_id(),
                    warnings: u128::from(completion.warnings()),
                });
                self.results(py)?.batch = stream.batch;
                if let Some((before, after)) = event.charset_change {
                    let connection = self.connection.borrow(py);
                    return Err(stream.charset_failure(py, &connection, before, after));
                }
                if let Some(rows) = &mut stream.rows {
                    if let Some(tail) = tail {
                        rows.push(tail).map_err(|error| output::memory(py, error))?;
                    }
                    rows.finish(total_rows.ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "row stream ended as a command result",
                        )
                    })?);
                } else {
                    self.reader.clear_result(py)?;
                }
            }
        }
        Ok(())
    }

    fn save_stream_failure(
        &self,
        py: Python<'_>,
        error: &PyErr,
        invalidates_statement: bool,
    ) -> PyResult<()> {
        if invalidates_statement {
            let old = self
                .statement
                .lock()
                .map_err(|_| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "The prepared handle slot is unavailable",
                    )
                })?
                .take();
            drop(old);
        }
        let snapshot = errors::SavedFailure::capture(py, error)
            .ok()
            .map(std::sync::Arc::new);
        let previous = {
            let mut state = self.results(py)?;
            state.failed = true;
            std::mem::replace(&mut state.failure_note, snapshot)
        };
        drop(previous);
        Ok(())
    }

    /// A failed local set publication is a terminal navigation boundary, not
    /// permission to skip to a later set. Keep the immediate error unchanged;
    /// snapshots may fail independently and cannot retain arbitrary traceback.
    fn save_set_failure(&self, py: Python<'_>, error: &PyErr) -> PyResult<()> {
        let snapshot = errors::SavedFailure::capture(py, error)
            .ok()
            .map(std::sync::Arc::new);
        let previous = {
            let mut state = self.results(py)?;
            state.failed = true;
            (
                std::mem::take(&mut state.pending),
                std::mem::replace(&mut state.failure_note, snapshot),
            )
        };
        // A diagnostic can own Python references. Release them after the lock.
        drop(previous);
        Ok(())
    }

    fn drain_current(
        &self,
        py: Python<'_>,
        connection: &Connection,
        stream: &mut crate::stream::Stream,
    ) -> PyResult<()> {
        if stream.failed {
            return Ok(());
        }
        let Some(rows) = &mut stream.rows else {
            return Ok(());
        };
        rows.discard();
        if rows.is_complete() {
            return Ok(());
        }
        stream.discard_current(py)?;
        let event = stream.advance(py, connection, usize::MAX)?;
        match event.value {
            sqlaero_core::result::text::StreamEvent::End {
                tail,
                completion,
                total_rows,
                metadata_status,
            } => {
                drop(tail);
                self.stream_event(
                    py,
                    stream,
                    crate::stream::Event {
                        value: sqlaero_core::result::text::StreamEvent::End {
                            tail: None,
                            completion,
                            total_rows,
                            metadata_status,
                        },
                        codec: None,
                        charset_change: event.charset_change,
                    },
                )?;
                Ok(())
            }
            _ => Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "discarding the current row result produced a nonterminal delivery",
            )),
        }
    }

    fn next_stream(&self, py: Python<'_>) -> PyResult<Option<bool>> {
        let Some(mut held) = self.hold_stream(py)? else {
            self.reader.end_set(py)?;
            return Ok(None);
        };
        let stream = held.get(py)?;
        let connection = self.connection.borrow(py);
        let result = (|| {
            if connection.native.is_closed() && !stream.finished() {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "Cannot switch to an unfinished streaming result because its Connection is closed; complete local results remain independently readable",
                ));
            }
            self.reader.end_set(py)?;
            self.drain_current(py, &connection, stream)?;
            stream.rows = None;
            if stream.pending.is_none() && stream.finished() {
                return Ok(None);
            }
            let Some(event) = stream.advance_optional(py, &connection, 1)? else {
                return Ok(None);
            };
            if matches!(
                event.value,
                sqlaero_core::result::text::StreamEvent::Rows(_)
            ) {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "next result has rows without a column header",
                ));
            }
            self.stream_event(py, stream, event)?;
            Ok(Some(true))
        })();
        if let Err(error) = &result {
            self.save_stream_failure(py, error, stream.statement_invalidated)?;
        }
        result
    }

    fn close_cursor(&self, py: Python<'_>) -> PyResult<()> {
        let _operation = self.reader.operation(py)?;
        let mut stream = self.results(py)?.stream.take();
        let connection = self.connection.borrow(py);
        let result = (|| {
            if let Some(stream) = &mut stream {
                stream.enter(py)?;
                if !connection.native.is_closed() {
                    self.drain_current(py, &connection, stream)?;
                    stream.rows = None;
                    while stream.pending.is_some() || !stream.finished() {
                        let Some(event) = stream.advance_optional(py, &connection, usize::MAX)?
                        else {
                            break;
                        };
                        if matches!(
                            event.value,
                            sqlaero_core::result::text::StreamEvent::Header(_)
                        ) {
                            stream.discard_current(py)?;
                        }
                        // Unselected later results never replace current
                        // metadata/counters and never materialize Python rows.
                    }
                }
            }
            Ok(())
        })();
        if let Some(stream) = &stream {
            self.results(py)?.batch = stream.batch;
        }
        let handle = self
            .statement
            .lock()
            .map_err(|_| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The prepared handle slot is unavailable",
                )
            })?
            .take();
        let had_handle = handle.is_some();
        drop(handle);
        let release: PyResult<()> = (|| {
            if !had_handle || connection.native.is_closed() {
                return Ok(());
            }
            let mut lease = match connection.native.try_command() {
                Ok(lease) => lease,
                Err(sqlaero_core::session::shared::AccessError::Busy) => return Ok(()),
                Err(sqlaero_core::session::shared::AccessError::Closed) => return Ok(()),
                Err(error) => return Err(errors::access(py, error)),
            };
            let mut local = connection.runtime(py)?.timeouts.operation();
            let budget = stream
                .as_mut()
                .map_or(&mut local, |stream| &mut stream.budget);
            crate::statement::flush(py, &connection, &mut lease, budget)
        })();
        let result = match (result, release) {
            (Err(primary), Err(cleanup)) => {
                errors::additional(py, &primary, cleanup, "cursor.statement_close");
                Err(primary)
            }
            (Ok(()), result) | (result, Ok(())) => result,
        };
        self.reader.retire();
        let old = {
            let mut state = self.results(py)?;
            (
                state.failure_note.take(),
                std::mem::take(&mut state.pending),
            )
        };
        drop(old);
        drop(stream);
        result
    }

    pub(crate) fn finish_response(&self, py: Python<'_>, enclosing: &IoBudget) -> PyResult<()> {
        // Buffered/local reads do not acquire a network-cleanup operation and
        // therefore cannot block a legal transaction command on their own.
        let needs_wire = self
            .results(py)?
            .stream
            .as_ref()
            .is_some_and(|stream| !stream.finished());
        if !needs_wire {
            return Ok(());
        }
        let _operation = self.reader.operation(py)?;
        let Some(mut held) = self.hold_stream(py)? else {
            return Ok(());
        };
        let stream = held.get(py)?;
        let connection = self.connection.borrow(py);
        if connection.native.is_closed() || stream.finished() {
            return Ok(());
        }
        if let Err(error) = stream.budget.cap_total(enclosing) {
            stream.abort(&connection);
            return Err(errors::timeout(py, error));
        }
        let result = (|| {
            self.drain_current(py, &connection, stream)?;
            stream.rows = None;
            while stream.pending.is_some() || !stream.finished() {
                let Some(event) = stream.advance_optional(py, &connection, usize::MAX)? else {
                    break;
                };
                if matches!(
                    event.value,
                    sqlaero_core::result::text::StreamEvent::Header(_)
                ) {
                    stream.discard_current(py)?;
                }
            }
            Ok(())
        })();
        self.results(py)?.batch = stream.batch;
        if let Err(error) = &result {
            self.save_stream_failure(py, error, stream.statement_invalidated)?;
        }
        result
    }

    fn stream_scroll(&self, py: Python<'_>, value: &Bound<'_, PyAny>, mode: &str) -> PyResult<()> {
        let _operation = self.reader.operation(py)?;
        self.reader.open(py)?;
        let absolute = match mode {
            "relative" => false,
            "absolute" => true,
            _ => {
                return Err(errors::builtin_message::<pyo3::exceptions::PyValueError>(
                    py,
                    "scroll mode must be 'relative' or 'absolute'",
                ));
            }
        };
        if value.is_instance_of::<pyo3::types::PyBool>() {
            return Err(errors::builtin_message::<pyo3::exceptions::PyTypeError>(
                py,
                "scroll value must support __index__; bool is not accepted",
            ));
        }
        // SAFETY: the attached public index protocol returns an owned exact int
        // or its original exception. No state lock or native view crosses it.
        let index = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Index(value.as_ptr()))?
        };
        let offset=index.extract::<i128>().map_err(|error| {
            if error.is_instance_of::<pyo3::exceptions::PyOverflowError>(py) {errors::builtin_message::<pyo3::exceptions::PyIndexError>(py,"scroll target exceeds the representable native row range; the position is unchanged")} else {error}
        })?;
        let mut held = self.hold_stream(py)?;
        let current = if let Some(held) = &mut held {
            held.get(py)?
                .rows
                .as_ref()
                .map_or(self.reader.position(py)?, |rows| rows.position())
        } else {
            self.reader.position(py)?
        };
        let target = if absolute {
            Some(offset)
        } else {
            (current as i128).checked_add(offset)
        };
        let target = target
            .filter(|target| *target >= 0)
            .and_then(|target| usize::try_from(target).ok())
            .ok_or_else(|| {
                errors::builtin_message::<pyo3::exceptions::PyIndexError>(
                    py,
                    "scroll target is outside the native row range; the position is unchanged",
                )
            })?;
        if target < current {
            return Err(errors::client(
                py,
                ClientErrorCode::UnsupportedCapability,
                "Streaming cursors support forward navigation only; no rows were replayed",
            ));
        }
        let Some(held) = &mut held else {
            return self.reader.forward_to(py, target);
        };
        let stream = held.get(py)?;
        if stream.rows.is_none() {
            return self.reader.forward_to(py, target);
        }
        let connection = self.connection.borrow(py);
        loop {
            let rows = stream.rows.as_mut().ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "stream row owner is unavailable",
                )
            })?;
            if !rows.is_complete() && connection.native.is_closed() {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "Cannot scroll an unfinished streaming result after its Connection closed",
                ));
            }
            let remaining = target.saturating_sub(rows.position());
            rows.skip(remaining);
            if rows.position() == target {
                return Ok(());
            }
            if rows.is_complete() {
                return Err(errors::builtin_message::<pyo3::exceptions::PyIndexError>(
                    py,
                    format!(
                        "scroll target {target} is beyond EOF; the streaming cursor actually skipped to position {}",
                        rows.position()
                    ),
                ));
            }
            let remaining = target.saturating_sub(rows.position());
            self.receive_stream_event(py, &connection, stream, remaining)?;
        }
    }

    fn receive_stream_event(
        &self,
        py: Python<'_>,
        connection: &Connection,
        stream: &mut crate::stream::Stream,
        remaining: usize,
    ) -> PyResult<()> {
        let result = stream
            .reserve(py, connection)
            .and_then(|()| stream.advance(py, connection, remaining))
            .and_then(|event| self.stream_event(py, stream, event));
        if result.is_err() && stream.batch.is_some() {
            // A complete first terminator remains a real contribution even
            // when saving its response fails before an End can be published.
            self.results(py)?.batch = stream.batch;
        }
        if let Err(error) = &result
            && !stream.resource_paused
        {
            self.save_stream_failure(py, error, stream.statement_invalidated)?;
        }
        result
    }

    fn stream_fetch<'py>(
        &self,
        py: Python<'py>,
        count: Option<usize>,
        one: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.reader.operation(py)?;
        self.reader.open(py)?;
        let Some(mut held) = self.hold_stream(py)? else {
            return self.reader.read_owned(py, count, one);
        };
        let stream = held.get(py)?;
        if stream.rows.is_none() {
            let output = self.reader.read_owned(py, count, one)?;
            self.reader.release_consumed();
            return Ok(output);
        }
        if stream.rows.as_ref().is_some_and(|rows| !rows.is_complete()) {
            let (failed, snapshot) = {
                let state = self.results(py)?;
                (
                    state.failed,
                    state.failure_note.as_ref().map(std::sync::Arc::clone),
                )
            };
            if let Some(snapshot) = snapshot {
                return Err(snapshot.restore(py));
            }
            if failed {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "The original stream failure could not be saved for reconstruction; this is a retained diagnostic limitation, not a new database failure",
                ));
            }
        }
        let connection = self.connection.borrow(py);
        loop {
            let rows = stream.rows.as_ref().ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "stream row owner is unavailable",
                )
            })?;
            if !rows.is_complete() && connection.native.is_closed() {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "Cannot fetch an unfinished streaming result because its Connection is closed; cached partial rows are not a complete local result",
                ));
            }
            if rows.is_complete() || count.is_some_and(|count| rows.available() >= count) {
                break;
            }
            let remaining =
                count.map_or(usize::MAX, |count| count.saturating_sub(rows.available()));
            self.receive_stream_event(py, &connection, stream, remaining)?;
        }
        let rows = stream.rows.as_mut().ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "stream rows were released before output",
            )
        })?;
        let incomplete = !rows.is_complete();
        let local = IoBudget::new(
            sqlaero_core::transport::deadline::TimeoutKind::Operation,
            None,
            None,
            None,
        );
        let budget = if incomplete { &stream.budget } else { &local };
        let result=self.reader.stream_read(py,rows,count,one,budget,|| {
            if incomplete && connection.native.is_closed() {Err(errors::client(py,ClientErrorCode::SessionState,"The Connection closed during conversion of an unfinished streaming result; the fetch position was not advanced"))} else {Ok(())}
        });
        if result.is_err() && incomplete && stream.budget.check().is_err() {
            stream.abort(&connection);
        }
        result
    }
    /// Stop future operations after mandatory transaction cleanup failed. Any
    /// already active Reader operation retains and releases its own data.
    pub(crate) fn retire(&self) {
        self.reader.retire();
        let statement = self
            .statement
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(statement);
        let old = {
            let mut state = match self.results.lock() {
                Ok(state) => state,
                Err(error) => error.into_inner(),
            };
            (
                state.failure_note.take(),
                std::mem::take(&mut state.pending),
                state.stream.take(),
            )
        };
        // An idle unread stream retires its wire owner here. If another Reader
        // operation owns it, HeldStream observes retirement on return instead.
        // Python diagnostic objects and native buffers are dropped unlocked.
        drop(old);
    }

    fn with_format(
        connection: Py<Connection>,
        types: Py<ScalarTypes>,
        options: Options,
        dictionary: bool,
        streaming: bool,
        prepared: bool,
    ) -> Self {
        Self {
            connection,
            reader: Reader::with_format(types, dictionary),
            settings: Mutex::new(Settings {
                options,
                arraysize: BigUint::from(1u8),
            }),
            counts: Mutex::new(None),
            results: Mutex::new(Results::default()),
            streaming,
            prepared,
            statement: Mutex::new(None),
        }
    }
    pub(crate) fn create(
        py: Python<'_>,
        connection: Py<Connection>,
        types: Py<ScalarTypes>,
        options: Options,
        dictionary: bool,
        streaming: bool,
        prepared: bool,
    ) -> PyResult<CursorHandle> {
        let core = Self::with_format(connection, types, options, dictionary, streaming, prepared);
        match (streaming, dictionary) {
            (false, false) => Py::new(py, Cursor { core }).map(CursorHandle::Tuple),
            (false, true) => Py::new(py, DictCursor { core }).map(CursorHandle::Dictionary),
            (true, false) => Py::new(py, SSCursor { core }).map(CursorHandle::StreamingTuple),
            (true, true) => {
                Py::new(py, SSDictCursor { core }).map(CursorHandle::StreamingDictionary)
            }
        }
    }
    fn settings(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Settings>> {
        self.settings.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor configuration is unavailable after a native failure",
            )
        })
    }
    fn counters(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Option<Counts>>> {
        self.counts.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor counters are unavailable after a native failure",
            )
        })
    }
    fn modifying(&self, py: Python<'_>, property: &str) -> PyResult<()> {
        if self.reader.is_closed() {
            Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                format!(
                    "Cannot modify Cursor.{property} because this cursor is closed or retired. Its saved value remains readable."
                ),
            ))
        } else {
            Ok(())
        }
    }
    fn results(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Results>> {
        self.results.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor results are unavailable after a native state failure",
            )
        })
    }
    fn publish_set(&self, py: Python<'_>, set: NativeSet) -> PyResult<()> {
        let completion = set.response.completion();
        let counts = Counts {
            rows: match &set.response {
                Response::Command(ok) => u128::from(ok.affected_rows()),
                Response::Rows(rows) => rows.row_count as u128,
                Response::Counted { rows, .. } => *rows as u128,
            },
            last_id: completion.last_insert_id(),
            warnings: u128::from(completion.warnings()),
        };
        match set.response {
            Response::Rows(rows) => self
                .reader
                .install(py, rows, set.options, set.names_codec)?,
            Response::Command(_) => self.reader.clear_result(py)?,
            Response::Counted { .. } => {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "A discarded internal result cannot be published",
                ));
            }
        }
        *self.counters(py)? = Some(counts);
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "one concrete request shares the operation's existing owners and policy"
    )]
    fn receive_request(
        &self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        mut job: Query,
        budget: &mut IoBudget,
        source_rows: std::ops::Range<usize>,
        final_request: bool,
        codec: OutputCodec,
        options: Options,
        collected: &mut Collected,
    ) -> PyResult<()> {
        let request_index = collected.prefix.commands;
        let mut first = None;
        let mut set_index = 0;
        let mut current_codec = Some(codec);
        loop {
            let response = match connection.drive_set(
                py,
                lease,
                &mut job,
                budget,
                CommandRole::UserSql,
            ) {
                Ok(response) => response,
                Err(failure) => {
                    if let Some(native) = failure.native_failure() {
                        collected.statement_invalidated |=
                            crate::statement::invalidates(&native.error);
                        collected.chain.fail(native);
                    }
                    let recovery_error = match (first, failure.confirmation()) {
                        (None, Some(confirmation)) => {
                            match collected.first_confirmation(py, confirmation, final_request) {
                                Ok(()) => {
                                    first = Some(confirmation);
                                    None
                                }
                                Err(error) => Some(error),
                            }
                        }
                        _ => None,
                    };
                    collected.total_unknown |= !final_request || first.is_none();
                    let error = failure.record_prefix(
                        py,
                        &mut collected.prefix,
                        source_rows.clone(),
                        first,
                    );
                    if let Some(secondary) = recovery_error {
                        errors::additional(py, &error, secondary, "cursor.first_confirmation");
                    }
                    errors::note_format(
                        py,
                        &error,
                        format_args!(
                            "while receiving request {request_index}, result set {set_index}; statement index is not inferred from result count"
                        ),
                    );
                    return Err(error);
                }
            };
            if first.is_none() {
                first = Some(collected.first_response(py, &response, final_request)?);
            }
            let finished = job.is_finished();
            if finished {
                collected
                    .prefix
                    .record_confirmation(
                        source_rows.clone(),
                        first.ok_or_else(|| {
                            errors::client(
                                py,
                                ClientErrorCode::InternalState,
                                "a completed command has no first result",
                            )
                        })?,
                    )
                    .map_err(|error| errors::wire(py, error))?;
            }
            // The common driver already applies tracked changes. Refresh only
            // when the actual terminator reports one, keeping ordinary rows
            // free from another facts lock while not freezing a whole request.
            let names_codec = (|| {
                if response.completion().status_flags().contains(
                    sqlaero_core::protocol::StatusFlags::SERVER_SESSION_STATE_CHANGED,
                ) {
                    current_codec = connection
                        .native
                        .facts
                        .lock()
                        .map_err(|_| {
                            errors::client(
                                py,
                                ClientErrorCode::InternalState,
                                "The completed result's metadata facts are unavailable",
                            )
                        })?
                        .metadata_codec();
                }
                if matches!(&response, Response::Rows(_)) {
                    current_codec.ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::UnsupportedCharset,
                            "The completed result's actual metadata encoding is unknown; the request-entry encoding was not substituted",
                        )
                    })
                } else {
                    Ok(codec)
                }
            })()
            .inspect_err(|_| {
                if !finished {
                    lease.invalidate();
                }
                collected.total_unknown |= !final_request;
            })?;
            let result = collected.chain.push(NativeSet {
                response,
                options,
                names_codec,
                request_index,
                set_index,
            });
            if let Err(error) = result {
                if !job.is_finished() {
                    lease.invalidate();
                }
                collected.total_unknown |= !final_request;
                return Err(output::memory(py, error));
            }
            if finished {
                return Ok(());
            }
            set_index += 1;
            if let Err(error) = input::check(py, budget) {
                lease.invalidate();
                collected.total_unknown |= !final_request;
                return Err(error);
            }
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "a CALL consumes the ordinary operation owners and result policy"
    )]
    fn receive_call(
        &self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        mut flow: crate::call::flow::Flow,
        budget: &mut IoBudget,
        source_rows: std::ops::Range<usize>,
        codec: OutputCodec,
        options: Options,
        collected: &mut Collected,
    ) -> PyResult<()> {
        let request_index = collected.prefix.commands;
        let mut first = None;
        let mut recorded = false;
        let mut set_index = 0;
        loop {
            let response = flow.advance_buffered(py, connection, lease, budget);
            collected.statement_invalidated |= flow.invalidates_statement();
            let recovery = if first.is_none() && flow.call_completed() {
                flow.call_confirmation().map_or(Ok(()), |confirmation| {
                    collected.first_confirmation(py, confirmation, true)?;
                    first = Some(confirmation);
                    Ok(())
                })
            } else {
                Ok(())
            };
            // CALL completion is independent of output SELECT/cleanup. Keep
            // this real fact even if a later private stage or host save fails.
            let recording = if !recorded && flow.call_completed() {
                if let Some(confirmation) = first.or_else(|| flow.call_confirmation()) {
                    collected
                        .prefix
                        .record_confirmation(source_rows.clone(), confirmation)
                        .map_err(|error| errors::wire(py, error))
                        .map(|()| recorded = true)
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            };
            let response = match response {
                Ok(response) => {
                    recovery?;
                    recording?;
                    response
                }
                Err(primary) => {
                    if let Err(secondary) = recovery {
                        errors::additional(py, &primary, secondary, "cursor.first_confirmation");
                    }
                    if let Some(native) = flow.take_native_failure() {
                        collected.chain.fail(native);
                    }
                    if let Err(secondary) = recording {
                        errors::additional(py, &primary, secondary, "cursor.call_confirmation");
                    }
                    collected.total_unknown |= first.is_none();
                    errors::note_format(
                        py,
                        &primary,
                        format_args!(
                            "while receiving CALL request {request_index}, public result set {set_index}; source rows {}..{}; no CALL request was replayed",
                            source_rows.start, source_rows.end,
                        ),
                    );
                    return Err(primary);
                }
            };
            let Some(response) = response else {
                if !flow.is_finished() || first.is_none() {
                    lease.invalidate();
                    return Err(errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "CALL ended without a complete public response and private finish",
                    ));
                }
                return Ok(());
            };
            if first.is_none() {
                first = Some(collected.first_response(py, &response, true)?);
            }
            let names_codec = if matches!(&response, Response::Rows(_)) {
                flow.metadata_codec().ok_or_else(|| errors::client(
                    py, ClientErrorCode::UnsupportedCharset,
                    "The CALL result's actual metadata encoding is unknown; the entry encoding was not substituted",
                ))
            } else {
                Ok(codec)
            };
            let result = names_codec.and_then(|names_codec| {
                collected
                    .chain
                    .push(NativeSet {
                        response,
                        options,
                        names_codec,
                        request_index,
                        set_index,
                    })
                    .map_err(|error| output::memory(py, error))
            });
            if let Err(error) = result {
                if !flow.synchronized() {
                    lease.invalidate();
                }
                return Err(error);
            }
            set_index = set_index.checked_add(1).ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "CALL result index overflow",
                )
            })?;
            if let Err(error) = input::check(py, budget) {
                if !flow.synchronized() {
                    lease.invalidate();
                }
                return Err(error);
            }
        }
    }

    fn handoff(&self, py: Python<'_>, begun: &mut bool) -> PyResult<()> {
        if !*begun {
            self.reader.clear_result(py)?;
            *self.counters(py)? = None;
            let old = std::mem::take(&mut *self.results(py)?);
            drop(old);
            *begun = true;
        }
        Ok(())
    }
    fn run<'py>(
        &self,
        py: Python<'py>,
        sql: &Bound<'_, PyAny>,
        args: Argument<'_>,
        many: &Argument<'_>,
        itemize: &Argument<'_>,
        local_file: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.reader.operation(py)?;
        self.reader.open(py)?;
        let connection = self.connection.borrow(py);
        let timeouts = connection.runtime(py)?.timeouts;
        let mut budget = timeouts.operation();
        let sql = sql.cast::<PyString>().map_err(|_| {
            crate::errors::builtin_message::<pyo3::exceptions::PyTypeError>(
                py,
                "sql must be str; already encoded bytes are not accepted",
            )
        })?;
        let many = many
            .0
            .as_ref()
            .map_or(Ok(false), |value| config::boolean(value, "many"))?;
        let itemize = itemize
            .0
            .as_ref()
            .map_or(Ok(true), |value| config::boolean(value, "itemize"))?;
        // The Cursor operation is already protected, but user __fspath__ runs
        // before acquiring the wire. The same original budget then covers
        // native opening, the upload and its actual final server response.
        let local_path = crate::local::path(py, local_file, &budget)?;
        if local_path.is_some() && (many || self.prepared) {
            return Err(errors::client(
                py,
                ClientErrorCode::UnsupportedCapability,
                "local_file requires one ordinary text LOAD DATA LOCAL INFILE; many/executemany and prepared uploads are not supported",
            ));
        }
        let mut lease = connection.acquire(py, &mut budget)?;
        let options = self.settings(py)?.options;
        let types = connection.types.borrow(py);
        let mut statement = if self.prepared {
            Some(self.hold_statement(py)?)
        } else {
            None
        };
        let mut collected = Collected::default();
        let mut begun = false;
        let mut live: Option<crate::stream::Stream> = None;
        let result = (|| {
            connection.ensure_context(py, &mut lease, &mut budget)?;
            let mut planner = binding::Planner::new(
                sql,
                args.0.as_ref(),
                many,
                itemize,
                if self.prepared {
                    binding::PlanMode::PreparedNative
                } else {
                    binding::PlanMode::Text
                },
                &types,
                &budget,
            )?;
            if local_path.is_some() && planner.is_batch() {
                return Err(errors::client(
                    py,
                    ClientErrorCode::UnsupportedCapability,
                    "local_file cannot be shared across a batch of logical input rows; use a single text LOAD operation",
                ));
            }
            loop {
                input::check(py, &budget)?;
                connection.ensure_context(py, &mut lease, &mut budget)?;
                let (context, ansi, names_codec) = connection.context(py)?;
                let Some(mut request) = planner.next(py, &context, ansi, &budget)? else {
                    break;
                };
                if let Some(template) = request.prepared.take() {
                    let slot = &mut statement
                        .as_mut()
                        .ok_or_else(|| {
                            errors::client(
                                py,
                                ClientErrorCode::InternalState,
                                "A native prepared request has no Cursor statement slot",
                            )
                        })?
                        .value;
                    let id = crate::statement::resolve(
                        py,
                        &connection,
                        &mut lease,
                        template,
                        &mut budget,
                        slot,
                    )?;
                    let slot = request.bytes.get_mut(1..5).ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "The prepared request has no native statement-ID field",
                        )
                    })?;
                    slot.copy_from_slice(&id.to_le_bytes());
                }
                let source = if self.prepared {
                    None
                } else {
                    let (bytes, is_local) =
                        crate::local::statement(py, request.bytes, &context, ansi, &budget)?;
                    request.bytes = bytes;
                    match (is_local, local_path.as_ref()) {
                        (true, Some(path)) if connection.local_infile && request.final_request => {
                            Some(crate::local::open(py, path, &budget)?)
                        }
                        (true, _) => {
                            return Err(errors::client(
                                py,
                                ClientErrorCode::UnsupportedCapability,
                                "LOAD DATA LOCAL INFILE requires connect(local_infile=True) and this single execute call's local_file path; no source was opened and SQL was not sent",
                            ));
                        }
                        (false, Some(_)) => {
                            return Err(errors::client(
                                py,
                                ClientErrorCode::UnsupportedCapability,
                                "local_file is accepted only for one reliably recognized ordinary text LOAD DATA LOCAL INFILE; multi-SQL, special executable comments and other statements are not upload authorizations",
                            ));
                        }
                        (false, None) => None,
                    }
                };
                let job = connection.query_job(py, &mut lease, request.bytes, false)?;
                let job = match source {
                    Some(source) => job
                        .with_local_infile(source)
                        .map_err(|error| errors::query(py, error))?,
                    None => job,
                };
                input::check(py, &budget)?;
                let job = if self.streaming && request.final_request {
                    job.with_streaming()
                        .map_err(|error| errors::query(py, error))?
                } else {
                    job
                };
                self.handoff(py, &mut begun)?;
                if self.streaming && request.final_request {
                    let prefix = ConfirmedPrefix {
                        commands: collected.prefix.commands,
                        source_rows: collected.prefix.source_rows,
                        row_count: collected.prefix.row_count,
                        warnings: collected.prefix.warnings,
                        last_insert_id: collected.prefix.last_insert_id,
                    };
                    live = Some(crate::stream::Stream::start(
                        py,
                        &connection,
                        &mut lease,
                        crate::stream::Job::Query(job),
                        budget,
                        options,
                        request.source_rows,
                        prefix,
                        collected.total,
                    )?);
                    break;
                }
                self.receive_request(
                    py,
                    &connection,
                    &mut lease,
                    job,
                    &mut budget,
                    request.source_rows,
                    request.final_request,
                    names_codec,
                    options,
                    &mut collected,
                )?;
            }
            Ok(())
        })();
        if collected.statement_invalidated || connection.native.is_closed() {
            drop(
                statement
                    .as_mut()
                    .and_then(|statement| statement.value.take()),
            );
        }
        if !begun {
            return result.map(|()| py.None().into_bound(py));
        }
        // Release the actual wire guard before returning a persistent owner or
        // constructing final host values. Only the native token crosses calls.
        drop(lease);
        self.publish_execution(
            py,
            &connection,
            statement.as_mut().map(|statement| &mut statement.value),
            Publication {
                collected,
                live,
                result,
                call_output: None,
            },
        )
    }
    fn run_call<'py>(
        &self,
        py: Python<'py>,
        proc_name: &Bound<'_, PyAny>,
        args: Argument<'_>,
        database: Option<&Bound<'_, PyAny>>,
        parameter_modes: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.reader.operation(py)?;
        self.reader.open(py)?;
        let connection = self.connection.borrow(py);
        let mut budget = connection.runtime(py)?.timeouts.operation();
        let proc_name = proc_name.cast::<PyString>().map_err(|_| {
            errors::builtin_message::<pyo3::exceptions::PyTypeError>(py, "proc_name must be str")
        })?;
        let mut lease = connection.acquire(py, &mut budget)?;
        connection.ensure_context(py, &mut lease, &mut budget)?;
        let (context, ansi, names_codec) = connection.context(py)?;
        let options = self.settings(py)?.options;
        let prefix = if self.prepared || parameter_modes.is_none_or(|value| value.is_none()) {
            ""
        } else {
            crate::call::slot_prefix(py, &connection)?
        };
        let types = connection.types.borrow(py);
        let mut plan = binding::call::NativeCall::new(
            proc_name,
            args.0.as_ref(),
            database,
            parameter_modes,
            prefix,
            if self.prepared {
                binding::PlanMode::PreparedNative
            } else {
                binding::PlanMode::Text
            },
            &types,
            &context,
            ansi,
            &budget,
        )?;
        drop(types);
        let multi_statements = if !self.prepared && !plan.output_positions.is_empty() {
            connection.native.facts.lock().map_err(|_| errors::client(
                py, ClientErrorCode::InternalState, "Confirmed CALL capabilities are unavailable",
            ))?.multi_statements.ok_or_else(|| errors::client(
                py, ClientErrorCode::SessionState,
                "multi_statements has not been confirmed; no CALL layout was guessed or sent",
            ))?
        } else {
            false
        };
        let call_output = crate::call::Output::new(
            py,
            &plan.output_positions,
            options,
            std::sync::Arc::clone(&connection.native.result_pool),
            connection.result_limit,
        )?;
        let mut statement = if self.prepared {
            Some(self.hold_statement(py)?)
        } else {
            None
        };
        if let Some(template) = plan.call.prepared.take() {
            let slot = &mut statement
                .as_mut()
                .ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "A prepared CALL has no current statement slot",
                    )
                })?
                .value;
            let id = crate::statement::resolve(
                py,
                &connection,
                &mut lease,
                template,
                &mut budget,
                slot,
            )?;
            let id_field = plan.call.bytes.get_mut(1..5).ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "The prepared CALL has no statement-ID field",
                )
            })?;
            id_field.copy_from_slice(&id.to_le_bytes());
        }
        let source_rows = plan.call.source_rows.clone();
        let scope = sqlaero_core::memory::Scope::new(
            std::sync::Arc::clone(&connection.native.result_pool),
            connection.result_limit,
        )
        .map_err(|error| output::memory(py, error))?;
        let flow = crate::call::flow::Flow::new(
            py,
            &connection,
            &mut lease,
            plan,
            std::sync::Arc::clone(&call_output),
            crate::call::flow::Policy {
                multi_statements,
                streaming: self.streaming,
                send_limit: context.limit,
            },
            &scope,
            &budget,
        )?;
        input::check(py, &budget)?;
        let mut begun = false;
        self.handoff(py, &mut begun)?;
        let mut collected = Collected::default();
        let mut live = None;
        let result = if self.streaming {
            crate::stream::Stream::start(
                py,
                &connection,
                &mut lease,
                crate::stream::Job::Call(flow),
                budget,
                options,
                source_rows,
                ConfirmedPrefix::default(),
                0,
            )
            .map(|mut stream| {
                stream.call_output = Some(std::sync::Arc::clone(&call_output));
                live = Some(stream);
            })
        } else {
            self.receive_call(
                py,
                &connection,
                &mut lease,
                flow,
                &mut budget,
                source_rows,
                names_codec,
                options,
                &mut collected,
            )
        };
        if !self.streaming {
            if result.is_ok() {
                call_output.finish();
            } else {
                call_output.fail();
            }
        }
        if collected.statement_invalidated || connection.native.is_closed() {
            drop(
                statement
                    .as_mut()
                    .and_then(|statement| statement.value.take()),
            );
        }
        drop(lease);
        self.publish_execution(
            py,
            &connection,
            statement.as_mut().map(|statement| &mut statement.value),
            Publication {
                collected,
                live,
                result,
                call_output: Some(call_output),
            },
        )
    }

    fn publish_execution<'py>(
        &self,
        py: Python<'py>,
        connection: &Connection,
        statement: Option<&mut Option<crate::statement::Cached>>,
        publication: Publication,
    ) -> PyResult<Bound<'py, PyAny>> {
        let Publication {
            mut collected,
            mut live,
            result,
            call_output,
        } = publication;
        let mut result = result.and_then(|()| {
            if let Some(stream) = &mut live {
                let event = stream.advance(py, connection, 1)?;
                stream.pending = Some(event);
            }
            Ok(())
        });
        if (live
            .as_ref()
            .is_some_and(|stream| stream.statement_invalidated)
            || connection.native.is_closed())
            && let Some(statement) = statement
        {
            drop(statement.take());
        }
        let first = collected.chain.take_next();
        let publication = if let Some(set) = first {
            self.publish_set(py, set)
        } else if result.is_ok()
            && let Some(stream) = &mut live
        {
            match stream.advance(py, connection, 1) {
                Ok(event) => self.stream_event(py, stream, event),
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        };
        if let Err(error) = publication {
            // The first visible set itself could not be published. Later
            // sets cannot become a substitute for that failed boundary.
            collected.chain = Chain::default();
            if let Some(stream) = &mut live {
                stream.abort(connection);
            }
            match &result {
                Err(primary) => errors::additional(py, primary, error, "cursor.publish_prefix"),
                Ok(()) => result = Err(error),
            }
        }
        let batch = if let Some(stream) = &live {
            stream.batch
        } else {
            (!collected.total_unknown && collected.final_first_known).then_some(collected.total)
        };
        let result = result.and_then(|()| {
            if self.streaming {
                output::optional_count(py, batch)
            } else {
                output::count(py, collected.total)
            }
        });
        if let Err(error) = &result {
            errors::note(py, error, collected.prefix.diagnostic());
        }
        let failure_note = result.as_ref().err().and_then(|error| {
            errors::SavedFailure::capture(py, error)
                .ok()
                .map(std::sync::Arc::new)
        });
        *self.results(py)? = Results {
            pending: if self.reader.is_closed() {
                Chain::default()
            } else {
                collected.chain
            },
            batch,
            failed: result.is_err(),
            started: true,
            failure_note,
            call_output,
            stream: if self.reader.is_closed() {
                None
            } else {
                live.map(|mut value| {
                    value.returned();
                    value
                })
            },
        };
        result
    }
}
macro_rules! cursor_api {
    ($(($name:ident,$set:ident,$doc:literal)),* $(,)?) => {
        cursor_api!(@impl Cursor,false; $(($name,$set,$doc)),*);
        cursor_api!(@impl DictCursor,false; $(($name,$set,$doc)),*);
        cursor_api!(@impl SSCursor,true; $(($name,$set,$doc)),*);
        cursor_api!(@impl SSDictCursor,true; $(($name,$set,$doc)),*);
    };
    (@impl $class:ident,$streaming:literal; $(($name:ident,$set:ident,$doc:literal)),* $(,)?) => {
        #[pymethods]
        impl $class {
            /// Preview every actual text request without sending SQL, advancing
            /// results or claiming execution. One request returns str; multiple
            /// requests return list[str] in send order. The last confirmed
            /// encoding/mode and logical send limit are used even while busy or
            /// closed. No operation timeout or result-memory quota is consumed.
            /// Unrepresentable lossless text raises ValueError; omitted args
            /// and an explicit None retain their normal execute meanings.
            #[pyo3(signature=(sql,args=Argument::default(),*,many=Argument::default(),itemize=Argument::default()))]
            #[pyo3(text_signature="($self, sql, args=..., *, many=False, itemize=True)")]
            fn mogrify<'py>(&self,py:Python<'py>,sql:&Bound<'py,PyAny>,args:Argument<'py>,many:Argument<'_>,itemize:Argument<'_>)->PyResult<Bound<'py,PyAny>> {
                (|| {
                    let connection=self.connection.borrow(py);
                    let reference=connection.auxiliary_context(py)?;
                    let many=many.0.as_ref().map_or(Ok(false),|value|config::boolean(value,"many"))?;
                    let itemize=itemize.0.as_ref().map_or(Ok(true),|value|config::boolean(value,"itemize"))?;
                    let sql=sql.cast::<PyString>().map_err(|_|errors::builtin_message::<pyo3::exceptions::PyTypeError>(py,"sql must be str; already encoded bytes are not accepted"))?;
                    let budget=IoBudget::new(sqlaero_core::transport::deadline::TimeoutKind::Operation,None,None,None);
                    let types=connection.types.borrow(py);
                    let mode=if self.prepared {binding::PlanMode::PreparedDisplay} else {binding::PlanMode::Text};
                    let mut planner=binding::Planner::new(sql,args.0.as_ref(),many,itemize,mode,&types,&budget)?;
                    let mut first:Option<Bound<'py,PyString>>=None;
                    let mut output:Option<Bound<'py,pyo3::types::PyList>>=None;
                    while let Some(request)=planner.next(py,&reference.binding,reference.ansi_quotes,&budget)? {
                        let text=request.bytes.get(1..).ok_or_else(||errors::client(py,ClientErrorCode::InternalState,"SQL preview produced no command payload"))?;
                        let value=binding::display(py,text,reference.binding.codec)?;
                        if let Some(output)=&output { output.append(value)?; }
                        else if let Some(previous)=first.take() {
                            let list=output::list(py,[previous.into_any(),value.into_any()])?;
                            output=Some(list);
                        } else { first=Some(value); }
                        input::check(py,&budget)?;
                    }
                    match (output,first) {
                        (Some(output),_)=>Ok(output.into_any()),
                        (_,Some(first))=>Ok(first.into_any()),
                        _=>Err(errors::client(py,ClientErrorCode::InternalState,"SQL preview produced no logical request")),
                    }
                })().map_err(|error|errors::at(py,error,"cursor.mogrify"))
            }
            $(#[getter] #[doc=$doc]
            fn $name(&self,py:Python<'_>)->PyResult<bool>{Ok(self.settings(py)?.options.$name)}
            #[setter] fn $set(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::boolean(value,stringify!($name))?;
                let _operation=self.reader.operation(py)?;self.modifying(py,stringify!($name))?;
                self.settings(py)?.options.$name=value;Ok(())
            })*
            /// Execute one SQL operation. SS returns None while a required
            /// first-result count is unknown; this is not execution failure.
            /// SS server routines must keep character_set_results stable within
            /// an unread row result. A detected change raises NotSupportedError
            /// at its real terminator with received count facts preserved;
            /// an unfinished response is retired. Use buffered Cursor/DictCursor
            /// for internal charset changes. Header-time detection and withdrawal
            /// of previously delivered values/metadata cannot be guaranteed.
            /// Omitted args sends original SQL; explicit None binds one NULL.
            /// many/itemize are exact bool and keyword-only. All request chunks
            /// share one deadline. Confirmed prefixes never imply a commit.
            /// Text directives after a statement changing input charset,
            /// sql_mode or time_zone, or after opaque CALL/EXECUTE, fail locally
            /// before the request sends. Run and confirm that statement first,
            /// then bind the next request. Hidden routine mutations are not
            /// predicted by a general SQL interpreter.
            /// Escape validates supported built-in values, approved real
            /// subclasses, containers and optional NumPy/Pandas/Arrow/Polars
            /// inputs. One-dimensional sources follow many/itemize; tables
            /// supply logical rows even when many=False. Providers are not
            /// imported by this API. bytearray and memoryview are rejected.
            /// datetime.time keeps its clock fields and microseconds, ignores
            /// tzinfo and never calls timezone methods; 00:30+01:00 remains
            /// 00:30:00. Aware datetime, in contrast, is normalized to UTC.
            /// Keep args and all nested mutable values unchanged until this
            /// call returns. Each constructed Rust request owns stable bytes;
            /// batch construction does not provide an atomic snapshot of all inputs.
            /// local_file optionally selects one regular local source through
            /// str/Path/PathLike. Upload requires connect(local_infile=True) and
            /// one ordinary text LOAD DATA LOCAL INFILE, without a batch or
            /// prepared mode. The SQL filename is only a protocol label and
            /// never selects the actual file. Rust opens the explicit source
            /// once and uploads only its opening length in bounded chunks.
            /// Keep its contents stable through return. Filesystem calls are
            /// cooperatively checked against the original operation budget;
            /// blocking OS I/O cannot promise a hard interruption deadline.
            /// Native opening errors preserve standard OSError/errno details.
            #[pyo3(signature=(sql,args=Argument::default(),*,many=Argument::default(),itemize=Argument::default(),local_file=None))]
            #[pyo3(text_signature="($self, sql, args=..., *, many=False, itemize=True, local_file=None)")]
            fn execute<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:Argument<'_>,many:Argument<'_>,itemize:Argument<'_>,local_file:Option<&Bound<'_,PyAny>>)->PyResult<Bound<'py,PyAny>> {self.run(py,sql,args,&many,&itemize,local_file).map_err(|error|errors::at(py,error,"cursor.execute"))}
            /// Call one safely quoted procedure using one positional argument
            /// row. Omitted args means zero positions; explicit None is NULL.
            /// parameter_modes explicitly declares IN/OUT/INOUT, never inferred
            /// from values. Results use this Cursor's normal nextset/fetch path.
            /// Modes must be an exact list/tuple of direction strings, or None
            /// for all IN; OUT requires a source NULL placeholder. MySQL still
            /// decides the actual procedure signature. Detectable disagreement
            /// may be reported after CALL was sent; the request is not replayed.
            /// Buffered calls return the first public count. SS returns None
            /// while that count is unknown. Prepared SS calls with declared
            /// outputs may read up to two native rows to identify the actual
            /// output flag, without buffering the full business result.
            /// Finish all public sets with nextset() until None, or close the
            /// Cursor within the original remaining budget, to obtain outputs.
            /// call_output_params is a local read-only original-position mapping.
            /// Decode uses actual fields and this operation's option snapshot;
            /// text user-variable output may expose dates and JSON as strings.
            /// Prepared output can preserve native date/JSON field types.
            #[pyo3(signature=(proc_name,args=Argument::default(),*,database=None,parameter_modes=None))]
            #[pyo3(text_signature="($self, proc_name, args=(), *, database=None, parameter_modes=None)")]
            fn callproc<'py>(&self,py:Python<'py>,proc_name:&Bound<'_,PyAny>,args:Argument<'_>,database:Option<&Bound<'_,PyAny>>,parameter_modes:Option<&Bound<'_,PyAny>>)->PyResult<Bound<'py,PyAny>> {
                self.run_call(py,proc_name,args,database,parameter_modes).map_err(|error|errors::at(py,error,"cursor.callproc"))
            }
            /// Execute using the same native many=True,itemize=True operation.
            /// Input value and timezone rules are the same as execute().
            /// Keep args and all nested mutable values unchanged until this
            /// call returns. Each constructed Rust request owns stable bytes;
            /// batch construction does not provide an atomic snapshot of all inputs.
            fn executemany<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:&Bound<'_,PyAny>)->PyResult<Bound<'py,PyAny>> {
                let many=true.into_pyobject(py)?.to_owned().into_any();
                self.run(py,sql,Argument(Some(args.clone())),&Argument(Some(many.clone())),&Argument(Some(many)),None)
                    .map_err(|error|errors::at(py,error,"cursor.executemany"))
            }
            /// Advance to the next actual result set, discarding unread rows of
            /// the current set. True selects a set; None confirms normal end.
            /// Buffered sets are already native-owned. A stored failure boundary
            /// raises its original cause rather than claiming natural exhaustion.
            fn nextset(&self,py:Python<'_>)->PyResult<Option<bool>> {
                let _operation=self.reader.operation(py)?;
                self.reader.open(py)?;
                if !self.results(py)?.started {return Err(errors::client(py,ClientErrorCode::SessionState,"Cursor has no result sequence; execute a command before nextset"));}
                let next=self.results(py)?.pending.take_next();
                if let Some(set)=next {
                    let (request,set_index)=(set.request_index,set.set_index);
                    if let Err(error)=self.publish_set(py,set) {
                        errors::note_format(py,&error,format_args!("while publishing request {request}, result set {set_index}; later sets cannot be skipped after this local failure"));
                        if let Err(recording)=self.save_set_failure(py,&error) {
                            errors::additional(py,&error,recording,"cursor.nextset.failure_boundary");
                        }
                        return Err(errors::at(py,error,"cursor.nextset"));
                    }
                    return Ok(Some(true));
                }
                if !$streaming {self.reader.end_set(py)?;}
                let (failed,snapshot)={let state=self.results(py)?;(state.failed,state.failure_note.as_ref().map(std::sync::Arc::clone))};
                if let Some(snapshot)=snapshot {return Err(errors::at(py,snapshot.restore(py),"cursor.nextset"));}
                if failed {return Err(errors::client(py,ClientErrorCode::SessionState,
                    "The immediate original failure was preserved, but its retained diagnostic could not be saved. This is a local diagnostic limitation, not a new database failure."));}
                let native=self.results(py)?.pending.failure().map(|failure|
                    (std::sync::Arc::clone(&failure.error),failure.codec));
                if let Some((error,codec))=native {
                    return Err(errors::at(py,errors::query_with_codec(py,error.as_ref(),codec),"cursor.nextset"));
                }
                if $streaming {return self.next_stream(py).map_err(|error|errors::at(py,error,"cursor.nextset"));}
                Ok(None)
            }
            /// Sum only the first result count of each actual request. None
            /// means a contribution or an unattempted request remains unknown.
            #[getter] fn batch_row_count<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {let count=self.results(py)?.batch;output::optional_count(py,count)}
            /// Confirmed callproc OUT/INOUT values by original argument index,
            /// or None while unready. Reading sends no SQL. Successful values
            /// are reused; output Decode follows this operation's frozen policy.
            /// Confirmed output stays readable after close and failed cleanup.
            #[getter] fn call_output_params<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                let output=self.results(py)?.call_output.as_ref().map(std::sync::Arc::clone);
                let Some(output)=output else{return Ok(py.None().into_bound(py));};
                if !output.is_ready(){return Ok(py.None().into_bound(py));}
                if let Some(cached)=output.cached(py){return Ok(cached);}
                let _operation=self.reader.operation(py)?;
                let connection=self.connection.borrow(py);
                output.get(py,&connection.types).map_err(|error|errors::at(py,error,"cursor.call_output_params"))
            }
            /// Move within the current result. Buffered results allow both
            /// directions, including EOF. Invalid positions raise IndexError.
            #[pyo3(signature=(value,mode="relative"))]
            fn scroll(&self,py:Python<'_>,value:&Bound<'_,PyAny>,mode:&str)->PyResult<()> {
                if $streaming {self.stream_scroll(py,value,mode).map_err(|error|errors::at(py,error,"cursor.scroll"))} else {self.reader.scroll(py,value,mode)}
            }
            /// Return the next row in this Cursor format, or None when a valid result is exhausted.
            /// Buffered fetching remains valid after its Connection is closed,
            /// as long as this Cursor stays open, and needs no network access.
            /// SS may continue its original response; no SQL is reissued.
            fn fetchone<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                if $streaming {self.stream_fetch(py,Some(1),true).map_err(|error|errors::at(py,error,"cursor.fetchone"))} else {self.reader.fetchone(py)}
            }
            /// Return at most size rows in this Cursor format atomically; None uses arraysize.
            /// Zero preserves the position but still requires a current result.
            /// Failed conversion leaves the complete original position unchanged.
            /// An SS resource error states whether its native response was
            /// safely retained for an explicit smaller fetch or release of
            /// other result owners. No partial batch or fresh deadline is given.
            /// Unsafe receive failures instead retire the incomplete Connection.
            #[pyo3(signature=(size=None))]
            fn fetchmany<'py>(&self,py:Python<'py>,size:Option<&Bound<'py,PyAny>>)->PyResult<Bound<'py,PyAny>> {
                if $streaming {let count=self.reader.fetch_size(py,size)?;self.stream_fetch(py,Some(count),false).map_err(|error|errors::at(py,error,"cursor.fetchmany"))} else {self.reader.fetchmany(py,size)}
            }
            /// Return all remaining rows in this Cursor format atomically.
            /// No SQL is reissued. SS follows fetchmany's explicit safe-resource
            /// continuation rule; failures never return a partial batch.
            fn fetchall<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                if $streaming {self.stream_fetch(py,None,false).map_err(|error|errors::at(py,error,"cursor.fetchall"))} else {self.reader.fetchall(py)}
            }
            /// Close only this Cursor, retaining readable local metadata,
            /// counters and configuration. SS drains unread data under the
            /// original budget; unsafe cleanup retires its Connection.
            pub(crate) fn close(&self,py:Python<'_>)->PyResult<()> {
                self.close_cursor(py).map_err(|error|errors::at(py,error,"cursor.close"))
            }
            /// True after this Cursor has been permanently retired locally.
            #[getter] fn closed(&self)->bool {self.reader.is_closed()}
            /// Fixed native prepared-execution policy selected at creation.
            /// This is readable after close and does not imply an active handle.
            #[getter] fn prepared(&self)->bool {self.core.prepared}
            /// Positive default row count for fetchmany(None); initial value 1.
            /// Local assignment sends no SQL, never changes existing decode
            /// policy, and fails after this Cursor is closed. Readable after close.
            #[getter] fn arraysize(&self,py:Python<'_>)->PyResult<BigUint>{Ok(self.settings(py)?.arraysize.clone())}
            #[setter] fn set_arraysize(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::positive(value,"arraysize")?;
                let count=usize::try_from(value.clone()).unwrap_or(usize::MAX);
                let _operation=self.reader.operation(py)?; self.modifying(py,"arraysize")?;
                self.reader.set_arraysize(py,count)?;self.settings(py)?.arraysize=value;Ok(())
            }
            /// PEP 249 row count, or -1 if not known. Readable after close.
            #[getter] fn rowcount<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                let rows=self.counters(py)?.as_ref().map(|value|value.rows);
                match rows {
                    None=>Ok((-1i32).into_pyobject(py)?.into_any()),
                    Some(rows)=>output::count(py,rows),
                }
            }
            /// Actual row count, or None if unknown; alias of affected_rows.
            #[getter] fn row_count<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let count=self.counters(py)?.as_ref().map(|value|value.rows);output::optional_count(py,count)}
            /// Actual affected/result row count, or None if unknown.
            #[getter] fn affected_rows<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.row_count(py)}
            /// Actual current-result insert ID, or None; no ID range is inferred.
            #[getter] fn lastrowid<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let count=self.counters(py)?.as_ref().and_then(|value|value.last_id).map(u128::from);output::optional_count(py,count)}
            /// Alias of lastrowid, reliably readable after close.
            #[getter] fn last_row_id<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.lastrowid(py)}
            /// Actual current-result warning count, or None when unknown.
            /// Readable after close; no warning history is accumulated.
            #[getter] fn warning_count<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let count=self.counters(py)?.as_ref().map(|value|value.warnings);output::optional_count(py,count)}
            /// Immutable current-result metadata, available lazily after close.
            #[getter] fn columns(&self,py:Python<'_>)->PyResult<Py<metadata::Columns>>{self.reader.columns(py)}
            /// Seven-field PEP 249 metadata, or None without a row result.
            #[getter] fn description<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.reader.description(py)}
            fn __traverse__(&self,visit:pyo3::class::PyVisit<'_>)->Result<(),pyo3::class::PyTraverseError>{
                visit.call(&self.connection)?;
                let state=self.results.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(snapshot)=&state.failure_note {snapshot.traverse(&visit)?;}
                if let Some(output)=&state.call_output {output.traverse(&visit)?;}
                Ok(())
            }
            fn __clear__(&self){
                let old={let mut state=self.results.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    (state.failure_note.take(),state.call_output.take(),state.stream.take())};
                drop(old);
            }
            fn __iter__(slf:PyRef<'_,Self>)->PyRef<'_,Self>{slf}
            fn __next__<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                let row=self.fetchone(py)?; if row.is_none(){Err(PyStopIteration::new_err(()))}else{Ok(row)}
            }
            fn __enter__<'py>(slf:PyRef<'py,Self>,py:Python<'py>)->PyResult<PyRef<'py,Self>>{slf.reader.open(py)?;Ok(slf)}
            fn __exit__(&self,py:Python<'_>,_typ:&Bound<'_,PyAny>,value:&Bound<'_,PyAny>,_traceback:&Bound<'_,PyAny>)->PyResult<bool>{
                match self.close(py) {
                    Ok(())=>Ok(false),
                    Err(error) if !value.is_none()=>{ errors::additional(py,&PyErr::from_value(value.clone()),error,"cursor.close");Ok(false) },
                    Err(error)=>Err(error),
                }
            }
        }
    }
}
cursor_api! {
    (decode_json,set_decode_json,"Default False. False returns the original JSON text as str. True parses the complete JSON into Python values; malformed, incomplete or out-of-range content raises DataError without text fallback. This applies to JSON fields, not JSON-looking TEXT. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_set,set_decode_set,"Default False. False returns SET text as str. True returns an ordered tuple[str, ...]; the empty SET is (), and an empty member is preserved. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_bit,set_decode_bit,"Default False. False returns the exact BIT bytes, including leading zero bytes. True returns an unsigned big-endian int, never a bool. Invalid BIT contents raise DataError. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_decimal,set_use_decimal,"Default True. True returns exact decimal.Decimal values for DECIMAL. False uses the native correctly rounded binary64 parser and returns float, potentially losing decimal precision; invalid or non-finite results raise DataError. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_timedelta,set_use_timedelta,"Default True. True returns datetime.timedelta for MySQL TIME, including negative and multi-day durations. False returns datetime.time and raises DataError for negative values or durations at least 24 hours. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_text_strict,set_decode_text_strict,"Default True. True raises UnicodeDecodeError for invalid text bytes. False preserves undecodable bytes through surrogateescape, without replacement characters. Binary fields remain bytes and input encoding is unaffected. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_date_strict,set_decode_date_strict,"Default True. True raises DataError for zero or otherwise unrepresentable DATE/DATETIME values. False returns their original calendar text as str. Valid values retain date/datetime output in both modes. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
}
