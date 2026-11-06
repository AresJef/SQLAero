//! One-operation LOCAL path attachment and read-only statement preparation.

use crate::{binding, config, errors, input};
use pyo3::prelude::*;
use sqlaero_core::bind::{Context, template::Compiler};
use sqlaero_core::protocol::local_infile::Source;
use sqlaero_core::transport::deadline::IoBudget;
use std::path::{Path, PathBuf};

/// Resolve one optional Python path before entering native wire protection.
/// The caller already protects this Cursor, and retains its original operation
/// budget across the path protocol, file opening, upload and final response.
pub(crate) fn path(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
    budget: &IoBudget,
) -> PyResult<Option<PathBuf>> {
    input::check(py, budget)?;
    let path = config::optional_path(value, "local_file")?;
    input::check(py, budget)?;
    Ok(path)
}

/// Open exactly one actual regular-file source after the caller has validated
/// the command shape and capability. This does not authorize a server filename
/// or send SQL. The same handle and opening length pass into the native upload.
/// Blocking filesystem calls are cooperative and cannot promise hard timeout.
pub(crate) fn open(py: Python<'_>, path: &Path, budget: &IoBudget) -> PyResult<Source> {
    input::check(py, budget)?;
    let source = py.detach(|| Source::open(path)).map_err(|source| {
        let primary = errors::io_error(py, &source);
        errors::note_format(
            py,
            &primary,
            format_args!("while preparing local_file source {path:?}; SQL has not been sent"),
        );
        errors::at(py, primary, "local_file preparation")
    })?;
    input::check(py, budget)?;
    Ok(source)
}

/// Check the final COM_QUERY allocation without another binding pass or copy.
/// A positive result only identifies one ordinary LOCAL LOAD; the caller still
/// owns file authorization, negotiated capability and one-operation lifetime.
pub(crate) fn statement(
    py: Python<'_>,
    request: Vec<u8>,
    context: &Context,
    ansi_quotes: bool,
    budget: &IoBudget,
) -> PyResult<(Vec<u8>, bool)> {
    input::check(py, budget)?;
    let mut compiler = Compiler::for_local_query(
        request,
        context.codec,
        context.no_backslash_escapes,
        ansi_quotes,
    );
    loop {
        let template = compiler
            .advance()
            .map_err(|error| binding::template_error(py, error))?;
        input::check(py, budget)?;
        if let Some(template) = template {
            let local = template.is_single_local_infile();
            input::check(py, budget)?;
            return Ok((template.into_source(), local));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
    use pyo3::types::PyModule;
    use sqlaero_core::charset::{codec::Codec, resolve};
    use sqlaero_core::transport::deadline::TimeoutKind;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn budget() -> IoBudget {
        IoBudget::new(TimeoutKind::Operation, None, None, None)
    }
    fn expired() -> IoBudget {
        IoBudget::starting_at(
            Instant::now() - Duration::from_secs(1),
            TimeoutKind::Operation,
            Some(Duration::from_millis(1)),
            None,
            None,
        )
    }
    fn context() -> Context {
        Context {
            codec: Codec::new(resolve(None, None).unwrap().charset),
            no_backslash_escapes: false,
            utc_confirmed: true,
            limit: 1_000_000,
        }
    }
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "sqlaero-a-local-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self(directory)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn none_and_nonpaths_follow_the_existing_path_boundary() {
        Python::initialize();
        Python::attach(|py| {
            assert!(path(py, None, &budget()).unwrap().is_none());
            assert!(
                path(py, Some(py.None().bind(py)), &budget())
                    .unwrap()
                    .is_none()
            );
            for source in [
                c"True",
                c"False",
                c"b'file'",
                c"1",
                c"1.5",
                c"iter([b'contents'])",
                c"__import__('io').BytesIO(b'body')",
            ] {
                let value = py.eval(source, None, None).unwrap();
                assert!(
                    path(py, Some(&value), &budget())
                        .unwrap_err()
                        .is_instance_of::<PyTypeError>(py)
                );
            }
            for source in [c"''", c"'a'+chr(0)+'b'"] {
                let value = py.eval(source, None, None).unwrap();
                assert!(
                    path(py, Some(&value), &budget())
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
            }
        });
    }

    #[test]
    fn path_protocol_runs_once_and_keeps_its_original_exception() {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::from_code(py, c"\nimport os\nclass Once:\n    calls = 0\n    def __init__(self, result):\n        self.result = result\n    def __fspath__(self):\n        self.calls += 1\n        assert self.calls == 1\n        return self.result\noriginal = RuntimeError('path identity')\nclass Broken:\n    calls = 0\n    def __fspath__(self):\n        self.calls += 1\n        raise original\n", c"local_case.py", c"local_case").unwrap();
            for source in [
                "授权-🙂.tsv".into_pyobject(py).unwrap().into_any(),
                "授权-🙂.tsv"
                    .as_bytes()
                    .into_pyobject(py)
                    .unwrap()
                    .into_any(),
            ] {
                let value = module.getattr("Once").unwrap().call1((source,)).unwrap();
                let native = path(py, Some(&value), &budget()).unwrap().unwrap();
                assert_eq!(native, PathBuf::from("授权-🙂.tsv"));
                assert_eq!(
                    value.getattr("calls").unwrap().extract::<usize>().unwrap(),
                    1
                );
            }
            let value = module.getattr("Broken").unwrap().call0().unwrap();
            let error = path(py, Some(&value), &budget()).unwrap_err();
            assert!(error.is_instance_of::<PyRuntimeError>(py));
            assert!(error.value(py).is(module.getattr("original").unwrap()));
            assert_eq!(
                value.getattr("calls").unwrap().extract::<usize>().unwrap(),
                1
            );
        });
    }

    #[test]
    fn opened_source_stays_on_the_same_file_and_fixed_opening_length() {
        Python::initialize();
        Python::attach(|py| {
            let directory = Directory::new();
            let source_path = directory.0.join("授权-🙂.tsv");
            fs::write(&source_path, b"original").unwrap();
            let value = source_path.clone().into_pyobject(py).unwrap();
            let native = path(py, Some(&value), &budget()).unwrap().unwrap();
            let mut source = open(py, &native, &budget()).unwrap();
            assert_eq!(source.length(), 8);
            let renamed = directory.0.join("renamed.tsv");
            fs::rename(&source_path, &renamed).unwrap();
            fs::write(&source_path, b"decoy-source").unwrap();
            use std::io::Write;
            fs::OpenOptions::new()
                .append(true)
                .open(&renamed)
                .unwrap()
                .write_all(b"-later")
                .unwrap();
            let mut bytes = [0; 64];
            assert_eq!(source.read_chunk(&mut bytes).unwrap(), 8);
            assert_eq!(&bytes[..8], b"original");
            assert_eq!(source.read_chunk(&mut bytes).unwrap(), 0);
            assert_eq!(source.read_bytes(), 8);
        });
    }

    #[test]
    fn preparation_errors_keep_os_identity_and_path_without_file_contents() {
        Python::initialize();
        Python::attach(|py| {
            let directory = Directory::new();
            let missing = directory.0.join("missing.tsv");
            let error = open(py, &missing, &budget()).err().unwrap();
            assert!(error.is_instance_of::<pyo3::exceptions::PyFileNotFoundError>(py));
            assert_eq!(
                error
                    .value(py)
                    .getattr("errno")
                    .unwrap()
                    .extract::<i32>()
                    .unwrap(),
                2
            );
            let notes = error
                .value(py)
                .getattr("__notes__")
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap();
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("missing.tsv")
                        && note.contains("SQL has not been sent"))
            );
            let error = open(py, &directory.0, &budget()).err().unwrap();
            assert!(error.is_instance_of::<pyo3::exceptions::PyOSError>(py));
            let empty = directory.0.join("empty.tsv");
            fs::write(&empty, []).unwrap();
            assert_eq!(open(py, &empty, &budget()).unwrap().length(), 0);
            #[cfg(unix)]
            {
                let link = directory.0.join("link.tsv");
                std::os::unix::fs::symlink(&empty, &link).unwrap();
                assert_eq!(open(py, &link, &budget()).unwrap().length(), 0);
            }
        });
    }

    #[test]
    fn expired_budget_precedes_user_protocol_and_filesystem_open() {
        Python::initialize();
        Python::attach(|py| {
            let trap = py.eval(c"type('Trap', (), {'__fspath__': lambda self: (_ for _ in ()).throw(AssertionError('must not call'))})()", None, None).unwrap();
            let error = path(py, Some(&trap), &expired()).unwrap_err();
            assert!(error.to_string().contains("operation deadline"));
            let error = open(py, Path::new("/does-not-exist/a-local-file"), &expired())
                .err()
                .unwrap();
            assert!(error.to_string().contains("operation deadline"));
            let error =
                statement(py, b"\x03SELECT 1".to_vec(), &context(), false, &expired()).unwrap_err();
            assert!(error.to_string().contains("operation deadline"));
        });
    }

    #[test]
    fn path_protocol_work_remains_in_the_original_operation_budget() {
        Python::initialize();
        Python::attach(|py| {
            let slow = py.eval(c"type('Slow', (), {'__fspath__': lambda self: (__import__('time').sleep(0.02), 'file')[1]})()", None, None).unwrap();
            let limited = IoBudget::new(
                TimeoutKind::Operation,
                Some(Duration::from_millis(1)),
                None,
                None,
            );
            let error = path(py, Some(&slow), &limited).unwrap_err();
            assert!(error.to_string().contains("operation deadline"));
        });
    }

    #[test]
    fn final_request_is_classified_without_binding_or_copying() {
        Python::initialize();
        Python::attach(|py| {
            for (sql, expected) in [
                (
                    "LOAD DATA LOCAL INFILE 'x; %s ? %%' INTO TABLE t; /* ; */",
                    true,
                ),
                (
                    "LOAD DATA LOCAL INFILE 'x' INTO TABLE t; DELETE FROM t",
                    false,
                ),
                ("/*!80000 LOAD DATA LOCAL INFILE 'x' INTO TABLE t */", false),
                ("SELECT 1", false),
            ] {
                let request = [b"\x03".as_slice(), sql.as_bytes()].concat();
                let pointer = request.as_ptr();
                let (request, local) =
                    statement(py, request, &context(), false, &budget()).unwrap();
                assert_eq!(local, expected, "{sql}");
                assert_eq!(request.as_ptr(), pointer);
                assert_eq!(&request[1..], sql.as_bytes());
            }
        });
    }
}
