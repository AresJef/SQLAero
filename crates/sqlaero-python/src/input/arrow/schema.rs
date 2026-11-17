//! Bounded C Data schema preparation at the attached source boundary.
//!
//! Names are separate complete provider facts. No C name or opaque metadata
//! is copied into positional native value types. Only a constant-size scalar
//! format is delegated to Arrow; structural traversal stays under Control.

use crate::errors;
use crate::input::control::Checkpoint;
use arrow_schema::{DataType, TimeUnit, ffi::FFI_ArrowSchema};
use pyo3::prelude::*;
use smallvec::SmallVec;
use sqlaero_core::error::ClientErrorCode;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::Arc;

/// Own only source schema facts. Compound children stay in their fallibly
/// prepared Vec; they are not bulk-copied into a second immutable field table.
pub(crate) struct Field {
    pub(super) kind: Arc<Type>,
    _flags: i64,
}
pub(crate) enum Type {
    Scalar(DataType),
    Struct(Vec<Field>),
    List(Field),
    LargeList(Field),
    FixedSizeList(Field, i32),
    Dictionary(DataType, Field),
}
#[derive(Clone, Copy)]
pub(crate) enum View<'a> {
    Scalar(&'a DataType),
    Struct(&'a [Field]),
    List(&'a Field),
    LargeList(&'a Field),
    FixedSizeList(&'a Field, &'a i32),
    Dictionary(&'a DataType, &'a Field),
}
impl Field {
    pub(super) fn data_type(&self) -> View<'_> {
        self.kind.view()
    }
}
impl Type {
    pub(super) fn view(&self) -> View<'_> {
        match self {
            Self::Scalar(value) => View::Scalar(value),
            Self::Struct(value) => View::Struct(value),
            Self::List(value) => View::List(value),
            Self::LargeList(value) => View::LargeList(value),
            Self::FixedSizeList(value, width) => View::FixedSizeList(value, width),
            Self::Dictionary(key, value) => View::Dictionary(key, value),
        }
    }
}
impl<'a> View<'a> {
    pub(super) fn scalar(self) -> Option<&'a DataType> {
        if let Self::Scalar(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(super) fn is_scalar(self, expected: &DataType) -> bool {
        self.scalar().is_some_and(|actual| actual == expected)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    format: *const c_char,
    name: *const c_char,
    metadata: *const c_char,
    flags: i64,
    n_children: i64,
    children: *const *const Self,
    dictionary: *const Self,
    release: Option<unsafe extern "C" fn(*mut Self)>,
    private_data: *mut c_void,
}
const _: () = assert!(size_of::<Header>() == size_of::<FFI_ArrowSchema>());
const _: () = assert!(align_of::<Header>() == align_of::<FFI_ArrowSchema>());

pub(super) fn import(
    py: Python<'_>,
    source: &FFI_ArrowSchema,
    logical: &mut super::names::Source<'_>,
    budget: &dyn Checkpoint,
) -> PyResult<Field> {
    // SAFETY: the moved standard CPU export owns this complete public C ABI
    // tree until its release. Only copied headers cross control boundaries.
    let root = unsafe { ptr::from_ref(source).cast::<Header>().read() };
    build(py, root, &mut SmallVec::new(), logical, budget)
}

fn child(py: Python<'_>, node: &Header, index: usize) -> PyResult<Header> {
    // SAFETY: build validated this live header's child count and pointer span.
    let pointer = unsafe { node.children.add(index).read() };
    if pointer.is_null() {
        return Err(super::invalid(
            py,
            "Arrow schema contains a null child pointer",
        ));
    }
    // SAFETY: each child belongs to the live owning root, copied before any
    // signal, deadline or lifecycle check can invoke Python.
    Ok(unsafe { pointer.read() })
}

fn descend(
    py: Python<'_>,
    node: Header,
    edge: super::names::Edge,
    path: &mut SmallVec<[super::names::Edge; 8]>,
    logical: &mut super::names::Source<'_>,
    budget: &dyn Checkpoint,
) -> PyResult<Field> {
    path.try_reserve(1).map_err(|_| errors::memory_error(py))?;
    path.push(edge);
    let result = build(py, node, path, logical, budget);
    path.pop();
    result
}

fn build(
    py: Python<'_>,
    node: Header,
    path: &mut SmallVec<[super::names::Edge; 8]>,
    logical: &mut super::names::Source<'_>,
    budget: &dyn Checkpoint,
) -> PyResult<Field> {
    budget.bytes(py, 8)?;
    if path.len() >= 64 {
        return Err(errors::client(
            py,
            ClientErrorCode::ContainerNesting,
            "Arrow schema exceeds 64 nested container levels",
        ));
    }
    if node.release.is_none() || node.format.is_null() {
        return Err(super::invalid(
            py,
            "Arrow schema is released or has no format",
        ));
    }
    if !node.metadata.is_null() {
        validate_metadata(py, node.metadata, budget)?;
    }
    let bytes = format(py, node.format, budget)?;
    // SAFETY: format validates UTF-8 in owned fragments and appends exactly
    // one terminator. No producer slice is retained through recursive checks.
    let format = unsafe { std::str::from_utf8_unchecked(&bytes[..bytes.len() - 1]) };
    let count = usize::try_from(node.n_children)
        .map_err(|_| super::invalid(py, "Arrow schema has a negative child count"))?;
    if count != 0 && node.children.is_null() {
        return Err(super::invalid(
            py,
            "Arrow schema is missing its child pointer array",
        ));
    }
    if count
        .checked_mul(size_of::<*const Header>())
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(super::invalid(
            py,
            "Arrow schema child pointer span exceeds the address range",
        ));
    }
    let mut dtype = match format {
        "+s" => {
            let mut fields = Vec::new();
            fields
                .try_reserve_exact(count)
                .map_err(|_| errors::memory_error(py))?;
            for index in 0..count {
                fields.push(descend(
                    py,
                    child(py, &node, index)?,
                    super::names::Edge::Field(index),
                    path,
                    logical,
                    budget,
                )?);
            }
            Type::Struct(fields)
        }
        "+l" | "+L" => {
            if count != 1 {
                return Err(super::invalid(
                    py,
                    "Arrow list schema must have exactly one child",
                ));
            }
            let field = descend(
                py,
                child(py, &node, 0)?,
                super::names::Edge::Value,
                path,
                logical,
                budget,
            )?;
            if format == "+l" {
                Type::List(field)
            } else {
                Type::LargeList(field)
            }
        }
        value if value.starts_with("+w:") => {
            if count != 1 {
                return Err(super::invalid(
                    py,
                    "Arrow fixed list schema must have exactly one child",
                ));
            }
            let width = value[3..]
                .parse::<i32>()
                .map_err(|_| super::invalid(py, "Arrow fixed list schema has an invalid width"))?;
            Type::FixedSizeList(
                descend(
                    py,
                    child(py, &node, 0)?,
                    super::names::Edge::Value,
                    path,
                    logical,
                    budget,
                )?,
                width,
            )
        }
        value if value.starts_with("ts") && value.as_bytes().get(3) == Some(&b':') => {
            if count != 0 {
                return Err(super::invalid(
                    py,
                    "Arrow timestamp schema cannot have children",
                ));
            }
            let unit = match value.as_bytes()[2] {
                b's' => TimeUnit::Second,
                b'm' => TimeUnit::Millisecond,
                b'u' => TimeUnit::Microsecond,
                b'n' => TimeUnit::Nanosecond,
                _ => {
                    return Err(super::invalid(
                        py,
                        "Arrow timestamp schema has an unsupported unit",
                    ));
                }
            };
            // The public C format loses everything after an embedded NUL.
            // Logical source metadata is length-bearing and authoritative;
            // an empty real name remains distinct from no timezone.
            let zone = logical
                .timezone(path, budget)?
                .map(|source| {
                    let text = super::super::unicode::snapshot(&source, budget)?;
                    arc_text(py, &text, budget)
                })
                .transpose()?;
            Type::Scalar(DataType::Timestamp(unit, zone))
        }
        _ => {
            // Every admitted non-timestamp scalar format is a small canonical
            // parameter string. Unsupported structural/new formats never enter
            // Arrow's recursive importer or unbounded diagnostic formatting.
            if count != 0 || format.starts_with('+') || format.len() > 64 {
                return Err(super::invalid(py, "unsupported Arrow C schema format"));
            }
            let mut scalar = node;
            scalar.format = bytes.as_ptr().cast();
            scalar.name = c"".as_ptr();
            scalar.metadata = ptr::null();
            scalar.children = ptr::null();
            scalar.dictionary = ptr::null();
            // SAFETY: this is a borrowed non-owning header with validated owned
            // UTF-8, no children or dictionary. It is never dropped as an FFI
            // owner; the constant-size scalar parser cannot traverse the root.
            let view = unsafe { &*ptr::from_ref(&scalar).cast::<FFI_ArrowSchema>() };
            Type::Scalar(DataType::try_from(view).map_err(|error| {
                errors::builtin_format::<pyo3::exceptions::PyTypeError>(py, format_args!("{error}"))
            })?)
        }
    };
    if !node.dictionary.is_null() {
        // SAFETY: a standard dictionary is another live schema under root.
        let dictionary = unsafe { node.dictionary.read() };
        let value = descend(
            py,
            dictionary,
            super::names::Edge::Value,
            path,
            logical,
            budget,
        )?;
        let Type::Scalar(key) = dtype else {
            return Err(super::invalid(
                py,
                "Arrow Dictionary key must be a scalar integer",
            ));
        };
        dtype = Type::Dictionary(key, value);
    }
    Ok(Field {
        kind: Arc::new(dtype),
        _flags: node.flags,
    })
}

/// Arrow's public metadata type uses Arc<str>. Its standard immutable-owner
/// allocation is atomic; all controllable long copy work is fragmented while
/// the unique unpublished allocation contains only MaybeUninit<u8>.
fn arc_text(py: Python<'_>, text: &str, budget: &dyn Checkpoint) -> PyResult<Arc<str>> {
    if text.len() <= 1024 {
        return Ok(Arc::from(text));
    }
    let mut output = Arc::<[u8]>::new_uninit_slice(text.len());
    let target = Arc::get_mut(&mut output).ok_or_else(|| {
        errors::client(
            py,
            ClientErrorCode::InternalState,
            "new native timezone allocation is not unique",
        )
    })?;
    for (destination, source) in target.chunks_mut(8192).zip(text.as_bytes().chunks(8192)) {
        for (destination, source) in destination.iter_mut().zip(source) {
            destination.write(*source);
        }
        budget.bytes(py, source.len())?;
    }
    // SAFETY: every byte was initialized from one valid owned UTF-8 string.
    // On an earlier error, MaybeUninit<u8> needs no element destructor.
    let output = unsafe { output.assume_init() };
    let raw = Arc::into_raw(output);
    // SAFETY: [u8] and str have the same DST layout; the complete bytes are
    // initialized valid UTF-8. Convert this exact allocation's sole raw owner.
    Ok(unsafe { Arc::from_raw(raw as *const str) })
}

fn format(
    py: Python<'_>,
    source: *const c_char,
    budget: &dyn Checkpoint,
) -> PyResult<SmallVec<[u8; 64]>> {
    const CHUNK: usize = 1024;
    // C char signedness varies by target; UTF-8 is always read as raw octets.
    let source = source.cast::<u8>();
    let mut bytes = [0u8; CHUNK + 3];
    let mut pending = 0;
    let mut output = SmallVec::new();
    let mut position = 0usize;
    loop {
        let prefix = pending;
        let mut length = pending;
        let mut ended = false;
        let mut scanned = 0;
        while length < CHUNK {
            // SAFETY: a conforming owned C Schema format is NUL terminated
            // and remains live under its moved export throughout this call.
            let value = unsafe { source.add(position).read() };
            position = position
                .checked_add(1)
                .ok_or_else(|| super::invalid(py, "Arrow schema format length overflows usize"))?;
            scanned += 1;
            if value == 0 {
                ended = true;
                break;
            }
            bytes[length] = value;
            length += 1;
        }
        let valid = match std::str::from_utf8(&bytes[..length]) {
            Ok(_) => length,
            Err(error) if error.error_len().is_none() && !ended => error.valid_up_to(),
            Err(_) => return Err(super::invalid(py, "Arrow schema format is not valid UTF-8")),
        };
        output
            .try_reserve(length - prefix + usize::from(ended))
            .map_err(|_| errors::memory_error(py))?;
        output.extend_from_slice(&bytes[prefix..length]);
        pending = length - valid;
        bytes.copy_within(valid..length, 0);
        budget.bytes(py, scanned)?;
        if ended {
            output.push(0);
            return Ok(output);
        }
    }
}

fn validate_metadata(
    py: Python<'_>,
    metadata: *const c_char,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let mut address = metadata as usize;
    let count = metadata_length(py, &mut address)?;
    for index in 0..count {
        if index % 1024 == 0 {
            super::super::check(py, budget)?;
        }
        let key_length = metadata_length(py, &mut address)?;
        let key_address = address;
        address = address
            .checked_add(key_length)
            .ok_or_else(|| super::invalid(py, "Arrow metadata address overflow"))?;
        let value_length = metadata_length(py, &mut address)?;
        address = address
            .checked_add(value_length)
            .ok_or_else(|| super::invalid(py, "Arrow metadata address overflow"))?;
        if key_length == b"ARROW:extension:name".len() {
            // SAFETY: C Data metadata provides length-prefixed byte regions;
            // only this bounded semantic key is read. Opaque values are skipped.
            let key = unsafe { std::slice::from_raw_parts(key_address as *const u8, key_length) };
            if key == b"ARROW:extension:name" {
                return Err(super::invalid(
                    py,
                    "Arrow extension storage is outside the supported input contract",
                ));
            }
        }
    }
    Ok(())
}

fn metadata_length(py: Python<'_>, address: &mut usize) -> PyResult<usize> {
    let next = address
        .checked_add(size_of::<i32>())
        .ok_or_else(|| super::invalid(py, "Arrow metadata address overflow"))?;
    // SAFETY: standard C Data metadata stores native-endian int32 lengths;
    // it need not align them. The producer guarantees this record is live.
    let length = unsafe { ptr::read_unaligned(*address as *const i32) };
    *address = next;
    usize::try_from(length).map_err(|_| super::invalid(py, "Arrow metadata has a negative length"))
}
