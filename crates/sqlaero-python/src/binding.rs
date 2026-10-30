//! Incremental input-row adaptation and native positional request assembly.

use crate::errors;
use crate::input::{ScalarTypes, check};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use smallvec::{SmallVec, smallvec};
use sqlaero_core::bind::{
    Context,
    template::{Template, TemplateError},
    writer,
};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;
use std::ops::Range;

enum Sequence<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
}
impl<'py> Sequence<'py> {
    fn from(value: &Bound<'py, PyAny>) -> Option<Self> {
        if value.is_exact_instance_of::<PyList>() {
            value
                .cast::<PyList>()
                .ok()
                .map(|value| Self::List(value.clone()))
        } else if value.is_exact_instance_of::<PyTuple>() {
            value
                .cast::<PyTuple>()
                .ok()
                .map(|value| Self::Tuple(value.clone()))
        } else {
            None
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::List(value) => value.len(),
            Self::Tuple(value) => value.len(),
        }
    }
    fn get(&self, index: usize) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::List(value) => value.get_item(index),
            Self::Tuple(value) => value.get_item(index),
        }
    }
    fn identity(&self) -> usize {
        match self {
            Self::List(value) => value.as_ptr() as usize,
            Self::Tuple(value) => value.as_ptr() as usize,
        }
    }
}

/// Retain the original input and its current row index, without materializing
/// row tuples or a whole-batch snapshot. The caller keeps mutable input stable.
pub(crate) struct Rows<'py> {
    root: Bound<'py, PyAny>,
    outer: Option<Sequence<'py>>,
    itemize: bool,
    position: usize,
    count: usize,
    empty: bool,
}
impl<'py> Rows<'py> {
    pub(crate) fn remaining(&self) -> usize {
        self.count.saturating_sub(self.position)
    }
    pub(crate) fn empty_root(&self) -> bool {
        self.empty
    }
    pub(crate) fn new(root: &Bound<'py, PyAny>, many: bool, itemize: bool) -> Self {
        let outer = if many { Sequence::from(root) } else { None };
        let empty = outer.as_ref().is_some_and(|outer| outer.len() == 0);
        let count = outer.as_ref().map_or(1, |outer| outer.len().max(1));
        Self {
            root: root.clone(),
            outer,
            itemize,
            position: 0,
            count,
            empty,
        }
    }
    /// Strongly own only the next row. Actual indexing remains checked, so a
    /// caller violating the mutation contract cannot create a stale pointer.
    pub(crate) fn next(&mut self) -> PyResult<Option<Row<'py>>> {
        if self.position >= self.count {
            return Ok(None);
        }
        let index = self.position;
        self.position += 1;
        self.at(index).map(Some)
    }
    /// Re-read only a rollover row when a confirmed session context changed.
    /// Normal rollover reuses encoded bytes and never calls this method.
    pub(crate) fn at(&self, index: usize) -> PyResult<Row<'py>> {
        let (value, outer, path) = match &self.outer {
            Some(outer) if !self.empty => {
                let value = outer.get(index).inspect_err(|error| {
                    errors::note(
                        self.root.py(),
                        error,
                        format!("while binding args[{index}]; current request was not sent"),
                    );
                })?;
                (value, Some(outer.identity()), smallvec![index])
            }
            _ => (self.root.clone(), None, SmallVec::new()),
        };
        Ok(Row {
            value,
            outer,
            path,
            itemize: self.itemize,
            zero_positions: self.empty,
        })
    }

    pub(crate) fn position(&self) -> usize {
        self.position
    }
}

pub(crate) struct Row<'py> {
    value: Bound<'py, PyAny>,
    outer: Option<usize>,
    path: SmallVec<[usize; 4]>,
    itemize: bool,
    zero_positions: bool,
}
impl Row<'_> {
    pub(crate) fn width(&self) -> usize {
        if self.zero_positions {
            0
        } else if self.itemize {
            Sequence::from(&self.value).map_or(1, |sequence| sequence.len())
        } else {
            1
        }
    }
    pub(crate) fn bind(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<Vec<u8>> {
        let py = self.value.py();
        let end = template.len();
        let mut output = Vec::new();
        writer::append(&mut output, &[3], context.limit)
            .map_err(|error| errors::wire(py, error))?;
        self.fill(template, types, context, budget, 0..end, &mut output)?;
        Ok(output)
    }

    /// Encode the one repeated row, without re-encoding it for rollover.
    pub(crate) fn bind_fragment(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        range: Range<usize>,
        output: &mut Vec<u8>,
    ) -> PyResult<()> {
        output.clear();
        self.fill(template, types, context, budget, range, output)
    }

    fn fill(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        range: Range<usize>,
        output: &mut Vec<u8>,
    ) -> PyResult<()> {
        let py = self.value.py();
        check(py, budget)?;
        let width = self.width();
        if width != template.slots() {
            return Err(errors::client(
                py,
                ClientErrorCode::ParameterCount,
                format!(
                    "SQL expects {} positions but {} provides {width}; current request was not sent",
                    template.slots(),
                    path_text(&self.path)
                ),
            ));
        }
        let mut visit = Visit {
            types,
            context,
            budget,
            active: SmallVec::new(),
            path: self.path,
            work: 0,
        };
        if let Some(identity) = self.outer {
            visit.enter(py, identity)?;
        }
        let row = if self.itemize && !self.zero_positions {
            Sequence::from(&self.value)
        } else {
            None
        };
        if let Some(row) = &row {
            visit.enter(py, row.identity())?;
        }
        let mut cursor = 0;
        for index in 0..width {
            let source = if let Some(row) = &row {
                row.get(index).inspect_err(|error| {
                    errors::note(
                        py,
                        error,
                        format!(
                            "while binding {}[{index}]; current request was not sent",
                            path_text(&visit.path)
                        ),
                    );
                })?
            } else {
                self.value.clone()
            };
            if template
                .append_range_until_slot(&range, &mut cursor, output, context.limit)
                .map_err(|error| errors::wire(py, error))?
                .is_none()
            {
                return Err(errors::client(
                    py,
                    ClientErrorCode::ParameterCount,
                    "binding position is unavailable",
                ));
            }
            if let Err(error) = visit.value(&source, output, row.as_ref().map(|_| index)) {
                errors::note(
                    py,
                    &error,
                    format!(
                        "while binding {}; current request was not sent",
                        path_text(&visit.path)
                    ),
                );
                return Err(error);
            }
        }
        if template
            .append_range_until_slot(&range, &mut cursor, output, context.limit)
            .map_err(|error| errors::wire(py, error))?
            .is_some()
        {
            return Err(errors::client(
                py,
                ClientErrorCode::ParameterCount,
                "unconsumed binding position",
            ));
        }
        check(py, budget)?;
        Ok(())
    }
}

struct Visit<'a> {
    types: &'a ScalarTypes,
    context: &'a Context,
    budget: &'a IoBudget,
    // Common shallow ancestry stays inline. This is an allocation hint, not
    // the nesting limit: deeper valid paths spill with checked reservation.
    active: SmallVec<[usize; 4]>,
    path: SmallVec<[usize; 4]>,
    work: usize,
}

/// Measure the production value visitor without copying its bytes into Python.
/// SQL compilation and positional row assembly are measured separately. This
/// entry retains every scalar, container, deadline, and capacity check used by
/// `Row::fill`; it does not implement an alternative escape algorithm.
pub(crate) fn encode_value(
    value: &Bound<'_, PyAny>,
    types: &ScalarTypes,
    context: &Context,
    budget: &IoBudget,
) -> PyResult<Vec<u8>> {
    check(value.py(), budget)?;
    let mut output = Vec::new();
    Visit {
        types,
        context,
        budget,
        active: SmallVec::new(),
        path: SmallVec::new(),
        work: 0,
    }
    .value(value, &mut output, None)?;
    check(value.py(), budget)?;
    Ok(output)
}

impl Visit<'_> {
    fn enter(&mut self, py: Python<'_>, identity: usize) -> PyResult<()> {
        if self.active.len() >= 64 || self.active.contains(&identity) {
            return Err(errors::client(
                py,
                ClientErrorCode::ContainerNesting,
                format!(
                    "{} contains a true cycle or exceeds 64 active container levels; current request was not sent",
                    path_text(&self.path)
                ),
            ));
        }
        self.active
            .try_reserve(1)
            .map_err(|error| pyo3::exceptions::PyMemoryError::new_err(error.to_string()))?;
        self.active.push(identity);
        Ok(())
    }
    /// Defer a leaf's diagnostic index until failure. Containers still keep
    /// their actual ancestry while traversing; successful scalars need no
    /// path allocation or push/pop and preserve the identical error location.
    fn value(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let py = value.py();
        self.work += 1;
        if self.work >= 16384 {
            if let Err(error) = check(py, self.budget) {
                if let Some(index) = child {
                    self.path.push(index);
                }
                return Err(error);
            }
            self.work = 0;
        }
        let Some(sequence) = Sequence::from(value) else {
            let result = self
                .types
                .write(value, output, self.context, Some(self.budget));
            if let Err(error) = &result {
                if let Some(index) = child {
                    self.path.push(index);
                }
                if let Ok(summary) = self.types.diagnostic(value) {
                    errors::note(py, error, format!("failed escape input: {summary}"));
                }
            }
            return result;
        };
        if let Some(index) = child {
            self.path.push(index);
        }
        self.enter(py, sequence.identity())?;
        let result = (|| {
            writer::append(output, b"(", self.context.limit)
                .map_err(|error| errors::wire(py, error))?;
            for index in 0..sequence.len() {
                if index != 0 {
                    writer::append(output, b",", self.context.limit)
                        .map_err(|error| errors::wire(py, error))?;
                }
                let value = sequence.get(index).inspect_err(|_| {
                    self.path.push(index);
                })?;
                self.value(&value, output, Some(index))?;
            }
            writer::append(output, b")", self.context.limit)
                .map_err(|error| errors::wire(py, error))
        })();
        self.active.pop();
        if result.is_ok() && child.is_some() {
            self.path.pop();
        }
        result
    }
}

fn path_text(path: &[usize]) -> String {
    let mut value = String::from("args");
    for index in path {
        value.push_str(&format!("[{index}]"));
    }
    value
}
pub(crate) fn template_error(py: Python<'_>, error: TemplateError) -> PyErr {
    match error {
        TemplateError::Wire(error) => errors::wire(py, error),
        error => errors::client(py, ClientErrorCode::TemplateBinding, error.to_string()),
    }
}
