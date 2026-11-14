//! Complete provider names, separate from NUL-terminated C Data names and from
//! JSON key policy. A stable schema is prepared lazily within this one call.

use super::invalid;
use crate::input::control::{self, Checkpoint};
use crate::input::source::Shape;
use crate::{errors, input};
use arrow_schema::DataType;
use pyo3::prelude::*;
use pyo3::types::PyString;

/// Only a Struct needs field names. List/Dictionary have one schema child, so
/// this bounded walk never scans a wide record or any value buffer.
pub(super) fn needed(dtype: &DataType) -> bool {
    match dtype {
        DataType::Struct(_) => true,
        DataType::List(child) | DataType::LargeList(child) | DataType::FixedSizeList(child, _) => {
            needed(child.data_type())
        }
        DataType::Dictionary(_, value) => needed(value),
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub(super) enum Edge {
    Field(usize),
    Value,
}

pub(super) enum Source<'py> {
    ArrowArray(Bound<'py, PyAny>),
    ArrowTable(Bound<'py, PyAny>),
    Ready(Box<Schema<'py>>),
}

pub(super) struct Schema<'py> {
    _owner: Bound<'py, PyAny>,
    nodes: Vec<Node<'py>>,
}

struct Node<'py> {
    dtype: Bound<'py, PyAny>,
    fields: Option<Fields<'py>>,
    value: Option<usize>,
}
struct Fields<'py> {
    getter: Bound<'py, PyAny>,
    slots: Vec<Field<'py>>,
    names_ready: bool,
}
#[derive(Default)]
struct Field<'py> {
    source: Option<Bound<'py, PyAny>>,
    name: Option<Bound<'py, PyAny>>,
    node: Option<usize>,
}

impl<'py> Source<'py> {
    pub(super) fn arrow(value: &Bound<'py, PyAny>, shape: Shape) -> Self {
        if matches!(shape, Shape::Table { .. }) {
            Self::ArrowTable(value.clone())
        } else {
            Self::ArrowArray(value.clone())
        }
    }

    fn schema(&mut self, budget: &dyn Checkpoint) -> PyResult<&mut Schema<'py>> {
        if !matches!(self, Self::Ready(_)) {
            let (owner, table) = match self {
                Self::ArrowArray(value) => (value.clone(), false),
                Self::ArrowTable(value) => (value.clone(), true),
                Self::Ready(_) => unreachable!(),
            };
            let py = owner.py();
            let dtype = control::callback(py, Some(budget), || {
                errors::attribute(&owner, if table { c"schema" } else { c"type" })
            })?;
            let mut nodes = Vec::new();
            nodes
                .try_reserve_exact(1)
                .map_err(|_| errors::memory_error(py))?;
            nodes.push(Node {
                dtype,
                fields: None,
                value: None,
            });
            *self = Self::Ready(input::boxed(
                py,
                Schema {
                    _owner: owner,
                    nodes,
                },
            )?);
        }
        let Self::Ready(schema) = self else {
            unreachable!()
        };
        Ok(schema)
    }

    pub(super) fn prepare_fields(
        &mut self,
        node: usize,
        count: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if count == 0 {
            return Ok(());
        }
        self.schema(budget)?.prepare_fields(node, count, budget)
    }

    pub(super) fn field(&self, node: usize, index: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        let Self::Ready(schema) = self else {
            return Ok(None);
        };
        Ok(schema
            .nodes
            .get(node)
            .and_then(|n| n.fields.as_ref())
            .and_then(|fields| fields.slots.get(index))
            .and_then(|field| field.name.clone()))
    }

    pub(super) fn child(
        &mut self,
        node: usize,
        edge: Edge,
        budget: &dyn Checkpoint,
    ) -> PyResult<usize> {
        self.schema(budget)?.child(node, edge, budget)
    }
}

impl<'py> Schema<'py> {
    fn fields(&mut self, node: usize, budget: &dyn Checkpoint) -> PyResult<()> {
        let current = self
            .nodes
            .get(node)
            .ok_or_else(|| invalid(self._owner.py(), "Arrow name schema node is missing"))?;
        if current.fields.is_some() {
            return Ok(());
        }
        let dtype = current.dtype.clone();
        let py = dtype.py();
        let count = control::callback(py, Some(budget), || dtype.len())?;
        let getter = control::callback(py, Some(budget), || errors::attribute(&dtype, c"field"))?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| errors::memory_error(py))?;
        for index in 0..count {
            if index.is_multiple_of(64) {
                input::check(py, budget)?;
            }
            slots.push(Field::default());
        }
        self.nodes[node].fields = Some(Fields {
            getter,
            slots,
            names_ready: false,
        });
        Ok(())
    }

    fn source_field(
        &mut self,
        node: usize,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.fields(node, budget)?;
        let py = self._owner.py();
        let fields = self.nodes[node]
            .fields
            .as_mut()
            .ok_or_else(|| invalid(py, "Arrow name fields were not prepared"))?;
        let field = fields
            .slots
            .get_mut(index)
            .ok_or_else(|| invalid(py, "Arrow name field is outside its schema"))?;
        if let Some(value) = &field.source {
            return Ok(value.clone());
        }
        let index = input::python_index(py, index)?;
        let arguments = crate::output::tuple_from_ready(py, [index])?;
        let value = control::callback(py, Some(budget), || fields.getter.call1(arguments))?;
        field.source = Some(value.clone());
        Ok(value)
    }

    fn prepare_fields(
        &mut self,
        node: usize,
        count: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.fields(node, budget)?;
        let py = self._owner.py();
        let fields = self.nodes[node]
            .fields
            .as_ref()
            .ok_or_else(|| invalid(py, "Arrow name fields were not prepared"))?;
        if fields.slots.len() != count {
            return Err(invalid(
                py,
                "complete Arrow names do not match the native field count",
            ));
        }
        if fields.names_ready {
            return Ok(());
        }
        for index in 0..count {
            let field = self.source_field(node, index, budget)?;
            // Explicitly approved necessary source snapshot: this single public
            // getter internally copies/decodes one complete name atomically.
            // No private C++ layout or NUL-terminated substitute is consulted.
            let name = control::callback(py, Some(budget), || errors::attribute(&field, c"name"));
            let name = name.inspect_err(|error| {
                errors::note_format(
                    py,
                    error,
                    format_args!("while reading native schema field name at position {index}"),
                );
            })?;
            name.cast::<PyString>()?;
            self.nodes[node]
                .fields
                .as_mut()
                .ok_or_else(|| invalid(py, "Arrow name fields disappeared"))?
                .slots[index]
                .name = Some(name);
        }
        self.nodes[node]
            .fields
            .as_mut()
            .ok_or_else(|| invalid(py, "Arrow name fields disappeared"))?
            .names_ready = true;
        Ok(())
    }

    fn child(&mut self, node: usize, edge: Edge, budget: &dyn Checkpoint) -> PyResult<usize> {
        let py = self._owner.py();
        let (cached, source) = match edge {
            Edge::Field(index) => {
                let source = self.source_field(node, index, budget)?;
                let child = self.nodes[node]
                    .fields
                    .as_ref()
                    .and_then(|fields| fields.slots.get(index))
                    .and_then(|field| field.node);
                (child, Some(source))
            }
            Edge::Value => (
                self.nodes
                    .get(node)
                    .ok_or_else(|| invalid(py, "Arrow name schema node is missing"))?
                    .value,
                None,
            ),
        };
        if let Some(node) = cached {
            return Ok(node);
        }
        let dtype = if let Some(field) = source {
            control::callback(py, Some(budget), || errors::attribute(&field, c"type"))?
        } else {
            let parent = self.nodes[node].dtype.clone();
            control::callback(py, Some(budget), || {
                errors::attribute(&parent, c"value_type")
            })?
        };
        self.nodes
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        let next = self.nodes.len();
        self.nodes.push(Node {
            dtype,
            fields: None,
            value: None,
        });
        match edge {
            Edge::Field(index) => {
                self.nodes[node]
                    .fields
                    .as_mut()
                    .ok_or_else(|| invalid(py, "Arrow parent name fields disappeared"))?
                    .slots[index]
                    .node = Some(next)
            }
            Edge::Value => self.nodes[node].value = Some(next),
        }
        Ok(next)
    }
}
