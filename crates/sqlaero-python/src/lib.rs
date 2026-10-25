//! Python input and output boundary for SQLAero's native MySQL core.

mod errors;
mod json;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

#[pymodule]
fn _sqlaero(module: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(module)?;
    module.add_function(wrap_pyfunction!(_parse_size, module)?)?;
    module.add_function(wrap_pyfunction!(_decode_json, module)?)?;
    Ok(())
}
