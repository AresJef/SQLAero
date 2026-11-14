//! Canonical record schemas and native fields retain their original owners.

use super::mask::{Reader, State};
use super::{NumpyArrayApi, NumpySource};
use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::cell::Target;
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use pyo3::exceptions::{PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

/// A flat arena owns source metadata, not values. Schema preparation, admission
/// and destruction never recurse on an untrusted dtype hierarchy. Output depth
/// still belongs to the actual target containers, including empty sources.
pub(crate) struct RecordSource<'py> {
    nodes: Vec<Node<'py>>,
    mask_state: Option<State<'py>>,
}

enum Node<'py> {
    Value(Box<Leaf<'py>>),
    Record(Box<Record<'py>>),
}

struct Leaf<'py> {
    data: NumpySource<'py>,
    mask: Option<Reader<'py>>,
}

struct Record<'py> {
    owner: Bound<'py, PyAny>,
    fields: Bound<'py, PyTuple>,
    dtype: Bound<'py, PyAny>,
    dtype_get: GetSet,
    layout: NumpySource<'py>,
    // Used only during iterative preparation, then released.
    view: Option<Bound<'py, PyAny>>,
    mask_view: Option<Bound<'py, PyAny>>,
    scalar_mask: bool,
    values: Vec<usize>,
}

impl<'py> Node<'py> {
    fn storage(&self) -> &NumpySource<'py> {
        match self {
            Self::Value(value) => &value.data,
            Self::Record(record) => &record.layout,
        }
    }

    fn owner(&self) -> &Bound<'py, PyAny> {
        match self {
            Self::Value(value) => value.data.owner(),
            Self::Record(record) => &record.owner,
        }
    }
}

impl<'py> Record<'py> {
    fn prepare(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        is_void: bool,
        mask: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Option<Self>> {
        let py = value.py();
        if mask.is_none() && crate::input::actual_instance(value, api.masked.bind(py)) {
            return Ok(None);
        }
        let dtype_get = if is_void {
            &api.record_dtype
        } else {
            &api.dtype
        };
        let dtype = dtype_get.get(value)?;
        let fields = match errors::attribute(&dtype, c"names")?.cast_into::<PyTuple>() {
            Ok(fields) => fields,
            Err(_) => return Ok(None),
        };
        let view = crate::input::control::callback(py, Some(budget), || {
            if is_void {
                api.record_array.bind(py).call1((value,))
            } else {
                // Exact native output type avoids child __array_finalize__.
                api.array_view.bind(py).call1((value, api.array.typ(py)))
            }
        })?;
        if !view.get_type().is(api.array.typ(py)) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "canonical record view must retain exact ndarray storage",
            ));
        }
        let layout = NumpySource::record_layout(if is_void { &view } else { value }, api, budget)?;
        let scalar_mask = mask
            .map(|mask| Reader::scalar(mask, api))
            .transpose()?
            .flatten()
            .is_some();
        let mask_view = if let Some(mask) = mask {
            if scalar_mask {
                Some(mask.clone())
            } else {
                if !api.accepts(mask) {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "structured mask requires actual bool fields with matching data axes",
                    ));
                }
                let mask_layout = NumpySource::record_layout(mask, api, budget)?;
                if layout.shape() != mask_layout.shape()
                    || (0..rank(layout.shape()))
                        .any(|axis| layout.axis_length(axis) != mask_layout.axis_length(axis))
                {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "structured mask axes must match their data axes",
                    ));
                }
                let mask_dtype = api.dtype.get(mask)?;
                let mask_names = errors::attribute(&mask_dtype, c"names")?
                    .cast_into::<PyTuple>()
                    .map_err(|_| {
                        errors::builtin_message::<PyTypeError>(
                            py,
                            "structured mask field names must match the data fields",
                        )
                    })?;
                if fields.len() != mask_names.len() {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "structured mask field names must match the data fields",
                    ));
                }
                for (left, right) in fields.iter().zip(mask_names.iter()) {
                    if !crate::input::unicode::same(
                        left.cast::<PyString>()?,
                        right.cast::<PyString>()?,
                        budget,
                    )? {
                        return Err(errors::builtin_message::<PyTypeError>(
                            py,
                            "structured mask field names must match the data fields",
                        ));
                    }
                }
                Some(crate::input::control::callback(py, Some(budget), || {
                    api.array_view.bind(py).call1((mask, api.array.typ(py)))
                })?)
            }
        } else {
            None
        };
        let mut values = Vec::new();
        values
            .try_reserve_exact(fields.len())
            .map_err(|_| errors::memory_error(py))?;
        Ok(Some(Self {
            owner: value.clone(),
            fields,
            dtype,
            dtype_get: dtype_get.clone_ref(py),
            layout,
            view: Some(view),
            mask_view,
            scalar_mask,
            values,
        }))
    }
}

fn rank(shape: Shape) -> usize {
    match shape {
        Shape::Scalar | Shape::Record { .. } => 0,
        Shape::Vector { .. } => 1,
        Shape::Table { .. } => 2,
        Shape::Tensor { dimensions, .. } => dimensions,
    }
}

fn elements(shape: Shape) -> usize {
    match shape {
        Shape::Scalar | Shape::Record { .. } => 1,
        Shape::Vector { length } => length,
        // Prepared native layouts already proved this product against nbytes.
        Shape::Table { rows, columns } => rows.saturating_mul(columns),
        Shape::Tensor { elements, .. } => elements,
    }
}

fn coordinate(shape: Shape, index: usize) -> (usize, usize) {
    match shape {
        Shape::Table { columns, .. } if columns != 0 => (index / columns, index % columns),
        _ => (index, 0),
    }
}

impl<'py> RecordSource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        is_void: bool,
    ) -> PyResult<Option<Self>> {
        Self::prepare(value, api, budget, is_void, None)
    }

    pub(super) fn new_masked(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        state: State<'py>,
    ) -> PyResult<Self> {
        Self::prepare(value, api, budget, false, Some(state))?.ok_or_else(|| {
            errors::builtin_message::<PyTypeError>(
                value.py(),
                "MaskedArray has no structured field schema",
            )
        })
    }

    fn prepare(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        is_void: bool,
        mask_state: Option<State<'py>>,
    ) -> PyResult<Option<Self>> {
        let py = value.py();
        let Some(root) = Record::prepare(
            value,
            api,
            budget,
            is_void,
            mask_state.as_ref().map(|state| &state.original),
        )?
        else {
            return Ok(None);
        };
        let mut nodes = Vec::new();
        nodes.try_reserve(1).map_err(|_| errors::memory_error(py))?;
        nodes.push(Node::Record(crate::input::boxed(py, root)?));
        let mut index = 0;
        while index < nodes.len() {
            crate::input::check(py, budget)?;
            let (view, fields, mask_view, scalar_mask) = match &mut nodes[index] {
                Node::Value(_) => {
                    index += 1;
                    continue;
                }
                Node::Record(record) => {
                    let view = record.view.take().ok_or_else(|| {
                        errors::builtin_message::<PyTypeError>(
                            py,
                            "record source preparation view is missing",
                        )
                    })?;
                    (
                        view,
                        record.fields.clone(),
                        record.mask_view.take(),
                        record.scalar_mask,
                    )
                }
            };
            for field in fields.iter() {
                crate::input::check(py, budget)?;
                if !field.is_exact_instance_of::<PyString>() {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "NumPy record field names must be strings",
                    ));
                }
                let current = crate::input::control::callback(py, Some(budget), || unsafe {
                    // SAFETY: retained view is an exact qualified ndarray.
                    // The public mapping slot returns an owning field view.
                    Bound::from_owned_ptr_or_err(py, (api.item)(view.as_ptr(), field.as_ptr()))
                })?;
                let field_mask = if let Some(mask_view) = &mask_view {
                    if scalar_mask {
                        Some(mask_view.clone())
                    } else {
                        Some(crate::input::control::callback(
                            py,
                            Some(budget),
                            || unsafe {
                                // SAFETY: qualified immutable ndarray slot owns its
                                // returned field view, including complete names.
                                Bound::from_owned_ptr_or_err(
                                    py,
                                    (api.item)(mask_view.as_ptr(), field.as_ptr()),
                                )
                            },
                        )?)
                    }
                } else {
                    None
                };
                let child = if let Some(record) =
                    Record::prepare(&current, api, budget, false, field_mask.as_ref())?
                {
                    Node::Record(crate::input::boxed(py, record)?)
                } else {
                    let data = NumpySource::new(&current, api, budget)?;
                    let mask = field_mask
                        .as_ref()
                        .map(|mask| Reader::new(mask, &data, api, budget))
                        .transpose()?;
                    Node::Value(crate::input::boxed(py, Leaf { data, mask })?)
                };
                nodes.try_reserve(1).map_err(|_| errors::memory_error(py))?;
                let child_index = nodes.len();
                nodes.push(child);
                if let Node::Record(record) = &mut nodes[index] {
                    // Capacity was reserved from the actual immutable schema.
                    record.values.push(child_index);
                }
            }
            index += 1;
        }
        if let Some(state) = &mask_state {
            state.validate(budget)?;
        }
        Ok(Some(Self { nodes, mask_state }))
    }

    fn shape_at(&self, node: usize, prefix: usize) -> PyResult<Shape> {
        let current = &self.nodes[node];
        let storage = current.storage();
        let dimensions = rank(storage.shape()).checked_sub(prefix).ok_or_else(|| {
            errors::builtin_message::<PyTypeError>(
                current.owner().py(),
                "record field axes do not contain their parent record axes",
            )
        })?;
        let axis = |n| storage.axis_length(prefix + n).unwrap_or(0);
        Ok(match dimensions {
            0 => match current {
                Node::Record(record) => Shape::Record {
                    fields: record.fields.len(),
                },
                Node::Value(_) => Shape::Scalar,
            },
            1 => Shape::Vector { length: axis(0) },
            2 if matches!(current, Node::Value(_)) => Shape::Table {
                rows: axis(0),
                columns: axis(1),
            },
            _ => {
                let mut count = Some(1usize);
                let mut empty = false;
                for current in 0..dimensions {
                    empty |= axis(current) == 0;
                    count = count.and_then(|n| n.checked_mul(axis(current)));
                }
                Shape::Tensor {
                    dimensions,
                    elements: if empty {
                        0
                    } else {
                        count.ok_or_else(|| errors::memory_error(storage.owner().py()))?
                    },
                }
            }
        })
    }

    fn field_name_at(&self, node: usize, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.nodes[node] {
            Node::Record(record) => record.fields.get_item(column).map(Some),
            Node::Value(_) => Ok(None),
        }
    }

    fn consume_projection(
        &mut self,
        node: usize,
        prefix: usize,
        index: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let shape = self.shape_at(node, prefix)?;
        let start = index
            .checked_mul(elements(shape))
            .ok_or_else(|| errors::memory_error(self.nodes[node].owner().py()))?;
        let mut projection = Projection {
            source: self,
            node,
            prefix,
            start,
            shape,
        };
        target.source(&mut projection, budget)
    }

    fn consume_record(
        &mut self,
        node: usize,
        index: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let prefix = rank(self.nodes[node].storage().shape());
        self.consume_projection(node, prefix, index, target, budget)
    }

    fn consume_field(
        &mut self,
        node: usize,
        index: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.nodes[node].owner().py();
        let (child, prefix) = match &self.nodes[node] {
            Node::Record(record) => {
                let child = *record.values.get(column).ok_or_else(|| {
                    errors::builtin_message::<PyIndexError>(
                        py,
                        "NumPy record coordinate is outside its schema",
                    )
                })?;
                if !record.dtype_get.get(&record.owner)?.is(&record.dtype) {
                    return Err(super::allocation::bounds_error(py));
                }
                let (row, cell) = coordinate(record.layout.shape(), index);
                record.layout.validate_record_cell(row, cell, budget)?;
                (child, rank(record.layout.shape()))
            }
            Node::Value(_) => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "record projection has no field source",
                ));
            }
        };
        self.consume_projection(child, prefix, index, target, budget)
    }

    fn consume_node(
        &mut self,
        node: usize,
        index: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        match &mut self.nodes[node] {
            Node::Value(value) => {
                let (row, column) = coordinate(value.data.shape(), index);
                if let Some(state) = &self.mask_state {
                    state.validate(budget)?;
                }
                let missing = value
                    .mask
                    .as_mut()
                    .map(|mask| mask.missing(row, column, budget))
                    .transpose()?
                    .unwrap_or(false);
                if let Some(state) = &self.mask_state {
                    state.validate(budget)?;
                }
                if missing {
                    value.data.validate_layout(budget)?;
                    target.scalar(
                        value.data.owner().py(),
                        sqlaero_transcode::value::ScalarRef::Null,
                    )
                } else {
                    value.data.consume_cell_to(row, column, target, budget)
                }
            }
            Node::Record(_) => self.consume_record(node, index, target, budget),
        }
    }
}

impl<'py> PreparedSource<'py> for RecordSource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.mask_state
            .as_ref()
            .map_or_else(|| self.nodes[0].owner(), |state| &state.owner)
    }

    fn shape(&self) -> Shape {
        match &self.nodes[0] {
            Node::Record(record) => match record.layout.shape() {
                Shape::Scalar => Shape::Record {
                    fields: record.fields.len(),
                },
                Shape::Table { rows, columns } => Shape::Tensor {
                    dimensions: 2,
                    elements: rows.saturating_mul(columns),
                },
                shape => shape,
            },
            Node::Value(value) => value.data.shape(),
        }
    }

    fn axis_length(&self, axis: usize) -> Option<usize> {
        self.nodes[0].storage().axis_length(axis)
    }

    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.field_name_at(0, column)
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        for node in &self.nodes {
            target.checkpoint(self.owner().py(), 1)?;
            if let Node::Value(value) = node {
                value.data.schema(target)?;
            }
        }
        Ok(())
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

impl RecordSource<'_> {
    crate::input::cell::destinations!();
    pub(super) fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        target.checkpoint(self.owner().py(), 1)?;
        match self.nodes[0].storage().shape() {
            Shape::Scalar => self.consume_field(0, 0, column, target, budget),
            _ => self.consume_record(0, row, target, budget),
        }
    }
}

/// A borrowed projection owns no recursive schema, exported pointer or Python
/// value tree. The root already admitted the complete immutable source schema.
struct Projection<'a, 'py> {
    source: &'a mut RecordSource<'py>,
    node: usize,
    prefix: usize,
    start: usize,
    shape: Shape,
}

impl<'py> PreparedSource<'py> for Projection<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.source.nodes[self.node].owner()
    }
    fn shape(&self) -> Shape {
        self.shape
    }
    fn axis_length(&self, axis: usize) -> Option<usize> {
        self.source.nodes[self.node]
            .storage()
            .axis_length(self.prefix + axis)
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.source.field_name_at(self.node, column)
    }
    fn schema(&self, _target: &mut dyn Target) -> PyResult<()> {
        // Complete admission belongs to the root, including empty/binary fields.
        Ok(())
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

impl Projection<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if matches!(self.shape, Shape::Record { .. }) {
            return self
                .source
                .consume_field(self.node, self.start, column, target, budget);
        }
        let local = match self.shape {
            Shape::Table { columns, .. } => {
                row.checked_mul(columns).and_then(|n| n.checked_add(column))
            }
            _ => Some(row),
        };
        let index = local
            .and_then(|n| self.start.checked_add(n))
            .ok_or_else(|| errors::memory_error(self.owner().py()))?;
        self.source.consume_node(self.node, index, target, budget)
    }
}
