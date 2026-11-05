//! Explicit diagnostics from current response metadata, without Cursor state.

use crate::{errors, input, output};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyString, PyTuple};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::protocol::framing::WireError;
use sqlaero_core::result::{
    decode::{Kind, Options, Plan},
    text::Buffered,
    value::RowEncoding,
};
use sqlaero_core::transport::deadline::IoBudget;

/// Decode only the actual SHOW WARNINGS fields. No result-name snapshot,
/// historical codec, Reader installation or preliminary SQL is involved.
pub(crate) fn warnings<'py>(
    py: Python<'py>,
    rows: Buffered,
    types: &input::ScalarTypes,
    options: Options,
    budget: &IoBudget,
) -> PyResult<Bound<'py, PyTuple>> {
    input::check(py, budget)?;
    if rows.encoding != RowEncoding::Text || rows.columns.len() != 3 {
        return Err(errors::wire(
            py,
            WireError::Malformed("SHOW WARNINGS did not return three text-protocol columns"),
        ));
    }
    let plans: [Plan; 3] = std::array::from_fn(|index| {
        let column = &rows.columns[index];
        Plan::new(
            column.type_code,
            column.flags,
            column.collation_id,
            column.column_length,
            options,
        )
    });
    let mut result = Vec::new();
    result
        .try_reserve(rows.row_count)
        .map_err(|_| errors::memory_error(py))?;
    for index in 0..rows.row_count {
        input::check(py, budget)?;
        let fields = rows.row(index).ok_or_else(|| {
            errors::wire(
                py,
                WireError::Malformed("SHOW WARNINGS row locations are unavailable"),
            )
        })?;
        let decode = |column: usize| -> PyResult<Bound<'py, PyAny>> {
            input::check(py, budget)?;
            let plan = plans[column];
            let expected = if column == 1 {
                matches!(plan.kind, Kind::Integer { .. })
            } else {
                matches!(plan.kind, Kind::Text)
            };
            if !expected {
                return Err(if column == 1 {
                    errors::wire(
                        py,
                        WireError::Malformed("SHOW WARNINGS Code is not an integer column"),
                    )
                } else {
                    errors::client(
                        py,
                        ClientErrorCode::UnsupportedCharset,
                        "SHOW WARNINGS text lacks a reliable supported text-column encoding",
                    )
                });
            }
            let bytes = fields[column]
                .get(&rows.bytes)
                .map_err(|error| errors::wire(py, error))?
                .ok_or_else(|| {
                    errors::wire(
                        py,
                        WireError::Malformed("SHOW WARNINGS contains an unexpected NULL field"),
                    )
                })?;
            let value =
                output::field(py, bytes, plan, types, Some(rows.scope())).inspect_err(|error| {
                    errors::note_format(
                        py,
                        error,
                        format_args!("while decoding SHOW WARNINGS row {index}, column {column}"),
                    );
                })?;
            input::check(py, budget)?;
            let valid = if column == 1 {
                value.is_exact_instance_of::<PyInt>()
            } else {
                value.is_exact_instance_of::<PyString>()
            };
            if !valid {
                return Err(errors::wire(
                    py,
                    WireError::Malformed("SHOW WARNINGS decoded field has an unexpected type"),
                ));
            }
            Ok(value)
        };
        let entry = output::tuple_from_ready(py, [decode(0)?, decode(1)?, decode(2)?])?;
        result.push(entry.into_any().unbind());
    }
    input::check(py, budget)?;
    output::tuple_from_ready_vec(py, result)
}

/// Exercise the real diagnostic decoder with complete real-format responses.
/// The shared Receiver owns protocol validation; no metadata-name snapshots,
/// transport, Cursor or alternate protocol parser are constructed here.
#[pyfunction]
#[pyo3(pass_module, signature = (packets, *, strict=true))]
pub(crate) fn _warnings_from_packets<'py>(
    module: &Bound<'py, PyModule>,
    packets: &Bound<'py, pyo3::types::PyList>,
    strict: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    use pyo3::exceptions::PyValueError;
    use pyo3::types::PyBytes;
    use sqlaero_core::protocol::CapabilityFlags;
    use sqlaero_core::result::text::{Receiver, Response};
    use sqlaero_core::session::query::QueryError;
    let py = module.py();
    let caps = CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF;
    let mut receiver = Receiver::isolated(caps, 256 * 1024 * 1024)
        .map_err(|error| errors::query(py, QueryError::Result(error)))?;
    let mut complete = None;
    for index in 0..packets.len() {
        if complete.is_some() {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "packet fixture contains a second response",
            ));
        }
        let packet = packets.get_item(index)?.cast_into::<PyBytes>()?;
        complete = receiver
            .push(packet.as_bytes())
            .map_err(|error| errors::query(py, QueryError::Result(error)))?;
        py.check_signals()?;
    }
    let Some(Response::Rows(rows)) = complete else {
        return Err(errors::builtin_message::<PyValueError>(
            py,
            "packet fixture has no complete diagnostic row response",
        ));
    };
    let owner = module.getattr(PyString::from_bytes(py, b"_scalar_types")?)?;
    let types = owner.extract::<PyRef<'_, input::ScalarTypes>>()?;
    let options = Options {
        decode_text_strict: strict,
        ..Options::default()
    };
    let budget = IoBudget::new(
        sqlaero_core::transport::deadline::TimeoutKind::Operation,
        None,
        None,
        None,
    );
    warnings(py, rows, &types, options, &budget)
}
