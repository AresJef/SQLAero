//! Prepared exact ndarray coordinates with short, reacquired native exports.
//!
//! NumPy requires callers to keep shared storage stable. Canonical public buffer
//! slots prepare metadata. Each new native read reacquires the original
//! source because even a retained view/export can be invalidated by setstate.
//! Fixed-width values can reuse an already copied, bounded Rust-owned block;
//! such bytes no longer borrow mutable NumPy memory.
//! Weak references guard normal resize, not arbitrary external NumPy/C writers.

use super::allocation::{Allocation, Export};
use super::{Kind, NumpyArrayApi, half_to_f64};
use crate::errors;
use crate::input::attribute::GetSet;
use crate::input::buffer::Canonical;
use crate::input::cell::{Schema, Target};
use crate::input::control::Checkpoint;
use crate::input::source::{PreparedSource, Shape};
use pyo3::exceptions::{PyBufferError, PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_transcode::temporal::{TimeKind, TimeUnit};
use sqlaero_transcode::value::ScalarRef;

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

impl RawBlock {
    /// Resolve an owned item with one checked byte-range contract. This avoids
    /// dividing the run length by a dynamic width merely to repeat the lookup.
    fn item(&self, index: usize, width: usize) -> Option<&[u8]> {
        let offset = index.checked_sub(self.start)?.checked_mul(width)?;
        self.bytes.get(offset..offset.checked_add(width)?)
    }

    fn copy_item(&self, index: usize, output: &mut [u8]) -> bool {
        // Four-byte machine words share this checked copy in both writers.
        // Other widths keep the general path and its original bounds checks.
        if output.len() == 4 {
            let Some(item) = self.item(index, 4).and_then(|item| item.first_chunk::<4>()) else {
                return false;
            };
            let Some(destination) = output.first_chunk_mut::<4>() else {
                return false;
            };
            *destination = *item;
            return true;
        }
        if let Some(item) = self.item(index, output.len()) {
            output.copy_from_slice(item);
            true
        } else {
            false
        }
    }

    /// A checked, already owned native-endian signed 64-bit word.
    fn signed64(&self, index: usize) -> Option<i64> {
        Some(i64::from_ne_bytes(self.item(index, 8)?.try_into().ok()?))
    }

    /// A checked, already owned native-endian floating-point word.
    fn float(&self, index: usize, kind: Kind) -> Option<f64> {
        match kind {
            #[cfg(Py_GIL_DISABLED)]
            Kind::F16 => Some(half_to_f64(u16::from_ne_bytes(
                self.item(index, 2)?.try_into().ok()?,
            ))),
            Kind::F32 => Some(f64::from(f32::from_ne_bytes(
                self.item(index, 4)?.try_into().ok()?,
            ))),
            Kind::F64 => Some(f64::from_ne_bytes(self.item(index, 8)?.try_into().ok()?)),
            _ => None,
        }
    }
}

/// Keep reusable Unicode capacity behind a small, lazily allocated handle.
struct UnicodeScratch {
    text: String,
}

/// Only the copying window owns these fresh exports. Prepared axis lengths and
/// current public strides are checked before any source address is formed;
/// no Python call, allocation or checkpoint occurs while reading their bytes.
struct NativeView<'owner, 'py> {
    // Fields drop in declaration order: end the derived export first.
    source: Export<'owner, 'py>,
    root: Export<'owner, 'py>,
    dimensions: usize,
    lengths: [usize; 2],
    strides: [isize; 2],
    tensor_axes: &'owner [usize],
    width: usize,
}

impl NativeView<'_, '_> {
    fn offset(&self, row: usize, column: usize) -> Option<usize> {
        if row >= self.lengths[0] || column >= self.lengths[1] {
            return None;
        }
        let mut offset = 0isize;
        if self.dimensions > 2 {
            let mut remainder = row;
            for axis in (0..self.dimensions).rev() {
                let length = self.tensor_axes[axis];
                if length == 0 {
                    return None;
                }
                let coordinate = remainder % length;
                remainder /= length;
                // SAFETY: native_view checked rank and every axis length;
                // this public stride vector remains live inside its export.
                let stride = unsafe { *self.source.raw.strides.add(axis) };
                offset =
                    offset.checked_add(isize::try_from(coordinate).ok()?.checked_mul(stride)?)?;
            }
        } else {
            for (axis, coordinate) in [row, column].into_iter().enumerate().take(self.dimensions) {
                let delta = isize::try_from(coordinate)
                    .ok()?
                    .checked_mul(self.strides[axis])?;
                offset = offset.checked_add(delta)?;
            }
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
        if self.dimensions > 2 {
            let mut expected = width;
            for axis in (0..self.dimensions).rev() {
                // SAFETY: the complete checked metadata stays exported.
                let stride = unsafe { *self.source.raw.strides.add(axis) };
                let length = self.tensor_axes[axis];
                if length > 1 && stride != expected {
                    return false;
                }
                let Some(next) = isize::try_from(length)
                    .ok()
                    .and_then(|n| expected.checked_mul(n))
                else {
                    return false;
                };
                expected = next;
            }
            true
        } else if self.dimensions == 1 {
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
    RecordLayout,
}

/// Recognize a fixed-width numeric field independently of its containing shape.
pub(super) fn fixed_kind(tag: &str, width: usize) -> Option<Kind> {
    match (tag, width) {
        ("i", 1) => Some(Kind::I8),
        ("i", 2) => Some(Kind::I16),
        ("i", 4) => Some(Kind::I32),
        ("i", 8) => Some(Kind::I64),
        ("u", 1) => Some(Kind::U8),
        ("u", 2) => Some(Kind::U16),
        ("u", 4) => Some(Kind::U32),
        ("u", 8) => Some(Kind::U64),
        ("f", 2) => Some(Kind::F16),
        ("f", 4) => Some(Kind::F32),
        ("f", 8) => Some(Kind::F64),
        ("b", 1) => Some(Kind::Bool),
        _ => None,
    }
}

/// Interpret one already owned canonical fixed-width number.
pub(super) fn fixed_number(kind: Kind, b: [u8; 8]) -> Option<ScalarRef<'static>> {
    Some(match kind {
        Kind::I8 => ScalarRef::Signed(i64::from(i8::from_ne_bytes([b[0]]))),
        Kind::I16 => ScalarRef::Signed(i64::from(i16::from_ne_bytes([b[0], b[1]]))),
        Kind::I32 => ScalarRef::Signed(i64::from(i32::from_ne_bytes([b[0], b[1], b[2], b[3]]))),
        Kind::I64 => ScalarRef::Signed(i64::from_ne_bytes(b)),
        Kind::U8 => ScalarRef::Unsigned(u64::from(b[0])),
        Kind::U16 => ScalarRef::Unsigned(u64::from(u16::from_ne_bytes([b[0], b[1]]))),
        Kind::U32 => ScalarRef::Unsigned(u64::from(u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))),
        Kind::U64 => ScalarRef::Unsigned(u64::from_ne_bytes(b)),
        Kind::F16 => ScalarRef::Float(half_to_f64(u16::from_ne_bytes([b[0], b[1]]))),
        Kind::F32 => ScalarRef::Float(f64::from(f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))),
        Kind::F64 => ScalarRef::Float(f64::from_ne_bytes(b)),
        Kind::Bool => ScalarRef::Boolean(b[0] != 0),
        Kind::DateTime | Kind::Duration => return None,
    })
}

fn dtype_kind(py: Python<'_>, tag: &str, width: usize, record: bool) -> PyResult<ArrayKind> {
    if let Some(kind) = fixed_kind(tag, width) {
        return Ok(ArrayKind::Scalar(kind));
    }
    Ok(match (tag, width) {
        ("M", 8) => ArrayKind::Scalar(Kind::DateTime),
        ("m", 8) => ArrayKind::Scalar(Kind::Duration),
        ("S", _) => ArrayKind::Bytes,
        ("U", width) if width.is_multiple_of(4) => ArrayKind::Unicode,
        ("O", width) if width == std::mem::size_of::<*mut pyo3::ffi::PyObject>() => {
            ArrayKind::Object
        }
        ("V", _) if record => ArrayKind::RecordLayout,
        _ => {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NumPy dtype is outside the supported fixed-width, text, binary and Object kinds",
            ));
        }
    })
}

pub(super) fn dtype_schema(
    dtype: &Bound<'_, PyAny>,
    budget: &dyn Checkpoint,
) -> PyResult<Option<Schema>> {
    let py = dtype.py();
    let width = super::super::control::callback(py, Some(budget), || {
        errors::attribute(dtype, c"itemsize")?.extract::<usize>()
    })?;
    let tag =
        super::super::control::callback(py, Some(budget), || errors::attribute(dtype, c"kind"))?;
    Ok(matches!(
        dtype_kind(py, tag.extract::<&str>()?, width, false)?,
        ArrayKind::Bytes
    )
    .then_some(Schema::Binary))
}

/// Original identity, prepared metadata and necessary normal-resize guards.
pub(crate) struct NumpySource<'py> {
    owner: Bound<'py, PyAny>,
    dtype: Bound<'py, PyAny>,
    dtype_get: GetSet,
    array: Canonical,
    item_get: pyo3::ffi::binaryfunc,
    allocation: Allocation<'py>,
    shape: Shape,
    // Ordinary0/1/2D sources keep an empty Vec without a metadata allocation.
    tensor_axes: Vec<usize>,
    kind: ArrayKind,
    width: usize,
    swap: bool,
    temporal: Option<(TimeUnit, u64, TimeKind)>,
    temporal_reader: Option<Py<PyAny>>,
    raw: Option<RawBlock>,
    // A single small handle; only eligible Unicode writers allocate storage.
    utf8: Option<Box<UnicodeScratch>>,
}

impl<'py> NumpySource<'py> {
    pub(super) fn new(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        Self::prepare(value, api, budget, false)
    }

    pub(super) fn record_layout(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        Self::prepare(value, api, budget, true)
    }

    fn prepare(
        value: &Bound<'py, PyAny>,
        api: &NumpyArrayApi,
        budget: &dyn Checkpoint,
        record: bool,
    ) -> PyResult<Self> {
        let py = value.py();
        let allocation = Allocation::new(value, api, budget)?;
        let dtype = api.dtype.get(value)?;
        if !api.supports_dtype(&dtype) {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "NumPy dtype is outside the explicitly supported native source classes",
            ));
        }
        // Only bounded metadata is copied from the canonical public export.
        // No payload, allocation, Python call or checkpoint occurs in this
        // attached window. The guard is released before reporting any error.
        let export = api.array.acquire(value, pyo3::ffi::PyBUF_STRIDES)?;
        let raw = &export.raw;
        let dimensions = usize::try_from(raw.ndim).ok();
        let width = usize::try_from(raw.itemsize).ok();
        let mut lengths = [1usize; 2];
        let mut extra = [std::mem::MaybeUninit::<usize>::uninit(); 64];
        let mut elements = Some(1usize);
        let mut zero = false;
        let mut valid = dimensions.is_some_and(|d| d <= extra.len())
            && width.is_some()
            && raw.len >= 0
            && raw.suboffsets.is_null();
        if valid && let Some(dimensions) = dimensions {
            valid = dimensions == 0 || (!raw.shape.is_null() && !raw.strides.is_null());
            if valid {
                for axis in 0..dimensions {
                    // SAFETY: checked ndim is at most64 and the public shape
                    // vector has that many entries, retained by this export.
                    let actual = unsafe { *raw.shape.add(axis) };
                    if let Ok(actual) = usize::try_from(actual) {
                        if dimensions <= 2 {
                            lengths[axis] = actual;
                        } else {
                            extra[axis].write(actual);
                        }
                        zero |= actual == 0;
                        elements = elements.and_then(|n| n.checked_mul(actual));
                    } else {
                        valid = false;
                    }
                }
                if zero {
                    elements = Some(0);
                }
                valid &= elements.and_then(|n| n.checked_mul(width.unwrap_or(0)))
                    == Some(raw.len as usize);
            }
        }
        drop(export);
        crate::input::check(py, budget)?;
        if !valid {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "NumPy storage requires valid native dimensions, lengths and byte width",
            ));
        }
        let dimensions = dimensions.unwrap_or(0);
        let width = width.unwrap_or(0);
        let mut tensor_axes = Vec::new();
        let shape = match dimensions {
            0 => Shape::Scalar,
            1 => Shape::Vector { length: lengths[0] },
            2 => Shape::Table {
                rows: lengths[0],
                columns: lengths[1],
            },
            _ => {
                tensor_axes
                    .try_reserve_exact(dimensions)
                    .map_err(|_| errors::memory_error(py))?;
                for axis in extra.iter().take(dimensions) {
                    // SAFETY: valid proves every actual axis was initialized
                    // above, and the raw export ended before this allocation.
                    tensor_axes.push(unsafe { axis.assume_init() });
                }
                Shape::Tensor {
                    dimensions,
                    elements: elements.unwrap_or(0),
                }
            }
        };
        let tag = errors::attribute(&dtype, c"kind")?;
        let tag = tag.extract::<&str>()?;
        let kind = dtype_kind(py, tag, width, record)?;
        // A structural layout never reads numeric payload bytes. Its leaf
        // readers prepare their own byte order; do not rescan a compound dtype.
        let native_order = record || errors::attribute(&dtype, c"isnative")?.extract::<bool>()?;
        let temporal_reader = matches!(kind, ArrayKind::Scalar(Kind::DateTime | Kind::Duration))
            .then(|| api.datetime_data.clone_ref(py));
        Ok(Self {
            owner: value.clone(),
            dtype_get: api.dtype.clone_ref(py),
            array: api.array.clone_ref(py),
            item_get: api.item,
            dtype,
            allocation,
            shape,
            tensor_axes,
            kind,
            width,
            swap: !native_order,
            temporal: None,
            temporal_reader,
            raw: None,
            utf8: None,
        })
    }

    /// Validate only physical source facts. No payload is read, no field is
    /// boxed, and all public exports end before target or protocol work.
    pub(super) fn validate_record_cell(
        &self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        crate::input::buffer::read(
            self.owner.py(),
            budget,
            self.allocation.native_root(),
            || {
                let view = self.native_view()?;
                let valid = view.offset(row, column).is_some();
                drop(view);
                if valid {
                    Ok(())
                } else {
                    Err(super::allocation::bounds_error(self.owner.py()))
                }
            },
        )
    }

    /// Masked payload is not read, but its prepared ownership and metadata
    /// must still be valid before a missing fact can be consumed.
    pub(crate) fn validate_layout(&self, budget: &dyn Checkpoint) -> PyResult<()> {
        crate::input::buffer::read(
            self.owner.py(),
            budget,
            self.allocation.native_root(),
            || {
                if self.allocation.is_source_root(&self.owner) {
                    let (root, dimensions, lengths) = self.checked_root()?;
                    // ANY_CONTIGUOUS already includes STRIDES. This is only a
                    // layout check, so reuse the exact owner's export without
                    // touching any bytes beneath a missing payload.
                    let valid = self.checked_strides(&root, dimensions, lengths).is_some();
                    drop(root);
                    return if valid {
                        Ok(())
                    } else {
                        Err(self.changed_layout())
                    };
                }
                drop(self.native_view()?);
                Ok(())
            },
        )
    }

    pub(super) fn is_boolean(&self) -> bool {
        matches!(self.kind, ArrayKind::Scalar(Kind::Bool))
    }

    /// Read only an already owned bool byte, without entering a source or
    /// control boundary. A miss keeps the normal guarded reader unchanged.
    pub(super) fn copied_bool(&self, row: usize, column: usize) -> Option<bool> {
        if !self.is_boolean() || self.width != 1 {
            return None;
        }
        let (rows, columns) = match self.shape {
            Shape::Scalar => (1, 1),
            Shape::Vector { length } => (length, 1),
            Shape::Table { rows, columns } => (rows, columns),
            Shape::Tensor { elements, .. } => (elements, 1),
            Shape::Record { .. } => return None,
        };
        if row >= rows || column >= columns {
            return None;
        }
        let index = row.checked_mul(columns)?.checked_add(column)?;
        self.raw
            .as_ref()?
            .item(index, 1)?
            .first()
            .map(|&byte| byte != 0)
    }

    pub(super) fn bool_cell(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        if !self.is_boolean() {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "NumPy mask requires actual bool storage",
            ));
        }
        Ok(self.cell_bytes(row, column, budget)?[0] != 0)
    }

    /// Finite unit facts are prepared once, after actual NaT recognition. The
    /// protocol cannot conceal a retired operation or a changed source layout.
    fn finite_temporal(&mut self, budget: &dyn Checkpoint) -> PyResult<(TimeUnit, u64, TimeKind)> {
        if let Some(prepared) = self.temporal {
            return Ok(prepared);
        }
        let py = self.owner.py();
        let kind = match self.kind {
            ArrayKind::Scalar(Kind::DateTime) => TimeKind::DateTime,
            ArrayKind::Scalar(Kind::Duration) => TimeKind::Duration,
            _ => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "NumPy source has no temporal unit schema",
                ));
            }
        };
        let reader = self.temporal_reader.as_ref().ok_or_else(|| {
            errors::builtin_message::<PyTypeError>(
                py,
                "NumPy finite temporal metadata reader is missing",
            )
        })?;
        let (unit, multiplier) = super::temporal_dtype_metadata(&self.dtype, reader, Some(budget))?;
        crate::input::buffer::read(py, budget, self.allocation.native_root(), || {
            // Validate again after the actual protocol. No source export is
            // retained through it or through the following Control boundary.
            drop(self.native_view()?);
            Ok(())
        })?;
        let facts = (unit, multiplier, kind);
        self.temporal = Some(facts);
        Ok(facts)
    }

    fn cell_bytes(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<[u8; 8]> {
        let mut bytes = [0u8; 8];
        let (rows, columns) = match self.shape {
            Shape::Scalar => (1, 1),
            Shape::Vector { length } => (length, 1),
            Shape::Table { rows, columns } => (rows, columns),
            Shape::Tensor { elements, .. } => (elements, 1),
            Shape::Record { .. } => (0, 0),
        };
        let total = rows.checked_mul(columns);
        let index = (row < rows && column < columns)
            .then(|| row.checked_mul(columns)?.checked_add(column))
            .flatten();
        if let (Some(total), Some(index)) = (total, index)
            && total >= RAW_BLOCK_MINIMUM
        {
            if !self
                .raw
                .as_ref()
                .is_some_and(|block| block.copy_item(index, &mut bytes[..self.width]))
            {
                self.read_block(index, total, columns, budget)?;
                if !self
                    .raw
                    .as_ref()
                    .is_some_and(|block| block.copy_item(index, &mut bytes[..self.width]))
                {
                    self.copy_source_range(row, column, 0, &mut bytes[..self.width], budget)?;
                }
            }
        } else {
            self.copy_source_range(row, column, 0, &mut bytes[..self.width], budget)?;
        }
        if self.swap {
            bytes[..self.width].reverse();
        }
        Ok(bytes)
    }

    /// Read a signed native mask/code/time value without Python number boxing.
    pub(crate) fn signed_at(&mut self, row: usize, budget: &dyn Checkpoint) -> PyResult<i64> {
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

    /// Copy one native scalar fact without constructing a Python numeric
    /// object. Non-scalar storage remains with its existing immediate reader.
    pub(crate) fn scalar_at(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<ScalarRef<'static>>> {
        let ArrayKind::Scalar(kind) = self.kind else {
            return Ok(None);
        };
        let bytes = self.cell_bytes(row, column, budget)?;
        self.scalar_fact(kind, bytes, budget).map(Some)
    }

    fn scalar_fact(
        &mut self,
        kind: Kind,
        b: [u8; 8],
        budget: &dyn Checkpoint,
    ) -> PyResult<ScalarRef<'static>> {
        if let Some(value) = fixed_number(kind, b) {
            return Ok(value);
        }
        self.temporal_fact(i64::from_ne_bytes(b), budget)
    }

    /// Read exact temporal storage and its qualified metadata. The shared NaT
    /// check stays before finite-unit acquisition for every consuming provider.
    pub(crate) fn temporal_at(
        &mut self,
        row: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<ScalarRef<'static>> {
        if !matches!(
            self.kind,
            ArrayKind::Scalar(Kind::DateTime | Kind::Duration)
        ) {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "Pandas temporal storage requires actual datetime64 or timedelta64 data",
            ));
        }
        let bytes = self.cell_bytes(row, 0, budget)?;
        self.temporal_fact(i64::from_ne_bytes(bytes), budget)
    }

    fn temporal_fact(
        &mut self,
        value: i64,
        budget: &dyn Checkpoint,
    ) -> PyResult<ScalarRef<'static>> {
        if value == i64::MIN {
            return Ok(ScalarRef::Null);
        }
        let (unit, multiplier, kind) = self.finite_temporal(budget)?;
        if multiplier == 0 {
            return Err(errors::builtin_message::<PyTypeError>(
                self.owner.py(),
                "NumPy datetime unit multiplier must be positive",
            ));
        }
        Ok(ScalarRef::Ticks {
            value,
            unit,
            multiplier,
            kind,
        })
    }

    /// Validate the current original owners once for one bounded native read.
    /// The returned guard must be dropped before making any Python call.
    fn native_view(&self) -> PyResult<NativeView<'_, 'py>> {
        let (root, dimensions, lengths) = self.checked_root()?;
        let source = self.array.acquire(&self.owner, pyo3::ffi::PyBUF_STRIDES)?;
        let Some(strides) = self.checked_strides(&source, dimensions, lengths) else {
            drop(source);
            drop(root);
            return Err(self.changed_layout());
        };
        Ok(NativeView {
            root,
            source,
            dimensions,
            lengths,
            strides,
            tensor_axes: &self.tensor_axes,
            width: self.width,
        })
    }

    fn checked_root(&self) -> PyResult<(Export<'_, 'py>, usize, [usize; 2])> {
        let py = self.owner.py();
        if !self.dtype_get.get(&self.owner)?.is(&self.dtype) {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "NumPy input dtype changed during escape; keep input storage stable",
            ));
        }
        let (dimensions, lengths) = match self.shape {
            Shape::Scalar => (0, [1, 1]),
            Shape::Vector { length } => (1, [length, 1]),
            Shape::Table { rows, columns } => (2, [rows, columns]),
            Shape::Tensor {
                dimensions,
                elements,
            } => (dimensions, [elements, 1]),
            Shape::Record { .. } => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "record is not an ndarray",
                ));
            }
        };
        let root = self.allocation.export()?;
        let current_dtype = self.dtype_get.get(&self.owner)?;
        if !current_dtype.is(&self.dtype) {
            drop(root);
            return Err(super::allocation::bounds_error(py));
        }
        Ok((root, dimensions, lengths))
    }

    /// Check the live canonical buffer's axes without reading its payload.
    fn checked_strides(
        &self,
        source: &Export<'_, 'py>,
        dimensions: usize,
        lengths: [usize; 2],
    ) -> Option<[isize; 2]> {
        let raw = &source.raw;
        let length = lengths[0]
            .checked_mul(if dimensions == 1 { 1 } else { lengths[1] })
            .and_then(|n| n.checked_mul(self.width));
        let mut valid = raw.ndim == dimensions as i32
            && raw.itemsize == self.width as isize
            && raw.len >= 0
            && Some(raw.len as usize) == length
            && !raw.buf.is_null()
            && (dimensions == 0 || (!raw.shape.is_null() && !raw.strides.is_null()))
            && raw.suboffsets.is_null();
        let mut strides = [0isize; 2];
        if valid {
            for axis in 0..dimensions {
                // SAFETY: a non-NULL shape/stride vector has `raw.ndim`
                // entries, and this loop is bounded by the checked ndim.
                let (length, stride) = unsafe { (*raw.shape.add(axis), *raw.strides.add(axis)) };
                let expected = if dimensions > 2 {
                    self.tensor_axes[axis]
                } else {
                    lengths[axis]
                };
                if length < 0 || length as usize != expected {
                    valid = false;
                    break;
                }
                if dimensions <= 2 {
                    strides[axis] = stride;
                }
            }
        }
        valid.then_some(strides)
    }

    fn changed_layout(&self) -> PyErr {
        errors::builtin_message::<PyBufferError>(
            self.owner.py(),
            "NumPy storage no longer matches its prepared shape or dtype",
        )
    }

    /// Copy only this bounded native fragment, releasing its export before
    /// writers, signal checks, Python calls, or recursive field processing.
    pub(super) fn copy_source_range(
        &self,
        row: usize,
        column: usize,
        start: usize,
        destination: &mut [u8],
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        crate::input::buffer::read(
            self.owner.py(),
            budget,
            self.allocation.native_root(),
            || self.copy_range_window(row, column, start, destination),
        )
    }

    fn copy_range_window(
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
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        crate::input::buffer::read(
            self.owner.py(),
            budget,
            self.allocation.native_root(),
            || self.read_block_window(start, total, columns, budget),
        )
    }

    fn read_block_window(
        &mut self,
        start: usize,
        total: usize,
        columns: usize,
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<&[u8]>> {
        let eligible = matches!(self.kind, ArrayKind::Bytes | ArrayKind::Unicode)
            && (1..=RAW_BLOCK_BYTES).contains(&self.width);
        if eligible {
            let (rows, columns) = match self.shape {
                Shape::Scalar => (1, 1),
                Shape::Vector { length } => (length, 1),
                Shape::Table { rows, columns } => (rows, columns),
                Shape::Tensor { elements, .. } => (elements, 1),
                Shape::Record { .. } => (0, 0),
            };
            let total = rows.checked_mul(columns);
            let index = (row < rows && column < columns)
                .then(|| row.checked_mul(columns)?.checked_add(column))
                .flatten();
            if let (Some(total), Some(index)) = (total, index)
                && total >= RAW_BLOCK_MINIMUM
            {
                if !self
                    .raw
                    .as_ref()
                    .is_some_and(|block| block.item(index, self.width).is_some())
                {
                    self.read_block(index, total, columns, budget)?;
                }
                return Ok(self
                    .raw
                    .as_ref()
                    .and_then(|block| block.item(index, self.width)));
            }
        }
        Ok(None)
    }

    /// Larger and short inputs preserve the original source-fragment reader.
    /// Writers without an owned-cell fast path consume bounded source copies.
    fn copy_cell_range(
        &mut self,
        row: usize,
        column: usize,
        start: usize,
        destination: &mut [u8],
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if let Some(end) = start.checked_add(destination.len())
            && end <= self.width
            && let Some(cell) = self.owned_cell(row, column, budget)?
            && let Some(fragment) = cell.get(start..end)
        {
            destination.copy_from_slice(fragment);
            return Ok(());
        }
        self.copy_source_range(row, column, start, destination, budget)
    }

    pub(crate) fn is_object(&self) -> bool {
        matches!(self.kind, ArrayKind::Object)
    }

    pub(crate) fn object_child(
        &self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if self.is_object() {
            self.element(row, column, budget).map(Some)
        } else {
            Ok(None)
        }
    }

    fn element(
        &self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.owner.py();
        let key = match self.shape {
            Shape::Scalar => crate::output::tuple_from_ready(py, [])?.into_any(),
            Shape::Vector { .. } => crate::input::python_index(py, row)?,
            Shape::Table { .. } => {
                let row = crate::input::python_index(py, row)?;
                let column = crate::input::python_index(py, column)?;
                crate::output::tuple_from_ready(py, [row, column])?.into_any()
            }
            Shape::Tensor {
                dimensions,
                elements,
            } => {
                if row >= elements || column != 0 {
                    return Err(errors::builtin_message::<PyIndexError>(
                        py,
                        "NumPy flat coordinate is outside its prepared axes",
                    ));
                }
                let mut coordinates = Vec::new();
                coordinates
                    .try_reserve_exact(dimensions)
                    .map_err(|_| errors::memory_error(py))?;
                let mut divisor = elements;
                let mut remainder = row;
                for &length in &self.tensor_axes {
                    divisor /= length;
                    let coordinate = remainder / divisor;
                    remainder %= divisor;
                    coordinates.push(crate::input::python_index(py, coordinate)?.unbind());
                }
                crate::output::tuple_from_ready_vec(py, coordinates)?.into_any()
            }
            Shape::Record { .. } => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "record is not an ndarray",
                ));
            }
        };
        crate::input::buffer::read(py, budget, self.allocation.native_root(), || {
            let view = self.native_view()?;
            if view.offset(row, column).is_none() {
                drop(view);
                return Err(super::allocation::bounds_error(py));
            }
            // Only the qualified native Object scalar getter uses this path.
            // It obtains an owning reference without invoking Python. Keep the
            // proven physical export until that reference has been acquired;
            // source and root exports then end before any Control callback.
            let result = self.get_item(&key);
            drop(view);
            result
        })
    }

    fn unicode_source(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Bound<'py, PyString>> {
        let py = self.owner.py();
        let count = self.content_width(row, column, budget)? / 4;
        crate::input::unicode::from_ucs4(py, count, self.swap, budget, |start, bytes| {
            self.copy_cell_range(row, column, start * 4, bytes, budget)
        })
    }

    fn get_item(&self, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        // SAFETY: the retained immutable ndarray base owns this public mapping
        // slot. Original source qualification and full scalar coordinates are
        // established before this call; it returns an owning reference or its
        // original Python exception, bypassing a subclass __getitem__.
        unsafe {
            Bound::from_owned_ptr_or_err(
                self.owner.py(),
                (self.item_get)(self.owner.as_ptr(), key.as_ptr()),
            )
        }
    }

    /// NumPy removes only terminal all-zero storage padding, preserving NULs
    /// inside the logical element. Unicode padding is counted in UCS4 units.
    fn content_width(
        &mut self,
        row: usize,
        column: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<usize> {
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
        output: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        if let Some(cell) = self.owned_cell(row, column, budget)? {
            // Preserve the original reverse padding windows and write checks,
            // but borrow the already owned cell rather than copying it twice.
            let length = if let Ok(bytes) = <[u8; 16]>::try_from(cell) {
                crate::input::check(py, budget)?;
                // Little-endian assembly puts the last nonzero byte in
                // the highest occupied position, regardless of host order.
                (u128::BITS - u128::from_le_bytes(bytes).leading_zeros()).div_ceil(8) as usize
            } else {
                let mut end = cell.len();
                loop {
                    if end == 0 {
                        break 0;
                    }
                    crate::input::check(py, budget)?;
                    let start = end.saturating_sub(4096);
                    if let Some(last) = cell[start..end].iter().rposition(|byte| *byte != 0) {
                        break start + last + 1;
                    }
                    end = start;
                }
            };
            output.bytes_start(py, length)?;
            for fragment in cell[..length].chunks(4096) {
                crate::input::check(py, budget)?;
                output.bytes_fragment(py, fragment)?;
            }
            return output.bytes_end(py);
        }
        let length = self.content_width(row, column, budget)?;
        output.bytes_start(py, length)?;
        let mut chunk = [0u8; 4096];
        let mut start = 0;
        while start < length {
            crate::input::check(py, budget)?;
            let take = (length - start).min(chunk.len());
            self.copy_cell_range(row, column, start, &mut chunk[..take], budget)?;
            output.bytes_fragment(py, &chunk[..take])?;
            start += take;
        }
        output.bytes_end(py)
    }

    fn write_unicode(
        &mut self,
        row: usize,
        column: usize,
        output: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        let swap = self.swap;
        let mut scratch = self.utf8.take();
        if let Some(cell) = self.owned_cell(row, column, budget)? {
            use crate::input::unicode::{SHORT_UCS4_BYTES, short_ucs4_count, short_ucs4_text};
            let mut utf8 = [0; SHORT_UCS4_BYTES];
            let text = if !swap && cell.len() <= SHORT_UCS4_BYTES {
                crate::input::check(py, budget)?;
                let count = short_ucs4_count(cell);
                output.text_minimum(py, count)?;
                if count != 0 {
                    crate::input::check(py, budget)?;
                }
                short_ucs4_text(&cell[..count * 4], &mut utf8)
            } else if Self::owned_unicode_text(py, cell, swap, output, budget, &mut scratch)? {
                Some(scratch.as_ref().map_or("", |scratch| scratch.text.as_str()))
            } else {
                None
            };
            // Both results own the UTF-8 outside the cell's captured source.
            let result = if let Some(text) = text {
                output.text_utf8(py, text, budget)
            } else {
                let source = self.unicode_source(row, column, budget)?;
                output.text(&source, budget)
            };
            if let Some(scratch) = scratch.as_deref_mut() {
                scratch.text.clear();
            }
            self.utf8 = scratch;
            return result;
        }
        self.utf8 = scratch;
        let length = self.content_width(row, column, budget)?;
        // Every logical non-padding code point needs at least one output byte
        // for the supported client codecs. Reject this known minimum before
        // allocating the cell's UTF-8 representation.
        output.text_minimum(py, length / 4)?;
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
                    let source = self.unicode_source(row, column, budget)?;
                    return output.text(&source, budget);
                };
                text.push(character);
            }
            start += take;
        }
        output.text_utf8(py, &text, budget)
    }

    /// Decode an already owned UCS4 cell without intermediate fragment copies.
    /// False requests the original canonical scalar's exceptional conversion.
    fn owned_unicode_text(
        py: Python<'_>,
        cell: &[u8],
        swap: bool,
        output: &(impl Target + ?Sized),
        budget: &dyn Checkpoint,
        scratch: &mut Option<Box<UnicodeScratch>>,
    ) -> PyResult<bool> {
        let mut length = 0;
        let mut end = cell.len();
        while end > 0 {
            crate::input::check(py, budget)?;
            let start = end.saturating_sub(4096);
            if let Some(last) = cell[start..end].iter().rposition(|byte| *byte != 0) {
                length = (start + last + 1).div_ceil(4) * 4;
                break;
            }
            end = start;
        }
        // Keep the original minimum-width rejection before UTF-8 allocation
        // or the canonical getter for an invalid UCS4 point.
        output.text_minimum(py, length / 4)?;
        if length == 0 {
            if let Some(scratch) = scratch.as_deref_mut() {
                scratch.text.clear();
            }
            return Ok(true);
        }
        // Preserve the first decode checkpoint before either allocation.
        crate::input::check(py, budget)?;
        if scratch.is_none() {
            *scratch = Some(crate::input::boxed(
                py,
                UnicodeScratch {
                    text: String::new(),
                },
            )?);
        }
        if let Some(scratch) = scratch.as_deref_mut() {
            let text = &mut scratch.text;
            text.clear();
            for (index, fragment) in cell[..length].chunks(4096).enumerate() {
                if index != 0 {
                    crate::input::check(py, budget)?;
                }
                // Exact growth avoids amortized doubling past the original
                // 8KiB cell cap; UTF-8 length cannot exceed these UCS4 bytes.
                text.try_reserve_exact(fragment.len())
                    .map_err(|_| errors::memory_error(py))?;
                for &bytes in fragment.as_chunks::<4>().0 {
                    let point = if swap {
                        u32::from_ne_bytes(bytes).swap_bytes()
                    } else {
                        u32::from_ne_bytes(bytes)
                    };
                    let Some(character) = char::from_u32(point) else {
                        text.clear();
                        return Ok(false);
                    };
                    text.push(character);
                }
            }
        }
        Ok(true)
    }
}

impl<'py> PreparedSource<'py> for NumpySource<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }

    fn shape(&self) -> Shape {
        self.shape
    }

    fn python_child(
        &mut self,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if matches!(self.shape, Shape::Vector { .. }) {
            self.object_child(index, 0, budget)
        } else {
            Ok(None)
        }
    }

    fn axis_length(&self, axis: usize) -> Option<usize> {
        match self.shape {
            Shape::Scalar | Shape::Record { .. } => None,
            Shape::Vector { length } => (axis == 0).then_some(length),
            Shape::Table { rows, columns } => match axis {
                0 => Some(rows),
                1 => Some(columns),
                _ => None,
            },
            Shape::Tensor { .. } => self.tensor_axes.get(axis).copied(),
        }
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        if matches!(self.kind, ArrayKind::Bytes) {
            target.schema(self.owner.py(), Schema::Binary)?;
        }
        Ok(())
    }

    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell_to(row, column, target, budget)
    }

    crate::input::source::cell_destinations!();
}

impl NumpySource<'_> {
    crate::input::cell::destinations!();

    pub(super) fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        output: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        output.checkpoint(py, 1)?;
        let kind = match self.kind {
            ArrayKind::Scalar(kind) => kind,
            ArrayKind::RecordLayout => {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "record layout facts require the field-aware source consumer",
                ));
            }
            ArrayKind::Bytes => return self.write_bytes(row, column, output, budget),
            ArrayKind::Unicode => return self.write_unicode(row, column, output, budget),
            ArrayKind::Object => {
                // Retain the public getter's owning reference. Never hold a raw
                // mutable Object slot across recursive target consumption.
                let value = self.element(row, column, budget)?;
                crate::input::check(py, budget)?;
                return output.python(&value, None);
            }
        };
        // The copied block already proved these native vector positions.
        // Keep short values, block misses, other layouts and byte orders on
        // the existing checked read and preserve every target checkpoint.
        // The shared half decoder stays in both builds. Only free-threaded
        // builds use it here because enabling this shortcut on the GIL build
        // regressed unrelated native integer workloads.
        if (matches!(kind, Kind::F32 | Kind::F64)
            || (cfg!(Py_GIL_DISABLED) && matches!(kind, Kind::F16)))
            && !self.swap
            && column == 0
            && matches!(self.shape, Shape::Vector { length } if row < length)
            && let Some(value) = self.raw.as_ref().and_then(|block| block.float(row, kind))
        {
            return output.float(py, value);
        }
        if matches!(kind, Kind::I64)
            && !self.swap
            && let Some(value) = self.copied_signed64(row, column)
        {
            return output.signed(py, value);
        }
        let b = self.cell_bytes(row, column, budget)?;
        let value = self.scalar_fact(kind, b, budget)?;
        output.scalar(py, value)
    }

    /// The owned block is indexed in logical row-major order, including when
    /// its source used checked noncontiguous strides.
    fn copied_signed64(&self, row: usize, column: usize) -> Option<i64> {
        let block = self.raw.as_ref()?;
        let index = match self.shape {
            Shape::Vector { length } if column == 0 && row < length => row,
            Shape::Table { rows, columns } if row < rows && column < columns => {
                row.checked_mul(columns)?.checked_add(column)?
            }
            _ => return None,
        };
        block.signed64(index)
    }
}
