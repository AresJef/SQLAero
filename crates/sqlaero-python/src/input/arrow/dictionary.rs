//! Retained native dictionary coordinates, never target-formatted labels.
use super::{ArrayRef, native};
use super::{dictionary_label, invalid};
use crate::errors;
use crate::input::cell::{TextIdentity, TextReference};
use crate::input::source::Shape;
use arrow_array::PrimitiveArray;
use arrow_array::types::{
    Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use arrow_buffer::ScalarBuffer;
use arrow_schema::DataType;
use pyo3::prelude::*;

const COLUMNS: usize = 8;
struct Labels {
    source: ArrayRef,
    keys: KeyReader,
    values: ArrayRef,
    identity: Option<TextIdentity>,
}
enum KeyReader {
    Int8(ScalarBuffer<i8>),
    Int16(ScalarBuffer<i16>),
    Int32(ScalarBuffer<i32>),
    Int64(ScalarBuffer<i64>),
    UInt8(ScalarBuffer<u8>),
    UInt16(ScalarBuffer<u16>),
    UInt32(ScalarBuffer<u32>),
    UInt64(ScalarBuffer<u64>),
}

impl KeyReader {
    fn prepare(py: Python<'_>, array: &ArrayRef) -> PyResult<Self> {
        macro_rules! keys {
            ($kind:ty, $variant:ident) => {{
                let dictionary = array
                    .as_any()
                    .downcast_ref::<native::Dictionary>()
                    .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
                let keys = dictionary
                    .keys()
                    .as_any()
                    .downcast_ref::<PrimitiveArray<$kind>>()
                    .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
                Ok(Self::$variant(keys.values().clone()))
            }};
        }
        match array.data_type() {
            DataType::Dictionary(key, _) => match key.as_ref() {
                DataType::Int8 => keys!(Int8Type, Int8),
                DataType::Int16 => keys!(Int16Type, Int16),
                DataType::Int32 => keys!(Int32Type, Int32),
                DataType::Int64 => keys!(Int64Type, Int64),
                DataType::UInt8 => keys!(UInt8Type, UInt8),
                DataType::UInt16 => keys!(UInt16Type, UInt16),
                DataType::UInt32 => keys!(UInt32Type, UInt32),
                DataType::UInt64 => keys!(UInt64Type, UInt64),
                _ => Err(invalid(py, "unsupported Arrow Dictionary key type")),
            },
            _ => Err(invalid(py, "Arrow value is not a Dictionary")),
        }
    }

    fn label(&self, py: Python<'_>, index: usize, length: usize) -> PyResult<usize> {
        macro_rules! key {
            ($keys:expr) => {{
                let keys = $keys;
                let key = keys
                    .get(index)
                    .copied()
                    .ok_or_else(|| invalid(py, "Arrow child index is outside its array"))?;
                usize::try_from(key)
                    .map_err(|_| invalid(py, "Arrow Dictionary key is negative or exceeds usize"))?
            }};
        }
        let label = match self {
            Self::Int8(keys) => key!(keys),
            Self::Int16(keys) => key!(keys),
            Self::Int32(keys) => key!(keys),
            Self::Int64(keys) => key!(keys),
            Self::UInt8(keys) => key!(keys),
            Self::UInt16(keys) => key!(keys),
            Self::UInt32(keys) => key!(keys),
            Self::UInt64(keys) => key!(keys),
        };
        if label >= length {
            return Err(invalid(
                py,
                "Arrow Dictionary key is outside the current dictionary",
            ));
        }
        Ok(label)
    }
}

/// Only physical owners, keys and labels are retained. Each target decides
/// independently whether and how a referenced native text value is written.
pub(super) struct Reader {
    mask: u8,
    columns: Vec<Option<Labels>>,
}
fn text_dictionary(dtype: &DataType) -> bool {
    matches!(dtype, DataType::Dictionary(_, values)
        if matches!(values.as_ref(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View))
}
impl Reader {
    pub(super) fn clear_chunk(&mut self) {
        for column in &mut self.columns {
            *column = None;
        }
    }

    pub(super) fn prepare(dtype: &DataType, shape: Shape) -> Option<Self> {
        let rows = match shape {
            Shape::Vector { length } => length,
            Shape::Table { rows, .. } => rows,
            _ => 0,
        };
        if rows < 32 {
            return None;
        }
        let mask = if matches!(shape, Shape::Table { .. }) {
            match dtype {
                DataType::Struct(fields) => {
                    fields
                        .iter()
                        .take(COLUMNS)
                        .enumerate()
                        .fold(0, |mask, (index, field)| {
                            mask | (u8::from(text_dictionary(field.data_type())) << index)
                        })
                }
                _ => 0,
            }
        } else {
            u8::from(text_dictionary(dtype))
        };
        (mask != 0).then(|| Self {
            mask,
            columns: Vec::new(),
        })
    }
    pub(super) fn has_column(&self, column: usize) -> bool {
        column < COLUMNS && self.mask & (1 << column) != 0
    }
    pub(super) fn reference<'a>(
        &'a mut self,
        py: Python<'_>,
        array: &ArrayRef,
        index: usize,
        column: usize,
    ) -> PyResult<Option<TextReference<'a>>> {
        if index >= array.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        // Parent validity is authoritative before any actual key or label.
        if array.is_null(index) {
            return Ok(None);
        }
        if !self
            .columns
            .get(column)
            .and_then(Option::as_ref)
            .is_some_and(|labels| labels.source.ptr_eq(array))
        {
            let (values, _) = dictionary_label(py, array, index)?;
            let keys = KeyReader::prepare(py, array)?;
            if self.columns.is_empty() {
                self.columns
                    .try_reserve_exact(COLUMNS)
                    .map_err(|_| errors::memory_error(py))?;
                self.columns.resize_with(COLUMNS, || None);
            }
            self.columns[column] = Some(Labels {
                source: array.clone(),
                keys,
                values: values.clone(),
                identity: TextIdentity::new(),
            });
        }
        let labels = self.columns[column]
            .as_ref()
            .ok_or_else(|| invalid(py, "Arrow dictionary facts were not prepared"))?;
        let label = labels.keys.label(py, index, labels.values.len())?;
        Ok(Some(TextReference {
            identity: labels.identity,
            values: &labels.values,
            index: label,
            channel: column,
        }))
    }
}
