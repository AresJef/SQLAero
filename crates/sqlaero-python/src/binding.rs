//! Incremental input-row adaptation and native positional request assembly.

use crate::errors;
use crate::input::sequence::Sequence;
use crate::input::source::{self, CellWriter, PreparedSource, Shape};
use crate::input::{ScalarTypes, check};
use pyo3::prelude::*;
use smallvec::{SmallVec, smallvec};
use sqlaero_core::bind::{
    Context,
    template::{Template, TemplateError},
    writer,
};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;
use std::ops::Range;

/// Retain an input owner and advance one logical row at a time. Native typed
/// providers retain their own current chunk; neither path materializes a batch.
pub(crate) struct Rows<'py> {
    root: Bound<'py, PyAny>,
    outer: Option<Sequence<'py>>,
    single: Option<Sequence<'py>>,
    native: Option<Box<dyn PreparedSource<'py> + 'py>>,
    itemize: bool,
    many: bool,
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
    pub(crate) fn is_batch(&self) -> bool {
        self.many
            || self
                .native
                .as_ref()
                .is_some_and(|v| matches!(v.shape(), Shape::Table { .. }))
    }
    pub(crate) fn new(
        root: &Bound<'py, PyAny>,
        many: bool,
        itemize: bool,
        types: &ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        Self::prepare(root, many, itemize, types, budget)
            .inspect_err(|error| note_preparation(root.py(), error, None))
    }
    fn prepare(
        root: &Bound<'py, PyAny>,
        many: bool,
        itemize: bool,
        types: &ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        let sequence = Sequence::from(root, types, &[])?;
        let native = if sequence.is_none() {
            source::prepare(root, types, budget)?
        } else {
            None
        };
        let outer = if many { sequence.clone() } else { None };
        let single = if many { None } else { sequence };
        let (count, empty) = match native.as_ref().map(|v| v.shape()) {
            Some(Shape::Vector { length }) => (if many { length.max(1) } else { 1 }, length == 0),
            Some(Shape::Record { fields }) => (1, fields == 0),
            Some(Shape::Table { rows, .. }) => (rows.max(1), rows == 0),
            None => (
                outer.as_ref().map_or(1, |v| v.len().max(1)),
                outer.as_ref().is_some_and(|v| v.len() == 0),
            ),
        };
        Ok(Self {
            root: root.clone(),
            outer,
            single,
            native,
            itemize,
            many,
            position: 0,
            count,
            empty,
        })
    }
    pub(crate) fn next<'r>(
        &'r mut self,
        types: &ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Option<Row<'r, 'py>>> {
        if self.position >= self.count {
            return Ok(None);
        }
        let index = self.position;
        self.position += 1;
        self.at(index, types, budget).map(Some)
    }
    /// Re-encode only the retained rollover row after a real session change.
    pub(crate) fn at<'r>(
        &'r mut self,
        index: usize,
        types: &ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Row<'r, 'py>> {
        let py = self.root.py();
        let path = (self.outer.is_some() && !self.empty).then_some(index);
        self.prepare_at(index, types, budget)
            .inspect_err(|error| note_preparation(py, error, path))
    }
    fn prepare_at<'r>(
        &'r mut self,
        index: usize,
        types: &ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Row<'r, 'py>> {
        if let Some(native) = self.native.as_mut() {
            let shape = native.shape();
            let native_row = match shape {
                Shape::Table { .. } => Some(index),
                Shape::Vector { .. } if self.many && !self.empty => Some(index),
                _ => None,
            };
            let path = if native_row.is_some() {
                smallvec![index]
            } else {
                SmallVec::new()
            };
            return Ok(Row {
                value: self.root.clone(),
                sequence: None,
                outer: None,
                path,
                itemize: self.itemize,
                zero_positions: self.empty || matches!(shape, Shape::Table { columns: 0, .. }),
                native: Some(NativeRow {
                    source: SourceRef::Borrowed(native.as_mut()),
                    row: native_row,
                }),
            });
        }
        let (value, outer, path) = match &self.outer {
            Some(outer) if !self.empty => {
                let value = outer.get(index)?;
                (value, Some(outer.identity()), smallvec![index])
            }
            _ => (self.root.clone(), None, SmallVec::new()),
        };
        let sequence = if !self.empty {
            if self.outer.is_none() {
                self.single.clone()
            } else {
                Sequence::from(&value, types, &[])?
            }
        } else {
            None
        };
        let native = if sequence.is_none() && !self.empty {
            source::prepare(&value, types, budget)?.map(|source| NativeRow {
                source: SourceRef::Owned(source),
                row: None,
            })
        } else {
            None
        };
        Ok(Row {
            value,
            sequence,
            outer,
            path,
            itemize: self.itemize,
            zero_positions: self.empty,
            native,
        })
    }
    pub(crate) fn position(&self) -> usize {
        self.position
    }
}

fn note_preparation(py: Python<'_>, error: &PyErr, index: Option<usize>) {
    if let Some(index) = index {
        errors::note_format(
            py,
            error,
            format_args!("while binding args[{index}]; current request was not sent"),
        );
    } else {
        errors::note_format(
            py,
            error,
            format_args!("while binding args; current request was not sent"),
        );
    }
}

enum SourceRef<'r, 'py> {
    Borrowed(&'r mut (dyn PreparedSource<'py> + 'py)),
    Owned(Box<dyn PreparedSource<'py> + 'py>),
}
impl<'py> SourceRef<'_, 'py> {
    fn source(&mut self) -> &mut (dyn PreparedSource<'py> + 'py) {
        match self {
            Self::Borrowed(v) => *v,
            Self::Owned(v) => v.as_mut(),
        }
    }
    fn shape(&self) -> Shape {
        match self {
            Self::Borrowed(v) => v.shape(),
            Self::Owned(v) => v.shape(),
        }
    }
}
struct NativeRow<'r, 'py> {
    source: SourceRef<'r, 'py>,
    row: Option<usize>,
}

pub(crate) struct Row<'r, 'py> {
    value: Bound<'py, PyAny>,
    sequence: Option<Sequence<'py>>,
    native: Option<NativeRow<'r, 'py>>,
    outer: Option<usize>,
    path: SmallVec<[usize; 4]>,
    itemize: bool,
    zero_positions: bool,
}
impl Row<'_, '_> {
    pub(crate) fn width(&self) -> usize {
        if self.zero_positions {
            return 0;
        }
        if !self.itemize {
            return 1;
        }
        if let Some(native) = &self.native {
            return match (native.source.shape(), native.row) {
                (Shape::Vector { length }, None) => length,
                (Shape::Record { fields }, _) => fields,
                (Shape::Table { columns, .. }, Some(_)) => columns,
                _ => 1,
            };
        }
        self.sequence.as_ref().map_or(1, Sequence::len)
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
        mut self,
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
            native_depth: 0,
            active_depth: 0,
        };
        if let Some(identity) = self.outer {
            visit.enter(py, identity)?;
        }
        let sequence = self.sequence.take();
        let row = if self.itemize {
            sequence.as_ref()
        } else {
            None
        };
        if let Some(row) = row {
            visit.enter_sequence(py, row)?;
        }
        if self.native.is_some() {
            visit.enter(py, self.value.as_ptr() as usize)?;
        }
        let mut cursor = 0;
        for index in 0..width {
            let source = if let Some(row) = row {
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
            let result = if let Some(native) = &mut self.native {
                let shape = native.source.shape();
                let source_row = native.row;
                if self.itemize {
                    let coordinates = match (shape, source_row) {
                        (Shape::Vector { .. }, Some(row)) => (row, 0),
                        (Shape::Vector { .. }, None) => (index, 0),
                        (Shape::Record { .. }, _) => (0, index),
                        (Shape::Table { .. }, Some(row)) => (row, index),
                        (Shape::Table { .. }, None) => (0, 0),
                    };
                    if matches!((shape, source_row), (Shape::Table { .. }, None)) {
                        visit.native_value(native.source.source(), output)
                    } else {
                        let positional =
                            !matches!((shape, source_row), (Shape::Vector { .. }, Some(_)));
                        if positional {
                            visit.push_index(py, index)?;
                        }
                        let result = native.source.source().write_cell(
                            coordinates.0,
                            coordinates.1,
                            output,
                            context,
                            budget,
                            &mut visit,
                        );
                        if result.is_ok() && positional {
                            visit.pop_index();
                        }
                        result
                    }
                } else if let Some(row) = source_row {
                    match shape {
                        Shape::Vector { .. } => native
                            .source
                            .source()
                            .write_cell(row, 0, output, context, budget, &mut visit),
                        Shape::Table { columns, .. } => {
                            visit.native_row(native.source.source(), row, columns, output)
                        }
                        Shape::Record { .. } => visit.native_value(native.source.source(), output),
                    }
                } else {
                    visit.native_value(native.source.source(), output)
                }
            } else if let Some(sequence) = sequence.as_ref().filter(|_| !self.itemize) {
                visit.sequence(sequence, output, None)
            } else {
                visit.value(&source, output, row.map(|_| index))
            };
            if let Err(error) = result {
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
    native_depth: usize,
    active_depth: usize,
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
        native_depth: 0,
        active_depth: 0,
    }
    .value(value, &mut output, None)?;
    check(value.py(), budget)?;
    Ok(output)
}

impl Visit<'_> {
    fn enter(&mut self, py: Python<'_>, identity: usize) -> PyResult<()> {
        if self.active_depth + self.native_depth >= 64 || self.active.contains(&identity) {
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
            .map_err(|_| errors::memory_error(py))?;
        self.active.push(identity);
        self.active_depth += 1;
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
        // Preserve the two exact-container checks in front of primitive
        // writes. Extended families are resolved only after scalar rejection.
        let sequence = if value.is_exact_instance_of::<pyo3::types::PyList>()
            || value.is_exact_instance_of::<pyo3::types::PyTuple>()
        {
            Sequence::from(value, self.types, &self.active)?
        } else {
            match self
                .types
                .try_write(value, output, self.context, Some(self.budget))
            {
                Ok(true) => return Ok(()),
                Err(error) => {
                    self.failed_value(value, child, &error);
                    return Err(error);
                }
                Ok(false) => (),
            }
            match Sequence::from(value, self.types, &self.active) {
                Ok(sequence) => sequence,
                Err(error) => {
                    self.failed_value(value, child, &error);
                    return Err(error);
                }
            }
        };
        if let Some(sequence) = sequence {
            self.sequence(&sequence, output, child)
        } else {
            self.native_or_unsupported(value, output, child)
        }
    }

    fn enter_sequence(&mut self, py: Python<'_>, sequence: &Sequence<'_>) -> PyResult<()> {
        self.enter(py, sequence.identity())?;
        if let Some(alias) = sequence.alias() {
            if self.active.contains(&alias) {
                return Err(errors::client(
                    py,
                    ClientErrorCode::ContainerNesting,
                    "normalized input contains a true cycle",
                ));
            }
            self.active
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            self.active.push(alias);
        }
        Ok(())
    }

    fn sequence(
        &mut self,
        sequence: &Sequence<'_>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let py = sequence.py();
        if let Some(index) = child {
            self.path.push(index);
        }
        self.enter_sequence(py, sequence)?;
        let result = (|| {
            writer::append(output, b"(", self.context.limit)
                .map_err(|error| errors::wire(py, error))?;
            for index in 0..sequence.len() {
                if index != 0 {
                    writer::append(output, b",", self.context.limit)
                        .map_err(|error| errors::wire(py, error))?;
                }
                sequence.with_item(index, |value| {
                    let value = value.inspect_err(|_| {
                        self.path.push(index);
                    })?;
                    self.value(value, output, Some(index))
                })?;
            }
            writer::append(output, b")", self.context.limit)
                .map_err(|error| errors::wire(py, error))
        })();
        if sequence.alias().is_some() {
            self.active.pop();
        }
        self.active.pop();
        self.active_depth -= 1;
        if result.is_ok() && child.is_some() {
            self.path.pop();
        }
        result
    }

    #[cold]
    fn native_or_unsupported(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let py = value.py();
        if let Some(index) = child {
            self.push_index(py, index)?;
        }
        let result = (|| {
            let Some(mut source) = source::prepare(value, self.types, self.budget)? else {
                return Err(crate::input::unsupported(value));
            };
            self.enter(py, value.as_ptr() as usize)?;
            let result = self.native_value(source.as_mut(), output);
            self.active.pop();
            self.active_depth -= 1;
            result
        })();
        if let Err(error) = &result {
            self.failed_value(value, None, error);
        }
        if result.is_ok() && child.is_some() {
            self.pop_index();
        }
        result
    }

    fn native_value<'py>(
        &mut self,
        source: &mut dyn PreparedSource<'py>,
        output: &mut Vec<u8>,
    ) -> PyResult<()> {
        let py = source.owner().py();
        match source.shape() {
            Shape::Vector { length } => {
                writer::append(output, b"(", self.context.limit)
                    .map_err(|e| errors::wire(py, e))?;
                for index in 0..length {
                    if index != 0 {
                        writer::append(output, b",", self.context.limit)
                            .map_err(|e| errors::wire(py, e))?;
                    }
                    self.push_index(py, index)?;
                    source.write_cell(index, 0, output, self.context, self.budget, self)?;
                    self.pop_index();
                }
                writer::append(output, b")", self.context.limit).map_err(|e| errors::wire(py, e))
            }
            Shape::Record { fields } => self.native_row(source, 0, fields, output),
            Shape::Table { rows, columns } => {
                writer::append(output, b"(", self.context.limit)
                    .map_err(|e| errors::wire(py, e))?;
                for row in 0..rows {
                    if row != 0 {
                        writer::append(output, b",", self.context.limit)
                            .map_err(|e| errors::wire(py, e))?;
                    }
                    self.push_index(py, row)?;
                    self.enter_native(py)?;
                    let result = self.native_row(source, row, columns, output);
                    self.leave_native();
                    result?;
                    self.pop_index();
                }
                writer::append(output, b")", self.context.limit).map_err(|e| errors::wire(py, e))
            }
        }
    }

    fn native_row<'py>(
        &mut self,
        source: &mut dyn PreparedSource<'py>,
        row: usize,
        columns: usize,
        output: &mut Vec<u8>,
    ) -> PyResult<()> {
        let py = source.owner().py();
        writer::append(output, b"(", self.context.limit).map_err(|e| errors::wire(py, e))?;
        for column in 0..columns {
            if column != 0 {
                writer::append(output, b",", self.context.limit)
                    .map_err(|e| errors::wire(py, e))?;
            }
            self.push_index(py, column)?;
            source.write_cell(row, column, output, self.context, self.budget, self)?;
            self.pop_index();
        }
        writer::append(output, b")", self.context.limit).map_err(|e| errors::wire(py, e))
    }

    /// Keep Python diagnostic construction off the successful traversal frame.
    /// The actual failure, child index, and best-effort note stay unchanged.
    #[cold]
    #[inline(never)]
    fn failed_value(&mut self, value: &Bound<'_, PyAny>, child: Option<usize>, error: &PyErr) {
        if let Some(index) = child {
            self.path.push(index);
        }
        if let Ok(summary) = self.types.diagnostic(value) {
            errors::note(value.py(), error, format!("failed escape input: {summary}"));
        }
    }
}

impl CellWriter for Visit<'_> {
    fn python(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        self.value(value, output, child)
    }
    fn enter_native(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.active_depth + self.native_depth >= 64 {
            return Err(errors::client(
                py,
                ClientErrorCode::ContainerNesting,
                format!(
                    "{} exceeds 64 active container levels; current request was not sent",
                    path_text(&self.path)
                ),
            ));
        }
        self.native_depth += 1;
        Ok(())
    }
    fn leave_native(&mut self) {
        self.native_depth -= 1;
    }
    fn push_index(&mut self, py: Python<'_>, index: usize) -> PyResult<()> {
        self.path
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        self.path.push(index);
        Ok(())
    }
    fn pop_index(&mut self) {
        self.path.pop();
    }
    fn checkpoint(&mut self, py: Python<'_>, work: usize) -> PyResult<()> {
        self.work = self.work.saturating_add(work);
        if self.work >= 16384 {
            self.work = 0;
            check(py, self.budget)?;
        }
        Ok(())
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
