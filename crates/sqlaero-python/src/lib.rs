//! Python input and output boundary for SQLAero's native MySQL core.

mod binding;
mod errors;
mod input;
mod json;

use pyo3::exceptions::{PyUnicodeDecodeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

/// Exercise the shared native size parser through the installed extension.
///
/// This private installation probe is not a public configuration API.
#[pyfunction]
fn _parse_size(value: &str) -> PyResult<usize> {
    sqlaero_core::config::size::parse_size(value)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Exercise the single-pass output decoder before result-set integration.
#[pyfunction]
fn _decode_json<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    json::decode(py, data)
}

/// Exercise the actual native scalar reader/writer before SQL binding is wired.
/// The returned bytes are diagnostic output, never per-value SQL Python str.
#[pyfunction]
#[pyo3(pass_module, signature = (data, *, utc_confirmed=true, limit=67108864, charset=None, no_backslash=false))]
fn _encode_scalar<'py>(
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    utc_confirmed: bool,
    limit: usize,
    charset: Option<&str>,
    no_backslash: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let owner = module.getattr("_scalar_types")?;
    let types = owner.extract::<PyRef<'_, input::ScalarTypes>>()?;
    let mut request = Vec::new();
    let encoding = sqlaero_core::charset::resolve(charset, None)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let context = sqlaero_core::bind::Context {
        codec: sqlaero_core::charset::codec::Codec::new(encoding.charset),
        no_backslash_escapes: no_backslash,
        utc_confirmed,
        limit,
    };
    types.write(data, &mut request, &context, None)?;
    Ok(PyBytes::new(module.py(), &request))
}

/// Validate the actual strict native codec before lazy row decoding is wired.
#[pyfunction]
#[pyo3(signature = (data, *, charset="utf8mb4"))]
fn _decode_text<'py>(data: &Bound<'py, PyBytes>, charset: &str) -> PyResult<Bound<'py, PyString>> {
    use sqlaero_core::charset::{
        codec::{Codec, CodecError},
        resolve,
    };
    let py = data.py();
    let codec = Codec::new(
        resolve(Some(charset), None)
            .map_err(|error| PyValueError::new_err(error.to_string()))?
            .charset,
    );
    match codec.decode(data.as_bytes()) {
        Ok(text) => Ok(PyString::new(py, &text)),
        Err(CodecError::Decode { start, end }) => {
            match py.get_type::<PyUnicodeDecodeError>().call1((
                codec.name(),
                data,
                start,
                end,
                "invalid or incomplete byte sequence",
            )) {
                Ok(error) => Err(PyErr::from_value(error)),
                Err(error) => Err(error),
            }
        }
        Err(CodecError::Wire(error)) => Err(errors::wire(py, error)),
        Err(error) => Err(errors::client::<errors::NotSupportedError>(
            py,
            sqlaero_core::error::ClientErrorCode::UnsupportedCharset,
            error.to_string(),
        )),
    }
}

/// Return actual native COM_QUERY requests from the incremental binder.
/// Production executes one prepared request before asking for the next row;
/// this private correctness probe only materializes the final request bytes.
#[pyfunction]
#[pyo3(pass_module, signature = (sql, args, *, many=false, itemize=true, charset=None, no_backslash=false, ansi_quotes=false, timeout_ns=None, limit=67108864))]
#[expect(
    clippy::too_many_arguments,
    reason = "private probe mirrors the separately specified binding controls"
)]
fn _bind_sql<'py>(
    module: &Bound<'py, PyModule>,
    sql: &Bound<'py, PyString>,
    args: &Bound<'py, PyAny>,
    many: bool,
    itemize: bool,
    charset: Option<&str>,
    no_backslash: bool,
    ansi_quotes: bool,
    timeout_ns: Option<u64>,
    limit: usize,
) -> PyResult<Bound<'py, pyo3::types::PyList>> {
    use sqlaero_core::{
        bind::{Context, template::Compiler},
        charset::{codec::Codec, resolve},
        transport::deadline::{IoBudget, TimeoutKind},
    };
    let py = module.py();
    let budget = IoBudget::new(
        TimeoutKind::Operation,
        timeout_ns.map(std::time::Duration::from_nanos),
        None,
        None,
    );
    input::check(py, &budget)?;
    let owner = module.getattr("_scalar_types")?;
    let types = owner.extract::<PyRef<'_, input::ScalarTypes>>()?;
    let context = Context {
        codec: Codec::new(
            resolve(charset, None)
                .map_err(|error| PyValueError::new_err(error.to_string()))?
                .charset,
        ),
        no_backslash_escapes: no_backslash,
        utc_confirmed: true,
        limit,
    };
    let mut encoded = Vec::new();
    // SQL source is not a sent request yet: the final binder applies its
    // complete byte bound after placeholder replacement, not to this template.
    let source_context = Context {
        limit: usize::MAX,
        ..context
    };
    input::text(sql, &mut encoded, &source_context, false, Some(&budget))?;
    let mut compiler = Compiler::new(encoded, context.codec, no_backslash, ansi_quotes);
    let template = loop {
        let step = py
            .detach(|| compiler.advance())
            .map_err(|error| binding::template_error(py, error))?;
        input::check(py, &budget)?;
        if let Some(template) = step {
            break template;
        }
    };
    let mut rows = binding::Rows::new(args, many, itemize);
    let output = pyo3::types::PyList::empty(py);
    while let Some(row) = rows.next()? {
        let request = row.bind(&template, &types, &context, &budget)?;
        output.append(PyBytes::new(py, &request))?;
    }
    Ok(output)
}

#[pymodule]
fn _sqlaero(module: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(module)?;
    module.add(
        "_scalar_types",
        Py::new(module.py(), input::ScalarTypes::new(module.py())?)?,
    )?;
    module.add_function(wrap_pyfunction!(_parse_size, module)?)?;
    module.add_function(wrap_pyfunction!(_decode_json, module)?)?;
    module.add_function(wrap_pyfunction!(_encode_scalar, module)?)?;
    module.add_function(wrap_pyfunction!(_decode_text, module)?)?;
    module.add_function(wrap_pyfunction!(_bind_sql, module)?)?;
    Ok(())
}
