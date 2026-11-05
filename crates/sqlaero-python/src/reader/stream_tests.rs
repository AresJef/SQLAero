//! The real parser supplies every batch; no synthetic completion is constructed.

use super::*;
use pyo3::types::{PyDict, PyModule};
use sqlaero_core::memory::{Pool, Scope};
use sqlaero_core::protocol::CapabilityFlags;
use sqlaero_core::result::text::{Header, Receiver, Response, RowBatch};
use sqlaero_core::result::value::RowEncoding;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};
use std::time::Duration;

fn scope() -> Arc<Scope> {
    Scope::new(Pool::new(1_000_000), 1_000_000).unwrap()
}
fn batch(scope: &Arc<Scope>, columns: &[(&str, u8)], rows: &[Vec<u8>], binary: bool) -> RowBatch {
    buffered(scope, columns, rows, binary).rows
}
fn buffered(
    scope: &Arc<Scope>,
    columns: &[(&str, u8)],
    rows: &[Vec<u8>],
    binary: bool,
) -> Buffered {
    let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF;
    let mut receiver = Receiver::with_encoding(
        capabilities,
        Arc::clone(scope),
        false,
        if binary {
            RowEncoding::Binary
        } else {
            RowEncoding::Text
        },
    )
    .unwrap();
    receiver.push(&[columns.len() as u8]).unwrap();
    for (name, kind) in columns {
        let mut packet = vec![3, b'd', b'e', b'f', 0, 0, 0, name.len() as u8];
        packet.extend_from_slice(name.as_bytes());
        packet.extend_from_slice(&[0, 12, 255, 0, 32, 0, 0, 0, *kind, 0, 0, 0, 0, 0]);
        receiver.push(&packet).unwrap();
    }
    for row in rows {
        receiver.push(row).unwrap();
    }
    let Some(Response::Rows(rows)) = receiver.push(&[0xfe, 0, 0, 2, 0, 0, 0]).unwrap() else {
        panic!("actual parser did not produce rows")
    };
    rows
}
fn header(rows: &RowBatch) -> Header {
    Header {
        encoding: rows.encoding,
        columns: Arc::clone(&rows.columns),
        metadata_status: rows.metadata_status,
    }
}
fn budget() -> IoBudget {
    IoBudget::new(TimeoutKind::Operation, None, None, None)
}
fn reader(py: Python<'_>, dictionary: bool) -> Reader {
    Reader::with_format(
        Py::new(py, ScalarTypes::new(py).unwrap()).unwrap(),
        dictionary,
    )
}

#[test]
fn reserve_precedes_append_and_empty_queue_returns_real_capacity() {
    let scope = scope();
    let rows = batch(&scope, &[("v", 3)], &[vec![1, b'1'], vec![1, b'2']], false);
    let mut cache = StreamRows::new(header(&rows), Options::default()).unwrap();
    cache.reserve_batch().unwrap();
    let reserved = scope.used();
    cache.push(rows).unwrap();
    assert_eq!(
        scope.used(),
        reserved,
        "append after reservation must not allocate"
    );
    assert_eq!(
        (cache.available(), cache.position(), cache.is_complete()),
        (2, 0, false)
    );
    assert_eq!(cache.skip(1), 1);
    assert_eq!((cache.available(), cache.position()), (1, 1));
    assert_eq!(cache.skip(10), 1);
    assert_eq!((cache.available(), cache.position()), (0, 2));
    assert!(scope.used() < reserved);
    assert_eq!(cache.batches.capacity(), 0);
    cache.finish(2);
    assert!(cache.is_complete());
}

#[test]
fn repeated_prefix_consumption_does_not_retain_old_batches_or_grow_heads() {
    let scope = scope();
    let first = batch(&scope, &[("v", 3)], &[vec![1, b'0']], false);
    let mut cache = StreamRows::new(header(&first), Options::default()).unwrap();
    drop(first);
    let empty = scope.used();
    for _ in 0..512 {
        cache.reserve_batch().unwrap();
        cache
            .push(batch(&scope, &[("v", 3)], &[vec![1, b'1']], false))
            .unwrap();
        assert_eq!(cache.skip(1), 1);
        assert_eq!(
            (cache.available(), cache.head, cache.batches.capacity()),
            (0, 0, 0)
        );
        assert_eq!(scope.used(), empty);
    }
    assert_eq!(cache.position(), 512);
}

#[test]
fn reservation_failure_preserves_existing_cache_and_credit() {
    let scope = scope();
    let rows = batch(&scope, &[("v", 3)], &[vec![1, b'1']], false);
    let mut cache = StreamRows::new(header(&rows), Options::default()).unwrap();
    cache.reserve_batch().unwrap();
    cache.push(rows).unwrap();
    let length = cache.batches.capacity();
    for _ in 1..length {
        cache
            .push(batch(&scope, &[("v", 3)], &[vec![1, b'1']], false))
            .unwrap();
    }
    let blocker = scope.charge(1_000_000 - scope.used() - 1).unwrap();
    let before = scope.used();
    assert!(cache.reserve_batch().is_err());
    assert_eq!(scope.used(), before);
    assert_eq!((cache.available(), cache.position()), (length, 0));
    drop(blocker);
    cache.reserve_batch().unwrap();
    cache
        .push(batch(&scope, &[("v", 3)], &[vec![1, b'2']], false))
        .unwrap();
    cache.discard();
    assert_eq!(
        (cache.available(), cache.position(), cache.is_complete()),
        (0, length + 1, false)
    );
}

#[test]
fn partial_prefix_queue_compacts_without_losing_remaining_order() {
    let scope = scope();
    let first = batch(&scope, &[("v", 3)], &[vec![1, b'0']], false);
    let mut cache = StreamRows::new(header(&first), Options::default()).unwrap();
    cache.reserve_batch().unwrap();
    cache.push(first).unwrap();
    for _ in 0..128 {
        cache.reserve_batch().unwrap();
        cache
            .push(batch(&scope, &[("v", 3)], &[vec![1, b'1']], false))
            .unwrap();
        assert_eq!(cache.skip(1), 1);
        assert_eq!(cache.available(), 1);
        assert!(cache.head < 8 && cache.batches.len() <= 8);
    }
    assert_eq!(cache.skip(1), 1);
    assert_eq!(cache.position(), 129);
}

#[test]
fn failed_batch_materialization_keeps_every_row_and_success_commits_once() {
    Python::initialize();
    Python::attach(|py| {
        let reader = reader(py, false);
        let scope = scope();
        let rows = batch(
            &scope,
            &[("v", 3)],
            &[vec![1, b'1'], vec![3, b'b', b'a', b'd']],
            false,
        );
        let _operation = reader.operation(py).unwrap();
        let mut cache = reader
            .stream_header(
                py,
                header(&rows),
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        cache.reserve_batch().unwrap();
        cache.push(rows).unwrap();
        assert!(
            reader
                .stream_read(py, &mut cache, Some(2), false, &budget(), || Ok(()))
                .is_err()
        );
        assert_eq!((cache.available(), cache.position()), (2, 0));
        let one = reader
            .stream_read(py, &mut cache, Some(1), true, &budget(), || Ok(()))
            .unwrap();
        assert_eq!(one.extract::<(i64,)>().unwrap(), (1,));
        assert_eq!((cache.available(), cache.position()), (1, 1));
        assert!(
            reader
                .stream_read(py, &mut cache, None, false, &budget(), || Ok(()))
                .is_err()
        );
        assert_eq!((cache.available(), cache.position()), (1, 1));
    });
}

#[test]
fn dictionary_output_spans_batches_and_reuses_keys_with_independent_rows() {
    Python::initialize();
    Python::attach(|py| {
        let reader = reader(py, true);
        let scope = scope();
        let first = batch(&scope, &[("v", 3)], &[vec![1, b'1']], false);
        let _operation = reader.operation(py).unwrap();
        let mut cache = reader
            .stream_header(
                py,
                header(&first),
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        cache.reserve_batch().unwrap();
        cache.push(first).unwrap();
        cache.reserve_batch().unwrap();
        cache
            .push(batch(&scope, &[("v", 3)], &[vec![1, b'2']], false))
            .unwrap();
        let output = reader
            .stream_read(py, &mut cache, None, false, &budget(), || Ok(()))
            .unwrap();
        let output = output.cast::<PyTuple>().unwrap();
        let first = output.get_item(0).unwrap().cast_into::<PyDict>().unwrap();
        let second = output.get_item(1).unwrap().cast_into::<PyDict>().unwrap();
        assert!(
            first
                .keys()
                .get_item(0)
                .unwrap()
                .is(second.keys().get_item(0).unwrap())
        );
        first.set_item("v", 99).unwrap();
        assert_eq!(
            second
                .get_item("v")
                .unwrap()
                .unwrap()
                .extract::<i64>()
                .unwrap(),
            2
        );
        assert_eq!((cache.available(), cache.position()), (0, 2));
    });
}

#[test]
fn binary_native_rows_share_the_same_output_loop() {
    Python::initialize();
    Python::attach(|py| {
        let reader = reader(py, false);
        let scope = scope();
        let mut packet = vec![0, 0];
        packet.extend_from_slice(&(-17_i64).to_le_bytes());
        let rows = batch(&scope, &[("v", 8)], &[packet], true);
        let _operation = reader.operation(py).unwrap();
        let mut cache = reader
            .stream_header(
                py,
                header(&rows),
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        cache.reserve_batch().unwrap();
        cache.push(rows).unwrap();
        cache.finish(1);
        let output = reader
            .stream_read(py, &mut cache, Some(1), true, &budget(), || Ok(()))
            .unwrap();
        assert_eq!(output.extract::<(i64,)>().unwrap(), (-17,));
        assert_eq!(
            (cache.available(), cache.position(), cache.is_complete()),
            (0, 1, true)
        );
    });
}

#[test]
fn final_deadline_check_after_a_real_output_callback_keeps_native_rows() {
    Python::initialize();
    Python::attach(|py| {
        let decimal = PyModule::import(py, "decimal").unwrap();
        let original = decimal.getattr("Decimal").unwrap();
        let namespace = PyDict::new(py);
        namespace.set_item("original", &original).unwrap();
        py.run(c"import time\nclass SlowDecimal:\n def __new__(cls, value):\n  time.sleep(0.02)\n  return original(value)", Some(&namespace), None).unwrap();
        decimal
            .setattr(
                "Decimal",
                namespace.get_item("SlowDecimal").unwrap().unwrap(),
            )
            .unwrap();
        let types = ScalarTypes::new(py);
        decimal.setattr("Decimal", original).unwrap();
        let reader = Reader::empty(Py::new(py, types.unwrap()).unwrap());
        let scope = scope();
        let rows = batch(&scope, &[("v", 246)], &[vec![3, b'1', b'.', b'2']], false);
        let _operation = reader.operation(py).unwrap();
        let mut cache = reader
            .stream_header(
                py,
                header(&rows),
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        cache.reserve_batch().unwrap();
        cache.push(rows).unwrap();
        let short = IoBudget::new(
            TimeoutKind::Operation,
            Some(Duration::from_millis(5)),
            None,
            None,
        );
        assert!(
            reader
                .stream_read(py, &mut cache, Some(1), true, &short, || Ok(()))
                .is_err()
        );
        assert_eq!((cache.available(), cache.position()), (1, 0));
        assert_eq!(reader.lock(py).unwrap().position, 0);
    });
}

#[test]
fn final_native_validation_failure_keeps_output_uncommitted() {
    Python::initialize();
    Python::attach(|py| {
        let reader = reader(py, false);
        let scope = scope();
        let rows = batch(&scope, &[("v", 3)], &[vec![1, b'7']], false);
        let _operation = reader.operation(py).unwrap();
        let mut cache = reader
            .stream_header(
                py,
                header(&rows),
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        cache.reserve_batch().unwrap();
        cache.push(rows).unwrap();
        let error = reader
            .stream_read(py, &mut cache, Some(1), true, &budget(), || {
                Err(errors::builtin_message::<pyo3::exceptions::PyRuntimeError>(
                    py,
                    "native owner closed",
                ))
            })
            .unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py));
        assert_eq!((cache.available(), cache.position()), (1, 0));
        assert_eq!(reader.lock(py).unwrap().position, 0);
        assert_eq!(
            reader
                .stream_read(py, &mut cache, Some(1), true, &budget(), || Ok(()))
                .unwrap()
                .extract::<(i64,)>()
                .unwrap(),
            (7,)
        );
    });
}

#[test]
fn owned_buffered_read_keeps_the_wide_path_and_exact_fetch_counts() {
    Python::initialize();
    Python::attach(|py| {
        let reader = reader(py, false);
        let scope = scope();
        let rows = buffered(
            &scope,
            &[("a", 3), ("b", 3), ("c", 3), ("d", 3), ("e", 3)],
            &[vec![1, b'1', 1, b'2', 1, b'3', 1, b'4', 1, b'5']],
            false,
        );
        let _operation = reader.operation(py).unwrap();
        reader
            .install(
                py,
                rows,
                Options::default(),
                OutputCodec::for_collation(255),
            )
            .unwrap();
        reader.set_arraysize(py, 2).unwrap();
        assert_eq!(reader.fetch_size(py, None).unwrap(), 2);
        let boolean = true.into_pyobject(py).unwrap().to_owned().into_any();
        assert!(reader.fetch_size(py, Some(&boolean)).is_err());
        let huge = py.eval(c"1 << 200", None, None).unwrap();
        assert_eq!(reader.fetch_size(py, Some(&huge)).unwrap(), usize::MAX);
        assert_eq!(
            reader
                .read_owned(py, Some(1), true)
                .unwrap()
                .extract::<(i64, i64, i64, i64, i64)>()
                .unwrap(),
            (1, 2, 3, 4, 5)
        );
        let state = reader.lock(py).unwrap();
        assert_eq!(state.position, 1);
        assert!(
            state.data.is_some(),
            "buffered EOF must retain rows for backward navigation"
        );
    });
}
