//! Canonical container facts and bounded forward acquisition at one boundary.

use super::ScalarTypes;
use super::control::{Checkpoint, callback};
use crate::errors;
use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyIterator, PyList, PySet, PyTuple};
use std::cell::RefCell;

/// Recognition is shared; a target selects the source projection it needs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    List,
    Tuple,
    Dict,
    Set,
    FrozenSet,
    Range,
    Keys,
    Values,
    Items,
}

pub(super) fn kind(value: &Bound<'_, PyAny>, types: &ScalarTypes) -> Option<Kind> {
    if value.is_instance_of::<PyList>() {
        return Some(Kind::List);
    }
    if value.is_instance_of::<PyTuple>() {
        return (!value.get_type().is(types.struct_time.bind(value.py()))).then_some(Kind::Tuple);
    }
    if value.is_instance_of::<PyDict>() {
        return Some(Kind::Dict);
    }
    if value.is_instance_of::<PySet>() {
        return Some(Kind::Set);
    }
    if value.is_instance_of::<PyFrozenSet>() {
        return Some(Kind::FrozenSet);
    }
    // SAFETY: public canonical type identities only; no object layout is read.
    unsafe {
        let pointer = value.as_ptr();
        if pyo3::ffi::Py_IS_TYPE(pointer, &raw mut pyo3::ffi::PyRange_Type) != 0 {
            Some(Kind::Range)
        } else if pyo3::ffi::Py_IS_TYPE(pointer, &raw mut pyo3::ffi::PyDictKeys_Type) != 0 {
            Some(Kind::Keys)
        } else if pyo3::ffi::Py_IS_TYPE(pointer, &raw mut pyo3::ffi::PyDictValues_Type) != 0 {
            Some(Kind::Values)
        } else if pyo3::ffi::Py_IS_TYPE(pointer, &raw mut pyo3::ffi::PyDictItems_Type) != 0 {
            Some(Kind::Items)
        } else {
            None
        }
    }
}

/// Do not acquire keys for a positional consumer which needs only values.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Projection {
    Values,
    Entries,
}

pub(crate) struct Types {
    dict_values: Py<PyAny>,
    dict_items: Py<PyAny>,
    set_iter: Py<PyAny>,
    frozen_iter: Py<PyAny>,
}

impl Types {
    pub(super) fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            dict_values: py.get_type::<PyDict>().getattr(c"values")?.unbind(),
            dict_items: py.get_type::<PyDict>().getattr(c"items")?.unbind(),
            set_iter: py.get_type::<PySet>().getattr(c"__iter__")?.unbind(),
            frozen_iter: py.get_type::<PyFrozenSet>().getattr(c"__iter__")?.unbind(),
        })
    }

    pub(super) fn prepare<'py>(
        &self,
        value: &Bound<'py, PyAny>,
        kind: Kind,
        projection: Projection,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Sequence<'py>> {
        let py = value.py();
        if kind == Kind::List {
            return Ok(Sequence::List(value.cast::<PyList>()?.clone()));
        }
        if kind == Kind::Tuple {
            return Ok(Sequence::Tuple(value.cast::<PyTuple>()?.clone()));
        }
        callback(py, budget, || {
            let (length, iterator) = match kind {
                Kind::Set => (
                    value.cast::<PySet>()?.len(),
                    self.set_iter
                        .bind(py)
                        .call1((value,))?
                        .cast_into::<PyIterator>()?,
                ),
                Kind::FrozenSet => (
                    value.cast::<PyFrozenSet>()?.len(),
                    self.frozen_iter
                        .bind(py)
                        .call1((value,))?
                        .cast_into::<PyIterator>()?,
                ),
                _ => {
                    let projected = match kind {
                        Kind::Dict => {
                            let descriptor = if projection == Projection::Entries {
                                &self.dict_items
                            } else {
                                &self.dict_values
                            };
                            descriptor.bind(py).call1((value,))?
                        }
                        _ => value.clone(),
                    };
                    (projected.len()?, projected.try_iter()?)
                }
            };
            Ok(Sequence::Iterator(super::boxed(
                py,
                IterSequence {
                    owner: value.clone(),
                    length,
                    values_only: kind == Kind::Items && projection == Projection::Values,
                    state: RefCell::new(IterState {
                        iterator: Some(iterator),
                        consumed: 0,
                        current: None,
                    }),
                },
            )?))
        })
    }
}

pub(crate) enum Sequence<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    Iterator(Box<IterSequence<'py>>),
}

enum Acquire<'py> {
    Current(Option<Bound<'py, PyAny>>),
    Next(Option<Bound<'py, PyIterator>>),
    Outside,
}

struct IterState<'py> {
    // Taken out before advancing: no RefCell guard survives a Python API call.
    iterator: Option<Bound<'py, PyIterator>>,
    consumed: usize,
    current: Option<Bound<'py, PyAny>>,
}

pub(crate) struct IterSequence<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    values_only: bool,
    state: RefCell<IterState<'py>>,
}

impl<'py> Sequence<'py> {
    pub(crate) fn plain(value: &Bound<'py, PyAny>) -> PyResult<Option<Self>> {
        if value.is_exact_instance_of::<PyList>() {
            Ok(Some(Self::List(value.cast::<PyList>()?.clone())))
        } else if value.is_exact_instance_of::<PyTuple>() {
            Ok(Some(Self::Tuple(value.cast::<PyTuple>()?.clone())))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn from(
        value: &Bound<'py, PyAny>,
        types: &ScalarTypes,
        active: &[usize],
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Option<Self>> {
        if let Some(value) = Self::plain(value)? {
            return Ok(Some(value));
        }
        let Some(kind) = kind(value, types) else {
            return Ok(None);
        };
        if active.contains(&(value.as_ptr() as usize)) {
            return Err(errors::client(
                value.py(),
                sqlaero_core::error::ClientErrorCode::ContainerNesting,
                "input contains a true cycle; current request was not sent",
            ));
        }
        types
            .sequences
            .prepare(value, kind, Projection::Values, budget)
            .map(Some)
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::List(value) => value.len(),
            Self::Tuple(value) => value.len(),
            Self::Iterator(value) => value.length,
        }
    }

    #[inline]
    pub(crate) fn row_item(
        &self,
        index: usize,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::List(value) => value.get_item(index),
            Self::Tuple(value) => value.get_item(index),
            Self::Iterator(_) => self.get(index, budget),
        }
    }

    pub(crate) fn get(
        &self,
        index: usize,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let Self::Iterator(value) = self else {
            return self.row_item(index, budget);
        };
        let py = value.owner.py();
        callback(py, budget, || {
            let prepared = {
                let mut state = value.state.try_borrow_mut().map_err(|_| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "container acquisition was re-entered",
                    )
                })?;
                if state.consumed != 0 && index == state.consumed - 1 {
                    Acquire::Current(state.current.clone())
                } else if index != state.consumed || index >= value.length {
                    Acquire::Outside
                } else {
                    Acquire::Next(state.iterator.take())
                }
            };
            // Exception construction also happens after the state borrow ends.
            let mut iterator = match prepared {
                Acquire::Current(value) => {
                    return value.ok_or_else(|| {
                        errors::builtin_message::<PyIndexError>(
                            py,
                            "current container item is unavailable",
                        )
                    });
                }
                Acquire::Outside => {
                    return Err(errors::builtin_message::<PyIndexError>(
                        py,
                        "container access is outside its forward or retained rollover row",
                    ));
                }
                Acquire::Next(iterator) => iterator.ok_or_else(|| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "container acquisition was re-entered",
                    )
                })?,
            };
            let acquired = iterator.next().transpose();
            // Restore the iterator before propagating an error or dropping an
            // old Python owner. No state guard spans either operation.
            {
                let mut state = value.state.try_borrow_mut().map_err(|_| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "container state could not be restored",
                    )
                })?;
                state.iterator = Some(iterator);
            }
            let next = acquired?.ok_or_else(|| {
                errors::builtin_message::<PyRuntimeError>(
                    py,
                    "input container changed length during escape",
                )
            })?;
            // A dict_items mapping proxy may forward values() to an overridden
            // dict-subclass method. Read its actual item tuple instead; no
            // whole mapping snapshot or user projection method is involved.
            let next = if value.values_only {
                next.cast::<PyTuple>()?.get_item(1)?
            } else {
                next
            };
            let previous = {
                let mut state = value.state.try_borrow_mut().map_err(|_| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "container acquisition was re-entered",
                    )
                })?;
                state.consumed += 1;
                state.current.replace(next.clone())
            };
            drop(previous);
            Ok(next)
        })
    }

    pub(crate) fn with_item(
        &self,
        index: usize,
        budget: Option<&dyn Checkpoint>,
        visit: impl FnOnce(PyResult<&Bound<'py, PyAny>>) -> PyResult<()>,
    ) -> PyResult<()> {
        match self {
            Self::List(value) => match value.get_item(index) {
                Ok(item) => visit(Ok(&item)),
                Err(error) => visit(Err(error)),
            },
            Self::Tuple(value) => match value.get_borrowed_item(index) {
                Ok(item) => visit(Ok(&item)),
                Err(error) => visit(Err(error)),
            },
            Self::Iterator(_) => match self.get(index, budget) {
                Ok(item) => visit(Ok(&item)),
                Err(error) => visit(Err(error)),
            },
        }
    }

    pub(crate) fn identity(&self) -> usize {
        match self {
            Self::List(value) => value.as_ptr() as usize,
            Self::Tuple(value) => value.as_ptr() as usize,
            Self::Iterator(value) => value.owner.as_ptr() as usize,
        }
    }
    pub(crate) fn py(&self) -> Python<'py> {
        match self {
            Self::List(value) => value.py(),
            Self::Tuple(value) => value.py(),
            Self::Iterator(value) => value.owner.py(),
        }
    }
}
