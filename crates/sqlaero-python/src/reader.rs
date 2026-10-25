//! Buffered fetch ownership and one atomic position commit per output call.

use crate::{errors, input::ScalarTypes, metadata, output};
use pyo3::exceptions::{PyOverflowError, PyStopIteration, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyTuple};
use sqlaero_core::charset::output::OutputCodec;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, Reservation};
use sqlaero_core::result::{
    decode::{Options, Plan},
    text::Buffered,
};
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

struct ReadData {
    rows: Buffered,
    plans: Buffer<Plan>,
    _owner: Reservation,
}
struct State {
    data: Option<Arc<ReadData>>,
    metadata: Option<Arc<metadata::Snapshot>>,
    columns: Option<Py<metadata::Columns>>,
    position: usize,
    total: usize,
    arraysize: usize,
}

/// A concrete per-Cursor operation owner. It never owns the connection wire.
pub(crate) struct Operation<'a>(&'a AtomicBool);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// The production Rust read component. Cursor embeds this ordinary Rust value;
/// its private Python exposure only permits direct contract checks and adds no
/// extra Python object or dispatch layer to production Cursor calls.
#[pyclass(frozen, name = "_Rows", module = "sqlaero._sqlaero")]
pub(crate) struct Reader {
    state: Mutex<State>,
    active: AtomicBool,
    closed: AtomicBool,
    types: Py<ScalarTypes>,
}
impl Reader {
    pub(crate) fn empty(types: Py<ScalarTypes>) -> Self {
        Self {
            state: Mutex::new(State {
                data: None,
                metadata: None,
                columns: None,
                position: 0,
                total: 0,
                arraysize: 1,
            }),
            active: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            types,
        }
    }
    pub(crate) fn operation(&self, py: Python<'_>) -> PyResult<Operation<'_>> {
        if self
            .active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(errors::client::<errors::InterfaceBusyError>(
                py,
                ClientErrorCode::InterfaceBusy,
                "Cursor is busy executing or reading another operation",
            ));
        }
        Ok(Operation(&self.active))
    }
    fn lock(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, State>> {
        match self.state.lock() {
            Ok(state) => Ok(state),
            Err(error) => {
                self.closed.store(true, Ordering::Release);
                error.into_inner().data.take();
                Err(errors::client::<errors::InternalError>(
                    py,
                    ClientErrorCode::InternalState,
                    "Cursor was retired after a native state failure",
                ))
            }
        }
    }
    fn open(&self, py: Python<'_>) -> PyResult<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(errors::client::<errors::InterfaceError>(
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
                    .push(Plan::new(
                        column.type_code,
                        column.flags,
                        column.collation_id,
                        column.column_length,
                        options,
                        codec,
                    ))
                    .map_err(|error| output::memory(py, error))?;
            }
            Some(Arc::new(ReadData {
                rows,
                plans,
                _owner: owner,
            }))
        };
        let old = {
            let mut state = self.lock(py)?;
            let old_metadata = state.metadata.replace(metadata);
            let old_columns = state.columns.take();
            state.position = 0;
            state.total = total;
            (
                std::mem::replace(&mut state.data, data),
                old_metadata,
                old_columns,
            )
        };
        drop(old);
        Ok(())
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
            (state.data.take(), old_metadata, old_columns)
        };
        drop(old);
        Ok(())
    }
    pub(crate) fn metadata(&self, py: Python<'_>) -> PyResult<Option<Arc<metadata::Snapshot>>> {
        Ok(self.lock(py)?.metadata.clone())
    }

    fn read<'py>(
        &self,
        py: Python<'py>,
        count: Option<usize>,
        one: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.operation(py)?;
        self.open(py)?;
        py.check_signals()?;
        let (data, start, end) = {
            let state = self.lock(py)?;
            if state.metadata.is_none() {
                return Err(errors::client::<errors::InterfaceError>(
                    py,
                    ClientErrorCode::SessionState,
                    "Cursor has no current row result",
                ));
            }
            let end = count.map_or(state.total, |count| {
                state.position.saturating_add(count).min(state.total)
            });
            (state.data.clone(), state.position, end)
        };
        if start == end {
            return Ok(if one {
                py.None().into_bound(py)
            } else {
                PyTuple::empty(py).into_any()
            });
        }
        let Some(data) = data else {
            return Err(errors::client::<errors::InternalError>(
                py,
                ClientErrorCode::InternalState,
                "unread Cursor rows have no native owner",
            ));
        };
        let types = self.types.borrow(py);
        let mut result = Vec::new();
        result
            .try_reserve(end - start)
            .map_err(|error| pyo3::exceptions::PyMemoryError::new_err(error.to_string()))?;
        for row_index in start..end {
            let fields = data.rows.row(row_index).ok_or_else(|| {
                errors::client::<errors::InternalError>(
                    py,
                    ClientErrorCode::InternalState,
                    "native row location is unavailable",
                )
            })?;
            let mut values = Vec::new();
            values
                .try_reserve(fields.len())
                .map_err(|error| pyo3::exceptions::PyMemoryError::new_err(error.to_string()))?;
            for (column_index, (cell, plan)) in fields.iter().zip(data.plans.iter()).enumerate() {
                let value = match cell
                    .get(&data.rows.bytes)
                    .map_err(|error| errors::wire(py, error))?
                {
                    None => py.None().into_bound(py),
                    Some(value) => {
                        match output::field(py, value, *plan, &types, Some(data.rows.scope())) {
                            Ok(value) => value,
                            Err(error) => {
                                let _ = error.add_note(py, format!("while decoding result row {row_index}, column {column_index}; fetch position remains {start}"));
                                return Err(error);
                            }
                        }
                    }
                };
                values.push(value.unbind());
                if column_index % 256 == 0 {
                    py.check_signals()?;
                }
            }
            result.push(PyTuple::new(py, values)?.unbind());
            if row_index % 64 == 0 {
                py.check_signals()?;
            }
        }
        let output = if one {
            result
                .pop()
                .ok_or_else(|| {
                    errors::client::<errors::InternalError>(
                        py,
                        ClientErrorCode::InternalState,
                        "single-row output is unavailable",
                    )
                })?
                .into_bound(py)
                .into_any()
        } else {
            PyTuple::new(py, result)?.into_any()
        };
        py.check_signals()?;
        let released = {
            let mut state = self.lock(py)?;
            state.position = end;
            if end == state.total {
                state.data.take()
            } else {
                None
            }
        };
        drop(released);
        Ok(output)
    }
}

#[pymethods]
impl Reader {
    /// Return the next complete tuple row, or None after a valid result ends.
    pub(crate) fn fetchone<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, Some(1), true)
    }
    /// Return at most size tuple rows atomically. None uses saved arraysize.
    #[pyo3(signature = (size=None))]
    pub(crate) fn fetchmany<'py>(
        &self,
        py: Python<'py>,
        size: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let size = match size {
            None => self.lock(py)?.arraysize,
            Some(size) if size.is_none() => self.lock(py)?.arraysize,
            Some(size)
                if size.is_exact_instance_of::<PyInt>() && !size.is_instance_of::<PyBool>() =>
            {
                if size.lt(0)? {
                    return Err(PyValueError::new_err("fetchmany size must not be negative"));
                }
                match size.extract::<usize>() {
                    Ok(value) => value,
                    Err(error) if error.is_instance_of::<PyOverflowError>(py) => usize::MAX,
                    Err(error) => return Err(error),
                }
            }
            Some(_) => {
                return Err(PyTypeError::new_err(
                    "fetchmany size must be an int or None; bool is not accepted",
                ));
            }
        };
        self.read(py, Some(size), false)
    }
    /// Return all remaining tuple rows, committing position only after success.
    pub(crate) fn fetchall<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, None, false)
    }
    /// Permanently retire this reader locally. Saved metadata remains readable.
    pub(crate) fn close(&self, py: Python<'_>) -> PyResult<()> {
        let _operation = self.operation(py)?;
        self.closed.store(true, Ordering::Release);
        let released = self.lock(py)?.data.take();
        drop(released);
        Ok(())
    }
    #[getter]
    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    /// Internal verification position, not an additional public Cursor API.
    #[getter]
    fn position(&self, py: Python<'_>) -> PyResult<usize> {
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
