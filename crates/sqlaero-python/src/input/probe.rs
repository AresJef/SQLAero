//! Installed diagnostic access to the native single-scalar sink contract.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::TextSink;
use sqlaero_core::charset::codec::Codec;
use sqlaero_core::transport::deadline::{IoBudget, TimeoutKind};

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

/// Materialize final diagnostic bytes from the real native row adapter.
/// Each list entry is one logical input row, with its scalar outputs in order.
#[pyfunction]
#[pyo3(pass_module, signature = (data, *, many=false, itemize=true, charset=None, no_backslash=false))]
pub(crate) fn _encode_native_rows<'py>(
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    many: bool,
    itemize: bool,
    charset: Option<&str>,
    no_backslash: bool,
) -> PyResult<Vec<Py<PyBytes>>> {
    let py = module.py();
    let owner = module.getattr(PyString::from_bytes(py, b"_scalar_types")?)?;
    let types = owner.extract::<PyRef<'_, super::ScalarTypes>>()?;
    let encoding = sqlaero_core::charset::resolve(charset, None)
        .map_err(|error| crate::errors::builtin_message::<PyValueError>(py, error.to_string()))?;
    let context = Context {
        codec: Codec::new(encoding.charset),
        no_backslash_escapes: no_backslash,
        utc_confirmed: true,
        limit: 67_108_864,
    };
    let budget = IoBudget::new(TimeoutKind::Operation, None, None, None);
    let mut rows = crate::binding::Rows::new(data, many, itemize, &types, &budget)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next(&types, &budget)? {
        let mut encoded = Vec::new();
        row.write_parameters(&types, &context, &budget, &mut TextSink::new(&mut encoded))?;
        result
            .try_reserve(1)
            .map_err(|_| crate::errors::memory_error(py))?;
        result.push(PyBytes::new(py, &encoded).unbind());
    }
    Ok(result)
}
