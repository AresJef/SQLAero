//! Pandas public storage adapters; native values reuse the common SQL writers.

use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::control::Checkpoint;
use crate::input::numpy::{NumpyScalar, NumpyTypes};
use crate::input::source::PreparedSource;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDateTime, PyModule, PyType, PyTzInfoAccess};
use sqlaero_transcode::temporal::{TimeKind, TimeUnit};
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
    timestamp_storage: GetSet,
    timedelta_storage: GetSet,
    sources: Py<source::SourceTypes>,
}

impl PandasTypes {
    /// Prepare the already-used provider on its own optional boundary path.
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        let module = errors::module(py, c"pandas")?;
        let timestamp = exact_type(&module, c"Timestamp")?;
        let timedelta = exact_type(&module, c"Timedelta")?;
        Ok(Self {
            missing: errors::attribute(module.as_any(), c"NA")?.unbind(),
            nat: errors::attribute(module.as_any(), c"NaT")?.unbind(),
            timestamp_storage: GetSet::new(timestamp.bind(py), c"asm8", None)?,
            timedelta_storage: GetSet::new(timedelta.bind(py), c"asm8", None)?,
            timestamp,
            timedelta,
            sources: Py::new(py, source::SourceTypes::new(&module)?)?,
        })
    }

    /// Normalize only supported public sources into the shared typed contract.
    pub(crate) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        numpy: &NumpyTypes,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
        self.sources
            .bind(value.py())
            .get()
            .prepare(value, numpy, budget, &self.sources)
    }

    /// Keep missing singletons ahead of datetime subclass handling. asm8
    /// retains the canonical scalar's unit and nanoseconds; aware Timestamp
    /// exposes UTC epoch ticks without a per-value Python timezone conversion.
    pub(crate) fn read_scalar(
        &self,
        value: &Bound<'_, PyAny>,
        numpy: &NumpyTypes,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<PandasScalar>> {
        let py = value.py();
        if value.is(self.missing.bind(py)) || value.is(self.nat.bind(py)) {
            return Ok(Some(PandasScalar::Null));
        }
        let (storage, aware) = if super::actual_instance(value, self.timestamp.bind(py)) {
            (
                &self.timestamp_storage,
                value.cast::<PyDateTime>()?.get_tzinfo().is_some(),
            )
        } else if super::actual_instance(value, self.timedelta.bind(py)) {
            (&self.timedelta_storage, false)
        } else {
            return Ok(None);
        };
        let scalar = super::control::callback(py, budget, || storage.get(value))?;
        let Some(NumpyScalar::Temporal {
            ticks,
            unit,
            multiplier,
            kind,
        }) = numpy.read(&scalar, budget)?
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
