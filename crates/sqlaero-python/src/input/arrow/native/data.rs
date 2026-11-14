//! Native structural owners. Leaves keep mature typed arrays; compound nodes
//! retain buffers and children without rebuilding them through recursive
//! ArrayData constructors or promising that unchecked dictionary keys are valid.

use arrow_array::{Array, ArrayRef as Leaf};
use arrow_buffer::{Buffer, NullBuffer, ScalarBuffer};
use arrow_schema::DataType;
use std::any::Any;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) enum ArrayRef {
    Leaf(Leaf),
    Record(Arc<Record>),
    Sequence(Arc<Sequence>),
    Dictionary(Arc<Dictionary>),
    Bytes(Arc<Bytes>),
    View(Arc<View>),
}

pub(crate) struct Record {
    pub(super) dtype: DataType,
    pub(super) len: usize,
    pub(super) nulls: Option<NullBuffer>,
    pub(super) columns: Vec<ArrayRef>,
}
pub(crate) struct Sequence {
    pub(super) dtype: DataType,
    pub(super) len: usize,
    pub(super) nulls: Option<NullBuffer>,
    pub(super) offsets: Offsets,
    pub(super) values: ArrayRef,
}
pub(super) enum Offsets {
    I32(ScalarBuffer<i32>),
    I64(ScalarBuffer<i64>),
    Fixed(usize),
}
pub(crate) struct Dictionary {
    pub(super) dtype: DataType,
    pub(super) keys: Leaf,
    pub(super) values: ArrayRef,
}
pub(crate) struct Bytes {
    pub(super) dtype: DataType,
    pub(super) len: usize,
    pub(super) nulls: Option<NullBuffer>,
    pub(super) offsets: Offsets,
    pub(super) values: Buffer,
}
pub(crate) struct View {
    pub(super) dtype: DataType,
    pub(super) nulls: Option<NullBuffer>,
    pub(super) views: ScalarBuffer<u128>,
    pub(super) buffers: Vec<Buffer>,
}

impl ArrayRef {
    pub(crate) fn data_type(&self) -> &DataType {
        match self {
            Self::Leaf(value) => value.data_type(),
            Self::Record(value) => &value.dtype,
            Self::Sequence(value) => &value.dtype,
            Self::Dictionary(value) => &value.dtype,
            Self::Bytes(value) => &value.dtype,
            Self::View(value) => &value.dtype,
        }
    }
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Leaf(value) => value.len(),
            Self::Record(value) => value.len,
            Self::Sequence(value) => value.len,
            Self::Dictionary(value) => value.keys.len(),
            Self::Bytes(value) => value.len,
            Self::View(value) => value.views.len(),
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn nulls(&self) -> Option<&NullBuffer> {
        match self {
            Self::Leaf(value) => value.nulls(),
            Self::Record(value) => value.nulls.as_ref(),
            Self::Sequence(value) => value.nulls.as_ref(),
            Self::Dictionary(value) => value.keys.nulls(),
            Self::Bytes(value) => value.nulls.as_ref(),
            Self::View(value) => value.nulls.as_ref(),
        }
    }
    pub(crate) fn is_null(&self, index: usize) -> bool {
        if let Self::Leaf(value) = self {
            return value.is_null(index);
        }
        self.nulls().is_some_and(|bits| bits.is_null(index))
    }
    pub(crate) fn null_count(&self) -> usize {
        if let Self::Leaf(value) = self {
            return value.null_count();
        }
        self.nulls().map_or(0, NullBuffer::null_count)
    }
    pub(crate) fn as_any(&self) -> &dyn Any {
        match self {
            Self::Leaf(value) => value.as_any(),
            Self::Record(value) => value.as_ref(),
            Self::Sequence(value) => value.as_ref(),
            Self::Dictionary(value) => value.as_ref(),
            Self::Bytes(value) => value.as_ref(),
            Self::View(value) => value.as_ref(),
        }
    }
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Leaf(a), Self::Leaf(b)) => Arc::ptr_eq(a, b),
            (Self::Record(a), Self::Record(b)) => Arc::ptr_eq(a, b),
            (Self::Sequence(a), Self::Sequence(b)) => Arc::ptr_eq(a, b),
            (Self::Dictionary(a), Self::Dictionary(b)) => Arc::ptr_eq(a, b),
            (Self::Bytes(a), Self::Bytes(b)) => Arc::ptr_eq(a, b),
            (Self::View(a), Self::View(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
impl Record {
    pub(crate) fn num_columns(&self) -> usize {
        self.columns.len()
    }
    pub(crate) fn column(&self, index: usize) -> &ArrayRef {
        &self.columns[index]
    }
}
impl Sequence {
    pub(crate) fn values(&self) -> &ArrayRef {
        &self.values
    }
    pub(crate) fn range(&self, index: usize) -> Option<(usize, usize)> {
        let (a, b) = self.offsets.range(index)?;
        (a <= b && b <= self.values.len()).then_some((a, b))
    }
}
impl Offsets {
    pub(super) fn range(&self, index: usize) -> Option<(usize, usize)> {
        let (a, b) = match self {
            Offsets::I32(values) => (
                usize::try_from(*values.get(index)?).ok()?,
                usize::try_from(*values.get(index + 1)?).ok()?,
            ),
            Offsets::I64(values) => (
                usize::try_from(*values.get(index)?).ok()?,
                usize::try_from(*values.get(index + 1)?).ok()?,
            ),
            Offsets::Fixed(width) => (
                index.checked_mul(*width)?,
                index.checked_add(1)?.checked_mul(*width)?,
            ),
        };
        (a <= b).then_some((a, b))
    }
}
impl Dictionary {
    pub(crate) fn keys(&self) -> &Leaf {
        &self.keys
    }
    pub(crate) fn values(&self) -> &ArrayRef {
        &self.values
    }
}
impl Bytes {
    pub(crate) fn value(&self, index: usize) -> Option<&[u8]> {
        if index >= self.len {
            return None;
        }
        let (start, end) = self.offsets.range(index)?;
        self.values.get(start..end)
    }
}
impl View {
    pub(crate) fn views(&self) -> &[u128] {
        &self.views
    }
    pub(crate) fn data_buffers(&self) -> &[Buffer] {
        &self.buffers
    }
}
