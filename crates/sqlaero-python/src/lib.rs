//! Python input and output boundary for SQLAero's native MySQL core.

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
    types.write(data, &mut request, &context)?;
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
    Ok(())
}
