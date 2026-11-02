//! Validate an owned C schema and import only metadata relevant to SQL values.
//!
//! Ordinary C Data metadata is binary. Apache's Field importer expects UTF-8
//! metadata, so an affected tree is presented through borrowed, metadata-free
//! headers. The producer headers and their release callbacks remain untouched.

use std::ffi::{CStr, c_char, c_void};
use std::ptr;

use arrow_schema::{Field, ffi::FFI_ArrowSchema};
use pyo3::prelude::*;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;

use crate::errors;

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

#[derive(Default)]
struct Inventory {
    nodes: usize,
    edges: usize,
    metadata: bool,
}

pub(super) fn import(
    py: Python<'_>,
    source: &FFI_ArrowSchema,
    budget: &IoBudget,
) -> PyResult<Field> {
    // SAFETY: the caller owns a live, standard CPU C Schema export. All of its
    // pointer storage remains valid until that owner invokes its release.
    let root = unsafe { &*ptr::from_ref(source).cast::<Header>() };
    let mut inventory = Inventory::default();
    validate(py, root, 0, &mut inventory, budget)?;
    if !inventory.metadata {
        return field(py, source);
    }
    let mut headers = Vec::new();
    let mut children = Vec::new();
    headers
        .try_reserve_exact(inventory.nodes)
        .map_err(|_| errors::memory_error(py))?;
    children
        .try_reserve_exact(inventory.edges)
        .map_err(|_| errors::memory_error(py))?;
    copy_headers(py, root, &mut headers, &mut children, budget)?;
    // SAFETY: the arenas cannot move or grow while the importer borrows this
    // root. Their headers borrow original strings, retain no metadata, and are
    // never converted into owning FFI_ArrowSchema values. Dropping the arenas
    // cannot call a producer release; source remains the sole owning export.
    let view = unsafe { &*ptr::from_ref(&headers[0]).cast::<FFI_ArrowSchema>() };
    field(py, view)
}

fn field(py: Python<'_>, source: &FFI_ArrowSchema) -> PyResult<Field> {
    Field::try_from(source).map_err(|error| {
        errors::builtin_format::<pyo3::exceptions::PyTypeError>(py, format_args!("{error}"))
    })
}

fn validate(
    py: Python<'_>,
    node: &Header,
    depth: usize,
    inventory: &mut Inventory,
    budget: &IoBudget,
) -> PyResult<()> {
    if depth >= 64 {
        return Err(errors::client(
            py,
            ClientErrorCode::ContainerNesting,
            "Arrow schema exceeds 64 nested container levels",
        ));
    }
    inventory.nodes = inventory
        .nodes
        .checked_add(1)
        .ok_or_else(|| errors::memory_error(py))?;
    if inventory.nodes.is_multiple_of(1024) {
        super::super::check(py, budget)?;
    }
    if node.release.is_none() || node.format.is_null() {
        return Err(super::invalid(
            py,
            "Arrow schema is released or has no format",
        ));
    }
    // SAFETY: standard live C Schema format/name strings are NUL terminated.
    unsafe { CStr::from_ptr(node.format) }
        .to_str()
        .map_err(|_| super::invalid(py, "Arrow schema format is not valid UTF-8"))?;
    if !node.name.is_null() {
        // SAFETY: the same producer-owned C string contract applies to name.
        unsafe { CStr::from_ptr(node.name) }
            .to_str()
            .map_err(|_| super::invalid(py, "Arrow schema name is not valid UTF-8"))?;
    }
    if !node.metadata.is_null() {
        inventory.metadata = true;
        validate_metadata(py, node.metadata, budget)?;
    }
    let count = usize::try_from(node.n_children)
        .map_err(|_| super::invalid(py, "Arrow schema has a negative child count"))?;
    inventory.edges = inventory
        .edges
        .checked_add(count)
        .ok_or_else(|| errors::memory_error(py))?;
    if count != 0 && node.children.is_null() {
        return Err(super::invalid(
            py,
            "Arrow schema is missing its child pointer array",
        ));
    }
    for index in 0..count {
        // SAFETY: a standard live schema supplies n_children valid pointers.
        let child = unsafe { *node.children.add(index) };
        // SAFETY: pointed-to child shares the source export's lifetime.
        let child = unsafe { child.as_ref() }
            .ok_or_else(|| super::invalid(py, "Arrow schema contains a null child pointer"))?;
        validate(py, child, depth + 1, inventory, budget)?;
    }
    // SAFETY: a non-null dictionary is another live producer-owned schema.
    if let Some(dictionary) = unsafe { node.dictionary.as_ref() } {
        validate(py, dictionary, depth + 1, inventory, budget)?;
    }
    Ok(())
}

fn validate_metadata(py: Python<'_>, metadata: *const c_char, budget: &IoBudget) -> PyResult<()> {
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

fn copy_headers(
    py: Python<'_>,
    source: &Header,
    headers: &mut Vec<Header>,
    children: &mut Vec<*const Header>,
    budget: &IoBudget,
) -> PyResult<usize> {
    let index = headers.len();
    if index.is_multiple_of(1024) {
        super::super::check(py, budget)?;
    }
    let start = children.len();
    let count = source.n_children as usize;
    // The complete checked inventory was reserved before either arena filled.
    // Thus these writes cannot reallocate and invalidate stored header pointers.
    children.resize(start + count, ptr::null());
    let mut header = *source;
    header.metadata = ptr::null();
    header.children = if count == 0 {
        ptr::null()
    } else {
        children.as_ptr().wrapping_add(start)
    };
    header.dictionary = ptr::null();
    headers.push(header);
    for child_index in 0..count {
        // SAFETY: validate checked every child; the owned export remains live.
        let child = unsafe { &**source.children.add(child_index) };
        let copied = copy_headers(py, child, headers, children, budget)?;
        children[start + child_index] = ptr::from_ref(&headers[copied]);
    }
    // SAFETY: a dictionary's validity was checked under the same source owner.
    if let Some(dictionary) = unsafe { source.dictionary.as_ref() } {
        let copied = copy_headers(py, dictionary, headers, children, budget)?;
        headers[index].dictionary = ptr::from_ref(&headers[copied]);
    }
    Ok(index)
}
