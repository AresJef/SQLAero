//! Retry configuration and attempt boundaries; no implicit database replay.

use crate::{config::Argument, errors, output};
use num_bigint::BigInt;
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyException, PyOSError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyTuple, PyType};
use sqlaero_core::retry::{Policy, Run};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

fn entries(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<Py<PyAny>>> {
    let py = value.py();
    let values = if let Ok(tuple) = value.cast::<PyTuple>() {
        tuple.iter().map(Bound::unbind).collect()
    } else if let Ok(list) = value.cast::<PyList>() {
        list.iter().map(Bound::unbind).collect()
    } else {
        vec![value.clone().unbind()]
    };
    if values.is_empty() {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            format!("{name} cannot be an empty collection"),
        ));
    }
    Ok(values)
}

/// Immutable AND condition: any listed type and any listed code must match.
/// None omits a condition; at least one nonempty condition is required. Code
/// matching reads only an actual SQLAeroError's code, never OSError.errno/text.
#[pyclass(module = "sqlaero.retry")]
pub(crate) struct RetryRule {
    types: Option<Py<PyTuple>>,
    codes: Option<Py<PyTuple>>,
    numbers: Option<Vec<BigInt>>,
}
impl RetryRule {
    fn matches(&self, py: Python<'_>, error: &PyErr) -> bool {
        if !error.is_instance_of::<PyException>(py) {
            return false;
        }
        let types = self
            .types
            .as_ref()
            .is_none_or(|types| error.is_instance(py, types.bind(py).as_any()));
        let codes = self.numbers.as_ref().is_none_or(|codes| {
            error
                .value(py)
                .extract::<PyRef<'_, errors::SQLAeroError>>()
                .ok()
                .and_then(|error| error.code().cloned())
                .is_some_and(|code| codes.contains(&code))
        });
        types && codes
    }
}
#[pymethods]
impl RetryRule {
    #[new]
    #[pyo3(signature=(*,error_types=None,error_codes=None))]
    fn new(
        py: Python<'_>,
        error_types: Option<&Bound<'_, PyAny>>,
        error_codes: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let types=error_types.map(|value| {
            let values=entries(value,"error_types")?;
            for (index,value) in values.iter().enumerate() {
                if !value.bind(py).cast::<PyType>().is_ok_and(|typ| typ.is_subclass_of::<PyException>().unwrap_or(false)) {
                    return Err(crate::errors::builtin_message::<PyTypeError>(py, format!("error_types[{index}] must be an Exception subclass, excluding control-flow BaseException classes")));
                }
            }
            output::tuple_from_ready_vec(py,values).map(Bound::unbind)
        }).transpose()?;
        let numbers = error_codes
            .map(|value| {
                entries(value, "error_codes")?
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let value = value.bind(py);
                        if !value.is_instance_of::<PyInt>() || value.is_instance_of::<PyBool>() {
                            return Err(crate::errors::builtin_message::<PyTypeError>(
                                py,
                                format!("error_codes[{index}] must be int or IntEnum, not bool"),
                            ));
                        }
                        errors::integer_value(value)
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        if types.is_none() && numbers.is_none() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "at least one of error_types or error_codes must be provided",
            ));
        }
        let codes = numbers
            .as_ref()
            .map(|values| {
                output::tuple_from_values(
                    py,
                    values
                        .iter()
                        .map(|value| errors::code_object(py, value).map(Bound::into_any)),
                )
                .map(Bound::unbind)
            })
            .transpose()?;
        Ok(Self {
            types,
            codes,
            numbers,
        })
    }
    /// Normalized immutable exception classes, or None when not constrained.
    #[getter]
    fn error_types(&self, py: Python<'_>) -> Option<Py<PyTuple>> {
        self.types.as_ref().map(|value| value.clone_ref(py))
    }
    /// Normalized plain integer codes, or None when not constrained.
    #[getter]
    fn error_codes(&self, py: Python<'_>) -> Option<Py<PyTuple>> {
        self.codes.as_ref().map(|value| value.clone_ref(py))
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.types)?;
        visit.call(&self.codes)
    }
    fn __clear__(&mut self) {
        self.types = None;
        self.codes = None;
    }
}

fn numeric(
    py: Python<'_>,
    value: &Argument<'_>,
    name: &str,
    default: f64,
) -> PyResult<(f64, Py<PyAny>)> {
    let original = value.0.as_ref().map_or_else(
        || {
            // SAFETY: this attached scalar constructor returns a new exact
            // float or its original allocation failure; it must not use an
            // infallible IntoPyObject conversion at the FFI boundary.
            unsafe {
                Bound::<PyAny>::from_owned_ptr_or_err(py, pyo3::ffi::PyFloat_FromDouble(default))
            }
        },
        |value| Ok(value.clone()),
    )?;
    if !original.is_exact_instance_of::<PyInt>() && !original.is_exact_instance_of::<PyFloat>() {
        return Err(crate::errors::builtin_message::<PyTypeError>(
            py,
            format!("{name} must be a built-in int or float, not bool"),
        ));
    }
    let number = original.extract::<f64>().map_err(|_| {
        crate::errors::builtin_message::<PyValueError>(
            py,
            format!("{name} cannot be represented safely"),
        )
    })?;
    if !number.is_finite() || number < 0.0 {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            format!("{name} must be finite and nonnegative"),
        ));
    }
    Ok((number, original.unbind()))
}

/// Reusable immutable retry configuration. Each iteration owns fresh native
/// count/backoff state. The caller owns connection replacement and replay safety.
#[pyclass(module = "sqlaero.retry")]
pub(crate) struct Retry {
    rules: Option<Py<PyTuple>>,
    policy: Policy,
    maximum: Option<u64>,
    interval: Py<PyAny>,
    multiplier: Py<PyAny>,
    cap: Py<PyAny>,
    jitter: bool,
}
impl Retry {
    fn matches(&self, py: Python<'_>, error: &PyErr) -> PyResult<bool> {
        let rules = self.rules.as_ref().ok_or_else(|| {
            crate::errors::builtin_message::<PyRuntimeError>(py, "retry configuration was released")
        })?;
        for rule in rules.bind(py).iter() {
            if rule.extract::<PyRef<'_, RetryRule>>()?.matches(py, error) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
#[pymethods]
impl Retry {
    #[new]
    #[pyo3(signature=(*rules,max_attempts=Argument::default(),wait_interval=Argument::default(),wait_multiplier=Argument::default(),max_wait_interval=Argument::default(),wait_jitter=Argument::default()))]
    #[pyo3(
        text_signature = "(*rules, max_attempts=3, wait_interval=1.0, wait_multiplier=1.0, max_wait_interval=10.0, wait_jitter=False)"
    )]
    fn new(
        py: Python<'_>,
        rules: &Bound<'_, PyTuple>,
        max_attempts: Argument<'_>,
        wait_interval: Argument<'_>,
        wait_multiplier: Argument<'_>,
        max_wait_interval: Argument<'_>,
        wait_jitter: Argument<'_>,
    ) -> PyResult<Self> {
        if rules.is_empty() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "Retry requires at least one RetryRule",
            ));
        }
        for (index, rule) in rules.iter().enumerate() {
            if !rule.is_instance_of::<RetryRule>() {
                return Err(crate::errors::builtin_message::<PyTypeError>(
                    py,
                    format!("rules[{index}] must be RetryRule"),
                ));
            }
        }
        let maximum = match max_attempts.0.as_ref() {
            None => Some(3),
            Some(value) if value.is_none() => None,
            Some(value) if value.is_exact_instance_of::<PyInt>() => Some(
                value
                    .extract::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        crate::errors::builtin_message::<PyValueError>(
                            py,
                            "max_attempts must be 1..2**64-1, or None for unlimited attempts",
                        )
                    })?,
            ),
            _ => {
                return Err(crate::errors::builtin_message::<PyTypeError>(
                    py,
                    "max_attempts must be a built-in int or None, not bool",
                ));
            }
        };
        let (initial, interval) = numeric(py, &wait_interval, "wait_interval", 1.0)?;
        let (factor, multiplier) = numeric(py, &wait_multiplier, "wait_multiplier", 1.0)?;
        let (limit, cap) = numeric(py, &max_wait_interval, "max_wait_interval", 10.0)?;
        let policy = Policy::new(maximum, initial, factor, limit).map_err(|name| {
            crate::errors::builtin_message::<PyValueError>(
                py,
                format!("{name} is outside its supported finite timing range"),
            )
        })?;
        let jitter = wait_jitter.0.as_ref().map_or(Ok(false), |value| {
            crate::config::boolean(value, "wait_jitter")
        })?;
        Ok(Self {
            rules: Some(rules.clone().unbind()),
            policy,
            maximum,
            interval,
            multiplier,
            cap,
            jitter,
        })
    }
    /// Immutable rule tuple. Rules are ORed; each rule combines its conditions with AND.
    #[getter]
    fn rules(&self, py: Python<'_>) -> Option<Py<PyTuple>> {
        self.rules.as_ref().map(|value| value.clone_ref(py))
    }
    /// Maximum attempts including the first, or None for no count limit.
    #[getter]
    fn max_attempts(&self) -> Option<u64> {
        self.maximum
    }
    /// Initial nonnegative waiting seconds; the first attempt never waits.
    #[getter]
    fn wait_interval(&self, py: Python<'_>) -> Py<PyAny> {
        self.interval.clone_ref(py)
    }
    /// Finite multiplier at least one; jitter does not alter its base sequence.
    #[getter]
    fn wait_multiplier(&self, py: Python<'_>) -> Py<PyAny> {
        self.multiplier.clone_ref(py)
    }
    /// Mandatory finite waiting cap, applied even to fixed/first-retry delays.
    #[getter]
    fn max_wait_interval(&self, py: Python<'_>) -> Py<PyAny> {
        self.cap.clone_ref(py)
    }
    /// Whether each actual delay is sampled uniformly from zero to its capped base.
    #[getter]
    fn wait_jitter(&self) -> bool {
        self.jitter
    }
    fn __iter__(slf: PyRef<'_, Self>) -> RetryIterator {
        let state = Arc::new(Mutex::new(slf.policy.run()));
        RetryIterator {
            owner: Some(slf.into()),
            state,
        }
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.rules)?;
        visit.call(&self.interval)?;
        visit.call(&self.multiplier)?;
        visit.call(&self.cap)
    }
    fn __clear__(&mut self) {
        self.rules = None;
    }
}

fn lock<'a>(py: Python<'_>, state: &'a Mutex<Run>) -> PyResult<std::sync::MutexGuard<'a, Run>> {
    state.lock().map_err(|_| {
        crate::errors::builtin_message::<PyRuntimeError>(py, "retry traversal state is unavailable")
    })
}
fn wait(py: Python<'_>, seconds: f64, jitter: bool) -> PyResult<()> {
    if seconds == 0.0 {
        return Ok(());
    }
    let seconds = if jitter {
        let mut bytes = [0; 8];
        getrandom::fill(&mut bytes)
            .map_err(|error| crate::errors::builtin_message::<PyOSError>(py, error.to_string()))?;
        let fraction = (u64::from_ne_bytes(bytes) >> 11) as f64 / ((1u64 << 53) - 1) as f64;
        seconds * fraction
    } else {
        seconds
    };
    let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
        crate::errors::builtin_message::<PyValueError>(
            py,
            "retry delay cannot be represented safely",
        )
    })?;
    let start = Instant::now();
    while let Some(remaining) = duration.checked_sub(start.elapsed()) {
        if remaining.is_zero() {
            break;
        }
        py.detach(|| {
            std::thread::sleep(remaining.min(sqlaero_core::transport::deadline::CONTROL_INTERVAL))
        });
        py.check_signals()?;
    }
    Ok(())
}
/// Internal iterator; it owns only this traversal, never another traversal's state.
#[pyclass(name = "_RetryIterator", module = "sqlaero._sqlaero")]
pub(crate) struct RetryIterator {
    owner: Option<Py<Retry>>,
    state: Arc<Mutex<Run>>,
}
#[pymethods]
impl RetryIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<RetryAttempt>> {
        let delay = lock(py, &self.state)?
            .begin()
            .map_err(|message| crate::errors::builtin_message::<PyRuntimeError>(py, message))?;
        let Some(delay) = delay else {
            return Ok(None);
        };
        let owner = self.owner.as_ref().ok_or_else(|| {
            crate::errors::builtin_message::<PyRuntimeError>(py, "retry traversal was released")
        })?;
        if let Err(error) = wait(py, delay, owner.borrow(py).jitter) {
            lock(py, &self.state)?.cancel();
            return Err(error);
        }
        Ok(Some(RetryAttempt {
            owner: Some(owner.clone_ref(py)),
            state: Arc::clone(&self.state),
            entered: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        }))
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.owner)
    }
    fn __clear__(&mut self) {
        self.owner = None;
    }
}

/// One attempt produced by Retry. Success stops iteration; only a matching
/// ordinary Exception with remaining attempts is suppressed. Control-flow
/// exceptions and the final original error propagate without retry history.
#[pyclass(module = "sqlaero.retry")]
pub(crate) struct RetryAttempt {
    owner: Option<Py<Retry>>,
    state: Arc<Mutex<Run>>,
    entered: AtomicBool,
    exited: AtomicBool,
}
#[pymethods]
impl RetryAttempt {
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        let py = slf.py();
        if slf.entered.swap(true, Ordering::AcqRel) {
            return Err(crate::errors::builtin_message::<PyRuntimeError>(
                py,
                "RetryAttempt can be entered only once",
            ));
        }
        Ok(slf)
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        _typ: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        if !self.entered.load(Ordering::Acquire) || self.exited.swap(true, Ordering::AcqRel) {
            return Err(crate::errors::builtin_message::<PyRuntimeError>(
                py,
                "RetryAttempt has no active context to exit",
            ));
        }
        let matched = if value.is_none() {
            false
        } else {
            let owner = self.owner.as_ref().ok_or_else(|| {
                crate::errors::builtin_message::<PyRuntimeError>(py, "retry attempt was released")
            })?;
            owner
                .borrow(py)
                .matches(py, &PyErr::from_value(value.clone()))?
        };
        lock(py, &self.state)?
            .finish(matched)
            .map_err(|message| crate::errors::builtin_message::<PyRuntimeError>(py, message))
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.owner)
    }
    fn __clear__(&mut self) {
        self.owner = None;
    }
}
