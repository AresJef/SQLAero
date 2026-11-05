//! Attached error and source-null boundaries around the shared native writer.

use super::sink::{Destination, emit};
use pyo3::prelude::*;
use sqlaero_core::bind::value::ValueRef;
use sqlaero_core::bind::{Context, temporal};
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
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    if ticks == i64::MIN {
        return emit(output, py, ValueRef::Null, context);
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
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    emit(
        output,
        py,
        ValueRef::Ticks {
            value: ticks,
            unit,
            multiplier,
            kind,
        },
        context,
    )
}

pub(crate) fn write_clock(
    py: Python<'_>,
    ticks: i64,
    unit: TimeUnit,
    multiplier: u64,
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    emit(
        output,
        py,
        ValueRef::ClockTicks {
            value: ticks,
            unit,
            multiplier,
        },
        context,
    )
}

pub(crate) fn write_date(
    py: Python<'_>,
    days: i64,
    output: &mut (impl Destination + ?Sized),
    context: &Context,
    _budget: Option<&IoBudget>,
) -> PyResult<()> {
    emit(output, py, ValueRef::DateDays(days), context)
}
