//! Prepared exact ndarray coordinates with short, reacquired native exports.
//!
//! NumPy requires callers to keep shared storage stable. A private view is used
//! only while preparing metadata. Each new native read reacquires the original
//! source because even a retained view/export can be invalidated by setstate.
//! Fixed-width values can reuse an already copied, bounded Rust-owned block;
//! such bytes no longer borrow mutable NumPy memory.
//! Weak references guard normal resize, not arbitrary external NumPy/C writers.

use super::allocation::{Allocation, Export};
use super::{Kind, half_to_f64};
use crate::errors;
use crate::input::source::{CellWriter, PreparedSource, Shape};
use crate::input::temporal::{self, TimeKind, TimeUnit};
use pyo3::exceptions::{PyBufferError, PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple, PyType};
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::protocol::framing::WireError;
use sqlaero_core::transport::deadline::IoBudget;

const RAW_BLOCK_ITEMS: usize = 1024;
const RAW_BLOCK_BYTES: usize = 8192;
const RAW_BLOCK_MINIMUM: usize = 32;

/// A small logical run already read from NumPy. This is machine data, never
/// SQL or Python values, and cannot be invalidated by a later Python callback.
/// Only numeric, S and U storage participates; Object slots are never copied.
struct RawBlock {
    start: usize,
    bytes: Vec<u8>,
}

/// Only the copying window owns these fresh exports. Metadata is copied into
/// native coordinates before any source address is formed; no Python call,
/// allocation or checkpoint occurs while reading their bytes.
struct NativeView<'owner, 'py> {
    // Fields drop in declaration order: end the derived export first.
    source: Export<'owner, 'py>,
    root: Export<'owner, 'py>,
    dimensions: usize,
    lengths: [usize; 2],
    strides: [isize; 2],
    width: usize,
}

impl NativeView<'_, '_> {
    fn offset(&self, row: usize, column: usize) -> Option<usize> {
        if row >= self.lengths[0] || column >= self.lengths[1] {
            return None;
        }
        let mut offset = 0isize;
        for (axis, coordinate) in [row, column].into_iter().enumerate().take(self.dimensions) {
            let delta = isize::try_from(coordinate)
                .ok()?
                .checked_mul(self.strides[axis])?;
            offset = offset.checked_add(delta)?;
        }
        let address = (self.source.raw.buf as usize).checked_add_signed(offset)?;
        if !self.root.contains(address as *const u8, self.width) {
            return None;
        }
        address.checked_sub(self.root.raw.buf as usize)
    }

    fn contiguous(&self) -> bool {
        let Ok(width) = isize::try_from(self.width) else {
            return false;
        };
        if self.dimensions == 1 {
            self.strides[0] == width
        } else {
            self.strides[1] == width
                && self.lengths[1]
                    .checked_mul(self.width)
                    .and_then(|stride| isize::try_from(stride).ok())
                    == Some(self.strides[0])
        }
    }
}

#[derive(Clone, Copy)]
enum ArrayKind {
    Scalar(Kind),
    Bytes,
    Unicode,
    Object,
}

/// Original identity, prepared metadata and necessary normal-resize guards.
pub(crate) struct NumpySource<'py> {
    owner: Bound<'py, PyAny>,
    dtype: Bound<'py, PyAny>,
    allocation: Allocation<'py>,
    shape: Shape,
    kind: ArrayKind,
    width: usize,
    swap: bool,
    temporal: Option<(TimeUnit, u64, TimeKind)>,
    raw: Option<RawBlock>,
}

impl<'py> NumpySource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        array_type: &Py<PyType>,
        datetime_data: &Py<PyAny>,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        let py = value.py();
        let allocation = Allocation::new(value, array_type, budget)?;
        // The view is never published to Python. Subsequent changes to the
        // original's shape do not change this view's coordinate metadata.
        let view_method = errors::attribute(value, c"view")?;
        let view = view_method.call0()?;
        crate::input::check(py, budget)?;
        if !view.get_type().is(array_type.bind(py)) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "exact ndarray.view must return an exact ndarray",
            ));
        }
        let dimensions = errors::attribute(&view, c"ndim")?.extract::<usize>()?;
        if !(1..=2).contains(&dimensions) {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "NumPy input supports exactly one or two dimensions",
            ));
        }
        let coordinates = errors::attribute(&view, c"shape")?;
        let coordinates = coordinates.cast::<PyTuple>()?;
        let shape = if dimensions == 1 {
            Shape::Vector {
                length: coordinates.get_item(0)?.extract()?,
            }
        } else {
            Shape::Table {
                rows: coordinates.get_item(0)?.extract()?,
                columns: coordinates.get_item(1)?.extract()?,
            }
        };
        let dtype = errors::attribute(&view, c"dtype")?;
        let width = errors::attribute(&dtype, c"itemsize")?.extract::<usize>()?;
        let tag = errors::attribute(&dtype, c"kind")?;
        let tag = tag.extract::<&str>()?;
        let kind = match (tag, width) {
            ("i", 1) => ArrayKind::Scalar(Kind::I8),
            ("i", 2) => ArrayKind::Scalar(Kind::I16),
            ("i", 4) => ArrayKind::Scalar(Kind::I32),
            ("i", 8) => ArrayKind::Scalar(Kind::I64),
            ("u", 1) => ArrayKind::Scalar(Kind::U8),
            ("u", 2) => ArrayKind::Scalar(Kind::U16),
            ("u", 4) => ArrayKind::Scalar(Kind::U32),
            ("u", 8) => ArrayKind::Scalar(Kind::U64),
            ("f", 2) => ArrayKind::Scalar(Kind::F16),
            ("f", 4) => ArrayKind::Scalar(Kind::F32),
            ("f", 8) => ArrayKind::Scalar(Kind::F64),
            ("b", 1) => ArrayKind::Scalar(Kind::Bool),
            ("M", 8) => ArrayKind::Scalar(Kind::DateTime),
            ("m", 8) => ArrayKind::Scalar(Kind::Duration),
            ("S", _) => ArrayKind::Bytes,
            ("U", width) if width.is_multiple_of(4) => ArrayKind::Unicode,
            ("O", width) if width == std::mem::size_of::<*mut pyo3::ffi::PyObject>() => {
                ArrayKind::Object
            }
            _ => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "NumPy dtype is outside the supported fixed-width, text, binary and Object kinds",
                ));
            }
        };
        let native_order = errors::attribute(&dtype, c"isnative")?.extract::<bool>()?;
        let temporal = match kind {
            ArrayKind::Scalar(Kind::DateTime) => {
                let (unit, multiplier) = super::temporal_metadata(&view, datetime_data)?;
                Some((unit, multiplier, TimeKind::DateTime))
            }
            ArrayKind::Scalar(Kind::Duration) => {
                let (unit, multiplier) = super::temporal_metadata(&view, datetime_data)?;
                Some((unit, multiplier, TimeKind::Duration))
            }
            _ => None,
        };
        Ok(Self {
            owner: value.clone(),
            dtype,
            allocation,
            shape,
            kind,
            width,
            swap: !native_order,
            temporal,
            raw: None,
        })
    }

    fn cell_bytes(&mut self, row: usize, column: usize, budget: &IoBudget) -> PyResult<[u8; 8]> {
        let mut bytes = [0u8; 8];
        let (rows, columns) = match self.shape {
            Shape::Vector { length } => (length, 1),
            Shape::Table { rows, columns } => (rows, columns),
            Shape::Record { .. } => (0, 0),
        };
        let total = rows.checked_mul(columns);
        let index = (row < rows && column < columns)
            .then(|| row.checked_mul(columns)?.checked_add(column))
            .flatten();
        if let (Some(total), Some(index)) = (total, index)
            && total >= RAW_BLOCK_MINIMUM
        {
            if !self.raw.as_ref().is_some_and(|block| {
                index >= block.start && index - block.start < block.bytes.len() / self.width
            }) {
                self.read_block(index, total, columns, budget)?;
            }
            let copied = self.raw.as_ref().and_then(|block| {
                let start = index.checked_sub(block.start)?.checked_mul(self.width)?;
                block.bytes.get(start..start.checked_add(self.width)?)
            });
            if let Some(copied) = copied {
                bytes[..self.width].copy_from_slice(copied);
            } else {
                self.copy_source_range(row, column, 0, &mut bytes[..self.width])?;
            }
        } else {
            self.copy_source_range(row, column, 0, &mut bytes[..self.width])?;
        }
        if self.swap {
            bytes[..self.width].reverse();
        }
        Ok(bytes)
    }

    /// Read a signed native mask/code/time value without Python number boxing.
    pub(crate) fn signed_at(&mut self, row: usize, budget: &IoBudget) -> PyResult<i64> {
        let read = match self.kind {
            ArrayKind::Scalar(Kind::I8) => 1,
            ArrayKind::Scalar(Kind::I16) => 2,
            ArrayKind::Scalar(Kind::I32) => 4,
            ArrayKind::Scalar(Kind::I64) => 8,
            _ => {
                return Err(errors::builtin_message::<PyTypeError>(
                    self.owner.py(),
                    "NumPy codes or ticks require a signed integer vector",
                ));
            }
        };
        if !matches!(self.shape, Shape::Vector { .. }) {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "NumPy codes or ticks require one dimension",
            ));
        }
        let bytes = self.cell_bytes(row, 0, budget)?;
        Ok(match read {
            1 => i64::from(i8::from_ne_bytes([bytes[0]])),
            2 => i64::from(i16::from_ne_bytes([bytes[0], bytes[1]])),
            4 => i64::from(i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
            _ => i64::from_ne_bytes(bytes),
        })
    }

    /// A nullable mask must be byte-per-element NumPy bool, never Arrow bits.
    pub(crate) fn bool_at(&mut self, row: usize, budget: &IoBudget) -> PyResult<bool> {
        if !matches!(self.kind, ArrayKind::Scalar(Kind::Bool))
            || !matches!(self.shape, Shape::Vector { .. })
        {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "NumPy missing-value mask requires a one-dimensional bool array",
            ));
        }
        Ok(self.cell_bytes(row, 0, budget)?[0] != 0)
    }

    /// Validate the current original owners once for one bounded native read.
    /// The returned guard must be dropped before making any Python call.
    fn native_view(&self) -> PyResult<NativeView<'_, 'py>> {
        let py = self.owner.py();
        if !errors::attribute(&self.owner, c"dtype")?.is(&self.dtype) {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "NumPy input dtype changed during escape; keep input storage stable",
            ));
        }
        let (dimensions, lengths) = match self.shape {
            Shape::Vector { length } => (1, [length, 1]),
            Shape::Table { rows, columns } => (2, [rows, columns]),
            Shape::Record { .. } => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "record is not an ndarray",
                ));
            }
        };
        let root = self.allocation.export()?;
        let source = match Export::acquire(&self.owner, pyo3::ffi::PyBUF_STRIDES) {
            Ok(source) => source,
            Err(error) => {
                drop(root);
                return Err(error);
            }
        };
        let raw = &source.raw;
        let length = lengths[0]
            .checked_mul(if dimensions == 1 { 1 } else { lengths[1] })
            .and_then(|n| n.checked_mul(self.width));
        let mut valid = raw.ndim == dimensions as i32
            && raw.itemsize == self.width as isize
            && raw.len >= 0
            && Some(raw.len as usize) == length
            && !raw.buf.is_null()
            && !raw.shape.is_null()
            && !raw.strides.is_null()
            && raw.suboffsets.is_null();
        let mut strides = [0isize; 2];
        if valid {
            for axis in 0..dimensions {
                // SAFETY: a non-NULL shape/stride vector has `raw.ndim`
                // entries, and this loop is bounded by the checked ndim.
                let (length, stride) = unsafe { (*raw.shape.add(axis), *raw.strides.add(axis)) };
                if length < 0 || length as usize != lengths[axis] {
                    valid = false;
                    break;
                }
                strides[axis] = stride;
            }
        }
        if !valid {
            drop(source);
            drop(root);
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "NumPy storage no longer matches its prepared shape or dtype",
            ));
        }
        Ok(NativeView {
            root,
            source,
            dimensions,
            lengths,
            strides,
            width: self.width,
        })
    }

    /// Copy only this bounded native fragment, releasing its export before
    /// writers, signal checks, Python calls, or recursive field processing.
    fn copy_source_range(
        &self,
        row: usize,
        column: usize,
        start: usize,
        destination: &mut [u8],
    ) -> PyResult<()> {
        let py = self.owner.py();
        if start
            .checked_add(destination.len())
            .is_none_or(|end| end > self.width)
        {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy fragment is outside its prepared item width",
            ));
        }
        let view = self.native_view()?;
        if row >= view.lengths[0] || column >= view.lengths[1] {
            drop(view);
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy source coordinate is outside its prepared shape",
            ));
        }
        let physical = view
            .offset(row, column)
            .and_then(|offset| offset.checked_add(start));
        if let Some(offset) = physical.filter(|_| !destination.is_empty()) {
            // SAFETY: the complete selected item lies in the retained root's
            // actual contiguous export. Derive the copied pointer from that
            // physical root, not from an as_strided pointer that could already
            // lie outside an allocation. The fragment lies within the checked
            // item width. No Python call, allocation or check runs in this
            // copy window. Forbidden external native mutation remains subject
            // to the producer's synchronization contract.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    view.root.raw.buf.cast::<u8>().add(offset),
                    destination.as_mut_ptr(),
                    destination.len(),
                );
            }
        }
        drop(view);
        if physical.is_none() {
            return Err(super::allocation::bounds_error(py));
        }
        Ok(())
    }

    /// Copy a bounded raw-value run without executing any future value's
    /// conversion, encoding or Python hook. A future bad coordinate truncates
    /// the run: its error is reported only if that actual cell is requested.
    fn read_block(
        &mut self,
        start: usize,
        total: usize,
        columns: usize,
        budget: &IoBudget,
    ) -> PyResult<()> {
        let py = self.owner.py();
        crate::input::check(py, budget)?;
        // Both callers prove 0 < width <= RAW_BLOCK_BYTES. Primitive widths
        // remain at most eight, retaining their existing 1024-item strategy.
        let count = (total - start)
            .min(RAW_BLOCK_ITEMS)
            .min(RAW_BLOCK_BYTES / self.width);
        let length = count * self.width;
        let mut block = self.raw.take().unwrap_or(RawBlock {
            start,
            bytes: Vec::new(),
        });
        block.bytes.clear();
        block
            .bytes
            .try_reserve_exact(length)
            .map_err(|_| errors::memory_error(py))?;
        let view = self.native_view()?;
        let row = start / columns;
        let column = start % columns;
        let first = view.offset(row, column);
        let mut copied = 0;
        if let Some(offset) = first {
            // A full positive contiguous span needs only one range proof and
            // one native copy. Other layouts retain per-coordinate proofs.
            // SAFETY: offset came from this actual root and names one item.
            let pointer = unsafe { view.root.raw.buf.cast::<u8>().add(offset) };
            if view.contiguous() && view.root.contains(pointer, length) {
                // SAFETY: the complete source span is proven above. Reserve
                // finished before exports were acquired. The destination is
                // unique spare storage for exactly length initialized bytes.
                unsafe { std::ptr::copy_nonoverlapping(pointer, block.bytes.as_mut_ptr(), length) };
                copied = length;
            } else {
                for index in start..start + count {
                    let Some(offset) = view.offset(index / columns, index % columns) else {
                        break;
                    };
                    // SAFETY: each complete item was checked against the
                    // root's actual allocation, even for negative/zero or
                    // transposed strides. Both offsets fit the reserved run;
                    // no callback, allocation or checkpoint occurs here.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            view.root.raw.buf.cast::<u8>().add(offset),
                            block.bytes.as_mut_ptr().add(copied),
                            self.width,
                        );
                    }
                    copied += self.width;
                }
            }
        }
        drop(view);
        if copied == 0 {
            return Err(super::allocation::bounds_error(py));
        }
        // SAFETY: precisely this prefix was initialized by the bounded copies
        // above, and copied never exceeds the reserved target length.
        unsafe { block.bytes.set_len(copied) };
        block.start = start;
        self.raw = Some(block);
        Ok(())
    }

    /// Borrow one complete S/U item from the existing bounded owned run.
    /// No source export survives this call; checkpoints may safely run while
    /// the returned bytes are borrowed. Object slots never enter this path.
    fn owned_cell(
        &mut self,
        row: usize,
        column: usize,
        budget: &IoBudget,
    ) -> PyResult<Option<&[u8]>> {
        let eligible = matches!(self.kind, ArrayKind::Bytes | ArrayKind::Unicode)
            && (1..=RAW_BLOCK_BYTES).contains(&self.width);
        if eligible {
            let (rows, columns) = match self.shape {
                Shape::Vector { length } => (length, 1),
                Shape::Table { rows, columns } => (rows, columns),
                Shape::Record { .. } => (0, 0),
            };
            let total = rows.checked_mul(columns);
            let index = (row < rows && column < columns)
                .then(|| row.checked_mul(columns)?.checked_add(column))
                .flatten();
            if let (Some(total), Some(index)) = (total, index)
                && total >= RAW_BLOCK_MINIMUM
            {
                if !self.raw.as_ref().is_some_and(|block| {
                    index >= block.start && index - block.start < block.bytes.len() / self.width
                }) {
                    self.read_block(index, total, columns, budget)?;
                }
                return Ok(self.raw.as_ref().and_then(|block| {
                    let offset = index.checked_sub(block.start)?.checked_mul(self.width)?;
                    block.bytes.get(offset..offset.checked_add(self.width)?)
                }));
            }
        }
        Ok(None)
    }

    /// Larger and short inputs preserve the original source-fragment reader.
    /// Unicode still consumes bounded copies for its existing scalar fallback.
    fn copy_cell_range(
        &mut self,
        row: usize,
        column: usize,
        start: usize,
        destination: &mut [u8],
        budget: &IoBudget,
    ) -> PyResult<()> {
        if let Some(end) = start.checked_add(destination.len())
            && end <= self.width
            && let Some(cell) = self.owned_cell(row, column, budget)?
            && let Some(fragment) = cell.get(start..end)
        {
            destination.copy_from_slice(fragment);
            return Ok(());
        }
        self.copy_source_range(row, column, start, destination)
    }

    fn element(&self, row: usize, column: usize) -> PyResult<Bound<'py, PyAny>> {
        let py = self.owner.py();
        // A real Python getter needs current source validation even when an
        // earlier S/U snapshot exists. In particular, Unicode's exceptional
        // scalar fallback must not inherit a cached empty-fragment shortcut.
        self.copy_source_range(row, column, 0, &mut [])?;
        let row = crate::input::python_index(py, row)?;
        match self.shape {
            Shape::Vector { .. } => self.owner.get_item(row),
            Shape::Table { .. } => {
                let column = crate::input::python_index(py, column)?;
                self.owner
                    .get_item(crate::output::tuple_from_ready(py, [row, column])?)
            }
            Shape::Record { .. } => Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "record is not an ndarray",
            )),
        }
    }

    /// NumPy removes only terminal all-zero storage padding, preserving NULs
    /// inside the logical element. Unicode padding is counted in UCS4 units.
    fn content_width(&mut self, row: usize, column: usize, budget: &IoBudget) -> PyResult<usize> {
        let py = self.owner.py();
        let mut end = self.width;
        let mut chunk = [0u8; 4096];
        while end > 0 {
            crate::input::check(py, budget)?;
            let start = end.saturating_sub(chunk.len());
            let length = end - start;
            self.copy_cell_range(row, column, start, &mut chunk[..length], budget)?;
            if let Some(last) = chunk[..length].iter().rposition(|byte| *byte != 0) {
                let bytes = start + last + 1;
                return Ok(if matches!(self.kind, ArrayKind::Unicode) {
                    bytes.div_ceil(4) * 4
                } else {
                    bytes
                });
            }
            end = start;
        }
        Ok(0)
    }

    fn write_bytes(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<()> {
        let py = self.owner.py();
        if let Some(cell) = self.owned_cell(row, column, budget)? {
            // Preserve the original reverse padding windows and write checks,
            // but borrow the already owned cell rather than copying it twice.
            let mut length = 0;
            let mut end = cell.len();
            while end > 0 {
                crate::input::check(py, budget)?;
                let start = end.saturating_sub(4096);
                if let Some(last) = cell[start..end].iter().rposition(|byte| *byte != 0) {
                    length = start + last + 1;
                    break;
                }
                end = start;
            }
            writer::binary_start(output, length, context.limit)
                .map_err(|error| errors::wire(py, error))?;
            for fragment in cell[..length].chunks(4096) {
                crate::input::check(py, budget)?;
                writer::binary_fragment(output, fragment, context.limit)
                    .map_err(|error| errors::wire(py, error))?;
            }
            return writer::binary_end(output, context.limit)
                .map_err(|error| errors::wire(py, error));
        }
        let length = self.content_width(row, column, budget)?;
        writer::binary_start(output, length, context.limit)
            .map_err(|error| errors::wire(py, error))?;
        let mut chunk = [0u8; 4096];
        let mut start = 0;
        while start < length {
            crate::input::check(py, budget)?;
            let take = (length - start).min(chunk.len());
            self.copy_cell_range(row, column, start, &mut chunk[..take], budget)?;
            writer::binary_fragment(output, &chunk[..take], context.limit)
                .map_err(|error| errors::wire(py, error))?;
            start += take;
        }
        writer::binary_end(output, context.limit).map_err(|error| errors::wire(py, error))
    }

    fn write_unicode(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<()> {
        let py = self.owner.py();
        let length = self.content_width(row, column, budget)?;
        // Every logical non-padding code point needs at least one output byte
        // for the supported client codecs. Reject this known minimum before
        // allocating the cell's UTF-8 representation.
        let minimum = output
            .len()
            .checked_add(length / 4)
            .and_then(|n| n.checked_add(2));
        if minimum.is_none_or(|minimum| minimum > context.limit) {
            return Err(errors::wire(
                py,
                WireError::Limit {
                    required: minimum.unwrap_or(usize::MAX),
                    limit: context.limit,
                },
            ));
        }
        let mut text = String::new();
        let mut chunk = [0u8; 4096];
        let mut start = 0;
        while start < length {
            crate::input::check(py, budget)?;
            let take = (length - start).min(chunk.len());
            self.copy_cell_range(row, column, start, &mut chunk[..take], budget)?;
            // A UCS4 point requires at most four UTF-8 bytes; this one bounded
            // reservation makes the following pushes allocation-free.
            text.try_reserve(take)
                .map_err(|_| errors::memory_error(py))?;
            for &bytes in chunk[..take].as_chunks::<4>().0 {
                let point = if self.swap {
                    u32::from_ne_bytes(bytes).swap_bytes()
                } else {
                    u32::from_ne_bytes(bytes)
                };
                let Some(character) = char::from_u32(point) else {
                    // NumPy permits UCS4 surrogates. The actual canonical
                    // scalar provides the existing codec's original Python
                    // exception payload only on this exceptional path.
                    let source = self.element(row, column)?;
                    return crate::input::text(
                        source.cast::<PyString>()?,
                        output,
                        context,
                        true,
                        Some(budget),
                    );
                };
                text.push(character);
            }
            start += take;
        }
        crate::input::text_utf8(py, &text, output, context, Some(budget))
    }
}

impl<'py> PreparedSource<'py> for NumpySource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        self.shape
    }

    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        visitor.checkpoint(py, 1)?;
        let kind = match self.kind {
            ArrayKind::Scalar(kind) => kind,
            ArrayKind::Bytes => return self.write_bytes(row, column, output, context, budget),
            ArrayKind::Unicode => return self.write_unicode(row, column, output, context, budget),
            ArrayKind::Object => {
                // The canonical public getter owns its return value. We never
                // retain a raw mutable Object slot or borrowed element across
                // Python recursion; upstream concurrent-write constraints are
                // distinct from this returned-reference ownership guarantee.
                let value = self.element(row, column)?;
                crate::input::check(py, budget)?;
                return visitor.python(&value, output, None);
            }
        };
        let b = self.cell_bytes(row, column, budget)?;
        let result = match kind {
            Kind::I8 => scalar::integer(output, i8::from_ne_bytes([b[0]]), context.limit),
            Kind::I16 => scalar::integer(output, i16::from_ne_bytes([b[0], b[1]]), context.limit),
            Kind::I32 => scalar::integer(
                output,
                i32::from_ne_bytes([b[0], b[1], b[2], b[3]]),
                context.limit,
            ),
            Kind::I64 => scalar::integer(output, i64::from_ne_bytes(b), context.limit),
            Kind::U8 => scalar::integer(output, b[0], context.limit),
            Kind::U16 => scalar::integer(output, u16::from_ne_bytes([b[0], b[1]]), context.limit),
            Kind::U32 => scalar::integer(
                output,
                u32::from_ne_bytes([b[0], b[1], b[2], b[3]]),
                context.limit,
            ),
            Kind::U64 => scalar::integer(output, u64::from_ne_bytes(b), context.limit),
            Kind::F16 => scalar::float(
                output,
                half_to_f64(u16::from_ne_bytes([b[0], b[1]])),
                context.limit,
            ),
            Kind::F32 => scalar::float(
                output,
                f64::from(f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
                context.limit,
            ),
            Kind::F64 => scalar::float(output, f64::from_ne_bytes(b), context.limit),
            Kind::Bool => scalar::integer(output, u8::from(b[0] != 0), context.limit),
            Kind::DateTime | Kind::Duration => {
                let Some((unit, multiplier, kind)) = self.temporal else {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "NumPy temporal metadata is missing",
                    ));
                };
                return temporal::write(
                    py,
                    i64::from_ne_bytes(b),
                    unit,
                    multiplier,
                    kind,
                    output,
                    context,
                    Some(budget),
                );
            }
        };
        result.map_err(|error| errors::wire(py, error))
    }
}
