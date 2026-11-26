//! Explicit caller-owned SQL conversion, confined to the Python boundary.

use super::control::{self, Checkpoint};
use crate::errors;
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::{
    Context,
    template::{Compiler, SpliceBoundary},
    writer,
};
use sqlaero_core::error::ClientErrorCode;
use std::cell::Cell;

/// Convert one value to caller-supplied SQL text during text binding.
///
/// converter is called once per visited position with the original value. It
/// must return a built-in str. Use ordinary parameters for untrusted data.
/// Text execution and local previews accept this wrapper. Native prepared
/// execution rejects it before invoking converter.
#[pyclass(module = "sqlaero")]
pub(crate) struct CustomEscape {
    value: Option<Py<PyAny>>,
    converter: Option<Py<PyAny>>,
}

#[pymethods]
impl CustomEscape {
    #[new]
    fn new(value: Py<PyAny>, converter: Bound<'_, PyAny>) -> PyResult<Self> {
        if !converter.is_callable() {
            return Err(errors::builtin_message::<PyTypeError>(
                converter.py(),
                "CustomEscape converter must be callable",
            ));
        }
        Ok(Self {
            value: Some(value),
            converter: Some(converter.unbind()),
        })
    }

    /// The original payload, kept alive without copying or conversion.
    #[getter]
    fn value(&self, py: Python<'_>) -> Py<PyAny> {
        self.value
            .as_ref()
            .map_or_else(|| py.None(), |value| value.clone_ref(py))
    }

    /// The original callable, kept alive without inspecting its signature.
    #[getter]
    fn converter(&self, py: Python<'_>) -> Py<PyAny> {
        self.converter
            .as_ref()
            .map_or_else(|| py.None(), |value| value.clone_ref(py))
    }

    fn __repr__(&self) -> &'static str {
        "CustomEscape(...)"
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.value)?;
        visit.call(&self.converter)
    }

    fn __clear__(&mut self) {
        self.value = None;
        self.converter = None;
    }
}

struct Converted {
    slot: usize,
    source: Py<CustomEscape>,
    text: Py<PyString>,
}

/// Only a row that might be rebound retains its per-position results.
#[derive(Default)]
pub(crate) struct Conversions {
    values: Vec<Converted>,
    replay: Option<usize>,
}

impl Conversions {
    pub(crate) fn begin_replay(&mut self) {
        self.replay = Some(0);
    }

    pub(crate) fn finish_replay(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.replay.take() == Some(self.values.len()) {
            Ok(())
        } else {
            Err(previous_position_changed(py))
        }
    }

    pub(crate) fn convert<'py>(
        &mut self,
        slot: usize,
        source: &Bound<'py, CustomEscape>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyString>> {
        let py = source.py();
        if let Some(next) = &mut self.replay {
            let prior = self
                .values
                .get(*next)
                .filter(|prior| prior.slot == slot && prior.source.bind(py).is(source))
                .ok_or_else(|| previous_position_changed(py))?;
            *next += 1;
            return Ok(prior.text.bind(py).clone());
        }
        self.values
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        let text = CustomEscape::convert(source, budget)?;
        self.values.push(Converted {
            slot,
            source: source.clone().unbind(),
            text: text.clone().unbind(),
        });
        Ok(text)
    }
}

fn previous_position_changed(py: Python<'_>) -> PyErr {
    errors::client(
        py,
        ClientErrorCode::TemplateBinding,
        "CustomEscape position changed while rebinding a pending row; current request was not sent",
    )
}

thread_local! {
    static CONVERSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct Conversion;

impl Conversion {
    fn enter(py: Python<'_>) -> PyResult<Self> {
        CONVERSION_DEPTH.with(|depth| {
            if depth.get() >= 64 {
                return Err(errors::client(
                    py,
                    ClientErrorCode::ContainerNesting,
                    "CustomEscape callbacks exceed 64 active conversions",
                ));
            }
            depth.set(depth.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for Conversion {
    fn drop(&mut self) {
        CONVERSION_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

impl CustomEscape {
    /// Clone the two owners in one short borrow, releasing it before Python.
    pub(crate) fn convert<'py>(
        object: &Bound<'py, Self>,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyString>> {
        let py = object.py();
        let (value, converter) = {
            let wrapper = object.try_borrow()?;
            let (Some(value), Some(converter)) = (&wrapper.value, &wrapper.converter) else {
                return Err(errors::builtin_message::<PyRuntimeError>(
                    py,
                    "CustomEscape references have been cleared",
                ));
            };
            (value.bind(py).clone(), converter.bind(py).clone())
        };
        let result = control::callback(py, Some(budget), || {
            let _conversion = Conversion::enter(py)?;
            converter.call1((value,))
        })?;
        result.cast_into_exact::<PyString>().map_err(|_| {
            errors::builtin_message::<PyTypeError>(
                py,
                "CustomEscape converter must return an exact built-in str",
            )
        })
    }
}

/// Encode and prove the independently embeddable portion with the shared lexer.
/// The original text is retained by the caller for any subsequent re-encoding.
pub(crate) fn write(
    source: &Bound<'_, PyString>,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<bool> {
    let py = source.py();
    let mut fragment = Vec::new();
    super::text(
        source,
        &mut fragment,
        &Context {
            limit: context.limit.saturating_sub(output.len()),
            ..*context
        },
        false,
        Some(budget),
    )?;
    let mut compiler = Compiler::for_fragment(fragment, context);
    let verified = loop {
        let step = py
            .detach(|| compiler.advance())
            .map_err(|error| crate::binding::template_error(py, error))?;
        super::check(py, budget)?;
        if let Some(verified) = step {
            break verified;
        }
    };
    let scalar = verified.fragment_is_scalar();
    for chunk in verified.into_source().chunks(super::WORK_BYTES) {
        super::check(py, budget)?;
        writer::append(output, chunk, context.limit).map_err(|error| errors::wire(py, error))?;
    }
    Ok(scalar)
}

pub(crate) fn boundary(
    py: Python<'_>,
    boundaries: &mut Vec<SpliceBoundary>,
    position: usize,
    fragment: bool,
) -> PyResult<()> {
    if let Some(last) = boundaries.last_mut().filter(|last| last.offset == position) {
        last.fragment |= fragment;
    } else {
        boundaries
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        boundaries.push(SpliceBoundary {
            offset: position,
            fragment,
        });
    }
    Ok(())
}

pub(crate) fn verify_splices(
    py: Python<'_>,
    output: Vec<u8>,
    boundaries: Option<Vec<SpliceBoundary>>,
    context: &Context,
    budget: &dyn Checkpoint,
) -> PyResult<Vec<u8>> {
    let Some(boundaries) = boundaries else {
        return Ok(output);
    };
    let mut compiler = Compiler::for_spliced_query(output, boundaries, context)
        .map_err(|error| errors::wire(py, error))?;
    loop {
        let step = py
            .detach(|| compiler.advance())
            .map_err(|error| crate::binding::template_error(py, error))?;
        super::check(py, budget)?;
        if let Some(verified) = step {
            return Ok(verified.into_source());
        }
    }
}
