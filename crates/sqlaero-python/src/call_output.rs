//! Local, independently owned managed-CALL outputs and atomic Python caching.

use crate::{errors, input, input::ScalarTypes, metadata, output, reader};
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use sqlaero_core::charset::output::OutputCodec;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, Reservation};
use sqlaero_core::result::{
    decode::{Options, Plan},
    text::RowBatch,
};
use std::mem::size_of;
use std::sync::{Arc, OnceLock};

struct Native {
    rows: Arc<RowBatch>,
    indices: Buffer<usize>,
    plans: Buffer<Plan>,
    metadata: Arc<metadata::Snapshot>,
    _owner: Reservation,
}

/// One confirmed output row, never the Connection or public result sequence.
/// The owner can survive Cursor changes/close and retains no input parameters.
pub(crate) struct Snapshot {
    native: Option<Native>,
    types: Py<ScalarTypes>,
    cache: OnceLock<Py<PyAny>>,
}

impl Snapshot {
    /// Confirmed absence of OUT/INOUT differs from an unready outer Option.
    pub(crate) fn empty(types: Py<ScalarTypes>) -> Self {
        Self {
            native: None,
            types,
            cache: OnceLock::new(),
        }
    }

    /// Freeze the actual one-row output and its operation-time decode policy.
    /// The managed executor validates completion before handing over this row.
    pub(crate) fn new(
        py: Python<'_>,
        rows: Arc<RowBatch>,
        indices: Buffer<usize>,
        options: Options,
        codec: OutputCodec,
        types: Py<ScalarTypes>,
    ) -> PyResult<Self> {
        if rows.row_count != 1
            || rows.columns.len() != indices.len()
            || rows.fields.len() != indices.len()
            || indices.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "managed CALL outputs require one complete row and ordered original argument positions",
            ));
        }
        let owner = rows
            .scope()
            .charge(size_of::<Self>() + 2 * size_of::<usize>())
            .map_err(|error| output::memory(py, error))?;
        let metadata = metadata::Snapshot::new(
            py,
            Some(Arc::clone(&rows.columns)),
            codec,
            options.decode_text_strict,
        )?;
        let mut plans = Buffer::tracked(rows.scope());
        plans
            .reserve(indices.len())
            .map_err(|error| output::memory(py, error))?;
        for (index, column) in rows.columns.iter().enumerate() {
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
            if index.is_multiple_of(64) {
                py.check_signals()?;
            }
        }
        Ok(Self {
            native: Some(Native {
                rows,
                indices,
                plans,
                metadata,
                _owner: owner,
            }),
            types,
            cache: OnceLock::new(),
        })
    }

    /// Decode locally and publish only a complete read-only mapping. A failed
    /// conversion leaves the native owner intact and never saves the exception.
    /// The caller holds its local Cursor operation permit during an uncached
    /// read; this also rejects callbacks recursively accessing the same getter.
    pub(crate) fn get<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if let Some(value) = self.cached(py) {
            return Ok(value);
        }
        py.check_signals()?;
        let dictionary = if let Some(native) = &self.native {
            let keys = output::tuple_from_values(
                py,
                native.indices.iter().enumerate().map(|(column, position)| {
                    if column.is_multiple_of(64) {
                        py.check_signals()?;
                    }
                    input::python_index(py, *position)
                }),
            )?;
            reader::Reader::snapshot_dict(
                py,
                &native.rows,
                &native.plans,
                &self.types,
                &native.metadata,
                &keys,
            )
            .inspect_err(|error| {
                errors::note(
                    py,
                    error,
                    "while decoding confirmed CALL output parameters; native outputs remain available",
                );
            })?
        } else {
            errors::dictionary(py)?
        };
        // SAFETY: this attached constructor owns a reference to the exact dict.
        // The dict is private and never mutated after its proxy is published.
        // NULL preserves the actual Python allocation exception.
        let mapping = unsafe {
            Bound::<PyAny>::from_owned_ptr_or_err(
                py,
                pyo3::ffi::PyDictProxy_New(dictionary.as_ptr()),
            )?
        };
        py.check_signals()?;
        // set performs only a native publication; Python decoding, callbacks,
        // allocation, signal checks and loser destruction all happen outside it.
        if let Err(unpublished) = self.cache.set(mapping.unbind()) {
            drop(unpublished);
        }
        self.cache
            .get()
            .map(|value| value.clone_ref(py).into_bound(py))
            .ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "managed CALL output cache publication is unavailable",
                )
            })
    }

    /// A cache hit needs no Cursor operation permit or network ownership.
    pub(crate) fn cached<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyAny>> {
        self.cache
            .get()
            .map(|value| value.clone_ref(py).into_bound(py))
    }

    /// The outer Cursor visits this snapshot while it owns its native handle.
    pub(crate) fn traverse(&self, visit: &PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.types)?;
        if let Some(value) = self.cache.get() {
            visit.call(value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::exceptions::{PyKeyboardInterrupt, PyMemoryError, PyRuntimeError, PyTypeError};
    use pyo3::types::{PyDict, PyTuple};
    use sqlaero_core::memory::{Pool, Scope};
    use sqlaero_core::protocol::CapabilityFlags;
    use sqlaero_core::result::{
        text::{Receiver, Response},
        value::RowEncoding,
    };

    fn assert_gil_mode(py: Python<'_>) {
        let enabled = py
            .import("sys")
            .unwrap()
            .call_method0("_is_gil_enabled")
            .unwrap()
            .extract::<bool>()
            .unwrap();
        assert_eq!(enabled, !cfg!(Py_GIL_DISABLED));
    }

    fn types(py: Python<'_>) -> Py<ScalarTypes> {
        assert_gil_mode(py);
        Py::new(py, ScalarTypes::new(py).unwrap()).unwrap()
    }

    fn rows(encoding: RowEncoding, columns: &[(u8, u16)], data: &[&[u8]]) -> Arc<RowBatch> {
        let scope = Scope::new(Pool::new(usize::MAX), 1024 * 1024).unwrap();
        let mut receiver = Receiver::with_encoding(
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            scope,
            false,
            encoding,
        )
        .unwrap();
        receiver.push(&[columns.len() as u8]).unwrap();
        for (code, collation) in columns {
            let mut packet = b"\x03def\x00\x00\x00\x01v\x01v\x0c".to_vec();
            packet.extend_from_slice(&collation.to_le_bytes());
            packet.extend_from_slice(&255_u32.to_le_bytes());
            packet.extend_from_slice(&[*code, 0, 0, 2, 0, 0]);
            receiver.push(&packet).unwrap();
        }
        for packet in data {
            receiver.push(packet).unwrap();
        }
        match receiver.push(b"\xfe\x00\x00\x02\x00\x00\x00").unwrap() {
            Some(Response::Rows(rows)) => Arc::new(rows.rows),
            _ => panic!("fixture must produce completed row storage"),
        }
    }

    fn snapshot(
        py: Python<'_>,
        rows: Arc<RowBatch>,
        positions: &[usize],
        options: Options,
    ) -> Snapshot {
        let mut indices = Buffer::tracked(rows.scope());
        for position in positions {
            indices.push(*position).unwrap();
        }
        Snapshot::new(
            py,
            rows,
            indices,
            options,
            OutputCodec::for_collation(255),
            types(py),
        )
        .unwrap()
    }

    #[test]
    fn confirmed_empty_is_readonly_and_cached() {
        Python::initialize();
        Python::attach(|py| {
            let snapshot = Snapshot::empty(types(py));
            assert!(snapshot.cached(py).is_none());
            let mapping = snapshot.get(py).unwrap();
            assert_eq!(mapping.len().unwrap(), 0);
            assert!(mapping.is(snapshot.get(py).unwrap()));
            assert!(
                mapping
                    .set_item(1, 2)
                    .unwrap_err()
                    .is_instance_of::<PyTypeError>(py)
            );
        });
    }

    #[test]
    fn real_fields_original_positions_and_values_outlive_native_rows() {
        Python::initialize();
        Python::attach(|py| {
            let rows = rows(
                RowEncoding::Text,
                &[(3, 63), (246, 63), (245, 255)],
                &[b"\x017\x0512.50\x07{\"v\":1}"],
            );
            let weak = Arc::downgrade(&rows);
            let snapshot = snapshot(
                py,
                rows,
                &[1, 4, 7],
                Options {
                    decode_json: true,
                    ..Options::default()
                },
            );
            let mapping = snapshot.get(py).unwrap();
            assert_eq!(mapping.get_item(1).unwrap().extract::<i64>().unwrap(), 7);
            assert_eq!(
                mapping
                    .get_item(4)
                    .unwrap()
                    .str()
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "12.50"
            );
            let json = mapping.get_item(7).unwrap();
            assert_eq!(json.get_item("v").unwrap().extract::<i64>().unwrap(), 1);
            json.set_item("v", 9).unwrap();
            assert!(mapping.is(snapshot.get(py).unwrap()));
            assert!(
                mapping
                    .set_item(4, 99)
                    .unwrap_err()
                    .is_instance_of::<PyTypeError>(py)
            );
            let keys = mapping.call_method0("keys").unwrap();
            let keys = py.get_type::<PyTuple>().call1((keys,)).unwrap();
            assert_eq!(keys.extract::<Vec<usize>>().unwrap(), [1, 4, 7]);
            drop(snapshot);
            assert!(weak.upgrade().is_none());
            assert_eq!(json.get_item("v").unwrap().extract::<i64>().unwrap(), 9);
            assert_eq!(mapping.get_item(1).unwrap().extract::<i64>().unwrap(), 7);
        });
    }

    #[test]
    fn binary_null_and_frozen_options_use_the_common_decoder() {
        Python::initialize();
        Python::attach(|py| {
            let native = rows(
                RowEncoding::Binary,
                &[(3, 63), (3, 63)],
                &[b"\x00\x08\x07\x00\x00\x00"],
            );
            let outputs = snapshot(py, native, &[2, 5], Options::default());
            let mapping = outputs.get(py).unwrap();
            assert_eq!(mapping.get_item(2).unwrap().extract::<i64>().unwrap(), 7);
            assert!(mapping.get_item(5).unwrap().is_none());
            let native = rows(RowEncoding::Text, &[(246, 63)], &[b"\x0512.50"]);
            let decimal = snapshot(py, Arc::clone(&native), &[3], Options::default());
            let float = snapshot(
                py,
                native,
                &[3],
                Options {
                    use_decimal: false,
                    ..Options::default()
                },
            );
            assert_eq!(
                decimal
                    .get(py)
                    .unwrap()
                    .get_item(3)
                    .unwrap()
                    .str()
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "12.50"
            );
            assert_eq!(
                float
                    .get(py)
                    .unwrap()
                    .get_item(3)
                    .unwrap()
                    .extract::<f64>()
                    .unwrap(),
                12.5
            );
        });
    }

    #[test]
    fn content_failure_does_not_publish_or_discard_native_values() {
        Python::initialize();
        Python::attach(|py| {
            let native = rows(RowEncoding::Text, &[(3, 63)], &[b"\x03bad"]);
            let snapshot = snapshot(py, native, &[4], Options::default());
            for _ in 0..2 {
                let error = snapshot.get(py).unwrap_err();
                assert_eq!(
                    error
                        .value(py)
                        .getattr("code")
                        .unwrap()
                        .extract::<u32>()
                        .unwrap(),
                    ClientErrorCode::DecodeContent as u32
                );
                assert!(snapshot.cached(py).is_none());
                assert!(snapshot.native.as_ref().unwrap().rows.row(0).is_some());
            }
        });
    }

    #[test]
    fn constructor_rejects_inconsistent_row_shape_and_duplicate_positions() {
        Python::initialize();
        Python::attach(|py| {
            for (native, positions) in [
                (
                    rows(RowEncoding::Text, &[(3, 63)], &[b"\x017", b"\x018"]),
                    vec![1],
                ),
                (
                    rows(RowEncoding::Text, &[(3, 63), (3, 63)], &[b"\x017\x018"]),
                    vec![1, 1],
                ),
                (rows(RowEncoding::Text, &[(3, 63)], &[b"\x017"]), vec![1, 2]),
            ] {
                let mut indices = Buffer::tracked(native.scope());
                for position in positions {
                    indices.push(position).unwrap();
                }
                assert!(
                    Snapshot::new(
                        py,
                        Arc::clone(&native),
                        indices,
                        Options::default(),
                        OutputCodec::for_collation(255),
                        types(py)
                    )
                    .is_err()
                );
                assert!(native.row(0).is_some());
            }
        });
    }

    #[test]
    fn constructor_quota_failure_keeps_the_callers_native_owner_retryable() {
        Python::initialize();
        Python::attach(|py| {
            let native = rows(RowEncoding::Text, &[(3, 63)], &[b"\x017"]);
            let mut indices = Buffer::tracked(native.scope());
            indices.push(3).unwrap();
            let exhausted = native
                .scope()
                .charge(1024 * 1024 - native.scope().used())
                .unwrap();
            let result = Snapshot::new(
                py,
                Arc::clone(&native),
                indices,
                Options::default(),
                OutputCodec::for_collation(255),
                types(py),
            );
            let Err(error) = result else {
                panic!("full scope must reject the new owner");
            };
            assert_eq!(
                error
                    .value(py)
                    .getattr("code")
                    .unwrap()
                    .extract::<u32>()
                    .unwrap(),
                ClientErrorCode::ResourceLimit as u32
            );
            assert!(native.row(0).is_some());
            drop(exhausted);
            let outputs = snapshot(py, native, &[3], Options::default());
            assert_eq!(
                outputs
                    .get(py)
                    .unwrap()
                    .get_item(3)
                    .unwrap()
                    .extract::<i64>()
                    .unwrap(),
                7
            );
        });
    }

    // This helper exists only in the Rust unit-test binary, never in the
    // production module or its public/private Python registration table.
    #[pyclass(frozen)]
    struct Probe {
        snapshot: Snapshot,
    }
    #[pymethods]
    impl Probe {
        fn get<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            self.snapshot.get(py)
        }

        fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
            self.snapshot.traverse(&visit)
        }
    }

    #[test]
    fn bounded_python_allocation_failures_retry_the_same_native_snapshot() {
        Python::initialize();
        Python::attach(|py| {
            let capi = py.import("_testcapi").unwrap();
            let bounded = capi.getattr("call_with_nomemory").ok();
            let enable = capi.getattr("set_nomemory").unwrap();
            let disable = capi.getattr("remove_mem_hooks").unwrap();
            let native = rows(RowEncoding::Text, &[(246, 63)], &[b"\x0512.50"]);
            let mut failures = 0;
            for offset in 0..32 {
                let probe = Py::new(
                    py,
                    Probe {
                        snapshot: snapshot(py, Arc::clone(&native), &[3], Options::default()),
                    },
                )
                .unwrap();
                let operation = probe.bind(py).getattr("get").unwrap();
                let result = if let Some(bounded) = &bounded {
                    bounded.call1((offset, offset + 1, &operation))
                } else {
                    enable.call1((offset, offset + 1)).unwrap();
                    let result = operation.call0();
                    disable.call0().unwrap();
                    result
                };
                if let Err(error) = result {
                    assert!(error.is_instance_of::<PyMemoryError>(py), "{error}");
                    failures += 1;
                    assert!(probe.borrow(py).snapshot.cached(py).is_none());
                }
                let mapping = operation.call0().unwrap();
                assert_eq!(
                    mapping
                        .get_item(3)
                        .unwrap()
                        .str()
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    "12.50"
                );
                assert!(mapping.is(operation.call0().unwrap()));
                if offset == 0 {
                    let referents = py
                        .import("gc")
                        .unwrap()
                        .call_method1("get_referents", (&probe,))
                        .unwrap();
                    assert!(
                        referents
                            .try_iter()
                            .unwrap()
                            .any(|value| value.unwrap().is(&mapping))
                    );
                }
            }
            assert!(failures > 0);
            assert_gil_mode(py);
        });
    }

    #[test]
    fn original_callback_exception_and_interrupt_are_retryable() {
        Python::initialize();
        Python::attach(|py| {
            let decimal = py.import("decimal").unwrap();
            let original_type = decimal.getattr("Decimal").unwrap();
            let context = PyDict::new(py);
            context.set_item("Original", &original_type).unwrap();
            py.run(c"failures = [RuntimeError('output callback primary'), KeyboardInterrupt('output interrupt')]\noriginals = tuple(failures)\nclass Decimal(Original):\n def __new__(cls, value):\n  if failures: raise failures.pop(0)\n  return Original.__new__(cls, value)", Some(&context), None).unwrap();
            decimal
                .setattr("Decimal", context.get_item("Decimal").unwrap().unwrap())
                .unwrap();
            let captured_types = types(py);
            decimal.setattr("Decimal", original_type).unwrap();
            let native = rows(RowEncoding::Text, &[(246, 63)], &[b"\x0512.50"]);
            let mut indices = Buffer::tracked(native.scope());
            indices.push(2).unwrap();
            let snapshot = Snapshot::new(
                py,
                native,
                indices,
                Options::default(),
                OutputCodec::for_collation(255),
                captured_types,
            )
            .unwrap();
            let originals = context.get_item("originals").unwrap().unwrap();
            for index in 0..2 {
                let error = snapshot.get(py).unwrap_err();
                assert!(error.value(py).is(originals.get_item(index).unwrap()));
                assert!(if index == 0 {
                    error.is_instance_of::<PyRuntimeError>(py)
                } else {
                    error.is_instance_of::<PyKeyboardInterrupt>(py)
                });
                assert!(snapshot.cached(py).is_none());
            }
            assert_eq!(
                snapshot
                    .get(py)
                    .unwrap()
                    .get_item(2)
                    .unwrap()
                    .str()
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "12.50"
            );
        });
    }
}
