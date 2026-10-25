//! Exact Python scalar readers; no Python objects cross into the native core.

use crate::errors::{self, DataError, InterfaceError};
use num_bigint::BigInt;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDate, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess, PyFloat, PyInt, PyTime,
    PyTimeAccess, PyTuple, PyType, PyTzInfoAccess,
};
use sqlaero_core::bind::scalar::{Clock, Date, ScalarError};
use sqlaero_core::bind::{scalar, writer};
use sqlaero_core::error::ClientErrorCode;

/// Module-owned immutable type references; there is no process-global Python
/// type pointer or per-value import, and a future interpreter can own its state.
#[pyclass(frozen)]
pub(crate) struct ScalarTypes {
    decimal: Py<PyType>,
    utc: Py<PyAny>,
}

impl ScalarTypes {
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            decimal: py
                .import("decimal")?
                .getattr("Decimal")?
                .cast_into()?
                .unbind(),
            utc: py
                .import("datetime")?
                .getattr("timezone")?
                .getattr("utc")?
                .unbind(),
        })
    }

    /// Read one supported exact scalar directly into the native request.
    /// Approved tzinfo code executes only here, while attached to Python.
    pub(crate) fn write(
        &self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        utc_confirmed: bool,
        limit: usize,
    ) -> PyResult<()> {
        let py = value.py();
        if value.is_none() {
            return writer::append(output, b"NULL", limit).map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyBool>() {
            return writer::append(
                output,
                if value.extract::<bool>()? { b"1" } else { b"0" },
                limit,
            )
            .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyInt>() {
            return integer(value, output, limit);
        }
        if value.is_exact_instance_of::<PyFloat>() {
            return scalar::float(output, value.extract::<f64>()?, limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyDateTime>() {
            let datetime = value.cast::<PyDateTime>()?;
            let mut date = date(datetime);
            let mut clock = clock(datetime);
            let offset = match datetime.get_tzinfo() {
                None => None,
                Some(zone) if zone.is(self.utc.bind(py)) => Some(0),
                Some(_) => {
                    // The exact built-in method validates the tzinfo return
                    // type and offset range, preserving its actual exception.
                    let offset = datetime.call_method0("utcoffset")?;
                    if offset.is_none() {
                        None
                    } else {
                        Some(delta(offset.cast::<PyDelta>()?))
                    }
                }
            };
            if let Some(offset) = offset {
                if !utc_confirmed {
                    return Err(errors::client::<InterfaceError>(
                        py,
                        ClientErrorCode::SessionState,
                        "aware datetime requires a reliably confirmed UTC MySQL session; time_zone is non-UTC or unknown",
                    ));
                }
                (date, clock) =
                    scalar::utc(date, clock, offset).map_err(|e| scalar_error(py, e))?;
            }
            return scalar::date_time(output, date, Some(clock), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyDate>() {
            return scalar::date_time(output, date(value.cast::<PyDate>()?), None, limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyTime>() {
            return scalar::time(output, clock(value.cast::<PyTime>()?), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.is_exact_instance_of::<PyDelta>() {
            return scalar::duration(output, delta(value.cast::<PyDelta>()?), limit)
                .map_err(|e| errors::wire(py, e));
        }
        if value.get_type().is(self.decimal.bind(py)) {
            let parts = value.call_method0("as_tuple")?;
            let parts = parts.cast::<PyTuple>()?;
            let exponent = parts.get_item(2)?;
            if !exponent.is_exact_instance_of::<PyInt>() {
                return writer::append(output, b"NULL", limit).map_err(|e| errors::wire(py, e));
            }
            let negative = parts.get_item(0)?.extract::<u8>()? != 0;
            let digits = parts.get_item(1)?.extract::<Vec<u8>>()?;
            return scalar::decimal(output, negative, &digits, exponent.extract::<i64>()?, limit)
                .map_err(|e| scalar_error(py, e));
        }
        Err(PyTypeError::new_err(format!(
            "unsupported exact escape type: {}",
            value.get_type().name()?
        )))
    }
}

fn integer(value: &Bound<'_, PyAny>, output: &mut Vec<u8>, limit: usize) -> PyResult<()> {
    let py = value.py();
    let mut overflow = 0;
    // SAFETY: exact-int dispatch precedes this call; value remains owned and
    // Python stays attached. The public C API reports overflow without a
    // lossy conversion or a Python string. No private integer layout is read.
    let compact = unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(value.as_ptr(), &mut overflow) };
    if overflow == 0 {
        if compact == -1
            && let Some(error) = PyErr::take(py)
        {
            return Err(error);
        }
        return scalar::integer(output, compact, limit).map_err(|e| errors::wire(py, e));
    }
    if overflow > 0
        && let Ok(unsigned) = value.extract::<u64>()
    {
        return scalar::integer(output, unsigned, limit).map_err(|e| errors::wire(py, e));
    }
    let value = value.extract::<BigInt>()?;
    scalar::large_integer(output, &value, limit).map_err(|e| scalar_error(py, e))
}

fn scalar_error(py: Python<'_>, error: ScalarError) -> PyErr {
    match error {
        ScalarError::Wire(error) => errors::wire(py, error),
        ScalarError::Range(message) => {
            errors::client::<DataError>(py, ClientErrorCode::NumericRange, message)
        }
    }
}
fn date(value: &impl PyDateAccess) -> Date {
    Date {
        year: value.get_year(),
        month: value.get_month(),
        day: value.get_day(),
    }
}
fn clock(value: &impl PyTimeAccess) -> Clock {
    Clock {
        hour: value.get_hour(),
        minute: value.get_minute(),
        second: value.get_second(),
        microsecond: value.get_microsecond(),
    }
}
fn delta(value: &impl PyDeltaAccess) -> i128 {
    (i128::from(value.get_days()) * 86_400 + i128::from(value.get_seconds())) * 1_000_000
        + i128::from(value.get_microseconds())
}
