//! Known standard sequences, retaining one current iterator item for rollover.

use super::ScalarTypes;
use crate::{errors, output};
use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyIterator, PyList, PySet, PyTuple};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) enum Sequence<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    Iterator(Rc<IterSequence<'py>>),
    Normalized {
        owner: Bound<'py, PyAny>,
        inner: Box<Self>,
    },
}
struct IterState<'py> {
    iterator: Bound<'py, PyIterator>,
    consumed: usize,
    current: Option<Bound<'py, PyAny>>,
}
pub(crate) struct IterSequence<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    state: RefCell<IterState<'py>>,
}
impl<'py> Sequence<'py> {
    pub(crate) fn from(
        value: &Bound<'py, PyAny>,
        types: &ScalarTypes,
        active: &[usize],
    ) -> PyResult<Option<Self>> {
        if value.is_exact_instance_of::<PyList>() {
            return Ok(Some(Self::List(value.cast::<PyList>()?.clone())));
        }
        if value.is_exact_instance_of::<PyTuple>() {
            return Ok(Some(Self::Tuple(value.cast::<PyTuple>()?.clone())));
        }
        Self::extended(value, types, active)
    }

    #[cold]
    fn extended(
        value: &Bound<'py, PyAny>,
        types: &ScalarTypes,
        active: &[usize],
    ) -> PyResult<Option<Self>> {
        let py = value.py();
        if value.get_type().is(types.struct_time.bind(py)) {
            return Ok(None);
        }
        // SAFETY: these addresses identify CPython's public canonical types;
        // no mutable type state or private object layout is read here.
        let (items, range, view) = unsafe {
            (
                pyo3::ffi::Py_IS_TYPE(value.as_ptr(), &raw mut pyo3::ffi::PyDictItems_Type) != 0,
                pyo3::ffi::Py_IS_TYPE(value.as_ptr(), &raw mut pyo3::ffi::PyRange_Type) != 0,
                pyo3::ffi::Py_IS_TYPE(value.as_ptr(), &raw mut pyo3::ffi::PyDictValues_Type) != 0
                    || pyo3::ffi::Py_IS_TYPE(value.as_ptr(), &raw mut pyo3::ffi::PyDictKeys_Type)
                        != 0,
            )
        };
        if active.contains(&(value.as_ptr() as usize)) {
            return Err(errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::ContainerNesting,
                "input contains a true cycle; current request was not sent",
            ));
        }
        // Real subclasses use their approved formal constructor exactly once.
        // The original identity remains visible through normalization.
        let constructor = if value.is_instance_of::<PyList>() {
            Some(PyList::type_object(py))
        } else if value.is_instance_of::<PyTuple>() {
            Some(PyTuple::type_object(py))
        } else if !value.is_exact_instance_of::<PyDict>() && value.is_instance_of::<PyDict>() {
            Some(PyDict::type_object(py))
        } else if !value.is_exact_instance_of::<PySet>() && value.is_instance_of::<PySet>() {
            Some(PySet::type_object(py))
        } else if !value.is_exact_instance_of::<PyFrozenSet>()
            && value.is_instance_of::<PyFrozenSet>()
        {
            Some(PyFrozenSet::type_object(py))
        } else {
            None
        };
        if let Some(constructor) = constructor {
            let normalized = constructor.call1(output::tuple_from_ready(py, [value.clone()])?)?;
            let inner = Self::from(&normalized, types, active)?.ok_or_else(|| {
                errors::builtin_message::<PyRuntimeError>(
                    py,
                    "formal container conversion did not produce its canonical base",
                )
            })?;
            return Ok(Some(Self::Normalized {
                owner: value.clone(),
                inner: super::boxed(py, inner)?,
            }));
        }
        let values = if value.is_exact_instance_of::<PyDict>() {
            // PyDict::values returns a list; the public method returns the
            // required live view instead, with no whole-dictionary snapshot.
            value.call_method0(c"values")?
        } else if items {
            // A dict_items view contributes values only. Its public mapping
            // proxy gives a values iterator without allocating item tuples.
            errors::attribute(value, c"mapping")?.call_method0(c"values")?
        } else if view
            || range
            || value.is_exact_instance_of::<PySet>()
            || value.is_exact_instance_of::<PyFrozenSet>()
        {
            value.clone()
        } else {
            return Ok(None);
        };
        let length = values.len()?;
        let iterator = values.try_iter()?;
        Ok(Some(Self::Iterator(Rc::new(IterSequence {
            owner: value.clone(),
            length,
            state: RefCell::new(IterState {
                iterator,
                consumed: 0,
                current: None,
            }),
        }))))
    }
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::List(v) => v.len(),
            Self::Tuple(v) => v.len(),
            Self::Iterator(v) => v.length,
            Self::Normalized { inner, .. } => inner.len(),
        }
    }
    pub(crate) fn get(&self, index: usize) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::List(v) => v.get_item(index),
            Self::Tuple(v) => v.get_item(index),
            Self::Normalized { inner, .. } => inner.get(index),
            Self::Iterator(v) => {
                let py = v.owner.py();
                let mut state = v.state.try_borrow_mut().map_err(|_| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "sequence iteration was re-entered",
                    )
                })?;
                if state.consumed != 0 && index == state.consumed - 1 {
                    return state.current.clone().ok_or_else(|| {
                        errors::builtin_message::<PyIndexError>(
                            py,
                            "current sequence item is unavailable",
                        )
                    });
                }
                if index != state.consumed || index >= v.length {
                    return Err(errors::builtin_message::<PyIndexError>(
                        py,
                        "sequence access is outside its forward row or retained rollover row",
                    ));
                }
                let next = state.iterator.next().transpose()?.ok_or_else(|| {
                    errors::builtin_message::<PyRuntimeError>(
                        py,
                        "input container changed length during escape",
                    )
                })?;
                state.consumed += 1;
                // The new value is owned before the previous one is dropped.
                // No borrowed mutable container slot survives Python callbacks.
                state.current = Some(next.clone());
                Ok(next)
            }
        }
    }
    pub(crate) fn with_item(
        &self,
        index: usize,
        visit: impl FnOnce(PyResult<&Bound<'py, PyAny>>) -> PyResult<()>,
    ) -> PyResult<()> {
        match self {
            Self::Normalized { inner, .. } => inner.with_item(index, visit),
            Self::Tuple(v) => match v.get_borrowed_item(index) {
                Ok(item) => visit(Ok(&item)),
                Err(error) => visit(Err(error)),
            },
            _ => match self.get(index) {
                Ok(item) => visit(Ok(&item)),
                Err(error) => visit(Err(error)),
            },
        }
    }
    pub(crate) fn identity(&self) -> usize {
        match self {
            Self::List(v) => v.as_ptr() as usize,
            Self::Tuple(v) => v.as_ptr() as usize,
            Self::Iterator(v) => v.owner.as_ptr() as usize,
            Self::Normalized { owner, .. } => owner.as_ptr() as usize,
        }
    }
    pub(crate) fn py(&self) -> Python<'py> {
        match self {
            Self::List(value) => value.py(),
            Self::Tuple(value) => value.py(),
            Self::Iterator(value) => value.owner.py(),
            Self::Normalized { owner, .. } => owner.py(),
        }
    }

    /// Track the private normalized container as the same logical level.
    pub(crate) fn alias(&self) -> Option<usize> {
        match self {
            Self::Normalized { inner, .. } => Some(inner.identity()),
            _ => None,
        }
    }
}
