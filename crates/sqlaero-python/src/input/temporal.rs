//! Attached error and source-null boundaries around the shared native writer.

use super::sink::{Destination, emit};
use crate::input::control::Checkpoint;
use pyo3::prelude::*;
use sqlaero_core::bind::value::ValueRef;
use sqlaero_core::bind::{Context, temporal};

pub(crate) use temporal::{TimeKind, TimeUnit};

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
    _budget: Option<&dyn Checkpoint>,
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
    _budget: Option<&dyn Checkpoint>,
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
    _budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    emit(output, py, ValueRef::DateDays(days), context)
}
