//! Bounded, call-local SQL bytes for repeatedly referenced native text labels.
//!
//! The existing input contract requires a conforming producer and stable input
//! throughout the call. An Arc retains lifetime, not exclusive buffer access.
//! Only successful native string encodings are retained; Python/Object hooks,
//! unreferenced labels and arbitrary nested values never enter this cache.

use std::sync::Arc;

use arrow_array::types::{
    Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use arrow_array::{ArrayRef, DictionaryArray};
use arrow_buffer::ScalarBuffer;
use arrow_schema::{DataType, Field};
use pyo3::prelude::*;
use sqlaero_core::bind::{Context, writer};
use sqlaero_core::transport::deadline::IoBudget;

use super::{CellWriter, Shape, dictionary_label, errors, invalid, write_value};

const COLUMNS: usize = 8;
const LABELS: usize = 64;
const BYTES: usize = 16 * 1024;

struct Labels {
    source: ArrayRef,
    keys: KeyReader,
    values: ArrayRef,
    encoded: Vec<Option<Vec<u8>>>,
    bytes: usize,
}

/// Owned buffers retain the already sliced key offsets and imported owner.
/// Validity stays authoritative in the exact retained source array.
/// Preparing them never inspects a future key or an unreferenced label.
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
                    .downcast_ref::<DictionaryArray<$kind>>()
                    .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
                Ok(Self::$variant(dictionary.keys().values().clone()))
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

impl Labels {
    /// Return only a newly written label's length; the caller retains the
    /// shared byte cap and performs the same fallible copy after encoding.
    fn write(
        &self,
        py: Python<'_>,
        label: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<Option<usize>> {
        if let Some(encoded) = &self.encoded[label] {
            // A cached label still charges the actual bounded copy.
            visitor.checkpoint(py, encoded.len())?;
            writer::append(output, encoded, context.limit)
                .map_err(|error| errors::wire(py, error))?;
            return Ok(None);
        }
        let start = output.len();
        write_value(py, &self.values, label, output, context, budget, visitor)?;
        Ok(Some(output.len() - start))
    }
}

/// At most eight lazily allocated label tables and 16 KiB of encoded bytes.
/// Ordinary primitive sources never allocate or populate this strategy.
pub(super) struct Cache {
    mask: u8,
    context: Option<(&'static str, bool)>,
    columns: Vec<Option<Labels>>,
    bytes: usize,
}

fn text_dictionary(dtype: &DataType) -> bool {
    matches!(dtype, DataType::Dictionary(_, values)
        if matches!(values.as_ref(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View))
}

impl Cache {
    pub(super) fn for_schema(field: &Field, shape: Shape) -> Option<Self> {
        let rows = match shape {
            Shape::Vector { length } => length,
            Shape::Table { rows, .. } => rows,
            Shape::Record { .. } => 0,
        };
        // A short input cannot amortize the cache's fixed strategy/table
        // allocation. Keep those existing paths free of new preparation work.
        if rows < 32 {
            return None;
        }
        let mask = if matches!(shape, Shape::Table { .. }) {
            match field.data_type() {
                DataType::Struct(fields) => {
                    fields
                        .iter()
                        .take(COLUMNS)
                        .enumerate()
                        .fold(0, |mask, (column, field)| {
                            mask | (u8::from(text_dictionary(field.data_type())) << column)
                        })
                }
                _ => 0,
            }
        } else {
            u8::from(text_dictionary(field.data_type()))
        };
        if mask == 0 {
            return None;
        }
        Some(Self {
            mask,
            context: None,
            columns: Vec::new(),
            bytes: 0,
        })
    }

    pub(super) fn has_column(&self, column: usize) -> bool {
        column < COLUMNS && self.mask & (1 << column) != 0
    }

    pub(super) fn clear_chunk(&mut self) {
        for column in &mut self.columns {
            *column = None;
        }
        self.bytes = 0;
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "reuse the actual provider write context without a parallel value contract"
    )]
    pub(super) fn write(
        &mut self,
        py: Python<'_>,
        column: usize,
        array: &ArrayRef,
        index: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        // These are the same parent checks as write_value. Missing dictionary
        // entries must not read a key or label, even to populate the cache.
        visitor.checkpoint(py, 1)?;
        if index >= array.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        if array.is_null(index) {
            return writer::append(output, b"NULL", context.limit)
                .map_err(|error| errors::wire(py, error));
        }
        let prepared_label = match self.columns.get(column).and_then(Option::as_ref) {
            Some(labels)
                if Arc::ptr_eq(&labels.source, array)
                    && self.context
                        == Some((context.codec.name(), context.no_backslash_escapes)) =>
            {
                Some(labels.keys.label(py, index, labels.values.len())?)
            }
            _ => None,
        };
        let label = if let Some(label) = prepared_label {
            label
        } else {
            // Resolve and validate this actual key before any old slot is
            // cleared, preserving cold/changed-context error ordering.
            let (values, label) = dictionary_label(py, array, index)?;
            if values.len() > LABELS || !text_dictionary(array.data_type()) {
                return write_value(py, values, label, output, context, budget, visitor);
            }
            let key = (context.codec.name(), context.no_backslash_escapes);
            if self.context != Some(key) {
                self.clear_chunk();
                self.context = Some(key);
            }
            if self.columns.is_empty() {
                self.columns
                    .try_reserve_exact(COLUMNS)
                    .map_err(|_| errors::memory_error(py))?;
                self.columns.resize_with(COLUMNS, || None);
            }
            let slot = &mut self.columns[column];
            if !slot
                .as_ref()
                .is_some_and(|labels| Arc::ptr_eq(&labels.values, values))
            {
                let mut encoded = Vec::new();
                encoded
                    .try_reserve_exact(values.len())
                    .map_err(|_| errors::memory_error(py))?;
                encoded.resize_with(values.len(), || None);
                let keys = KeyReader::prepare(py, array)?;
                if let Some(previous) = slot.take() {
                    self.bytes -= previous.bytes;
                }
                *slot = Some(Labels {
                    source: array.clone(),
                    keys,
                    values: values.clone(),
                    encoded,
                    bytes: 0,
                });
            }
            let labels = slot
                .as_mut()
                .ok_or_else(|| invalid(py, "Arrow label cache was not prepared"))?;
            if !Arc::ptr_eq(&labels.source, array) {
                let keys = KeyReader::prepare(py, array)?;
                labels.source = array.clone();
                labels.keys = keys;
            }
            label
        };
        let labels = self.columns[column]
            .as_mut()
            .ok_or_else(|| invalid(py, "Arrow label cache was not prepared"))?;
        if let Some(length) = labels.write(py, label, output, context, budget, visitor)?
            && length <= BYTES - self.bytes
        {
            let mut encoded = Vec::new();
            encoded
                .try_reserve_exact(length)
                .map_err(|_| errors::memory_error(py))?;
            encoded.extend_from_slice(&output[output.len() - length..]);
            labels.encoded[label] = Some(encoded);
            labels.bytes += length;
            self.bytes += length;
        }
        Ok(())
    }
}
