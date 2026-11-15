//! MaskedArray base data and actual instance masks, without filled() or hooks.

use super::{NumpyArrayApi, NumpySource, RecordSource};
use crate::errors;
use crate::input::cell::Target;
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyString};
use sqlaero_transcode::value::ScalarRef;

pub(crate) enum Reader<'py> {
    Scalar(bool),
    Array(Box<NumpySource<'py>>),
}

/// One source-owned mask snapshot. Replacement is checked after every callback
/// boundary before reuse; its mutable array storage keeps the shared external
/// stability/all-writers-synchronize contract.
#[derive(Clone)]
pub(crate) struct State<'py> {
    pub(super) owner: Bound<'py, PyAny>,
    pub(super) original: Bound<'py, PyAny>,
    key: Bound<'py, PyString>,
}

impl<'py> State<'py> {
    pub(crate) fn new(value: &Bound<'py, PyAny>, budget: &dyn Checkpoint) -> PyResult<Self> {
        crate::input::check(value.py(), budget)?;
        let key = PyString::from_bytes(value.py(), b"_mask")?;
        let original = crate::input::attribute::instance_key(value, &key, "_mask", budget)?;
        Ok(Self {
            owner: value.clone(),
            original,
            key,
        })
    }

    pub(crate) fn validate(&self, budget: &dyn Checkpoint) -> PyResult<()> {
        if !crate::input::attribute::instance_key(&self.owner, &self.key, "_mask", budget)?
            .is(&self.original)
        {
            return Err(errors::builtin_message::<PyBufferError>(
                self.owner.py(),
                "MaskedArray mask changed during escape; keep data and mask storage stable",
            ));
        }
        Ok(())
    }

    pub(crate) fn mask(&self) -> &Bound<'py, PyAny> {
        &self.original
    }
}

impl<'py> Reader<'py> {
    pub(super) fn scalar(value: &Bound<'py, PyAny>, api: &NumpyArrayApi) -> PyResult<Option<bool>> {
        if value.is_exact_instance_of::<PyBool>() {
            value.extract().map(Some)
        } else if value.get_type().is(api.boolean.typ(value.py())) {
            Ok(Some(super::fixed_bytes(value, &api.boolean, 1)?[0] != 0))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn new(
        value: &Bound<'py, PyAny>,
        data: &NumpySource<'py>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let py = value.py();
        let mask = if let Some(value) = Self::scalar(value, api)? {
            Self::Scalar(value)
        } else if api.accepts(value) {
            let source = NumpySource::new(value, api, budget)?;
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
            Self::Array(crate::input::boxed(py, source)?)
        } else {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "MaskedArray mask must be an actual bool value or supported bool ndarray",
            ));
        };
        Ok(mask)
    }

    pub(crate) fn missing(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        match self {
            Self::Scalar(value) => Ok(*value),
            Self::Array(source) => source.bool_cell(row, column, budget),
        }
    }

    /// Recheck source identity before reuse. A copied byte or immutable bool
    /// cannot run a protocol; a source read keeps its original post-check.
    pub(crate) fn checked_missing(
        &mut self,
        state: &State<'py>,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        state.validate(budget)?;
        let copied = match self {
            Self::Scalar(value) => Some(*value),
            Self::Array(source) => source.copied_bool_cell(row, column),
        };
        if let Some(value) = copied {
            return Ok(value);
        }
        let value = self.missing(row, column, budget)?;
        state.validate(budget)?;
        Ok(value)
    }
}

/// Read actual mask first, then acquire payload only at an unmasked position.
pub(crate) struct MaskedSource<'py> {
    data: Data<'py>,
    state: State<'py>,
}

enum Data<'py> {
    Value {
        source: Box<NumpySource<'py>>,
        mask: Reader<'py>,
    },
    Record(RecordSource<'py>),
}

impl<'py> MaskedSource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let py = value.py();
        let state = State::new(value, budget)?;
        let dtype = api.dtype.get(value)?;
        let data = if errors::attribute(&dtype, c"names")?.is_none() {
            let source = NumpySource::new(value, api, budget)?;
            let mask = Reader::new(&state.original, &source, api, budget)?;
            Data::Value {
                source: crate::input::boxed(py, source)?,
                mask,
            }
        } else {
            Data::Record(RecordSource::new_masked(value, api, budget, state.clone())?)
        };
        state.validate(budget)?;
        Ok(Self { data, state })
    }
}

impl<'py> PreparedSource<'py> for MaskedSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.state.owner
    }
    fn shape(&self) -> Shape {
        match &self.data {
            Data::Value { source, .. } => source.shape(),
            Data::Record(source) => source.shape(),
        }
    }
    fn child_shape(&self, index: usize) -> PyResult<Shape> {
        match &self.data {
            Data::Value { source, .. } => source.child_shape(index),
            Data::Record(source) => source.child_shape(index),
        }
    }
    fn python_child(
        &mut self,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.state.validate(budget)?;
        match &mut self.data {
            Data::Record(source) => source.python_child(index, budget),
            Data::Value { source, mask }
                if matches!(source.shape(), Shape::Vector { .. }) && source.is_object() =>
            {
                let missing = mask.missing(index, 0, budget)?;
                self.state.validate(budget)?;
                if missing {
                    source.validate_layout(budget)?;
                    Ok(None)
                } else {
                    source.object_child(index, 0, budget)
                }
            }
            _ => Ok(None),
        }
    }
    fn consume_child(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.state.validate(budget)?;
        match &mut self.data {
            Data::Record(source) => source.consume_child(index, row, column, target, budget),
            Data::Value { source, .. } => match source.shape() {
                Shape::Vector { .. } => self.consume_cell_to(index, 0, target, budget),
                Shape::Table { .. } => self.consume_cell_to(index, row, target, budget),
                _ => Err(errors::builtin_message::<PyTypeError>(
                    self.owner().py(),
                    "masked source child requires a source axis",
                )),
            },
        }
    }
    fn axis_length(&self, axis: usize) -> Option<usize> {
        match &self.data {
            Data::Value { source, .. } => source.axis_length(axis),
            Data::Record(source) => source.axis_length(axis),
        }
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.data {
            Data::Value { .. } => Ok(None),
            Data::Record(source) => source.field_name(column),
        }
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        match &self.data {
            Data::Value { source, .. } => source.schema(target),
            Data::Record(source) => source.schema(target),
        }
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
        match &mut self.data {
            Data::Record(source) => {
                self.state.validate(budget)?;
                source.consume_cell_to(row, column, target, budget)
            }
            Data::Value { source, mask } => {
                let missing = mask.checked_missing(&self.state, row, column, budget)?;
                if missing {
                    source.validate_layout(budget)?;
                    target.scalar(self.state.owner.py(), ScalarRef::Null)
                } else {
                    source.consume_cell_to(row, column, target, budget)
                }
            }
        }
    }
}
