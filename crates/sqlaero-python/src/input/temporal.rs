//! Attached error and source-null boundaries around the shared native writer.

use pyo3::prelude::*;
use sqlaero_core::bind::{Context, temporal, writer};
use sqlaero_core::transport::deadline::IoBudget;

pub(crate) use temporal::{TimeKind, TimeUnit};

#[expect(
    clippy::too_many_arguments,
    reason = "fixed source-adapter boundary avoids a per-value owned representation"
)]
pub(crate) fn write(
    py: Python<'_>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
    output: &mut Vec<u8>,
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    if ticks == i64::MIN {
        return writer::append(output, b"NULL", context.limit)
            .map_err(|error| crate::errors::wire(py, error));
    }
    if multiplier == 0 {
        return Err(crate::errors::builtin_message::<
            pyo3::exceptions::PyTypeError,
        >(
            py, "NumPy datetime unit multiplier must be positive"
        ));
    }
    write_valid(py, ticks, unit, multiplier, kind, output, context, budget)
}

#[expect(
    clippy::too_many_arguments,
    reason = "source-specific null handling must not turn valid Arrow minimum ticks into NULL"
)]
pub(crate) fn write_valid(
    py: Python<'_>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    kind: TimeKind,
    output: &mut Vec<u8>,
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    temporal::write(output, ticks, unit, multiplier, kind, context.limit)
        .map_err(|error| super::scalar_error(py, error))
}

pub(crate) fn write_clock(
    py: Python<'_>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    output: &mut Vec<u8>,
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    temporal::write_clock(output, ticks, unit, multiplier, context.limit)
        .map_err(|error| super::scalar_error(py, error))
}

pub(crate) fn write_date(
    py: Python<'_>,
    days: i64,
    output: &mut Vec<u8>,
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    temporal::write_date(output, days, context.limit)
        .map_err(|error| super::scalar_error(py, error))
}
