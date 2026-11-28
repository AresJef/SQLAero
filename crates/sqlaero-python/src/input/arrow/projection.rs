//! Borrowed native compound cells. A projection retains the original owner
//! and array; it never builds Python rows or a normalized Rust value tree.

use super::schema::View as TypeView;
use super::{
    ArrayRef, Int64Array, Int64Views, Read, StructArray, consume_scalar, dictionary_label, invalid,
    names,
};
use crate::input::cell::Target;
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use pyo3::prelude::*;

enum Kind<'a> {
    Scalar,
    Record(&'a StructArray),
    Slice {
        values: &'a ArrayRef,
        start: usize,
        length: usize,
    },
    Int64Slice {
        values: &'a Int64Array,
        start: usize,
        length: usize,
    },
}

impl<'a> Kind<'a> {
    fn shape(&self) -> Shape {
        match self {
            Self::Scalar => Shape::Scalar,
            Self::Record(record) => Shape::Record {
                fields: record.num_columns(),
            },
            Self::Slice { length, .. } | Self::Int64Slice { length, .. } => {
                Shape::Vector { length: *length }
            }
        }
    }

    fn prepare_leaf(self) -> Self {
        match self {
            Self::Slice {
                values,
                start,
                length,
            } => {
                // The parent already validated its range. Borrow one typed
                // child from the same owned array for the complete list.
                if length > 1
                    && let ArrayRef::Leaf(values) = values
                    && let Some(values) = values.as_any().downcast_ref::<Int64Array>()
                {
                    Self::Int64Slice {
                        values,
                        start,
                        length,
                    }
                } else {
                    Self::Slice {
                        values,
                        start,
                        length,
                    }
                }
            }
            other => other,
        }
    }
}

fn resolve<'a>(
    py: Python<'_>,
    mut array: &'a ArrayRef,
    mut index: usize,
    budget: &dyn Checkpoint,
) -> PyResult<(&'a ArrayRef, usize)> {
    loop {
        if index >= array.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        // Only this native node contains indirections. Scalar and compound
        // consumers retain their own NULL handling after the index check.
        if !matches!(array, ArrayRef::Dictionary(_)) || array.is_null(index) {
            return Ok((array, index));
        }
        // Ordinary scalar work is already charged by the target. Transparent
        // dictionary chains additionally accumulate bounded metadata work.
        budget.bytes(py, 8)?;
        (array, index) = dictionary_label(py, array, index)?;
    }
}

pub(super) fn scalar_storage(array: &ArrayRef) -> bool {
    matches!(
        array,
        ArrayRef::Leaf(_) | ArrayRef::Bytes(_) | ArrayRef::View(_)
    )
}

fn kind<'a>(py: Python<'_>, array: &'a ArrayRef, index: usize) -> PyResult<Kind<'a>> {
    // Every physical leaf has scalar shape, including NULL. Its consumer
    // still reads the actual value type and null bit; no schema is bypassed.
    if scalar_storage(array) {
        return Ok(Kind::Scalar);
    }
    if array.is_null(index) {
        return Ok(Kind::Scalar);
    }
    match array.data_type() {
        TypeView::Struct(_) => array
            .as_any()
            .downcast_ref::<StructArray>()
            .map(Kind::Record)
            .ok_or_else(|| invalid(py, "Arrow Struct physical type differs")),
        TypeView::List(_) | TypeView::LargeList(_) | TypeView::FixedSizeList(_, _) => {
            let ArrayRef::Sequence(values) = array else {
                return Err(invalid(py, "Arrow list physical type differs"));
            };
            let (start, end) = values
                .range(index)
                .ok_or_else(|| invalid(py, "Arrow list offsets are outside the values array"))?;
            // The existing range reader checked both offset order and the
            // upper bound against this same retained child.
            Ok(Kind::Slice {
                values: values.values(),
                start,
                length: end - start,
            })
        }
        _ => Ok(Kind::Scalar),
    }
}

pub(super) fn shape(
    owner: &Bound<'_, PyAny>,
    array: &ArrayRef,
    index: usize,
    budget: &dyn Checkpoint,
) -> PyResult<Shape> {
    let (array, index) = resolve(owner.py(), array, index, budget)?;
    Ok(kind(owner.py(), array, index)?.shape())
}

pub(super) fn consume<'py>(
    mut read: Read<'_, 'py>,
    array: &ArrayRef,
    index: usize,
    target: &mut (impl Target + ?Sized),
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let (resolved, index) = resolve(read.owner.py(), array, index, budget)?;
    let selected = kind(read.owner.py(), resolved, index)?;
    if matches!(selected, Kind::Scalar) {
        return consume_scalar(
            read.owner.py(),
            resolved,
            index,
            target,
            budget,
            read.offsets,
        );
    }
    if matches!(array, ArrayRef::Dictionary(_)) && target.needs_field_names() {
        read.dictionary(array.data_type(), true, budget)?;
    }
    target.source(
        &mut View {
            read,
            index,
            selected: selected.prepare_leaf(),
        },
        budget,
    )
}

pub(super) fn consume_child<'py>(
    mut read: Read<'_, 'py>,
    array: &ArrayRef,
    index: usize,
    row: usize,
    column: usize,
    target: &mut (impl Target + ?Sized),
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let (resolved, index) = resolve(read.owner.py(), array, index, budget)?;
    let selected = kind(read.owner.py(), resolved, index)?;
    if matches!(selected, Kind::Scalar) {
        if row != 0 || column != 0 {
            return Err(invalid(
                read.owner.py(),
                "Arrow selected scalar is outside its actual row shape",
            ));
        }
        return consume_scalar(
            read.owner.py(),
            resolved,
            index,
            target,
            budget,
            read.offsets,
        );
    }
    if matches!(array, ArrayRef::Dictionary(_)) && target.needs_field_names() {
        read.dictionary(array.data_type(), true, budget)?;
    }
    consume_selected_child(read, &selected, index, row, column, target, budget)
}

/// Consume a child using the current owner's already validated parent shape.
/// Child indices, validity, schema positions and values are still checked at
/// their original boundaries; no formatted answers or foreign owners are cached.
fn consume_selected_child<'py>(
    mut read: Read<'_, 'py>,
    selected: &Kind<'_>,
    index: usize,
    row: usize,
    column: usize,
    target: &mut (impl Target + ?Sized),
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    match selected {
        Kind::Int64Slice {
            values,
            start,
            length,
        } if row < *length && column == 0 => {
            let py = read.owner.py();
            target.checkpoint(py, 1)?;
            Int64Views::write_value(py, values, *start + row, target)
        }
        Kind::Record(record) if row == 0 && column < record.num_columns() => {
            let value = record.column(column);
            // Imported fields share the parent's row window. Physical scalar
            // storage needs no child projection or nested field names.
            if scalar_storage(value) {
                return consume_scalar(read.owner.py(), value, index, target, budget, read.offsets);
            }
            let node = if target.needs_field_names() {
                read.child(names::Edge::Field(column), value.data_type(), true, budget)?
            } else {
                0
            };
            consume(read.at(node), value, index, target, budget)
        }
        Kind::Slice {
            values,
            start,
            length,
        } if row < *length && column == 0 => {
            // The retained physical leaf already has scalar shape. Its normal
            // consumer still checks the checkpoint, index, NULL and value type;
            // dictionaries and compounds keep the recursive resolution below.
            if scalar_storage(values) {
                return consume_scalar(
                    read.owner.py(),
                    values,
                    *start + row,
                    target,
                    budget,
                    read.offsets,
                );
            }
            let node = if target.needs_field_names() {
                read.child(names::Edge::Value, values.data_type(), true, budget)?
            } else {
                0
            };
            consume(read.at(node), values, *start + row, target, budget)
        }
        _ => Err(invalid(
            read.owner.py(),
            "Arrow selected cell is outside its actual row shape",
        )),
    }
}

struct View<'a, 'py> {
    read: Read<'a, 'py>,
    index: usize,
    selected: Kind<'a>,
}

impl<'py> PreparedSource<'py> for View<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.read.owner
    }
    fn shape(&self) -> Shape {
        self.selected.shape()
    }
    fn schema(&self, _target: &mut dyn Target) -> PyResult<()> {
        Ok(())
    }
    fn prepare_field_names(&mut self, budget: &dyn Checkpoint) -> PyResult<bool> {
        let Kind::Record(record) = self.selected else {
            return Ok(false);
        };
        self.read
            .names
            .prepare_fields(self.read.node, record.num_columns(), budget)?;
        Ok(true)
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.read.names.field(self.read.node, column)
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_selected_to(row, column, target, budget)
    }
    crate::input::source::cell_destinations!();
}

impl View<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut impl Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        // A list has one element schema. Keep its destination concrete, while
        // heterogeneous records retain the shared dynamic consumer to bound
        // the generated scalar paths for every record field and destination.
        if matches!(self.selected, Kind::Slice { .. } | Kind::Int64Slice { .. }) {
            self.consume_selected_to(row, column, target, budget)
        } else {
            self.consume_cell(row, column, target, budget)
        }
    }

    fn consume_selected_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let node = self.read.node;
        consume_selected_child(
            self.read.at(node),
            &self.selected,
            self.index,
            row,
            column,
            target,
            budget,
        )
    }
}
