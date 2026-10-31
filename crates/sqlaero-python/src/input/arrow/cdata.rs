//! One owned C Data boundary normalization for a Null array's unused buffer.
//!
//! Some conforming-to-consumer producers expose one null pointer slot for the
//! Null logical type. Apache's importer requires zero slots. Null has no value
//! storage, so ignoring that empty slot preserves every logical value. We never
//! alter a buffer pointer, schema, producer value or shared provider object.

use std::ffi::c_void;
use std::ptr;

use arrow_array::ffi::FFI_ArrowArray;
use arrow_schema::DataType;
use pyo3::prelude::*;
use sqlaero_core::transport::deadline::IoBudget;

use crate::errors;

/// The public ArrowArray C ABI. Apache exposes release/private_data accessors,
/// but no public setter for its buffer count, so the single adjustment is here.
#[derive(Clone, Copy)]
#[repr(C)]
struct ArrayAbi {
    length: i64,
    null_count: i64,
    offset: i64,
    n_buffers: i64,
    n_children: i64,
    buffers: *mut *const c_void,
    children: *mut *mut FFI_ArrowArray,
    dictionary: *mut FFI_ArrowArray,
    release: Option<unsafe extern "C" fn(*mut FFI_ArrowArray)>,
    private_data: *mut c_void,
}

const _: () = assert!(size_of::<ArrayAbi>() == size_of::<FFI_ArrowArray>());
const _: () = assert!(align_of::<ArrayAbi>() == align_of::<FFI_ArrowArray>());

struct Change {
    header: *mut ArrayAbi,
    original: ArrayAbi,
}

struct Release {
    original_root: ArrayAbi,
    children: Vec<Change>,
    root_null: bool,
    allocation_capacity: usize,
}

fn contains_null(dtype: &DataType) -> bool {
    match dtype {
        DataType::Null => true,
        DataType::List(child) | DataType::LargeList(child) | DataType::FixedSizeList(child, _) => {
            contains_null(child.data_type())
        }
        DataType::Struct(fields) => fields.iter().any(|field| contains_null(field.data_type())),
        DataType::Dictionary(_, value) => contains_null(value),
        _ => false,
    }
}

/// The caller exclusively owns the moved export and has already checked its
/// supported schema. The original root release remains responsible for every
/// producer allocation, and will observe its exact original headers.
pub(super) fn normalize_nulls(
    py: Python<'_>,
    array: &mut FFI_ArrowArray,
    dtype: &DataType,
    budget: &IoBudget,
) -> PyResult<()> {
    if !contains_null(dtype) {
        return Ok(());
    }
    let root = ptr::from_mut(array).cast::<ArrayAbi>();
    // SAFETY: the live moved FFI_ArrowArray implements this public repr(C) ABI;
    // no raw reference is retained through a Python callback or a move.
    let original_root = unsafe { *root };
    let mut children = Vec::new();
    let mut root_null = false;
    collect(py, root, dtype, true, &mut root_null, &mut children, budget)?;
    if !root_null && children.is_empty() {
        return Ok(());
    }

    // Allocate fallibly before touching any export header. One-element Vec
    // storage gives a stable allocation without Box::new's infallible path.
    let mut allocation = Vec::new();
    allocation
        .try_reserve_exact(1)
        .map_err(|_| errors::memory_error(py))?;
    let capacity = allocation.capacity();
    allocation.push(Release {
        original_root,
        children,
        root_null,
        allocation_capacity: capacity,
    });
    let state = allocation.as_mut_ptr();
    // SAFETY: all nodes are private headers of this one moved export. All
    // fallible preparation has finished. Install restoration before changing
    // only the redundant empty Null buffer counts; no Python call runs here.
    unsafe {
        (*root).private_data = state.cast();
        (*root).release = Some(release);
        if (*state).root_null {
            (*root).n_buffers = 0;
        }
        for change in &(*state).children {
            (*change.header).n_buffers = 0;
        }
    }
    std::mem::forget(allocation);
    Ok(())
}

fn collect(
    py: Python<'_>,
    pointer: *mut ArrayAbi,
    dtype: &DataType,
    root: bool,
    root_null: &mut bool,
    changes: &mut Vec<Change>,
    budget: &IoBudget,
) -> PyResult<()> {
    if !contains_null(dtype) {
        return Ok(());
    }
    super::super::check(py, budget)?;
    // SAFETY: the supported standard producer keeps the complete C Data tree
    // alive until root release. Null child pointers are rejected before here.
    let header = unsafe { *pointer };
    match dtype {
        DataType::Null => {
            if header.n_buffers == 0 {
                return Ok(());
            }
            if header.n_buffers != 1 || header.buffers.is_null() {
                return Err(super::invalid(
                    py,
                    "Arrow Null has an invalid buffer layout",
                ));
            }
            // SAFETY: this live header declares exactly one readable buffer
            // pointer. We accept only an unused null slot, never value storage.
            if !unsafe { *header.buffers }.is_null() {
                return Err(super::invalid(py, "Arrow Null has a nonempty data buffer"));
            }
            if root {
                *root_null = true;
            } else {
                changes
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                changes.push(Change {
                    header: pointer,
                    original: header,
                });
            }
            Ok(())
        }
        DataType::Struct(fields) => {
            if usize::try_from(header.n_children).ok() != Some(fields.len()) {
                return Err(super::invalid(
                    py,
                    "Arrow Struct has an invalid child count",
                ));
            }
            for (index, field) in fields.iter().enumerate() {
                if contains_null(field.data_type()) {
                    let child = child(py, &header, index)?;
                    collect(
                        py,
                        child,
                        field.data_type(),
                        false,
                        root_null,
                        changes,
                        budget,
                    )?;
                }
            }
            Ok(())
        }
        DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _) => {
            if header.n_children != 1 {
                return Err(super::invalid(py, "Arrow List has an invalid child count"));
            }
            collect(
                py,
                child(py, &header, 0)?,
                field.data_type(),
                false,
                root_null,
                changes,
                budget,
            )
        }
        DataType::Dictionary(_, value) => {
            if header.dictionary.is_null() {
                return Err(super::invalid(py, "Arrow Dictionary is missing its values"));
            }
            collect(
                py,
                header.dictionary.cast(),
                value,
                false,
                root_null,
                changes,
                budget,
            )
        }
        _ => Ok(()),
    }
}

fn child(py: Python<'_>, header: &ArrayAbi, index: usize) -> PyResult<*mut ArrayAbi> {
    if header.children.is_null() {
        return Err(super::invalid(
            py,
            "Arrow array is missing its child pointers",
        ));
    }
    // SAFETY: the caller checked the declared count, and this standard live
    // array provides that many child pointers. Null is rejected, not followed.
    let child = unsafe { *header.children.add(index) };
    if child.is_null() {
        Err(super::invalid(
            py,
            "Arrow array contains a null child pointer",
        ))
    } else {
        Ok(child.cast())
    }
}

unsafe extern "C" fn release(array: *mut FFI_ArrowArray) {
    // SAFETY: installed only by normalize_nulls, which transfers exactly one
    // initialized Release allocation into private_data. The C owner invokes
    // this once after all array/buffer borrows have ended, while attached.
    unsafe {
        let root = array.cast::<ArrayAbi>();
        let state = (*root).private_data.cast::<Release>();
        let allocation = Vec::from_raw_parts(state, 1, (*state).allocation_capacity);
        let state = &*state;
        for change in &state.children {
            ptr::write(change.header, change.original);
        }
        ptr::write(root, state.original_root);
        if let Some(original) = state.original_root.release {
            original(array);
        }
        drop(allocation);
    }
}
