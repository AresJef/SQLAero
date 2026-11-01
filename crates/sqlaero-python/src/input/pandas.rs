//! Pandas public storage adapters; native values reuse the common SQL writers.

use crate::errors;
use crate::input::numpy::{NumpyScalar, NumpyTypes};
use crate::input::source::PreparedSource;
use crate::input::temporal::{TimeKind, TimeUnit};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyType};
use sqlaero_core::transport::deadline::IoBudget;
use std::ffi::CStr;

mod source;

/// Scalar facts are native before they enter the shared SQL writing path.
pub(crate) enum PandasScalar {
    Null,
    Temporal {
        ticks: i64,
        unit: TimeUnit,
        multiplier: u64,
        kind: TimeKind,
        aware: bool,
    },
}

/// Optional canonical identities owned by this Python module's type cache.
pub(crate) struct PandasTypes {
    missing: Py<PyAny>,
    nat: Py<PyAny>,
    timestamp: Py<PyType>,
    timedelta: Py<PyType>,
    sources: source::SourceTypes,
}

impl PandasTypes {
    /// Prepare the already-used provider on its own optional boundary path.
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        let module = errors::module(py, c"pandas")?;
        Ok(Self {
            missing: errors::attribute(module.as_any(), c"NA")?.unbind(),
            nat: errors::attribute(module.as_any(), c"NaT")?.unbind(),
            timestamp: exact_type(&module, c"Timestamp")?,
            timedelta: exact_type(&module, c"Timedelta")?,
            sources: source::SourceTypes::new(&module)?,
        })
    }

    /// Normalize only supported public sources into the shared typed contract.
    pub(crate) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        numpy: &NumpyTypes,
        budget: &IoBudget,
    ) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
        self.sources.prepare(value, numpy, budget)
    }

    /// Keep missing singletons ahead of datetime subclass handling. asm8
    /// retains the canonical scalar's unit and nanoseconds; aware Timestamp
    /// exposes UTC epoch ticks without a per-value Python timezone conversion.
    pub(crate) fn read_scalar(
        &self,
        value: &Bound<'_, PyAny>,
        numpy: &NumpyTypes,
    ) -> PyResult<Option<PandasScalar>> {
        let py = value.py();
        if value.is(self.missing.bind(py)) || value.is(self.nat.bind(py)) {
            return Ok(Some(PandasScalar::Null));
        }
        let (recognized, aware) = if value.get_type().is(self.timestamp.bind(py)) {
            (true, !errors::attribute(value, c"tzinfo")?.is_none())
        } else {
            (value.get_type().is(self.timedelta.bind(py)), false)
        };
        if !recognized {
            return Ok(None);
        }
        let scalar = errors::attribute(value, c"asm8")?;
        let Some(NumpyScalar::Temporal {
            ticks,
            unit,
            multiplier,
            kind,
        }) = numpy.read(&scalar)?
        else {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical Pandas asm8 must expose its exact NumPy datetime64 or timedelta64 scalar",
            ));
        };
        Ok(Some(PandasScalar::Temporal {
            ticks,
            unit,
            multiplier,
            kind,
            aware,
        }))
    }
}

fn exact_type(module: &Bound<'_, PyModule>, name: &CStr) -> PyResult<Py<PyType>> {
    Ok(errors::attribute(module.as_any(), name)?
        .cast_into::<PyType>()?
        .unbind())
}
