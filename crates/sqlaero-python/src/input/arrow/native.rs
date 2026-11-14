//! Bounded preparation of a qualified, stable CPU C Data export.
//!
//! Keep Arrow's mature native arrays and typed buffers, but do not delegate a
//! whole structural walk, unknown null-count scan or alignment copy to one
//! unchecked-length library call. Each imported buffer shares the moved root
//! release owner; no Python rows, callbacks or normalized value tree are built.

use super::invalid;
use crate::errors;
use crate::input::control::Checkpoint;
use arrow_array::ffi::FFI_ArrowArray;
use arrow_array::{NullArray, make_array};
use arrow_buffer::{BooleanBuffer, Buffer, MutableBuffer, NullBuffer, ScalarBuffer};
use arrow_data::ArrayData;
use arrow_schema::DataType;
use pyo3::prelude::*;
use std::ffi::c_void;
mod data;
use data::Offsets;
pub(crate) use data::{ArrayRef, Bytes, Dictionary, Record, Sequence, View};
use std::ptr::{self, NonNull};
use std::sync::Arc;

#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    length: i64,
    null_count: i64,
    offset: i64,
    n_buffers: i64,
    n_children: i64,
    buffers: *const *const c_void,
    children: *const *const Self,
    dictionary: *const Self,
    release: Option<unsafe extern "C" fn(*mut Self)>,
    private_data: *mut c_void,
}
const _: () = assert!(size_of::<Header>() == size_of::<FFI_ArrowArray>());
const _: () = assert!(align_of::<Header>() == align_of::<FFI_ArrowArray>());

struct Read<'a> {
    owner: Arc<FFI_ArrowArray>,
    budget: &'a dyn Checkpoint,
}

pub(super) fn import(
    py: Python<'_>,
    source: FFI_ArrowArray,
    dtype: &DataType,
    budget: &dyn Checkpoint,
) -> PyResult<ArrayRef> {
    let owner = Arc::new(source);
    // SAFETY: this owned standard export is live, qualified and stable. The
    // ABI is public; no private Python layout or unowned provider view is used.
    let header = unsafe { ptr::from_ref(owner.as_ref()).cast::<Header>().read() };
    Read { owner, budget }.array(py, header, dtype, None, 0)
}

fn size(py: Python<'_>, value: i64) -> PyResult<usize> {
    usize::try_from(value)
        .map_err(|_| invalid(py, "Arrow C array has a negative or unrepresentable size"))
}

fn add(py: Python<'_>, a: usize, b: usize) -> PyResult<usize> {
    a.checked_add(b)
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or_else(|| invalid(py, "Arrow C array span exceeds the address range"))
}

fn mul(py: Python<'_>, a: usize, b: usize) -> PyResult<usize> {
    a.checked_mul(b)
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or_else(|| invalid(py, "Arrow C buffer span exceeds the address range"))
}

impl Read<'_> {
    fn pointer(&self, py: Python<'_>, node: Header, index: usize) -> PyResult<*const u8> {
        if index >= size(py, node.n_buffers)? || node.buffers.is_null() {
            return Err(invalid(
                py,
                "Arrow C array is missing a required buffer slot",
            ));
        }
        // SAFETY: the qualified source owns this public pointer array; header
        // sizes are checked before indexing, and owner outlives the read.
        Ok(unsafe { node.buffers.add(index).read().cast() })
    }

    fn buffer(
        &self,
        py: Python<'_>,
        node: Header,
        index: usize,
        length: usize,
        alignment: usize,
    ) -> PyResult<Buffer> {
        if length == 0 {
            return Ok(MutableBuffer::try_with_capacity(0)
                .map_err(|_| errors::memory_error(py))?
                .into());
        }
        let source = self.pointer(py, node, index)?;
        let Some(source) = NonNull::new(source.cast_mut()) else {
            return Err(invalid(py, "Arrow C array has a null nonempty buffer"));
        };
        add(py, source.as_ptr() as usize, length)?;
        if (source.as_ptr() as usize).is_multiple_of(alignment) {
            // SAFETY: the qualified source guarantees this buffer's span and
            // stable storage. Cloning the moved root keeps every child buffer
            // and its exact producer release alive through the native owner.
            return Ok(unsafe {
                Buffer::from_custom_allocation(source, length, self.owner.clone())
            });
        }
        let mut output =
            MutableBuffer::try_with_capacity(length).map_err(|_| errors::memory_error(py))?;
        for start in (0..length).step_by(crate::input::WORK_BYTES) {
            let count = (length - start).min(crate::input::WORK_BYTES);
            // SAFETY: the private allocation has checked capacity. The stable
            // source span is live; no check/callback occurs inside this bounded
            // copy. An error later drops only initialized byte storage.
            unsafe {
                ptr::copy_nonoverlapping(
                    source.as_ptr().add(start),
                    output.as_mut_ptr().add(start),
                    count,
                );
                output.set_len(start + count);
            }
            self.budget.bytes(py, count)?;
        }
        Ok(output.into())
    }

    fn nulls(
        &self,
        py: Python<'_>,
        node: Header,
        offset: usize,
        len: usize,
    ) -> PyResult<Option<NullBuffer>> {
        if len == 0 || node.null_count == 0 {
            return Ok(None);
        }
        if node.null_count < -1 || node.null_count > node.length {
            return Err(invalid(py, "Arrow C array has an invalid null count"));
        }
        if self.pointer(py, node, 0)?.is_null() {
            return if node.null_count == -1 {
                Ok(None)
            } else {
                Err(invalid(py, "Arrow C array is missing its validity buffer"))
            };
        }
        let bytes = add(py, offset, len)?.div_ceil(8);
        let bits = BooleanBuffer::new(self.buffer(py, node, 0, bytes, 1)?, offset, len);
        // A selected struct/fixed-list window may differ from the producer's
        // full count. Known zero avoids all scanning; every other actual scan
        // uses bounded bit windows under the original Control.
        let count = if offset == size(py, node.offset)?
            && len == size(py, node.length)?
            && node.null_count >= 0
        {
            node.null_count as usize
        } else {
            let mut count = 0;
            for start in (0..len).step_by(crate::input::WORK_BYTES * 8) {
                let n = (len - start).min(crate::input::WORK_BYTES * 8);
                count += n - bits.slice(start, n).count_set_bits();
                self.budget.bytes(py, n.div_ceil(8))?;
            }
            count
        };
        // SAFETY: the qualified full count or bounded selected-window count
        // describes these exact validity bits. No value payload was inspected.
        Ok((count != 0).then(|| unsafe { NullBuffer::new_unchecked(bits, count) }))
    }

    fn child(&self, py: Python<'_>, node: Header, index: usize) -> PyResult<Header> {
        if index >= size(py, node.n_children)? || node.children.is_null() {
            return Err(invalid(py, "Arrow C array is missing a required child"));
        }
        // SAFETY: qualified, live public child pointer array, checked index.
        let child = unsafe { node.children.add(index).read() };
        if child.is_null() {
            return Err(invalid(py, "Arrow C array has a null child pointer"));
        }
        // SAFETY: the moved root still owns this child's complete header.
        Ok(unsafe { child.read() })
    }

    fn array(
        &self,
        py: Python<'_>,
        node: Header,
        dtype: &DataType,
        window: Option<(usize, usize)>,
        depth: usize,
    ) -> PyResult<ArrayRef> {
        self.budget.bytes(py, 8)?;
        if depth >= 64 || node.release.is_none() {
            return Err(invalid(
                py,
                "Arrow C array is released or too deeply nested",
            ));
        }
        let original_len = size(py, node.length)?;
        let mut offset = size(py, node.offset)?;
        let len = if let Some((start, len)) = window {
            if add(py, start, len)? > original_len {
                return Err(invalid(py, "Arrow child selection exceeds its array"));
            }
            offset = add(py, offset, start)?;
            len
        } else {
            original_len
        };
        add(py, offset, len)?;
        let children = size(py, node.n_children)?;
        if matches!(dtype, DataType::Null) {
            return Ok(ArrayRef::Leaf(Arc::new(NullArray::new(len))));
        }
        let nulls = self.nulls(py, node, offset, len)?;
        match dtype {
            DataType::Struct(fields) => {
                if children != fields.len() {
                    return Err(invalid(
                        py,
                        "Arrow Struct child count differs from its schema",
                    ));
                }
                let mut arrays = Vec::new();
                arrays
                    .try_reserve_exact(fields.len())
                    .map_err(|_| errors::memory_error(py))?;
                for (index, field) in fields.iter().enumerate() {
                    arrays.push(self.array(
                        py,
                        self.child(py, node, index)?,
                        field.data_type(),
                        Some((offset, len)),
                        depth + 1,
                    )?);
                }
                Ok(ArrayRef::Record(Arc::new(Record {
                    dtype: dtype.clone(),
                    len,
                    nulls,
                    columns: arrays,
                })))
            }
            DataType::List(_) | DataType::LargeList(_) => {
                self.list(py, node, dtype, (offset, len), nulls, depth)
            }
            DataType::FixedSizeList(field, width) => {
                if children != 1 {
                    return Err(invalid(py, "Arrow fixed list must have one child"));
                }
                let width_usize = usize::try_from(*width)
                    .map_err(|_| invalid(py, "Arrow fixed list width is negative"))?;
                let values = self.array(
                    py,
                    self.child(py, node, 0)?,
                    field.data_type(),
                    Some((mul(py, offset, width_usize)?, mul(py, len, width_usize)?)),
                    depth + 1,
                )?;
                Ok(ArrayRef::Sequence(Arc::new(Sequence {
                    dtype: dtype.clone(),
                    len,
                    nulls,
                    offsets: Offsets::Fixed(width_usize),
                    values,
                })))
            }
            DataType::Dictionary(key, value) => {
                if children != 0 || node.dictionary.is_null() {
                    return Err(invalid(
                        py,
                        "Arrow Dictionary has invalid children or no values",
                    ));
                }
                // SAFETY: the qualified moved root owns this dictionary header.
                let dictionary = unsafe { node.dictionary.read() };
                let values = self.array(py, dictionary, value, None, depth + 1)?;
                let ArrayRef::Leaf(keys) = self.leaf(py, node, key, (offset, len), nulls)? else {
                    return Err(invalid(
                        py,
                        "Arrow Dictionary key is not a primitive buffer",
                    ));
                };
                // Retain raw integer keys; only an actual selected value is
                // checked against its own current dictionary. No unsafe native
                // constructor promises that unvisited keys are in range.
                Ok(ArrayRef::Dictionary(Arc::new(Dictionary {
                    dtype: dtype.clone(),
                    keys,
                    values,
                })))
            }
            _ if children != 0 => Err(invalid(py, "Arrow scalar C array has unexpected children")),
            _ => self.leaf(py, node, dtype, (offset, len), nulls),
        }
    }

    fn list(
        &self,
        py: Python<'_>,
        node: Header,
        dtype: &DataType,
        window: (usize, usize),
        nulls: Option<NullBuffer>,
        depth: usize,
    ) -> PyResult<ArrayRef> {
        if node.n_children != 1 {
            return Err(invalid(py, "Arrow list must have one child"));
        }
        let (offset, len) = window;
        let (field, width) = match dtype {
            DataType::List(field) => (field, 4),
            DataType::LargeList(field) => (field, 8),
            _ => return Err(invalid(py, "Arrow list physical layout differs")),
        };
        let offsets = self.offsets(py, node, width, offset, len)?;
        let values = self.array(
            py,
            self.child(py, node, 0)?,
            field.data_type(),
            None,
            depth + 1,
        )?;
        Ok(ArrayRef::Sequence(Arc::new(Sequence {
            dtype: dtype.clone(),
            len,
            nulls,
            offsets,
            values,
        })))
    }

    fn offsets(
        &self,
        py: Python<'_>,
        node: Header,
        width: usize,
        offset: usize,
        len: usize,
    ) -> PyResult<Offsets> {
        let end = add(py, offset, len)?;
        let buffer = self.buffer(py, node, 1, mul(py, add(py, end, 1)?, width)?, width)?;
        let n = add(py, len, 1)?;
        Ok(if width == 4 {
            Offsets::I32(ScalarBuffer::new(buffer, offset, n))
        } else {
            Offsets::I64(ScalarBuffer::new(buffer, offset, n))
        })
    }

    fn leaf(
        &self,
        py: Python<'_>,
        node: Header,
        dtype: &DataType,
        window: (usize, usize),
        nulls: Option<NullBuffer>,
    ) -> PyResult<ArrayRef> {
        let (offset, len) = window;
        let end = add(py, offset, len)?;
        let mut buffers = Vec::new();
        let count = size(py, node.n_buffers)?;
        let (expected, bytes, alignment) = match dtype {
            DataType::Boolean => (2, end.div_ceil(8), 1),
            DataType::FixedSizeBinary(width) => (
                2,
                mul(
                    py,
                    end,
                    usize::try_from(*width)
                        .map_err(|_| invalid(py, "Arrow binary width is negative"))?,
                )?,
                1,
            ),
            DataType::Utf8 | DataType::Binary => (3, mul(py, add(py, end, 1)?, 4)?, 4),
            DataType::LargeUtf8 | DataType::LargeBinary => (3, mul(py, add(py, end, 1)?, 8)?, 8),
            DataType::Utf8View | DataType::BinaryView => {
                if count < 3 {
                    return Err(invalid(py, "Arrow byte view is missing its buffers"));
                }
                (count, mul(py, end, 16)?, 16)
            }
            _ => {
                let width = dtype
                    .primitive_width()
                    .ok_or_else(|| invalid(py, "unsupported native Arrow scalar layout"))?;
                (2, mul(py, end, width)?, width)
            }
        };
        if count != expected {
            return Err(invalid(
                py,
                "Arrow C array buffer count differs from its scalar schema",
            ));
        }
        buffers
            .try_reserve_exact(count - 1)
            .map_err(|_| errors::memory_error(py))?;
        buffers.push(self.buffer(py, node, 1, bytes, alignment)?);
        match dtype {
            DataType::Utf8 | DataType::Binary | DataType::LargeUtf8 | DataType::LargeBinary => {
                let length = if len == 0 && offset == 0 {
                    0
                } else if alignment == 4 {
                    // SAFETY: the retained aligned offsets buffer spans end+1
                    // entries. The copied scalar is detached before checks.
                    let value = unsafe { buffers[0].as_ptr().cast::<i32>().add(end).read() };
                    usize::try_from(value)
                        .map_err(|_| invalid(py, "Arrow byte offsets are negative"))?
                } else {
                    // SAFETY: same retained checked span, for large offsets.
                    let value = unsafe { buffers[0].as_ptr().cast::<i64>().add(end).read() };
                    usize::try_from(value)
                        .map_err(|_| invalid(py, "Arrow byte offsets are negative"))?
                };
                let values = self.buffer(py, node, 2, length, 1)?;
                let offsets = buffers
                    .pop()
                    .ok_or_else(|| invalid(py, "Arrow offsets were not prepared"))?;
                let n = add(py, len, 1)?;
                let offsets = if alignment == 4 {
                    Offsets::I32(ScalarBuffer::new(offsets, offset, n))
                } else {
                    Offsets::I64(ScalarBuffer::new(offsets, offset, n))
                };
                return Ok(ArrayRef::Bytes(Arc::new(Bytes {
                    dtype: dtype.clone(),
                    len,
                    nulls,
                    offsets,
                    values,
                })));
            }
            DataType::Utf8View | DataType::BinaryView => {
                let views = buffers
                    .pop()
                    .ok_or_else(|| invalid(py, "Arrow views were not prepared"))?;
                let views = ScalarBuffer::<u128>::new(views, offset, len);
                let lengths = self.pointer(py, node, count - 1)?;
                if count > 3 && lengths.is_null() {
                    return Err(invalid(py, "Arrow view has no variadic buffer lengths"));
                }
                mul(py, count - 3, size_of::<i64>())?;
                for index in 2..count - 1 {
                    // SAFETY: qualified source supplies count-3 i64 lengths;
                    // unaligned read supports the public ABI's minimum layout.
                    let length = unsafe { lengths.cast::<i64>().add(index - 2).read_unaligned() };
                    buffers.push(self.buffer(py, node, index, size(py, length)?, 1)?);
                    self.budget.bytes(py, 8)?;
                }
                return Ok(ArrayRef::View(Arc::new(View {
                    dtype: dtype.clone(),
                    nulls,
                    views,
                    buffers,
                })));
            }
            DataType::FixedSizeBinary(width) => {
                let width = usize::try_from(*width)
                    .map_err(|_| invalid(py, "Arrow binary width is negative"))?;
                let values = buffers
                    .pop()
                    .ok_or_else(|| invalid(py, "Arrow binary bytes were not prepared"))?;
                let values =
                    values.slice_with_length(mul(py, offset, width)?, mul(py, len, width)?);
                return Ok(ArrayRef::Bytes(Arc::new(Bytes {
                    dtype: dtype.clone(),
                    len,
                    nulls,
                    offsets: Offsets::Fixed(width),
                    values,
                })));
            }
            _ => {}
        }
        let builder = ArrayData::builder(dtype.clone())
            .len(len)
            .offset(offset)
            .nulls(nulls)
            .buffers(buffers);
        // SAFETY: this is one qualified scalar C layout, not a recursive
        // importer. Required spans, alignment, buffer counts and validity are
        // established above; this leaf contains only fixed primitive bit
        // patterns, never a UTF-8 promise, unchecked offset or dictionary key.
        let data = unsafe { builder.skip_validation(true) }
            .build()
            .map_err(|error| {
                errors::builtin_format::<pyo3::exceptions::PyTypeError>(py, format_args!("{error}"))
            })?;
        // Preserve mature constant-size scalar layout and endpoint checks.
        // This does not scan all values or a recursive child tree.
        data.validate().map_err(|error| {
            errors::builtin_format::<pyo3::exceptions::PyTypeError>(py, format_args!("{error}"))
        })?;
        Ok(ArrayRef::Leaf(make_array(data)))
    }
}
