//! Bounded, call-local SQL bytes for repeatedly referenced native text labels.
//!
//! The existing input contract requires a conforming producer and stable input
//! throughout the call. An Arc retains lifetime, not exclusive buffer access.
//! Only successful native string encodings are retained; Python/Object hooks,
//! unreferenced labels and arbitrary nested values never enter this cache.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_schema::{DataType, Field};
use pyo3::prelude::*;
use sqlaero_core::bind::{Context, writer};
use sqlaero_core::transport::deadline::IoBudget;

use super::{CellWriter, Shape, dictionary_label, errors, invalid, write_value};

const COLUMNS: usize = 8;
const LABELS: usize = 64;
const BYTES: usize = 16 * 1024;

struct Labels {
    values: ArrayRef,
    encoded: Vec<Option<Vec<u8>>>,
    bytes: usize,
}

/// Eight small label tables plus at most 16 KiB of encoded bytes. Ordinary
/// primitive sources do not allocate or populate this optional strategy.
pub(super) struct Cache {
    mask: u8,
    context: Option<(&'static str, bool)>,
    columns: [Option<Labels>; COLUMNS],
    bytes: usize,
}

fn text_dictionary(dtype: &DataType) -> bool {
    matches!(dtype, DataType::Dictionary(_, values)
        if matches!(values.as_ref(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View))
}

impl Cache {
    pub(super) fn for_schema(
        py: Python<'_>,
        field: &Field,
        shape: Shape,
    ) -> PyResult<Option<Box<Self>>> {
        let rows = match shape {
            Shape::Vector { length } => length,
            Shape::Table { rows, .. } => rows,
            Shape::Record { .. } => 0,
        };
        // A short input cannot amortize the cache's fixed strategy/table
        // allocation. Keep those existing paths free of new preparation work.
        if rows < 32 {
            return Ok(None);
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
            return Ok(None);
        }
        crate::input::boxed(
            py,
            Self {
                mask,
                context: None,
                columns: std::array::from_fn(|_| None),
                bytes: 0,
            },
        )
        .map(Some)
    }

    pub(super) fn has_column(&self, column: usize) -> bool {
        column < COLUMNS && self.mask & (1 << column) != 0
    }

    pub(super) fn clear_chunk(&mut self) {
        self.columns = std::array::from_fn(|_| None);
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
        let (values, label) = dictionary_label(py, array, index)?;
        if values.len() > LABELS || !text_dictionary(array.data_type()) {
            return write_value(py, values, label, output, context, budget, visitor);
        }
        let key = (context.codec.name(), context.no_backslash_escapes);
        if self.context != Some(key) {
            // No raw data slice exists here. Dropping a previous dictionary
            // may release its producer; subsequent value access is reacquired.
            self.clear_chunk();
            self.context = Some(key);
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
            if let Some(previous) = slot.take() {
                self.bytes -= previous.bytes;
            }
            *slot = Some(Labels {
                values: values.clone(),
                encoded,
                bytes: 0,
            });
        }
        let labels = slot
            .as_mut()
            .ok_or_else(|| invalid(py, "Arrow label cache was not prepared"))?;
        if let Some(encoded) = &labels.encoded[label] {
            // Account for the actual bounded copy, retaining signal/deadline
            // progress even when UTF-8 validation and escaping are reused.
            visitor.checkpoint(py, encoded.len())?;
            return writer::append(output, encoded, context.limit)
                .map_err(|error| errors::wire(py, error));
        }
        let start = output.len();
        write_value(py, values, label, output, context, budget, visitor)?;
        let length = output.len() - start;
        if length <= BYTES - self.bytes {
            let mut encoded = Vec::new();
            encoded
                .try_reserve_exact(length)
                .map_err(|_| errors::memory_error(py))?;
            encoded.extend_from_slice(&output[start..]);
            labels.encoded[label] = Some(encoded);
            labels.bytes += length;
            self.bytes += length;
        }
        Ok(())
    }
}
