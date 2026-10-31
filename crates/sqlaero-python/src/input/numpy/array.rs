//! Prepared exact ndarray coordinates with short, reacquired native exports.
//!
//! NumPy requires callers to keep shared storage stable. A private view is used
//! only while preparing metadata. Every later access reacquires the original
//! source because even a retained view/export can be invalidated by setstate.
//! Weak references guard normal resize, not arbitrary external NumPy/C writers.

use super::allocation::Allocation;
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
use std::mem::MaybeUninit;

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
        })
    }

    fn cell_bytes(&self, row: usize, column: usize) -> PyResult<[u8; 8]> {
        let mut bytes = [0u8; 8];
        self.copy_cell_range(row, column, 0, &mut bytes[..self.width])?;
        if self.swap {
            bytes[..self.width].reverse();
        }
        Ok(bytes)
    }

    /// Read a signed native mask/code/time value without Python number boxing.
    pub(crate) fn signed_at(&self, row: usize) -> PyResult<i64> {
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
        let bytes = self.cell_bytes(row, 0)?;
        Ok(match read {
            1 => i64::from(i8::from_ne_bytes([bytes[0]])),
            2 => i64::from(i16::from_ne_bytes([bytes[0], bytes[1]])),
            4 => i64::from(i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
            _ => i64::from_ne_bytes(bytes),
        })
    }

    /// A nullable mask must be byte-per-element NumPy bool, never Arrow bits.
    pub(crate) fn bool_at(&self, row: usize) -> PyResult<bool> {
        if !matches!(self.kind, ArrayKind::Scalar(Kind::Bool))
            || !matches!(self.shape, Shape::Vector { .. })
        {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "NumPy missing-value mask requires a one-dimensional bool array",
            ));
        }
        Ok(self.cell_bytes(row, 0)?[0] != 0)
    }

    /// Copy only this bounded native fragment, releasing its export before
    /// writers, signal checks, Python calls, or recursive field processing.
    fn copy_cell_range(
        &self,
        row: usize,
        column: usize,
        start: usize,
        destination: &mut [u8],
    ) -> PyResult<()> {
        let py = self.owner.py();
        if !errors::attribute(&self.owner, c"dtype")?.is(&self.dtype) {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "NumPy input dtype changed during escape; keep input storage stable",
            ));
        }
        if start
            .checked_add(destination.len())
            .is_none_or(|end| end > self.width)
        {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy fragment is outside its prepared item width",
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
        if row >= lengths[0] || column >= lengths[1] {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "NumPy source coordinate is outside its prepared shape",
            ));
        }
        let root = self.allocation.export()?;
        let mut raw = MaybeUninit::<pyo3::ffi::Py_buffer>::uninit();
        // SAFETY: exact NumPy exports populate this owned stack descriptor or
        // preserve their actual Python error; this thread remains attached.
        if unsafe {
            pyo3::ffi::PyObject_GetBuffer(
                self.owner.as_ptr(),
                raw.as_mut_ptr(),
                pyo3::ffi::PyBUF_STRIDES,
            )
        } < 0
        {
            let error = PyErr::fetch(py);
            drop(root);
            return Err(error);
        }
        // SAFETY: the successful constructor initialized the full descriptor.
        let mut raw = unsafe { raw.assume_init() };
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
        let mut offset = 0isize;
        if valid {
            for (axis, coordinate) in [row, column].into_iter().enumerate().take(dimensions) {
                // SAFETY: a non-NULL shape/stride vector has `raw.ndim`
                // entries, and this loop is bounded by the checked ndim.
                let (length, stride) = unsafe { (*raw.shape.add(axis), *raw.strides.add(axis)) };
                if length < 0 || length as usize != lengths[axis] {
                    valid = false;
                    break;
                }
                let next = isize::try_from(coordinate)
                    .ok()
                    .and_then(|coordinate| coordinate.checked_mul(stride))
                    .and_then(|delta| offset.checked_add(delta));
                if let Some(next) = next {
                    offset = next;
                } else {
                    valid = false;
                    break;
                }
            }
        }
        let metadata_valid = valid;
        let physical = if valid {
            (raw.buf as usize)
                .checked_add_signed(offset)
                .filter(|address| root.contains(*address as *const u8, self.width))
                .and_then(|address| address.checked_sub(root.raw.buf as usize))
                .and_then(|offset| offset.checked_add(start))
        } else {
            None
        };
        valid = physical.is_some();
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
                    root.raw.buf.cast::<u8>().add(offset),
                    destination.as_mut_ptr(),
                    destination.len(),
                );
            }
        }
        // SAFETY: this descriptor came from one successful export and is
        // released exactly once while attached. No borrowed bytes survive.
        unsafe { pyo3::ffi::PyBuffer_Release(&mut raw) };
        drop(root);
        if !valid {
            if metadata_valid {
                return Err(super::allocation::bounds_error(py));
            }
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "NumPy storage no longer matches its prepared shape or dtype",
            ));
        }
        Ok(())
    }

    fn element(&self, row: usize, column: usize) -> PyResult<Bound<'py, PyAny>> {
        let py = self.owner.py();
        self.copy_cell_range(row, column, 0, &mut [])?;
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
    fn content_width(&self, row: usize, column: usize, budget: &IoBudget) -> PyResult<usize> {
        let py = self.owner.py();
        let mut end = self.width;
        let mut chunk = [0u8; 4096];
        while end > 0 {
            crate::input::check(py, budget)?;
            let start = end.saturating_sub(chunk.len());
            let length = end - start;
            self.copy_cell_range(row, column, start, &mut chunk[..length])?;
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
        &self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<()> {
        let py = self.owner.py();
        let length = self.content_width(row, column, budget)?;
        writer::binary_start(output, length, context.limit)
            .map_err(|error| errors::wire(py, error))?;
        let mut chunk = [0u8; 4096];
        let mut start = 0;
        while start < length {
            crate::input::check(py, budget)?;
            let take = (length - start).min(chunk.len());
            self.copy_cell_range(row, column, start, &mut chunk[..take])?;
            writer::binary_fragment(output, &chunk[..take], context.limit)
                .map_err(|error| errors::wire(py, error))?;
            start += take;
        }
        writer::binary_end(output, context.limit).map_err(|error| errors::wire(py, error))
    }

    fn write_unicode(
        &self,
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
            self.copy_cell_range(row, column, start, &mut chunk[..take])?;
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
        let b = self.cell_bytes(row, column)?;
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
