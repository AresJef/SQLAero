//! MaskedArray base data and actual instance masks, without filled() or hooks.

use super::{NumpyArrayApi, NumpySource};
use crate::errors;
use crate::input::cell::Target;
use crate::input::control::{self, Checkpoint};
use crate::input::source::{PreparedSource, Shape};
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict};
use sqlaero_transcode::value::ScalarRef;

enum Mask<'py> {
    Scalar(bool),
    Array(Box<NumpySource<'py>>),
}

/// One source-owned mask snapshot. Replacement is checked after every callback
/// boundary before reuse; its mutable array storage keeps the shared external
/// stability/all-writers-synchronize contract.
struct State<'py> {
    owner: Bound<'py, PyAny>,
    original: Bound<'py, PyAny>,
    mask: Mask<'py>,
}

fn current<'py>(value: &Bound<'py, PyAny>, budget: &dyn Checkpoint) -> PyResult<Bound<'py, PyAny>> {
    control::callback(value.py(), Some(budget), || {
        // SAFETY: value is a retained actual MaskedArray. The documented public
        // API returns an owned instance dictionary without __getattribute__ or
        // __dict__ replacements; NULL context is required by its public API.
        // https://docs.python.org/3.12/c-api/object.html#c.PyObject_GenericGetDict
        let dictionary = unsafe {
            Bound::from_owned_ptr_or_err(
                value.py(),
                pyo3::ffi::PyObject_GenericGetDict(value.as_ptr(), std::ptr::null_mut()),
            )
        }?
        .cast_into::<PyDict>()?;
        // A hostile dictionary key can run equality; the common wrapper owns
        // the pre/post lifecycle checks, with no exported buffer in this scope.
        dictionary.get_item("_mask")?.ok_or_else(|| {
            errors::builtin_message::<PyTypeError>(
                value.py(),
                "MaskedArray actual instance has no mask storage",
            )
        })
    })
}

impl<'py> State<'py> {
    fn new(
        value: &Bound<'py, PyAny>,
        data: &NumpySource<'py>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let py = value.py();
        let original = current(value, budget)?;
        let mask = if original.is_exact_instance_of::<PyBool>() {
            Mask::Scalar(original.extract()?)
        } else if original.get_type().is(api.boolean.typ(py)) {
            Mask::Scalar(super::fixed_bytes(&original, &api.boolean, 1)?[0] != 0)
        } else if api.accepts(&original) {
            let source = NumpySource::new(&original, api, budget)?;
            let same_axes = source.shape() == data.shape()
                && match data.shape() {
                    Shape::Tensor { dimensions, .. } => (0..dimensions)
                        .all(|axis| source.axis_length(axis) == data.axis_length(axis)),
                    _ => true,
                };
            if !source.is_boolean() || !same_axes {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "MaskedArray mask must have actual bool dtype and the same axes as its data",
                ));
            }
            Mask::Array(crate::input::boxed(py, source)?)
        } else {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "MaskedArray mask must be an actual bool value or supported bool ndarray",
            ));
        };
        Ok(Self {
            owner: value.clone(),
            original,
            mask,
        })
    }

    fn missing(&mut self, row: usize, column: usize, budget: &dyn Checkpoint) -> PyResult<bool> {
        if !current(&self.owner, budget)?.is(&self.original) {
            return Err(errors::builtin_message::<PyBufferError>(
                self.owner.py(),
                "MaskedArray mask changed during escape; keep data and mask storage stable",
            ));
        }
        match &mut self.mask {
            Mask::Scalar(value) => Ok(*value),
            Mask::Array(source) => source.bool_cell(row, column, budget),
        }
    }
}

/// Read actual mask first, then acquire payload only at an unmasked position.
pub(crate) struct MaskedSource<'py> {
    data: NumpySource<'py>,
    state: State<'py>,
}

impl<'py> MaskedSource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let data = NumpySource::new(value, api, budget)?;
        let state = State::new(value, &data, api, budget)?;
        Ok(Self { data, state })
    }
}

impl<'py> PreparedSource<'py> for MaskedSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.state.owner
    }
    fn shape(&self) -> Shape {
        self.data.shape()
    }
    fn axis_length(&self, axis: usize) -> Option<usize> {
        self.data.axis_length(axis)
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        self.data.schema(target)
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell_to(row, column, target, budget)
    }
    crate::input::source::cell_destinations!();
}

impl MaskedSource<'_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        target.checkpoint(self.owner().py(), 1)?;
        if self.state.missing(row, column, budget)? {
            self.data.validate_layout(budget)?;
            target.scalar(self.owner().py(), ScalarRef::Null)
        } else {
            self.data.consume_cell_to(row, column, target, budget)
        }
    }
}
