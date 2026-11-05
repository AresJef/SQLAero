//! Native Planner requests are checked independently of the display formatter.

use super::*;
use pyo3::types::{PyDict, PyTuple};
use sqlaero_core::charset::{codec::Codec, resolve};
use sqlaero_core::transport::deadline::TimeoutKind;
use std::sync::Arc;

fn context(charset: &str, limit: usize) -> Context {
    Context {
        codec: Codec::new(resolve(Some(charset), None).unwrap().charset),
        no_backslash_escapes: false,
        utc_confirmed: true,
        limit,
    }
}
fn budget() -> IoBudget {
    IoBudget::new(TimeoutKind::Operation, None, None, None)
}
fn expression<'py>(py: Python<'py>, source: &std::ffi::CStr) -> Bound<'py, PyAny> {
    py.eval(source, None, None).unwrap()
}

#[test]
fn one_shared_template_and_fresh_native_types_for_every_row() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let args = expression(
            py,
            c"[-1,2**64-1,chr(65)+chr(0)+chr(39)+chr(92)+chr(0x1f600),None]",
        );
        let context = context("utf8mb4", 4096);
        let mut planner = Planner::new(
            &sql,
            Some(&args),
            true,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let mut requests = Vec::new();
        while let Some(request) = planner.next(py, &context, false, &budget()).unwrap() {
            requests.push(request);
        }
        assert_eq!(requests.len(), 4);
        let first = requests[0].prepared.as_ref().unwrap();
        assert_eq!(first.request.as_slice(), b"\x16SELECT ?");
        assert_eq!(first.parameters, 1);
        assert!(first.key.bound);
        for (index, request) in requests.iter().enumerate() {
            assert!(Arc::ptr_eq(
                &first.request,
                &request.prepared.as_ref().unwrap().request
            ));
            assert_eq!(request.source_rows, index..index + 1);
            assert_eq!(request.final_request, index == 3);
            assert_eq!(&request.bytes[..10], &[23, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        }
        assert_eq!(&requests[0].bytes[12..14], &[8, 0]);
        assert_eq!(&requests[0].bytes[14..], &(-1_i64).to_le_bytes());
        assert_eq!(&requests[1].bytes[12..14], &[8, 128]);
        assert_eq!(&requests[1].bytes[14..], &u64::MAX.to_le_bytes());
        assert_eq!(&requests[2].bytes[12..14], &[253, 0]);
        assert_eq!(&requests[2].bytes[14..23], &[254, 8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&requests[2].bytes[23..], "A\0'\\😀".as_bytes());
        assert_eq!(&requests[3].bytes[10..], &[1, 1, 6, 0]);
    });
}

#[test]
fn omitted_args_keep_raw_prepare_source_and_zero_parameter_execute() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let sql = PyString::from_bytes(py, b"SELECT '%%', '%s', %(name)s /* ? %s */").unwrap();
        let context = context("utf8mb4", 4096);
        let mut planner = Planner::new(
            &sql,
            None,
            true,
            false,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let request = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        assert_eq!(request.bytes, [23, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        let template = request.prepared.unwrap();
        assert_eq!(
            template.request.as_slice(),
            b"\x16SELECT '%%', '%s', %(name)s /* ? %s */"
        );
        assert_eq!(template.parameters, 0);
        assert!(!template.key.bound);
        assert_eq!(request.source_rows, 0..0);
        assert!(request.final_request);
        let raw = PyString::from_bytes(py, b"SELECT ?").unwrap();
        assert!(
            Planner::new(
                &raw,
                None,
                false,
                true,
                PlanMode::PreparedNative,
                &types,
                &budget()
            )
            .unwrap()
            .next(py, &context, false, &budget())
            .is_err()
        );
    });
}

#[test]
fn empty_roles_and_explicit_none_remain_distinct() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let context = context("utf8mb4", 4096);
        let sql = PyString::from_bytes(py, b"SELECT 1").unwrap();
        let empty = PyTuple::empty(py).into_any();
        let mut bound = Planner::new(
            &sql,
            Some(&empty),
            true,
            false,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let bound = bound.next(py, &context, false, &budget()).unwrap().unwrap();
        assert_eq!(bound.source_rows, 0..0);
        let mut unbound = Planner::new(
            &sql,
            None,
            false,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let unbound = unbound
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        let (a, b) = (bound.prepared.unwrap(), unbound.prepared.unwrap());
        assert_eq!(a.request, b.request);
        assert!(a.key != b.key);
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let none = py.None().into_bound(py);
        let mut planner = Planner::new(
            &sql,
            Some(&none),
            false,
            false,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let request = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        assert_eq!(request.source_rows, 0..1);
        assert_eq!(request.prepared.unwrap().parameters, 1);
        assert_eq!(&request.bytes[10..], &[1, 1, 6, 0]);
    });
}

#[test]
fn native_values_never_merge_and_structural_fragments_fail_locally() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let context = context("utf8mb4", 4096);
        let sql = PyString::from_bytes(py, b"INSERT t VALUES (%s)").unwrap();
        let args = expression(py, c"[1,2]");
        for mode in [
            PlanMode::Text,
            PlanMode::PreparedDisplay,
            PlanMode::PreparedNative,
        ] {
            let mut planner =
                Planner::new(&sql, Some(&args), true, true, mode, &types, &budget()).unwrap();
            let first = planner
                .next(py, &context, false, &budget())
                .unwrap()
                .unwrap();
            if mode == PlanMode::Text {
                assert_eq!(first.bytes, b"\x03INSERT t VALUES (1),(2)");
                assert!(first.prepared.is_none() && first.final_request);
            } else {
                let second = planner
                    .next(py, &context, false, &budget())
                    .unwrap()
                    .unwrap();
                assert!(!first.final_request && second.final_request);
                if mode == PlanMode::PreparedDisplay {
                    assert_eq!(first.bytes, b"\x03INSERT t VALUES (1)");
                    assert!(first.prepared.is_none() && second.prepared.is_none());
                } else {
                    assert!(first.prepared.is_some() && second.prepared.is_some());
                }
            }
        }
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        for (value, itemize) in [(c"((1,2),)", true), (c"(1,2)", false)] {
            let args = expression(py, value);
            let error = Planner::new(
                &sql,
                Some(&args),
                false,
                itemize,
                PlanMode::PreparedNative,
                &types,
                &budget(),
            )
            .unwrap()
            .next(py, &context, false, &budget())
            .err()
            .unwrap();
            assert!(error.is_instance_of::<crate::errors::NotSupportedError>(py));
        }
    });
}

#[test]
fn each_python_protocol_is_read_once_and_later_rows_are_not_precollected() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let namespace = PyDict::new(py);
        py.run(c"calls=[]\nclass Traced(int):\n def __int__(self):\n  calls.append(int.__int__(self))\n  if int.__int__(self)==2: raise RuntimeError('second row')\n  return 41\nrows=[Traced(1),Traced(2)]", Some(&namespace), None).unwrap();
        let args = namespace.get_item("rows").unwrap().unwrap();
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let context = context("utf8mb4", 4096);
        let mut planner = Planner::new(
            &sql,
            Some(&args),
            true,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let first = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        assert_eq!(&first.bytes[14..], &41_i64.to_le_bytes());
        assert_eq!(
            namespace
                .get_item("calls")
                .unwrap()
                .unwrap()
                .extract::<Vec<i64>>()
                .unwrap(),
            [1]
        );
        let error = planner.next(py, &context, false, &budget()).err().unwrap();
        assert!(error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py));
        assert_eq!(
            namespace
                .get_item("calls")
                .unwrap()
                .unwrap()
                .extract::<Vec<i64>>()
                .unwrap(),
            [1, 2]
        );
    });
}

#[test]
fn lexical_keys_change_without_treating_utc_or_packet_limit_as_server_prepare_context() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let args = expression(py, c"[1,2,3]");
        let mut context = context("utf8mb4", 4096);
        let mut planner = Planner::new(
            &sql,
            Some(&args),
            true,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let first = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap()
            .prepared
            .unwrap();
        context.utc_confirmed = false;
        context.limit = 8192;
        let second = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap()
            .prepared
            .unwrap();
        assert!(first.key == second.key);
        assert_eq!(first.request, second.request);
        context.no_backslash_escapes = true;
        let third = planner
            .next(py, &context, true, &budget())
            .unwrap()
            .unwrap()
            .prepared
            .unwrap();
        assert!(first.key != third.key && third.key.ansi && third.key.no_backslash);
    });
}

#[test]
fn native_capacity_counts_execute_bytes_and_complete_prepare_request() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let args = expression(py, c"b'x'*100");
        for (limit, success) in [(115, true), (114, false)] {
            let context = context("utf8mb4", limit);
            let result = Planner::new(
                &sql,
                Some(&args),
                false,
                true,
                PlanMode::PreparedNative,
                &types,
                &budget(),
            )
            .unwrap()
            .next(py, &context, false, &budget());
            if success {
                assert_eq!(result.unwrap().unwrap().bytes.len(), 115);
            } else {
                assert!(result.is_err());
            }
        }
        let raw = PyString::from_bytes(py, b"SELECT 1").unwrap();
        let context = context("utf8mb4", 9);
        assert!(
            Planner::new(
                &raw,
                None,
                false,
                true,
                PlanMode::PreparedNative,
                &types,
                &budget()
            )
            .unwrap()
            .next(py, &context, false, &budget())
            .is_err()
        );
        let sql = PyString::from_bytes(
            py,
            b"SELECT %s /* a complete template longer than the allowed prepare request */",
        )
        .unwrap();
        assert!(
            Planner::new(
                &sql,
                Some(&args),
                false,
                true,
                PlanMode::PreparedNative,
                &types,
                &budget()
            )
            .unwrap()
            .next(py, &context, false, &budget())
            .is_err()
        );
    });
}

#[test]
fn provider_tables_use_shared_rows_and_numbers_temporals_keep_sink_types() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let context = context("utf8mb4", 4096);
        let sql = PyString::from_bytes(py, b"SELECT %s,%s").unwrap();
        let args = expression(
            py,
            c"__import__('numpy').array([[1,2],[3,4]],dtype='int64')",
        );
        let mut planner = Planner::new(
            &sql,
            Some(&args),
            false,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let first = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        let second = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        assert_eq!(first.source_rows, 0..1);
        assert_eq!(second.source_rows, 1..2);
        assert!(Arc::ptr_eq(
            &first.prepared.unwrap().request,
            &second.prepared.unwrap().request
        ));
        let sql = PyString::from_bytes(py, b"SELECT %s,%s,%s,%s").unwrap();
        let args = expression(py, c"(10**30,__import__('decimal').Decimal('1.2300'),__import__('datetime').date(2024,2,29),__import__('numpy').datetime64('2024-01-01T00:00:00.123456789','ns'))");
        let mut planner = Planner::new(
            &sql,
            Some(&args),
            false,
            true,
            PlanMode::PreparedNative,
            &types,
            &budget(),
        )
        .unwrap();
        let request = planner
            .next(py, &context, false, &budget())
            .unwrap()
            .unwrap();
        assert_eq!(&request.bytes[12..20], &[246, 0, 246, 0, 10, 0, 253, 0]);
        assert!(
            request
                .bytes
                .windows(29)
                .any(|part| part == b"2024-01-01 00:00:00.123456789")
        );
    });
}
