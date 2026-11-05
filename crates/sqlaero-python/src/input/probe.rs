//! Installed diagnostic access to the native single-scalar sink contract.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::TextSink;
use sqlaero_core::charset::codec::Codec;

/// Compare native sink adaptation with the established text scalar path.
/// This diagnostic output is not an execution route or a public interface.
#[pyfunction]
#[pyo3(pass_module, signature = (data, *, utc_confirmed=true, limit=67108864, charset=None, no_backslash=false))]
pub(crate) fn _encode_native_scalar<'py>(
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    utc_confirmed: bool,
    limit: usize,
    charset: Option<&str>,
    no_backslash: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let py = module.py();
    let owner = module.getattr(PyString::from_bytes(py, b"_scalar_types")?)?;
    let types = owner.extract::<PyRef<'_, super::ScalarTypes>>()?;
    let encoding = sqlaero_core::charset::resolve(charset, None)
        .map_err(|error| crate::errors::builtin_message::<PyValueError>(py, error.to_string()))?;
    let context = Context {
        codec: Codec::new(encoding.charset),
        no_backslash_escapes: no_backslash,
        utc_confirmed,
        limit,
    };
    let mut request = Vec::new();
    types.write_native(data, &mut TextSink::new(&mut request), &context, None)?;
    Ok(PyBytes::new(py, &request))
}
