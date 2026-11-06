//! Buffered fetch ownership and one atomic position commit per output call.

use crate::{errors, input, input::ScalarTypes, metadata, output};
use pyo3::exceptions::{
    PyOverflowError, PyStopIteration, PyTypeError, PyUnicodeDecodeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyTuple};
use sqlaero_core::charset::output::OutputCodec;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, MemoryError, Reservation};
use sqlaero_core::result::{
    decode::{Options, Plan},
    lifecycle::Lifecycle,
    text::{Buffered, Cell, Header, RowBatch},
};
use sqlaero_core::transport::deadline::IoBudget;
use std::mem::size_of;
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(test)]
mod stream_tests;

/// Bound aggregate field work without restarting a signal check at every row.
/// Row checkpoints and long-value decoder checkpoints remain independent.
#[derive(Default)]
struct FetchProgress {
    fields: usize,
    bytes: usize,
}
impl FetchProgress {
    fn field(&mut self, py: Python<'_>, bytes: usize) -> PyResult<()> {
        self.fields += 1;
        self.bytes = self.bytes.saturating_add(bytes);
        if self.fields == 64 || self.bytes >= 64 * 1024 {
            py.check_signals()?;
            self.fields = 0;
            self.bytes = 0;
        }
        Ok(())
    }
}

struct ReadData {
    rows: Buffered,
    plans: Buffer<Plan>,
    _owner: Reservation,
}
/// Fully prepared but unpublished result. All fallible native/Python output
/// preparation finishes before Cursor exposes this execution's new snapshot.
pub(crate) struct PreparedResult {
    metadata: Arc<metadata::Snapshot>,
    data: Option<Arc<ReadData>>,
    total: usize,
}
/// One immutable layout and received native batches, without transport or a
/// manufactured completion. Full consumed batches release their actual storage;
/// a partially consumed batch keeps only its necessary bounded owner alive.
pub(crate) struct StreamRows {
    columns: Arc<sqlaero_core::result::text::Columns>,
    encoding: sqlaero_core::result::value::RowEncoding,
    plans: Buffer<Plan>,
    batches: Buffer<Option<RowBatch>>,
    head: usize,
    front: usize,
    available: usize,
    position: usize,
    total: Option<usize>,
    _owner: Reservation,
}
impl StreamRows {
    fn new(header: Header, options: Options) -> Result<Self, MemoryError> {
        let scope = header.columns.scope();
        let owner = scope.charge(size_of::<Self>())?;
        let mut plans = Buffer::tracked(scope);
        plans.reserve(header.columns.len())?;
        for column in header.columns.iter() {
            plans.push(
                Plan::new(
                    column.type_code,
                    column.flags,
                    column.collation_id,
                    column.column_length,
                    options,
                )
                .with_decimals(column.decimals),
            )?;
        }
        Ok(Self {
            batches: Buffer::tracked(scope),
            plans,
            columns: header.columns,
            encoding: header.encoding,
            head: 0,
            front: 0,
            available: 0,
            position: 0,
            total: None,
            _owner: owner,
        })
    }
    /// Reserve before advancing the network parser. Compaction moves existing
    /// owners in place, so a subsequent push needs no allocation.
    pub(crate) fn reserve_batch(&mut self) -> Result<(), MemoryError> {
        if self.batches.len() == self.batches.capacity() && self.head != 0 {
            let retained = self.batches.len() - self.head;
            self.batches.as_mut_slice().rotate_left(self.head);
            self.batches.truncate(retained);
            self.head = 0;
        }
        self.batches.reserve(1)
    }
    pub(crate) fn push(&mut self, rows: RowBatch) -> Result<(), MemoryError> {
        let available = self
            .available
            .checked_add(rows.row_count)
            .filter(|count| self.position.checked_add(*count).is_some())
            .ok_or(MemoryError::Result {
                required: usize::MAX,
                limit: usize::MAX,
            })?;
        if rows.row_count != 0 {
            self.batches.push(Some(rows))?;
            self.available = available;
        }
        Ok(())
    }
    pub(crate) fn available(&self) -> usize {
        self.available
    }
    pub(crate) fn position(&self) -> usize {
        self.position
    }
    /// The outer protocol owner passes only an actually confirmed row total.
    pub(crate) fn finish(&mut self, total: usize) {
        self.total = Some(total);
        if self.available == 0 {
            self.release_queue();
        }
    }
    pub(crate) fn is_complete(&self) -> bool {
        self.total.is_some()
    }
    /// Discard only actually received rows; it never fabricates an EOF.
    pub(crate) fn discard(&mut self) {
        self.skip(self.available);
    }
    pub(crate) fn skip(&mut self, count: usize) -> usize {
        let skipped = count.min(self.available);
        let mut remaining = skipped;
        while remaining != 0 {
            let Some(rows) = self.batches[self.head].as_ref() else {
                self.head += 1;
                continue;
            };
            let take = remaining.min(rows.row_count - self.front);
            self.front += take;
            remaining -= take;
            if self.front == rows.row_count {
                drop(self.batches.as_mut_slice()[self.head].take());
                self.head += 1;
                self.front = 0;
            }
        }
        self.available -= skipped;
        self.position += skipped;
        if self.available == 0 {
            self.release_queue();
        }
        skipped
    }
    fn release_queue(&mut self) {
        self.batches = Buffer::tracked(self.columns.scope());
        self.head = 0;
        self.front = 0;
    }
}

/// A borrowed view lets buffered and streaming sources share exactly one field
/// decoder. Buffered callers provide constant base zero and keep their existing
/// DIRECT/DICT/BINARY specialization and ownership graph.
struct RowSource<'a> {
    rows: &'a RowBatch,
    plans: &'a [Plan],
    base: usize,
}
struct State {
    data: Option<Arc<ReadData>>,
    metadata: Option<Arc<metadata::Snapshot>>,
    columns: Option<Py<metadata::Columns>>,
    position: usize,
    total: usize,
    arraysize: usize,
    abandoned: bool,
}

/// A concrete per-Cursor operation owner. It never owns the connection wire.
pub(crate) struct Operation<'a>(&'a Reader);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        // Retirement never frees an active fetch's owned Arc. Clear only the
        // Reader's retained rows after that operation reaches its safe exit.
        if self.0.lifecycle.finish() {
            self.0.release_retired_rows();
        }
    }
}

/// The production Rust read component. Cursor embeds this ordinary Rust value;
/// its private Python exposure only permits direct contract checks and adds no
/// extra Python object or dispatch layer to production Cursor calls.
#[pyclass(frozen, name = "_Rows", module = "sqlaero._sqlaero")]
pub(crate) struct Reader {
    state: Mutex<State>,
    lifecycle: Lifecycle,
    types: Py<ScalarTypes>,
    dictionary: bool,
}
impl Reader {
    /// Decode one confirmed output row directly into a private integer-keyed
    /// dict. The temporary context is a Rust value, not a Python Reader object;
    /// it borrows the same native row/plans and calls the ordinary field loop.
    /// No fetch position or retained input is created, so a failed mapping
    /// construction can retry from the original immutable native snapshot.
    pub(crate) fn snapshot_dict<'py>(
        py: Python<'py>,
        rows: &RowBatch,
        plans: &[Plan],
        types: &Py<ScalarTypes>,
        metadata: &Arc<metadata::Snapshot>,
        keys: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        if rows.row_count != 1 || plans.len() != rows.columns.len() || keys.len() != plans.len() {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "A confirmed CALL output snapshot requires one row and matching field plans and position keys",
            ));
        }
        let reader = Self::with_format(types.clone_ref(py), true);
        reader.lock(py)?.metadata = Some(Arc::clone(metadata));
        let source = RowSource {
            rows,
            plans,
            base: 0,
        };
        let types = types.borrow(py);
        let mut progress = FetchProgress::default();
        let output = match rows.encoding {
            sqlaero_core::result::value::RowEncoding::Binary => reader
                .row_tuple::<true, true, true>(
                    py,
                    &source,
                    &types,
                    0,
                    0,
                    &mut progress,
                    Some(keys),
                )?,
            sqlaero_core::result::value::RowEncoding::Text => reader
                .row_tuple::<true, true, false>(
                    py,
                    &source,
                    &types,
                    0,
                    0,
                    &mut progress,
                    Some(keys),
                )?,
        };
        output
            .cast_into::<pyo3::types::PyDict>()
            .map_err(Into::into)
    }

    /// Share the existing exact built-in fetch count boundary with SS callers.
    pub(crate) fn fetch_size(
        &self,
        py: Python<'_>,
        size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<usize> {
        let size = match size {
            None => self.lock(py)?.arraysize,
            Some(size) if size.is_none() => self.lock(py)?.arraysize,
            Some(size)
                if size.is_exact_instance_of::<PyInt>() && !size.is_instance_of::<PyBool>() =>
            {
                if size.lt(0)? {
                    return Err(crate::errors::builtin_message::<PyValueError>(
                        py,
                        "fetchmany size must not be negative",
                    ));
                }
                match size.extract::<usize>() {
                    Ok(value) => value,
                    Err(error) if error.is_instance_of::<PyOverflowError>(py) => usize::MAX,
                    Err(error) => return Err(error),
                }
            }
            Some(_) => {
                return Err(crate::errors::builtin_message::<PyTypeError>(
                    py,
                    "fetchmany size must be an int or None; bool is not accepted",
                ));
            }
        };
        Ok(size)
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.lifecycle.is_retired()
    }
    pub(crate) fn empty(types: Py<ScalarTypes>) -> Self {
        Self::with_format(types, false)
    }
    pub(crate) fn with_format(types: Py<ScalarTypes>, dictionary: bool) -> Self {
        Self {
            state: Mutex::new(State {
                data: None,
                metadata: None,
                columns: None,
                position: 0,
                total: 0,
                arraysize: 1,
                abandoned: false,
            }),
            lifecycle: Lifecycle::default(),
            types,
            dictionary,
        }
    }
    pub(crate) fn operation(&self, py: Python<'_>) -> PyResult<Operation<'_>> {
        if !self.lifecycle.try_begin() {
            return Err(errors::client(
                py,
                ClientErrorCode::InterfaceBusy,
                "Cursor is busy executing or reading another operation",
            ));
        }
        Ok(Operation(self))
    }
    fn release_retired_rows(&self) {
        let released = match self.state.lock() {
            Ok(mut state) => state.data.take(),
            Err(error) => error.into_inner().data.take(),
        };
        drop(released);
    }
    /// Mandatory context cleanup can retire a busy Cursor without cancelling
    /// its current local operation or destroying metadata and saved counters.
    pub(crate) fn retire(&self) {
        if self.lifecycle.retire() {
            self.release_retired_rows();
        }
    }
    fn lock(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, State>> {
        match self.state.lock() {
            Ok(state) => Ok(state),
            Err(error) => {
                let _ = self.lifecycle.retire();
                error.into_inner().data.take();
                Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "Cursor was retired after a native state failure",
                ))
            }
        }
    }
    pub(crate) fn open(&self, py: Python<'_>) -> PyResult<()> {
        if self.lifecycle.is_retired() {
            Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Cursor is closed or permanently retired",
            ))
        } else {
            Ok(())
        }
    }
    /// Install only a fully consumed, resource-valid final result. The outer
    /// executor owns this Reader's operation guard for the entire user call.
    pub(crate) fn install(
        &self,
        py: Python<'_>,
        rows: Buffered,
        options: Options,
        codec: OutputCodec,
    ) -> PyResult<()> {
        self.publish(py, Self::prepare(py, rows, options, codec)?)
    }
    pub(crate) fn prepare(
        py: Python<'_>,
        rows: Buffered,
        options: Options,
        codec: OutputCodec,
    ) -> PyResult<PreparedResult> {
        let metadata = metadata::Snapshot::new(
            py,
            Some(Arc::clone(&rows.columns)),
            codec,
            options.decode_text_strict,
        )?;
        let total = rows.row_count;
        let data = if total == 0 {
            None
        } else {
            let owner = rows
                .scope()
                .charge(size_of::<ReadData>() + 2 * size_of::<usize>() - size_of::<Buffered>())
                .map_err(|error| output::memory(py, error))?;
            let mut plans = Buffer::tracked(rows.scope());
            plans
                .reserve(rows.columns.len())
                .map_err(|error| output::memory(py, error))?;
            for column in rows.columns.iter() {
                plans
                    .push(
                        Plan::new(
                            column.type_code,
                            column.flags,
                            column.collation_id,
                            column.column_length,
                            options,
                        )
                        .with_decimals(column.decimals),
                    )
                    .map_err(|error| output::memory(py, error))?;
            }
            Some(Arc::new(ReadData {
                rows,
                plans,
                _owner: owner,
            }))
        };
        Ok(PreparedResult {
            metadata,
            data,
            total,
        })
    }
    pub(crate) fn publish(&self, py: Python<'_>, prepared: PreparedResult) -> PyResult<()> {
        let PreparedResult {
            metadata,
            data,
            total,
        } = prepared;
        let old = {
            let mut state = self.lock(py)?;
            let old_metadata = state.metadata.replace(metadata);
            let old_columns = state.columns.take();
            state.position = 0;
            state.total = total;
            state.abandoned = false;
            (
                std::mem::replace(&mut state.data, data),
                old_metadata,
                old_columns,
            )
        };
        drop(old);
        Ok(())
    }
    /// Prepare a stream's layout once and publish only fully prepared metadata.
    /// The caller owns Reader.operation; this method never acquires it again.
    pub(crate) fn stream_header(
        &self,
        py: Python<'_>,
        header: Header,
        options: Options,
        codec: OutputCodec,
    ) -> PyResult<StreamRows> {
        let metadata = metadata::Snapshot::new(
            py,
            Some(Arc::clone(&header.columns)),
            codec,
            options.decode_text_strict,
        )?;
        let rows = StreamRows::new(header, options).map_err(|error| output::memory(py, error))?;
        self.publish(
            py,
            PreparedResult {
                metadata,
                data: None,
                total: 0,
            },
        )?;
        Ok(rows)
    }

    fn stream_materialize<'py, const DIRECT: bool, const DICT: bool, const BINARY: bool>(
        &self,
        py: Python<'py>,
        cache: &StreamRows,
        count: usize,
        one: bool,
        types: &ScalarTypes,
        keys: Option<&Bound<'py, PyTuple>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let start = cache.position;
        let mut progress = FetchProgress::default();
        let mut index = start;
        let mut sources = cache
            .batches
            .iter()
            .skip(cache.head)
            .filter_map(Option::as_ref);
        let mut front = cache.front;
        let mut source = sources.next().map(|rows| RowSource {
            rows,
            plans: &cache.plans,
            base: start - front,
        });
        let mut next = || -> PyResult<Bound<'py, PyAny>> {
            let Some(current) = &source else {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "stream row owner is unavailable",
                ));
            };
            let row = self.row_tuple::<DIRECT, DICT, BINARY>(
                py,
                current,
                types,
                index,
                start,
                &mut progress,
                keys,
            )?;
            index += 1;
            front += 1;
            if front == current.rows.row_count {
                front = 0;
                source = sources.next().map(|rows| RowSource {
                    rows,
                    plans: &cache.plans,
                    base: index,
                });
            }
            Ok(row)
        };
        if one {
            next()
        } else {
            output::tuple_from_values(py, (0..count).map(|_| next())).map(Bound::into_any)
        }
    }

    /// Consume only cached rows. The outer owner receives enough rows first and
    /// supplies the original budget or a local budget for an already complete set.
    /// Output, final checks and native ownership publication form one commit.
    pub(crate) fn stream_read<'py>(
        &self,
        py: Python<'py>,
        rows: &mut StreamRows,
        count: Option<usize>,
        one: bool,
        budget: &IoBudget,
        validate: impl FnOnce() -> PyResult<()>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.open(py)?;
        input::check(py, budget)?;
        let count = count.unwrap_or(rows.available).min(rows.available);
        let count = if one { count.min(1) } else { count };
        let output = if count == 0 {
            if one {
                py.None().into_bound(py)
            } else {
                PyTuple::empty(py).into_any()
            }
        } else {
            let types = self.types.borrow(py);
            let keys = if self.dictionary {
                let metadata = self.lock(py)?.metadata.clone().ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "stream dictionary metadata is unavailable",
                    )
                })?;
                Some(metadata.key_tuple(py)?)
            } else {
                None
            };
            match (rows.encoding, self.dictionary, rows.plans.len() <= 4) {
                (sqlaero_core::result::value::RowEncoding::Text, true, _) => self
                    .stream_materialize::<true, true, false>(
                        py,
                        rows,
                        count,
                        one,
                        &types,
                        keys.as_ref(),
                    )?,
                (sqlaero_core::result::value::RowEncoding::Binary, true, _) => self
                    .stream_materialize::<true, true, true>(
                        py,
                        rows,
                        count,
                        one,
                        &types,
                        keys.as_ref(),
                    )?,
                (sqlaero_core::result::value::RowEncoding::Text, false, true) => self
                    .stream_materialize::<true, false, false>(py, rows, count, one, &types, None)?,
                (sqlaero_core::result::value::RowEncoding::Binary, false, true) => self
                    .stream_materialize::<true, false, true>(py, rows, count, one, &types, None)?,
                (sqlaero_core::result::value::RowEncoding::Text, false, false) => self
                    .stream_materialize::<false, false, false>(
                        py, rows, count, one, &types, None,
                    )?,
                (sqlaero_core::result::value::RowEncoding::Binary, false, false) => self
                    .stream_materialize::<false, false, true>(py, rows, count, one, &types, None)?,
            }
        };
        input::check(py, budget)?;
        validate()?;
        let mut state = self.lock(py)?;
        rows.skip(count);
        state.position = rows.position;
        if let Some(total) = rows.total {
            state.total = total;
        }
        Ok(output)
    }

    /// The real send handoff clears only this Cursor's current result. Calling
    /// this during local validation would incorrectly destroy a valid old row.
    pub(crate) fn clear_result(&self, py: Python<'_>) -> PyResult<()> {
        let old = {
            let mut state = self.lock(py)?;
            let old_metadata = state.metadata.take();
            let old_columns = state.columns.take();
            state.position = 0;
            state.total = 0;
            state.abandoned = false;
            (state.data.take(), old_metadata, old_columns)
        };
        drop(old);
        Ok(())
    }
    /// Confirmed end of a sequence keeps only its final metadata/counters;
    /// explicitly abandoned native rows cannot later be recovered by scroll.
    pub(crate) fn end_set(&self, py: Python<'_>) -> PyResult<()> {
        let released = {
            let mut state = self.lock(py)?;
            state.position = state.total;
            state.abandoned = true;
            state.data.take()
        };
        drop(released);
        Ok(())
    }

    pub(crate) fn metadata(&self, py: Python<'_>) -> PyResult<Option<Arc<metadata::Snapshot>>> {
        Ok(self.lock(py)?.metadata.clone())
    }

    pub(crate) fn scroll(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        mode: &str,
    ) -> PyResult<()> {
        let _operation = self.operation(py)?;
        self.open(py)?;
        let absolute = match mode {
            "relative" => false,
            "absolute" => true,
            _ => {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "scroll mode must be 'relative' or 'absolute'",
                ));
            }
        };
        if value.is_instance_of::<PyBool>() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "scroll value must support __index__; bool is not accepted",
            ));
        }
        // SAFETY: attached Python owns value. The public integer protocol
        // returns a new owning exact integer or the original callback error.
        // No native state mutex or borrowed source pointer crosses that call.
        let index = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyNumber_Index(value.as_ptr()))?
        };
        let offset = match index.extract::<i128>() {
            Ok(value) => value,
            Err(error) if error.is_instance_of::<PyOverflowError>(py) => {
                return Err(errors::builtin_message::<pyo3::exceptions::PyIndexError>(
                    py,
                    "scroll target is outside the current result",
                ));
            }
            Err(error) => return Err(error),
        };
        py.check_signals()?;
        let mut state = self.lock(py)?;
        if state.metadata.is_none() || state.abandoned {
            drop(state);
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Cursor has no current row result to scroll",
            ));
        }
        let target = if absolute {
            Some(offset)
        } else {
            (state.position as i128).checked_add(offset)
        };
        let Some(target) = target.filter(|target| *target >= 0 && *target <= state.total as i128)
        else {
            drop(state);
            return Err(errors::builtin_message::<pyo3::exceptions::PyIndexError>(
                py,
                "scroll target is outside the current result; the position is unchanged",
            ));
        };
        state.position = target as usize;
        Ok(())
    }

    pub(crate) fn set_arraysize(&self, py: Python<'_>, value: usize) -> PyResult<()> {
        self.lock(py)?.arraysize = value;
        Ok(())
    }

    /// Decode one row using the same checked field converter for both
    /// strategies. Const specialization removes per-row strategy branches.
    #[inline(always)]
    #[expect(
        clippy::too_many_arguments,
        reason = "one shared field loop preserves atomic position and common diagnostics for both final row formats"
    )]
    fn row_tuple<'py, const DIRECT: bool, const DICT: bool, const BINARY: bool>(
        &self,
        py: Python<'py>,
        data: &RowSource<'_>,
        types: &ScalarTypes,
        row_index: usize,
        start: usize,
        progress: &mut FetchProgress,
        keys: Option<&Bound<'py, PyTuple>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let fields = row_index
            .checked_sub(data.base)
            .and_then(|index| data.rows.row(index))
            .ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "native row location is unavailable",
                )
            })?;
        let mut decode = |column_index: usize,
                          cell: &Cell,
                          plan: Plan|
         -> PyResult<Bound<'py, PyAny>> {
            let source = cell
                .get(&data.rows.bytes)
                .map_err(|error| errors::wire(py, error))?;
            let bytes = source.map_or(0, <[u8]>::len);
            let value = match source {
                None => py.None().into_bound(py),
                Some(value) => {
                    let decoded = if BINARY {
                        let column = data.rows.columns.get(column_index).ok_or_else(|| {
                            errors::client(
                                py,
                                ClientErrorCode::InternalState,
                                "binary result column is unavailable",
                            )
                        })?;
                        sqlaero_core::protocol::binary::value(column, value)
                            .map_err(|error| errors::wire(py, error))
                            .and_then(|value| {
                                output::value(py, value, plan, types, Some(data.rows.scope()))
                            })
                    } else {
                        output::field(py, value, plan, types, Some(data.rows.scope()))
                    };
                    match decoded {
                        Ok(value) => value,
                        Err(error) => {
                            let key = self
                                .state
                                .lock()
                                .ok()
                                .and_then(|state| state.metadata.clone())
                                .and_then(|metadata| metadata.diagnostic_key(py, column_index))
                                .map_or_else(String::new, |key| format!(" (key={key})"));
                            // Keep standard Unicode diagnostics without another
                            // field copy. Other failures share one bounded byte
                            // preview, independently of the output value's type.
                            let preview = if error.is_instance_of::<PyUnicodeDecodeError>(py) {
                                String::new()
                            } else {
                                format!("; {}", input::binary_diagnostic("field", value, 24))
                            };
                            errors::note(
                                py,
                                &error,
                                format!(
                                    "while decoding result row {row_index}, column {column_index}{key}; fetch position remains {start}{preview}"
                                ),
                            );
                            return Err(error);
                        }
                    }
                }
            };
            progress.field(py, bytes)?;
            Ok(value)
        };
        let row = if DICT {
            let keys = keys.ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "dictionary row keys are unavailable",
                )
            })?;
            let row = errors::dictionary(py)?;
            for (index, (cell, plan)) in fields.iter().zip(data.plans.iter()).enumerate() {
                row.set_item(keys.get_item(index)?, decode(index, cell, *plan)?)?;
            }
            row.into_any()
        } else if DIRECT {
            let mut values = fields
                .iter()
                .zip(data.plans.iter())
                .enumerate()
                .map(|(index, (cell, plan))| decode(index, cell, *plan));
            let length = values.len();
            let mut next = || {
                values.next().unwrap_or_else(|| {
                    Err(errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "native row ended early",
                    ))
                })
            };
            // This existing specialization admits at most four fields. Decode
            // those final values before allocating the row tuple; stack arrays
            // need neither a heap buffer nor GC registration transitions.
            match length {
                0 => output::tuple_from_ready(py, []),
                1 => output::tuple_from_ready(py, [next()?]),
                2 => output::tuple_from_ready(py, [next()?, next()?]),
                3 => output::tuple_from_ready(py, [next()?, next()?, next()?]),
                4 => output::tuple_from_ready(py, [next()?, next()?, next()?, next()?]),
                _ => Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "native narrow row exceeds its prepared width",
                )),
            }?
            .into_any()
        } else {
            let mut values = Vec::new();
            values
                .try_reserve(fields.len())
                .map_err(|_| errors::memory_error(py))?;
            for (index, (cell, plan)) in fields.iter().zip(data.plans.iter()).enumerate() {
                values.push(decode(index, cell, *plan)?.unbind());
            }
            output::tuple_from_ready_vec(py, values)?.into_any()
        };
        if row_index.is_multiple_of(64) {
            py.check_signals()?;
        }
        Ok(row)
    }

    /// Materialize the final result without changing the caller's position.
    /// The buffered specialization preserves the old wide-row ownership graph;
    /// the direct specialization hides unfinished tuples across callbacks.
    fn materialize<'py, const DIRECT: bool, const DICT: bool, const BINARY: bool>(
        &self,
        py: Python<'py>,
        data: &ReadData,
        types: &ScalarTypes,
        range: std::ops::Range<usize>,
        one: bool,
        keys: Option<&Bound<'py, PyTuple>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let start = range.start;
        let data = &RowSource {
            rows: &data.rows,
            plans: &data.plans,
            base: 0,
        };
        let mut progress = FetchProgress::default();
        if DIRECT {
            if one {
                self.row_tuple::<true, DICT, BINARY>(
                    py,
                    data,
                    types,
                    start,
                    start,
                    &mut progress,
                    keys,
                )
            } else if range.len() == 1 {
                let row = self.row_tuple::<true, DICT, BINARY>(
                    py,
                    data,
                    types,
                    start,
                    start,
                    &mut progress,
                    keys,
                )?;
                output::tuple_from_ready(py, [row]).map(Bound::into_any)
            } else {
                output::tuple_from_values(
                    py,
                    range.map(|index| {
                        self.row_tuple::<true, DICT, BINARY>(
                            py,
                            data,
                            types,
                            index,
                            start,
                            &mut progress,
                            keys,
                        )
                    }),
                )
                .map(Bound::into_any)
            }
        } else {
            let mut result = Vec::new();
            result
                .try_reserve(range.len())
                .map_err(|_| errors::memory_error(py))?;
            for index in range {
                result.push(
                    self.row_tuple::<false, DICT, BINARY>(
                        py,
                        data,
                        types,
                        index,
                        start,
                        &mut progress,
                        keys,
                    )?
                    .unbind(),
                );
            }
            if one {
                result
                    .pop()
                    .map(|value| value.into_bound(py))
                    .ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "single-row output is unavailable",
                        )
                    })
            } else {
                output::tuple_from_ready_vec(py, result).map(Bound::into_any)
            }
        }
    }

    fn materialize_format<'py, const BINARY: bool>(
        &self,
        py: Python<'py>,
        data: &ReadData,
        range: std::ops::Range<usize>,
        one: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let types = self.types.borrow(py);
        // Select a monomorphized strategy once per fetch. Wide rows keep
        // the original field and result buffers, outside the narrow fast path.
        if self.dictionary {
            let metadata = { self.lock(py)?.metadata.clone() };
            let metadata = metadata.ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "dictionary result metadata is unavailable",
                )
            })?;
            let keys = metadata.key_tuple(py)?;
            self.materialize::<true, true, BINARY>(py, data, &types, range, one, Some(&keys))
        } else if data.plans.len() <= 4 {
            self.materialize::<true, false, BINARY>(py, data, &types, range, one, None)
        } else {
            self.materialize::<false, false, BINARY>(py, data, &types, range, one, None)
        }
    }

    fn read<'py>(
        &self,
        py: Python<'py>,
        count: Option<usize>,
        one: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.operation(py)?;
        self.read_owned(py, count, one)
    }

    /// The caller already holds this Reader's operation for state selection and
    /// the complete read. No connection check is added to local buffered rows.
    /// Streaming consumers release a completely delivered buffered prefix at
    /// EOF; ordinary buffered cursors retain it for their approved navigation.
    pub(crate) fn release_consumed(&self) {
        let released = match self.state.lock() {
            Ok(mut state) if state.position == state.total => state.data.take(),
            Ok(_) => None,
            Err(error) => error.into_inner().data.take(),
        };
        drop(released);
    }
    /// Skip a known buffered prefix for an SS cursor without creating values.
    /// The outer Cursor holds its operation; an out-of-range forward target
    /// keeps the actual consumed EOF rather than inventing a rollback.
    pub(crate) fn forward_to(&self, py: Python<'_>, target: usize) -> PyResult<()> {
        let mut state = self.lock(py)?;
        if state.metadata.is_none() || state.abandoned {
            drop(state);
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Cursor has no current row result to scroll",
            ));
        }
        if target < state.position {
            drop(state);
            return Err(errors::client(
                py,
                ClientErrorCode::UnsupportedCapability,
                "Streaming cursors support forward navigation only",
            ));
        }
        state.position = target.min(state.total);
        let actual = state.position;
        let released = if actual == state.total {
            state.data.take()
        } else {
            None
        };
        drop(state);
        drop(released);
        if actual != target {
            return Err(errors::builtin_message::<pyo3::exceptions::PyIndexError>(
                py,
                format!(
                    "scroll target {target} is beyond EOF; the streaming cursor actually skipped to position {actual}"
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn read_owned<'py>(
        &self,
        py: Python<'py>,
        count: Option<usize>,
        one: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.open(py)?;
        py.check_signals()?;
        let current = {
            let state = self.lock(py)?;
            state.metadata.as_ref().map(|_| {
                let end = count.map_or(state.total, |count| {
                    state.position.saturating_add(count).min(state.total)
                });
                (state.data.clone(), state.position, end)
            })
        };
        let Some((data, start, end)) = current else {
            return Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                "Cursor has no current row result",
            ));
        };
        if start == end {
            return Ok(if one {
                py.None().into_bound(py)
            } else {
                PyTuple::empty(py).into_any()
            });
        }
        let Some(data) = data else {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "unread Cursor rows have no native owner",
            ));
        };
        // Choose the wire representation once per fetch, leaving the existing
        // text specialization free of a per-field binary dispatch branch.
        let output = match data.rows.encoding {
            sqlaero_core::result::value::RowEncoding::Text => {
                self.materialize_format::<false>(py, &data, start..end, one)?
            }
            sqlaero_core::result::value::RowEncoding::Binary => {
                self.materialize_format::<true>(py, &data, start..end, one)?
            }
        };
        py.check_signals()?;
        // Buffered navigation retains this current set's native rows at EOF.
        // Explicit nextset, a real new send handoff, close and final ownership
        // release remain the only boundaries which discard its row storage.
        self.lock(py)?.position = end;
        Ok(output)
    }
}

#[pymethods]
impl Reader {
    /// Return the next complete tuple row, or None after a valid result ends.
    pub(crate) fn fetchone<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, Some(1), true)
            .map_err(|error| errors::at(py, error, "cursor.fetchone"))
    }
    /// Return at most size tuple rows atomically. None uses saved arraysize.
    #[pyo3(signature = (size=None))]
    pub(crate) fn fetchmany<'py>(
        &self,
        py: Python<'py>,
        size: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let size = self.fetch_size(py, size)?;
        self.read(py, Some(size), false)
            .map_err(|error| errors::at(py, error, "cursor.fetchmany"))
    }
    /// Return all remaining tuple rows, committing position only after success.
    pub(crate) fn fetchall<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, None, false)
            .map_err(|error| errors::at(py, error, "cursor.fetchall"))
    }
    /// Permanently retire this reader locally. Saved metadata remains readable.
    pub(crate) fn close(&self, py: Python<'_>) -> PyResult<()> {
        let _operation = self.operation(py)?;
        let _ = self.lifecycle.retire();
        let released = self.lock(py)?.data.take();
        drop(released);
        Ok(())
    }
    #[getter]
    fn closed(&self) -> bool {
        self.lifecycle.is_retired()
    }
    /// Internal verification position, not an additional public Cursor API.
    #[getter]
    pub(crate) fn position(&self, py: Python<'_>) -> PyResult<usize> {
        Ok(self.lock(py)?.position)
    }
    /// Actual cached column labels, without preparing unique lookup keys.
    #[getter]
    fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let Some(metadata) = self.metadata(py)? else {
            return Ok(PyTuple::empty(py));
        };
        metadata.names(py)
    }
    /// Immutable metadata for the current result; usable after close. Creating
    /// this view does not materialize labels, keys, or ColumnInfo objects.
    #[getter]
    pub(crate) fn columns(&self, py: Python<'_>) -> PyResult<Py<metadata::Columns>> {
        let snapshot = {
            let state = self.lock(py)?;
            if let Some(value) = &state.columns {
                return Ok(value.clone_ref(py));
            }
            state.metadata.clone()
        };
        let view = match &snapshot {
            Some(snapshot) => Arc::clone(snapshot),
            None => metadata::Snapshot::new(py, None, OutputCodec::for_collation(255), true)?,
        };
        let value = Py::new(py, metadata::Columns::new(view))?;
        let mut state = self.lock(py)?;
        let current = match (&snapshot, &state.metadata) {
            (Some(before), Some(now)) => Arc::ptr_eq(before, now),
            (None, None) => true,
            _ => false,
        };
        if current {
            if let Some(value) = &state.columns {
                return Ok(value.clone_ref(py));
            }
            state.columns = Some(value.clone_ref(py));
        }
        Ok(value)
    }
    /// PEP 249 seven-field tuples from the same execution-time metadata.
    #[getter]
    pub(crate) fn description<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self.metadata(py)? {
            Some(metadata) => metadata.description(py),
            None => Ok(py.None().into_bound(py)),
        }
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let value = self.fetchone(py)?;
        if value.is_none() {
            Err(PyStopIteration::new_err(()))
        } else {
            Ok(value)
        }
    }
}
