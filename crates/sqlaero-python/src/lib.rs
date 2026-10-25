//! Python input and output boundary for SQLAero's native MySQL core.

mod errors;
mod input;
mod json;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

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
#[pyo3(pass_module, signature = (data, *, utc_confirmed=true, limit=67108864))]
fn _encode_scalar<'py>(
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    utc_confirmed: bool,
    limit: usize,
) -> PyResult<Bound<'py, PyBytes>> {
    let owner = module.getattr("_scalar_types")?;
    let types = owner.extract::<PyRef<'_, input::ScalarTypes>>()?;
    let mut request = Vec::new();
    types.write(data, &mut request, utc_confirmed, limit)?;
    Ok(PyBytes::new(module.py(), &request))
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
    Ok(())
}
