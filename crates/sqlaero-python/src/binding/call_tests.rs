//! Expected CALL bytes come from the approved contract, not from the planner.

use super::*;
use pyo3::types::{PyList, PyModule, PyTuple};
use sqlaero_core::charset::{codec::Codec, resolve};
use sqlaero_core::transport::deadline::TimeoutKind;

fn budget() -> IoBudget {
    IoBudget::new(TimeoutKind::Operation, None, None, None)
}
fn context(limit: usize) -> Context {
    Context {
        codec: Codec::new(resolve(None, None).unwrap().charset),
        no_backslash_escapes: false,
        utc_confirmed: true,
        limit,
    }
}
fn call<'py>(
    py: Python<'py>,
    args: Option<&Bound<'py, PyAny>>,
    modes: Option<&Bound<'py, PyAny>>,
    mode: PlanMode,
    limit: usize,
) -> PyResult<NativeCall> {
    let proc = PyString::from_bytes(py, b"adjust").unwrap();
    let types = ScalarTypes::new(py).unwrap();
    NativeCall::new(
        &proc,
        args,
        None,
        modes,
        "_sqlaero_call_7_",
        mode,
        &types,
        &context(limit),
        false,
        &budget(),
    )
}

#[test]
fn omitted_empty_and_explicit_null_use_distinct_call_positions() {
    Python::initialize();
    Python::attach(|py| {
        let omitted = call(py, None, None, PlanMode::Text, 4096).unwrap();
        assert_eq!(omitted.call.bytes, b"\x03CALL `adjust`()");
        assert!(omitted.directions.is_empty() && omitted.output_positions.is_empty());
        assert!(
            omitted.initialize.is_none() && omitted.select.is_none() && omitted.cleanup.is_none()
        );
        let empty = PyTuple::empty(py).into_any();
        assert_eq!(
            call(py, Some(&empty), None, PlanMode::Text, 4096)
                .unwrap()
                .call
                .bytes,
            b"\x03CALL `adjust`()"
        );
        assert_eq!(
            call(py, Some(py.None().bind(py)), None, PlanMode::Text, 4096)
                .unwrap()
                .call
                .bytes,
            b"\x03CALL `adjust`(NULL)"
        );
    });
}

#[test]
fn text_output_slots_keep_original_positions_and_initialize_every_call() {
    Python::initialize();
    Python::attach(|py| {
        let args = py.eval(c"(5,None,10)", None, None).unwrap();
        let modes = py.eval(c"('in','OuT','InOuT')", None, None).unwrap();
        let plan = call(py, Some(&args), Some(&modes), PlanMode::Text, 4096).unwrap();
        assert_eq!(
            plan.directions,
            [Direction::In, Direction::Out, Direction::InOut]
        );
        assert_eq!(plan.output_positions, [1, 2]);
        assert_eq!(
            plan.initialize.as_deref(),
            Some(b"\x03SET @_sqlaero_call_7_0=NULL,@_sqlaero_call_7_1=10".as_slice())
        );
        assert_eq!(
            plan.call.bytes,
            b"\x03CALL `adjust`(5,@_sqlaero_call_7_0,@_sqlaero_call_7_1)"
        );
        assert_eq!(
            plan.select.as_deref(),
            Some(b"\x03SELECT @_sqlaero_call_7_0,@_sqlaero_call_7_1".as_slice())
        );
        assert_eq!(
            plan.cleanup.as_deref(),
            Some(b"\x03SET @_sqlaero_call_7_0=NULL,@_sqlaero_call_7_1=NULL".as_slice())
        );
        let expected = b"\x03SET @_sqlaero_call_7_0=NULL,@_sqlaero_call_7_1=10;CALL `adjust`(5,@_sqlaero_call_7_0,@_sqlaero_call_7_1);SELECT @_sqlaero_call_7_0,@_sqlaero_call_7_1;SET @_sqlaero_call_7_0=NULL,@_sqlaero_call_7_1=NULL";
        assert_eq!(
            plan.combined(py, expected.len(), &budget())
                .unwrap()
                .as_deref(),
            Some(expected.as_slice())
        );
        assert!(
            plan.combined(py, expected.len() - 1, &budget())
                .unwrap()
                .is_none()
        );
        assert_eq!(args.get_item(0).unwrap().extract::<i32>().unwrap(), 5);
        assert!(args.get_item(1).unwrap().is_none());
    });
}

#[test]
fn text_nested_fragments_keep_one_position_but_prepared_rejects_structure() {
    Python::initialize();
    Python::attach(|py| {
        let args = py.eval(c"((1,2),['x','y'])", None, None).unwrap();
        let plan = call(py, Some(&args), None, PlanMode::Text, 4096).unwrap();
        assert_eq!(plan.call.bytes, b"\x03CALL `adjust`((1,2),('x','y'))");
        assert_eq!(plan.directions.len(), 2);
        assert!(call(py, Some(&args), None, PlanMode::PreparedNative, 4096).is_err());
    });
}

#[test]
fn prepared_call_uses_native_parameters_without_text_user_variables() {
    Python::initialize();
    Python::attach(|py| {
        let args = py.eval(c"(-1,None,2**64-1)", None, None).unwrap();
        let modes = py.eval(c"['IN','OUT','INOUT']", None, None).unwrap();
        let plan = call(
            py,
            Some(&args),
            Some(&modes),
            PlanMode::PreparedNative,
            4096,
        )
        .unwrap();
        let template = plan.call.prepared.as_ref().unwrap();
        assert_eq!(template.request.as_slice(), b"\x16CALL `adjust`(?,?,?)");
        assert_eq!(template.parameters, 3);
        assert_eq!(&plan.call.bytes[..10], &[23, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&plan.call.bytes[10..18], &[2, 1, 8, 0, 6, 0, 8, 128]);
        assert_eq!(&plan.call.bytes[18..26], &(-1_i64).to_le_bytes());
        assert_eq!(&plan.call.bytes[26..], &u64::MAX.to_le_bytes());
        assert_eq!(plan.output_positions, [1, 2]);
        assert!(plan.initialize.is_none() && plan.select.is_none() && plan.cleanup.is_none());
        assert!(plan.combined(py, 4096, &budget()).unwrap().is_none());
    });
}

#[test]
fn mode_validation_is_exact_and_does_not_call_conversion_hooks() {
    Python::initialize();
    Python::attach(|py| {
        let args = py.eval(c"(None,)", None, None).unwrap();
        let cases = [c"'OUT'", c"iter(['OUT'])", c"type('L',(list,),{'__iter__':lambda s:(_ for _ in ()).throw(AssertionError())})(['OUT'])", c"[1]", c"[' OUT']", c"['OUTPUT']", c"[]", c"['IN','OUT']"];
        for source in cases {
            let modes = py.eval(source, None, None).unwrap();
            assert!(
                call(py, Some(&args), Some(&modes), PlanMode::Text, 4096).is_err(),
                "{source:?}"
            );
        }
        for mode in [PlanMode::Text, PlanMode::PreparedNative] {
            let modes = PyList::new(py, ["OUT"]).unwrap().into_any();
            let bad = py.eval(c"(7,)", None, None).unwrap();
            assert!(
                call(py, Some(&bad), Some(&modes), mode, 4096)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("OUT")
            );
        }
    });
}

#[test]
fn names_are_separate_quoted_identifiers_and_share_savepoint_validation() {
    Python::initialize();
    Python::attach(|py| {
        let types = ScalarTypes::new(py).unwrap();
        let proc = PyString::from_bytes(py, "select.a`; DROP TABLE t; --🙂".as_bytes()).unwrap();
        let database = PyString::from_bytes(py, b"db`x").unwrap().into_any();
        let plan = NativeCall::new(
            &proc,
            None,
            Some(&database),
            None,
            "_sqlaero_call_7_",
            PlanMode::Text,
            &types,
            &context(4096),
            false,
            &budget(),
        )
        .unwrap();
        assert_eq!(
            plan.call.bytes,
            "\x03CALL `db``x`.`select.a``; DROP TABLE t; --🙂`()".as_bytes()
        );
        let name = PyString::from_bytes(py, b"sp`name").unwrap().into_any();
        let savepoint = input::control::savepoint(
            &name,
            input::control::SavepointAction::Create,
            &context(4096),
            &budget(),
        )
        .unwrap();
        assert_eq!(savepoint, b"\x03SAVEPOINT `sp``name`");
        for source in [c"''", c"'a'+chr(0)+'b'"] {
            let name = py.eval(source, None, None).unwrap();
            let error = input::control::savepoint(
                &name,
                input::control::SavepointAction::Create,
                &context(0),
                &budget(),
            )
            .unwrap_err();
            assert!(error.is_instance_of::<pyo3::exceptions::PyValueError>(py));
            let proc = name.cast::<PyString>().unwrap();
            assert!(
                NativeCall::new(
                    proc,
                    None,
                    None,
                    None,
                    "_sqlaero_call_7_",
                    PlanMode::Text,
                    &types,
                    &context(4096),
                    false,
                    &budget()
                )
                .is_err()
            );
        }
        let wrong = py
            .eval(
                c"type('Trap',(),{'__str__':lambda s:(_ for _ in ()).throw(AssertionError())})()",
                None,
                None,
            )
            .unwrap();
        assert!(
            NativeCall::new(
                &proc,
                None,
                Some(&wrong),
                None,
                "_sqlaero_call_7_",
                PlanMode::Text,
                &types,
                &context(4096),
                false,
                &budget()
            )
            .err()
            .unwrap()
            .is_instance_of::<pyo3::exceptions::PyTypeError>(py)
        );
        let mut name = vec![b'a'; 300_000];
        name.extend_from_slice("é".as_bytes());
        let proc = PyString::from_bytes(py, &name).unwrap();
        let ascii = Context {
            codec: Codec::new(resolve(Some("ascii"), None).unwrap().charset),
            ..context(400_000)
        };
        let error = NativeCall::new(
            &proc,
            None,
            None,
            None,
            "_sqlaero_call_7_",
            PlanMode::Text,
            &types,
            &ascii,
            false,
            &budget(),
        )
        .err()
        .unwrap();
        assert!(error.is_instance_of::<pyo3::exceptions::PyUnicodeEncodeError>(py));
        assert_eq!(
            error
                .value(py)
                .getattr("start")
                .unwrap()
                .extract::<usize>()
                .unwrap(),
            300_000
        );
        assert!(error.value(py).getattr("object").unwrap().is(&proc));
    });
}

#[test]
fn prepared_sources_use_real_nulls_and_reject_only_actual_multirow_inputs() {
    Python::initialize();
    Python::attach(|py| {
        let numpy = PyModule::import(py, "numpy").unwrap();
        let arrow = PyModule::import(py, "pyarrow").unwrap();
        let scope = pyo3::types::PyDict::new(py);
        scope.set_item("np", &numpy).unwrap();
        scope.set_item("pa", &arrow).unwrap();
        let modes = py.eval(c"['IN','OUT']", None, None).unwrap();
        for source in [
            c"np.array([[5,None]],dtype=object)",
            c"np.array((5,None),dtype=[('a','i8'),('b',object)])[()]",
            c"pa.table({'a':[5],'b':[None]})",
        ] {
            for mode in [PlanMode::Text, PlanMode::PreparedNative] {
                let args = py.eval(source, Some(&scope), None).unwrap();
                let plan = call(py, Some(&args), Some(&modes), mode, 4096).unwrap();
                assert_eq!(plan.output_positions, [1]);
                if mode == PlanMode::Text {
                    assert_eq!(plan.call.bytes, b"\x03CALL `adjust`(5,@_sqlaero_call_7_0)");
                }
            }
        }
        for source in [c"np.array([[1,2],[3,4]])", c"pa.table({'a':[1,2]})"] {
            let args = py.eval(source, Some(&scope), None).unwrap();
            assert!(
                call(py, Some(&args), None, PlanMode::Text, 4096)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("multirow")
            );
        }
        for source in [
            c"np.array([],dtype='i8')",
            c"np.empty((0,2),dtype='i8')",
            c"np.array((),dtype=[])[()]",
        ] {
            let args = py.eval(source, Some(&scope), None).unwrap();
            assert_eq!(
                call(py, Some(&args), None, PlanMode::Text, 4096)
                    .unwrap()
                    .call
                    .bytes,
                b"\x03CALL `adjust`()"
            );
        }
    });
}

#[test]
fn each_scalar_and_container_protocol_runs_once_without_reading_later_bad_values() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::from_code(py, c"\nclass Once(int):\n    calls = 0\n    def __int__(self):\n        self.calls += 1\n        assert self.calls == 1\n        return int.__int__(self)\nclass OnceList(list):\n    calls = 0\n    def __iter__(self):\n        self.calls += 1\n        assert self.calls == 1\n        return list.__iter__(self)\n", c"call_once.py", c"call_once").unwrap();
        for mode in [PlanMode::Text, PlanMode::PreparedNative] {
            let value = module.getattr("Once").unwrap().call1((17,)).unwrap();
            let args = module
                .getattr("OnceList")
                .unwrap()
                .call1((PyTuple::new(py, [value.clone()]).unwrap(),))
                .unwrap();
            assert!(call(py, Some(&args), None, mode, 4096).is_ok());
            assert_eq!(
                value.getattr("calls").unwrap().extract::<usize>().unwrap(),
                1
            );
            assert_eq!(
                args.getattr("calls").unwrap().extract::<usize>().unwrap(),
                1
            );
            let later = module.getattr("Once").unwrap().call1((9,)).unwrap();
            let args = PyTuple::new(
                py,
                [3_i32.into_pyobject(py).unwrap().into_any(), later.clone()],
            )
            .unwrap()
            .into_any();
            let modes = py.eval(c"['OUT','IN']", None, None).unwrap();
            assert!(call(py, Some(&args), Some(&modes), mode, 4096).is_err());
            assert_eq!(
                later.getattr("calls").unwrap().extract::<usize>().unwrap(),
                0
            );
        }
    });
}

#[test]
fn independent_stage_limits_allow_a_legal_segmented_call_without_rebinding() {
    Python::initialize();
    Python::attach(|py| {
        let args = py.eval(c"('a'*600,'b'*600)", None, None).unwrap();
        let modes = py.eval(c"['IN','INOUT']", None, None).unwrap();
        let plan = call(py, Some(&args), Some(&modes), PlanMode::Text, 800).unwrap();
        assert!(plan.call.bytes.len() < 800 && plan.initialize.as_ref().unwrap().len() < 800);
        assert!(plan.combined(py, 800, &budget()).unwrap().is_none());
        let too_long = py.eval(c"('a'*801,'b')", None, None).unwrap();
        assert!(call(py, Some(&too_long), Some(&modes), PlanMode::Text, 800).is_err());
        let out = py.eval(c"['OUT']", None, None).unwrap();
        for source in [c"('NULL',)", c"(0,)", c"(False,)", c"('',)"] {
            let args = py.eval(source, None, None).unwrap();
            for mode in [PlanMode::Text, PlanMode::PreparedNative] {
                assert!(call(py, Some(&args), Some(&out), mode, 4096).is_err());
            }
        }
        for source in [c"(float('nan'),)", c"(float('inf'),)"] {
            let nonfinite = py.eval(source, None, None).unwrap();
            for mode in [PlanMode::Text, PlanMode::PreparedNative] {
                assert!(call(py, Some(&nonfinite), Some(&out), mode, 4096).is_err());
            }
        }
    });
}

#[test]
fn long_text_binary_and_budget_checks_share_the_native_boundaries() {
    Python::initialize();
    Python::attach(|py| {
        let args = py
            .eval(c"('🙂'+chr(39)+chr(92)+chr(0))*70000", None, None)
            .unwrap();
        let text = call(py, Some(&args), None, PlanMode::Text, 2_000_000).unwrap();
        assert!(text.call.bytes.len() > 256 * 1024);
        let native = call(py, Some(&args), None, PlanMode::PreparedNative, 2_000_000).unwrap();
        assert_eq!(&native.call.bytes[12..14], &[253, 0]);
        assert_eq!(native.call.bytes.len(), 23 + 7 * 70_000);
        let bytes = py.eval(c"bytes(range(256))*2000", None, None).unwrap();
        let plan = call(py, Some(&bytes), None, PlanMode::PreparedNative, 2_000_000).unwrap();
        assert_eq!(&plan.call.bytes[14..18], &[253, 0, 208, 7]);
        assert_eq!(plan.call.bytes.len(), 18 + 512_000);
        let types = ScalarTypes::new(py).unwrap();
        let proc = PyString::from_bytes(py, b"p").unwrap();
        let expired = IoBudget::starting_at(
            std::time::Instant::now() - std::time::Duration::from_secs(1),
            TimeoutKind::Operation,
            Some(std::time::Duration::from_millis(1)),
            None,
            None,
        );
        assert!(
            NativeCall::new(
                &proc,
                Some(&args),
                None,
                None,
                "_sqlaero_call_7_",
                PlanMode::Text,
                &types,
                &context(2_000_000),
                false,
                &expired
            )
            .err()
            .unwrap()
            .to_string()
            .contains("operation deadline")
        );
    });
}

#[test]
fn planner_batch_identity_uses_the_prepared_row_role() {
    Python::initialize();
    Python::attach(|py| {
        let sql = PyString::from_bytes(py, b"SELECT %s").unwrap();
        let types = ScalarTypes::new(py).unwrap();
        let args = py.eval(c"(1,)", None, None).unwrap();
        for many in [false, true] {
            let plan = super::super::Planner::new(
                &sql,
                Some(&args),
                many,
                true,
                PlanMode::Text,
                &types,
                &budget(),
            )
            .unwrap();
            assert_eq!(plan.is_batch(), many);
        }
        let table = py
            .eval(c"__import__('numpy').array([[1]])", None, None)
            .unwrap();
        assert!(
            super::super::Planner::new(
                &sql,
                Some(&table),
                false,
                true,
                PlanMode::Text,
                &types,
                &budget()
            )
            .unwrap()
            .is_batch()
        );
        assert!(
            !super::super::Planner::new(&sql, None, true, true, PlanMode::Text, &types, &budget())
                .unwrap()
                .is_batch()
        );
    });
}
