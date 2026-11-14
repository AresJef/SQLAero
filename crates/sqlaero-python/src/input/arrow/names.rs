//! Complete provider names, separate from NUL-terminated C Data names and from
//! JSON key policy. A stable schema is prepared lazily within this one call.

use super::invalid;
use super::schema::View;
use crate::input::control::{self, Checkpoint};
use crate::input::source::Shape;
use crate::{errors, input};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};

/// Only a Struct needs field names. List/Dictionary have one schema child, so
/// this bounded walk never scans a wide record or any value buffer.
pub(super) fn needed(dtype: View<'_>) -> bool {
    match dtype {
        View::Struct(_) => true,
        View::List(child) | View::LargeList(child) | View::FixedSizeList(child, _) => {
            needed(child.data_type())
        }
        View::Dictionary(_, value) => needed(value.data_type()),
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub(super) enum Edge {
    Field(usize),
    Value,
}

pub(crate) enum Source<'py> {
    ArrowArray(Bound<'py, PyAny>),
    ArrowTable(Bound<'py, PyAny>),
    ArrowType(Bound<'py, PyAny>),
    PolarsArray(Bound<'py, PyAny>),
    PolarsTable {
        owner: Bound<'py, PyAny>,
        dtypes: Bound<'py, PyAny>,
    },
    Ready(Box<Schema<'py>>),
}

pub(crate) struct Schema<'py> {
    _owner: Bound<'py, PyAny>,
    nodes: Vec<Node<'py>>,
    polars: bool,
    table: Option<Bound<'py, PyAny>>,
}

struct Node<'py> {
    dtype: Bound<'py, PyAny>,
    fields: Option<Fields<'py>>,
    value: Option<usize>,
}
struct Fields<'py> {
    getter: Option<Bound<'py, PyAny>>,
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
    pub(crate) fn arrow(value: &Bound<'py, PyAny>, shape: Shape) -> Self {
        if matches!(shape, Shape::Table { .. }) {
            Self::ArrowTable(value.clone())
        } else {
            Self::ArrowArray(value.clone())
        }
    }

    pub(crate) fn polars(dtype: Bound<'py, PyAny>, table: Option<Bound<'py, PyAny>>) -> Self {
        if let Some(owner) = table {
            Self::PolarsTable {
                owner,
                dtypes: dtype,
            }
        } else {
            Self::PolarsArray(dtype)
        }
    }

    fn schema(&mut self, budget: &dyn Checkpoint) -> PyResult<&mut Schema<'py>> {
        if !matches!(self, Self::Ready(_)) {
            let (owner, attribute, polars, table) = match self {
                Self::ArrowArray(value) => (value.clone(), Some(c"type"), false, None),
                Self::ArrowTable(value) => (value.clone(), Some(c"schema"), false, None),
                Self::ArrowType(dtype) => (dtype.clone(), None, false, None),
                Self::PolarsArray(dtype) => (dtype.clone(), None, true, None),
                Self::PolarsTable { owner, dtypes } => {
                    (dtypes.clone(), None, true, Some(owner.clone()))
                }
                Self::Ready(_) => unreachable!(),
            };
            let py = owner.py();
            let dtype = if let Some(attribute) = attribute {
                control::callback(py, Some(budget), || errors::attribute(&owner, attribute))?
            } else {
                owner.clone()
            };
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
                    polars,
                    table,
                },
            )?);
        }
        let Self::Ready(schema) = self else {
            unreachable!()
        };
        Ok(schema)
    }

    /// Recover complete logical timezone metadata, which the C ABI's NUL
    /// terminator cannot represent. This path does not prepare JSON names or
    /// allocate a wide record's name slots. Only actual timestamp nodes use it.
    pub(super) fn timezone(
        &mut self,
        path: &[Edge],
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyString>>> {
        let schema = self.schema(budget)?;
        let mut dtype = schema.nodes[0].dtype.clone();
        let py = dtype.py();
        for (depth, edge) in path.iter().enumerate() {
            budget.bytes(py, 8)?;
            dtype = match edge {
                Edge::Field(index) if schema.polars && depth == 0 && schema.table.is_some() => {
                    dtype.cast::<PyList>()?.get_item(*index)?
                }
                Edge::Field(index) if schema.polars => {
                    let fields = control::callback(py, Some(budget), || {
                        errors::attribute(&dtype, c"fields")
                    })?;
                    let field = fields.cast::<PyList>()?.get_item(*index)?;
                    control::callback(py, Some(budget), || errors::attribute(&field, c"dtype"))?
                }
                Edge::Field(index) => control::callback(py, Some(budget), || {
                    let field = dtype.call_method1(c"field", (*index,))?;
                    errors::attribute(&field, c"type")
                })?,
                Edge::Value => control::callback(py, Some(budget), || {
                    errors::attribute(
                        &dtype,
                        if schema.polars {
                            c"inner"
                        } else {
                            c"value_type"
                        },
                    )
                })?,
            };
        }
        let value = control::callback(py, Some(budget), || {
            errors::attribute(&dtype, if schema.polars { c"time_zone" } else { c"tz" })
        })?;
        if value.is_none() {
            Ok(None)
        } else {
            Ok(Some(value.cast_into::<PyString>()?))
        }
    }

    pub(crate) fn prepare_fields(
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

    pub(crate) fn field(&self, node: usize, index: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
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

    /// Reuse an already acquired canonical column header. It contains source
    /// metadata only, not a row/value conversion or a target-formatted key.
    pub(crate) fn bind_polars_column(
        &mut self,
        index: usize,
        source: Bound<'py, PyAny>,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let schema = self.schema(budget)?;
        schema.fields(0, budget)?;
        let py = source.py();
        let field = schema.nodes[0]
            .fields
            .as_mut()
            .and_then(|fields| fields.slots.get_mut(index))
            .ok_or_else(|| invalid(py, "Polars name column is outside its schema"))?;
        field.source = Some(source);
        Ok(())
    }

    pub(crate) fn acquire_field(
        &mut self,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.schema(budget)?.acquire_name(0, index, budget)
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
        let polars_fields = if self.polars && !(node == 0 && self.table.is_some()) {
            Some(
                control::callback(py, Some(budget), || errors::attribute(&dtype, c"fields"))?
                    .cast_into::<PyList>()?,
            )
        } else {
            None
        };
        let count = if let Some(fields) = &polars_fields {
            fields.len()
        } else {
            control::callback(py, Some(budget), || dtype.len())?
        };
        let getter = if let Some(table) = self.table.as_ref().filter(|_| node == 0) {
            Some(control::callback(py, Some(budget), || {
                errors::attribute(table, c"to_series")
            })?)
        } else if self.polars {
            None
        } else {
            Some(control::callback(py, Some(budget), || {
                errors::attribute(&dtype, c"field")
            })?)
        };
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| errors::memory_error(py))?;
        for index in 0..count {
            if index.is_multiple_of(64) {
                input::check(py, budget)?;
            }
            slots.push(Field {
                source: polars_fields
                    .as_ref()
                    .map(|fields| fields.get_item(index))
                    .transpose()?,
                ..Field::default()
            });
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
        let getter = fields
            .getter
            .as_ref()
            .ok_or_else(|| invalid(py, "native field source getter is missing"))?;
        let value = control::callback(py, Some(budget), || getter.call1(arguments))?;
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
            self.acquire_name(node, index, budget)?;
        }
        self.nodes[node]
            .fields
            .as_mut()
            .ok_or_else(|| invalid(py, "Arrow name fields disappeared"))?
            .names_ready = true;
        Ok(())
    }

    fn acquire_name(
        &mut self,
        node: usize,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.fields(node, budget)?;
        let py = self._owner.py();
        if let Some(name) = self.nodes[node]
            .fields
            .as_ref()
            .and_then(|fields| fields.slots.get(index))
            .and_then(|field| field.name.clone())
        {
            return Ok(name);
        }
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
            .name = Some(name.clone());
        Ok(name)
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
        let dtype = if self.polars && node == 0 && self.table.is_some() {
            self.nodes[0].dtype.cast::<PyList>()?.get_item(match edge {
                Edge::Field(index) => index,
                Edge::Value => {
                    return Err(invalid(py, "table schema has no anonymous value child"));
                }
            })?
        } else if let Some(field) = source {
            control::callback(py, Some(budget), || {
                errors::attribute(&field, if self.polars { c"dtype" } else { c"type" })
            })?
        } else {
            let parent = self.nodes[node].dtype.clone();
            control::callback(py, Some(budget), || {
                errors::attribute(&parent, if self.polars { c"inner" } else { c"value_type" })
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
