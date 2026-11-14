//! Borrowed native compound cells. A projection retains the original owner
//! and array; it never builds Python rows or a normalized Rust value tree.

use super::{
    Array, ArrayRef, DataType, StructArray, consume_scalar, dictionary_label, invalid, offset,
};
use crate::input::cell::Target;
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use arrow_array::{FixedSizeListArray, LargeListArray, ListArray};
use pyo3::prelude::*;

enum Kind<'a> {
    Scalar,
    Record(&'a StructArray),
    Slice {
        values: &'a ArrayRef,
        start: usize,
        length: usize,
    },
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
        if array.is_null(index) || !matches!(array.data_type(), DataType::Dictionary(_, _)) {
            return Ok((array, index));
        }
        // Ordinary scalar work is already charged by the target. Transparent
        // dictionary chains additionally accumulate bounded metadata work.
        budget.bytes(py, 8)?;
        (array, index) = dictionary_label(py, array, index)?;
    }
}

fn kind<'a>(py: Python<'_>, array: &'a ArrayRef, index: usize) -> PyResult<Kind<'a>> {
    if array.is_null(index) {
        return Ok(Kind::Scalar);
    }
    let (values, start, end) = match array.data_type() {
        DataType::Struct(_) => {
            return array
                .as_any()
                .downcast_ref::<StructArray>()
                .map(Kind::Record)
                .ok_or_else(|| invalid(py, "Arrow Struct physical type differs"));
        }
        DataType::List(_) => {
            let values = array
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or_else(|| invalid(py, "Arrow List physical type differs"))?;
            let offsets = values.value_offsets();
            (
                values.values(),
                usize::try_from(offsets[index]).ok(),
                usize::try_from(offsets[index + 1]).ok(),
            )
        }
        DataType::LargeList(_) => {
            let values = array
                .as_any()
                .downcast_ref::<LargeListArray>()
                .ok_or_else(|| invalid(py, "Arrow LargeList physical type differs"))?;
            let offsets = values.value_offsets();
            (
                values.values(),
                usize::try_from(offsets[index]).ok(),
                usize::try_from(offsets[index + 1]).ok(),
            )
        }
        DataType::FixedSizeList(_, _) => {
            let values = array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .ok_or_else(|| invalid(py, "Arrow FixedSizeList physical type differs"))?;
            let width = usize::try_from(values.value_length())
                .map_err(|_| invalid(py, "Arrow FixedSizeList width is negative"))?;
            let start = index.checked_mul(width);
            (
                values.values(),
                start,
                start.and_then(|n| n.checked_add(width)),
            )
        }
        _ => return Ok(Kind::Scalar),
    };
    let (Some(start), Some(end)) = (start, end) else {
        return Err(invalid(
            py,
            "Arrow list offsets are negative or overflow usize",
        ));
    };
    if start > end || end > values.len() {
        return Err(invalid(
            py,
            "Arrow list offsets are outside the values array",
        ));
    }
    Ok(Kind::Slice {
        values,
        start,
        length: end - start,
    })
}

pub(super) fn shape(
    owner: &Bound<'_, PyAny>,
    array: &ArrayRef,
    index: usize,
    budget: &dyn Checkpoint,
) -> PyResult<Shape> {
    let (array, index) = resolve(owner.py(), array, index, budget)?;
    Ok(match kind(owner.py(), array, index)? {
        Kind::Scalar => Shape::Scalar,
        Kind::Record(record) => Shape::Record {
            fields: record.num_columns(),
        },
        Kind::Slice { length, .. } => Shape::Vector { length },
    })
}

pub(super) fn consume<'py>(
    owner: &Bound<'py, PyAny>,
    array: &ArrayRef,
    index: usize,
    target: &mut dyn Target,
    budget: &dyn Checkpoint,
    offsets: &mut Option<Box<offset::Offsets<'py>>>,
) -> PyResult<()> {
    let (array, index) = resolve(owner.py(), array, index, budget)?;
    let selected = kind(owner.py(), array, index)?;
    if matches!(selected, Kind::Scalar) {
        return consume_scalar(owner.py(), array, index, target, budget, offsets);
    }
    target.source(
        &mut View {
            owner,
            array,
            index,
            selected,
            offsets,
        },
        budget,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "consume one already selected native row, without an allocated source or value tree"
)]
pub(super) fn consume_child<'py>(
    owner: &Bound<'py, PyAny>,
    array: &ArrayRef,
    index: usize,
    row: usize,
    column: usize,
    target: &mut dyn Target,
    budget: &dyn Checkpoint,
    offsets: &mut Option<Box<offset::Offsets<'py>>>,
) -> PyResult<()> {
    let (array, index) = resolve(owner.py(), array, index, budget)?;
    match kind(owner.py(), array, index)? {
        Kind::Scalar if row == 0 && column == 0 => {
            consume_scalar(owner.py(), array, index, target, budget, offsets)
        }
        Kind::Record(record) if row == 0 && column < record.num_columns() => {
            consume(owner, record.column(column), index, target, budget, offsets)
        }
        Kind::Slice {
            values,
            start,
            length,
        } if row < length && column == 0 => {
            consume(owner, values, start + row, target, budget, offsets)
        }
        _ => Err(invalid(
            owner.py(),
            "Arrow selected cell is outside its actual row shape",
        )),
    }
}

struct View<'a, 'py> {
    owner: &'a Bound<'py, PyAny>,
    array: &'a ArrayRef,
    index: usize,
    selected: Kind<'a>,
    offsets: &'a mut Option<Box<offset::Offsets<'py>>>,
}

impl<'py> PreparedSource<'py> for View<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.owner
    }
    fn shape(&self) -> Shape {
        match self.selected {
            Kind::Scalar => Shape::Scalar,
            Kind::Record(record) => Shape::Record {
                fields: record.num_columns(),
            },
            Kind::Slice { length, .. } => Shape::Vector { length },
        }
    }
    fn schema(&self, _target: &mut dyn Target) -> PyResult<()> {
        Ok(())
    }
    fn field_name(&self, _column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        Err(invalid(
            self.owner.py(),
            "full Arrow field-name source migration is not connected yet",
        ))
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        consume_child(
            self.owner,
            self.array,
            self.index,
            row,
            column,
            target,
            budget,
            self.offsets,
        )
    }
    crate::input::source::cell_destinations!();
}

impl View<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell(row, column, target, budget)
    }
}
