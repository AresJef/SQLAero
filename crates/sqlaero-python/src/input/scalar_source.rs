//! The single extended scalar registry, shared by every output target.
//!
//! Primitive recognition remains inlined separately. This registry retains
//! source owners at the attached boundary; targets receive the acquired native
//! facts and never repeat Python/provider type selection.

use super::{ScalarTypes, clock, date, delta, numpy, pandas};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyTime, PyTuple};
use sqlaero_transcode::value::ScalarRef;

pub(super) enum Source<'a, 'py> {
    Native(ScalarRef<'a>),
    Datetime(&'a Bound<'py, PyDateTime>),
    Clock(&'a Bound<'py, PyTime>),
    Decimal(&'a Bound<'py, PyAny>),
    StructTime(&'a Bound<'py, PyTuple>),
    Json(&'a Bound<'py, super::json::Json>),
    Pandas {
        owner: &'a Bound<'py, PyAny>,
        facts: pandas::PandasTemporal,
    },
    Numpy(numpy::NumpyScalar),
}

pub(super) fn read<'a, 'py>(
    types: &ScalarTypes,
    value: &'a Bound<'py, PyAny>,
    budget: Option<&dyn super::control::Checkpoint>,
) -> PyResult<Option<Source<'a, 'py>>> {
    let py = value.py();
    // JSON is final. Probe its exact type without creating errors for other scalars.
    if value.is_exact_instance_of::<super::json::Json>() {
        return Ok(Some(Source::Json(value.cast_exact()?)));
    }
    // Standard exact types do not pay optional-provider discovery costs.
    if value.is_exact_instance_of::<PyDateTime>() {
        return Ok(Some(Source::Datetime(value.cast()?)));
    }
    if value.is_exact_instance_of::<PyDate>() {
        return Ok(Some(Source::Native(ScalarRef::Date(date(
            value.cast::<PyDate>()?,
        )))));
    }
    if value.is_exact_instance_of::<PyTime>() {
        return Ok(Some(Source::Clock(value.cast()?)));
    }
    if value.is_exact_instance_of::<PyDelta>() {
        return Ok(Some(Source::Native(ScalarRef::Duration(delta(
            value.cast::<PyDelta>()?,
        )))));
    }
    if value.get_type().is(types.decimal_type(py)) {
        return Ok(Some(Source::Decimal(value)));
    }
    if value.get_type().is(types.struct_time.bind(py)) {
        return Ok(Some(Source::StructTime(value.cast()?)));
    }
    if let Some(numpy) = types.numpy_types(py, budget)? {
        let mut pandas = types.pandas.get();
        let mut missing = pandas.is_some_and(|types| types.is_missing(value));
        // The proven NumPy base rules out every registered scalar type. Only
        // then may a cached Pandas singleton skip the NumPy type-table scan.
        if missing && numpy.outside_numeric_base(&value.get_type()) {
            return Ok(Some(Source::Native(ScalarRef::Null)));
        }
        if let Some(facts) = numpy.read(value, budget)? {
            return Ok(Some(Source::Numpy(facts)));
        }
        if pandas.is_none() {
            pandas = types.pandas_types(py, budget)?;
            missing = pandas.is_some_and(|types| types.is_missing(value));
        }
        if missing {
            return Ok(Some(Source::Native(ScalarRef::Null)));
        }
        // Pandas temporal facts win over standard datetime/timedelta ancestry.
        if let Some(pandas) = pandas
            && let Some(facts) = pandas.read_temporal(value, numpy, budget)?
        {
            return Ok(Some(Source::Pandas {
                owner: value,
                facts,
            }));
        }
    }
    if value.is_instance_of::<PyDateTime>() {
        Ok(Some(Source::Datetime(value.cast()?)))
    } else if value.is_instance_of::<PyDate>() {
        Ok(Some(Source::Native(ScalarRef::Date(date(
            value.cast::<PyDate>()?,
        )))))
    } else if value.is_instance_of::<PyTime>() {
        Ok(Some(Source::Clock(value.cast()?)))
    } else if value.is_instance_of::<PyDelta>() {
        Ok(Some(Source::Native(ScalarRef::Duration(delta(
            value.cast::<PyDelta>()?,
        )))))
    } else if super::actual_instance(value, types.decimal_type(py)) {
        Ok(Some(Source::Decimal(value)))
    } else {
        Ok(types
            .uuid_value(value, budget)?
            .map(|value| Source::Native(ScalarRef::Uuid(value))))
    }
}

pub(super) fn clock_fact(value: &Bound<'_, PyTime>) -> ScalarRef<'static> {
    // MySQL does not request the time-only tzinfo protocol.
    ScalarRef::Time {
        clock: clock(value),
        offset_micros: None,
    }
}
