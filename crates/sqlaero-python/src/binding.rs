//! Incremental input-row adaptation and native positional request assembly.

use crate::errors;
use crate::input::sequence::Sequence;
use crate::input::source::{self, CellWriter, FragmentRole, PreparedSource, Shape};
use crate::input::{ScalarTypes, check};
use pyo3::prelude::*;
use pyo3::types::PyString;
use smallvec::{SmallVec, smallvec};
use sqlaero_core::bind::{
    Context,
    batch::{Blocks, Layout},
    template::{Compiler, Template, TemplateError},
    writer,
};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;
use std::ops::Range;
use std::sync::Arc;

pub(crate) mod call;
mod prepared_display;
#[cfg(test)]
mod prepared_native_tests;

/// Select the request boundary policy. Prepared display is readable SQL,
/// never the binary payload and never eligible for text VALUES merging.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanMode {
    Text,
    PreparedDisplay,
    PreparedNative,
}

/// Lexical and binding identity only. The execution owner also checks actual
/// database/collation/full SQL-mode facts before reusing a server handle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreparedKey {
    pub(crate) charset: &'static str,
    pub(crate) no_backslash: bool,
    pub(crate) ansi: bool,
    pub(crate) bound: bool,
}
impl PreparedKey {
    fn new(context: &Context, ansi: bool, bound: bool) -> Self {
        Self {
            charset: context.codec.name(),
            no_backslash: context.no_backslash_escapes,
            ansi,
            bound,
        }
    }
}

/// Shared complete COM_STMT_PREPARE source, never a Python SQL object. Cloning
/// a row request retains the same native buffer instead of copying its SQL.
#[derive(Clone)]
pub(crate) struct PreparedTemplate {
    pub(crate) request: Arc<Vec<u8>>,
    pub(crate) parameters: u16,
    pub(crate) key: PreparedKey,
}

/// One complete target request and its actual logical input coverage.
pub(crate) struct Request {
    pub(crate) bytes: Vec<u8>,
    pub(crate) source_rows: Range<usize>,
    pub(crate) final_request: bool,
    pub(crate) prepared: Option<PreparedTemplate>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ContextKey {
    charset: &'static str,
    no_backslash: bool,
    ansi: bool,
    utc: bool,
    limit: usize,
}
impl ContextKey {
    fn new(context: &Context, ansi: bool) -> Self {
        Self {
            charset: context.codec.name(),
            no_backslash: context.no_backslash_escapes,
            ansi,
            utc: context.utc_confirmed,
            limit: context.limit,
        }
    }
}

struct Prepared {
    key: ContextKey,
    template: Template,
    layout: Option<Layout>,
    blocks: Option<Blocks>,
    native: Option<PreparedTemplate>,
}
impl Prepared {
    fn new(
        sql: &Bound<'_, PyString>,
        context: &Context,
        ansi: bool,
        merge: bool,
        mode: PlanMode,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        let py = sql.py();
        let mut encoded = Vec::new();
        crate::input::text(
            sql,
            &mut encoded,
            &Context {
                limit: usize::MAX,
                ..*context
            },
            false,
            Some(budget),
        )?;
        let mut compiler = if mode == PlanMode::PreparedNative {
            Compiler::for_prepared(encoded, context.codec, context.no_backslash_escapes, ansi)
        } else if mode == PlanMode::PreparedDisplay {
            Compiler::for_prepared_display(
                encoded,
                context.codec,
                context.no_backslash_escapes,
                ansi,
            )
        } else if merge {
            Compiler::for_batch(encoded, context.codec, context.no_backslash_escapes, ansi)
        } else {
            Compiler::new(encoded, context.codec, context.no_backslash_escapes, ansi)
        };
        let mut template = loop {
            let step = py
                .detach(|| compiler.advance())
                .map_err(|error| template_error(py, error))?;
            check(py, budget)?;
            if let Some(template) = step {
                break template;
            }
        };
        if mode == PlanMode::PreparedDisplay {
            validate_prepared_template(py, &template, context.limit, false, budget)?;
        }
        let native = if mode == PlanMode::PreparedNative {
            let parameters = prepared_count(py, template.slots())?;
            Some(PreparedTemplate {
                request: Arc::new(validate_prepared_template(
                    py,
                    &template,
                    context.limit,
                    false,
                    budget,
                )?),
                parameters,
                key: PreparedKey::new(context, ansi, true),
            })
        } else {
            None
        };
        let layout = if mode == PlanMode::Text {
            template.prepare_merge()
        } else {
            None
        };
        let blocks = layout
            .as_ref()
            .map(|layout| Blocks::new(&template, layout, context.limit, context.limit))
            .transpose()
            .map_err(|error| errors::wire(py, error))?;
        Ok(Self {
            key: ContextKey::new(context, ansi),
            template,
            layout,
            blocks,
            native,
        })
    }
}

/// Incrementally assemble target requests without owning a command lease or
/// reading session facts. The caller supplies a confirmed context per request;
/// auxiliary callers can repeatedly supply one historical reference instead.
pub(crate) struct Planner<'types, 'py> {
    sql: Bound<'py, PyString>,
    rows: Option<Rows<'py>>,
    types: &'types ScalarTypes,
    mode: PlanMode,
    merge: bool,
    prepared: Option<Prepared>,
    pending: Option<(usize, Vec<u8>, ContextKey, FragmentRole)>,
    source_end: usize,
    finished: bool,
}
impl<'types, 'py> Planner<'types, 'py> {
    pub(crate) fn is_batch(&self) -> bool {
        self.rows.as_ref().is_some_and(Rows::is_batch)
    }

    pub(crate) fn new(
        sql: &Bound<'py, PyString>,
        args: Option<&Bound<'py, PyAny>>,
        many: bool,
        itemize: bool,
        mode: PlanMode,
        types: &'types ScalarTypes,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        check(sql.py(), budget)?;
        let rows = args
            .map(|args| Rows::new(args, many, itemize, types, budget))
            .transpose()?;
        let merge = mode == PlanMode::Text && rows.as_ref().is_some_and(Rows::is_batch);
        Ok(Self {
            sql: sql.clone(),
            rows,
            types,
            mode,
            merge,
            prepared: None,
            pending: None,
            source_end: 0,
            finished: false,
        })
    }

    pub(crate) fn next(
        &mut self,
        py: Python<'py>,
        context: &Context,
        ansi: bool,
        budget: &IoBudget,
    ) -> PyResult<Option<Request>> {
        if self.finished {
            return Ok(None);
        }
        check(py, budget)?;
        let Some(rows) = self.rows.as_mut() else {
            let (bytes, prepared) = if self.mode != PlanMode::Text {
                let mut encoded = Vec::new();
                crate::input::text(
                    &self.sql,
                    &mut encoded,
                    &Context {
                        limit: usize::MAX,
                        ..*context
                    },
                    false,
                    Some(budget),
                )?;
                let mut compiler = Compiler::for_unbound_prepared(
                    encoded,
                    context.codec,
                    context.no_backslash_escapes,
                    ansi,
                );
                let template = loop {
                    let step = py
                        .detach(|| compiler.advance())
                        .map_err(|error| template_error(py, error))?;
                    check(py, budget)?;
                    if let Some(template) = step {
                        break template;
                    }
                };
                use sqlaero_core::protocol::prepared::Parameters;
                let native = Parameters::new(0, 0, context.limit)
                    .and_then(Parameters::finish)
                    .map_err(|error| crate::input::sink::failure(py, error))?;
                let mut bytes =
                    validate_prepared_template(py, &template, context.limit, true, budget)?;
                if self.mode == PlanMode::PreparedNative {
                    (
                        native,
                        Some(PreparedTemplate {
                            request: Arc::new(bytes),
                            parameters: 0,
                            key: PreparedKey::new(context, ansi, false),
                        }),
                    )
                } else {
                    drop(native);
                    bytes[0] = 3;
                    (bytes, None)
                }
            } else {
                let mut bytes = vec![3];
                crate::input::text(&self.sql, &mut bytes, context, false, Some(budget))?;
                (bytes, None)
            };
            self.finished = true;
            return Ok(Some(Request {
                bytes,
                source_rows: 0..0,
                final_request: true,
                prepared,
            }));
        };
        let key = ContextKey::new(context, ansi);
        if self
            .prepared
            .as_ref()
            .is_none_or(|prepared| prepared.key != key)
        {
            self.prepared = Some(Prepared::new(
                &self.sql,
                context,
                ansi,
                self.merge && self.mode == PlanMode::Text,
                self.mode,
                budget,
            )?);
        }
        let prepared = self.prepared.as_mut().ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "SQL preparation was not published",
            )
        })?;
        let start = self.source_end;
        let empty = rows.empty_root();
        let (bytes, end) =
            if let (Some(layout), Some(blocks)) = (&prepared.layout, &mut prepared.blocks) {
                let mut fragment = Vec::new();
                let mut end = start;
                let mut ordinary = None;
                loop {
                    let index;
                    let role;
                    if let Some((saved, bytes, saved_key, saved_role)) = self.pending.take() {
                        index = saved;
                        fragment = bytes;
                        role = if saved_key != key {
                            rows.at(index, self.types, budget)?.bind_repeated(
                                &prepared.template,
                                self.types,
                                context,
                                budget,
                                layout,
                                &mut fragment,
                            )?
                        } else {
                            saved_role
                        };
                    } else {
                        index = rows.position();
                        let Some(row) = rows.next(self.types, budget)? else {
                            break;
                        };
                        role = row.bind_repeated(
                            &prepared.template,
                            self.types,
                            context,
                            budget,
                            layout,
                            &mut fragment,
                        )?;
                    }
                    if role != FragmentRole::Row {
                        blocks
                            .check_single(&fragment)
                            .map_err(|error| errors::wire(py, error))?;
                        if blocks.rows() != 0 {
                            self.pending = Some((index, fragment, key, role));
                        } else {
                            ordinary = Some(
                                blocks
                                    .single(&fragment)
                                    .map_err(|error| errors::wire(py, error))?,
                            );
                            end = if empty { 0 } else { index + 1 };
                        }
                        break;
                    }
                    if !blocks
                        .push(&fragment)
                        .map_err(|error| errors::wire(py, error))?
                    {
                        self.pending = Some((index, fragment, key, role));
                        break;
                    }
                    end = if empty { 0 } else { index + 1 };
                }
                let bytes = match ordinary {
                    Some(bytes) => bytes,
                    None => {
                        let Some(bytes) = blocks.take().map_err(|error| errors::wire(py, error))?
                        else {
                            self.finished = true;
                            return Ok(None);
                        };
                        bytes
                    }
                };
                (bytes, end)
            } else {
                let index = self
                    .pending
                    .as_ref()
                    .map_or(rows.position(), |pending| pending.0);
                let row = if self.pending.take().is_some() {
                    rows.at(index, self.types, budget)?
                } else {
                    let Some(row) = rows.next(self.types, budget)? else {
                        self.finished = true;
                        return Ok(None);
                    };
                    row
                };
                let bytes = match self.mode {
                    PlanMode::PreparedDisplay => {
                        row.display_parameters(&prepared.template, self.types, context, budget)?
                    }
                    PlanMode::PreparedNative => row.native_parameters(
                        &prepared.template,
                        prepared
                            .native
                            .as_ref()
                            .ok_or_else(|| {
                                errors::client(
                                    py,
                                    ClientErrorCode::InternalState,
                                    "native prepared template was not retained",
                                )
                            })?
                            .parameters,
                        self.types,
                        context,
                        budget,
                    )?,
                    PlanMode::Text => row.bind(&prepared.template, self.types, context, budget)?,
                };
                (bytes, if empty { 0 } else { index + 1 })
            };
        self.source_end = end;
        let final_request = self.pending.is_none() && rows.remaining() == 0;
        self.finished = final_request;
        check(py, budget)?;
        Ok(Some(Request {
            bytes,
            source_rows: start..end,
            final_request,
            prepared: prepared.native.clone(),
        }))
    }
}

fn prepared_count(py: Python<'_>, count: usize) -> PyResult<u16> {
    u16::try_from(count).map_err(|_| {
        errors::client(
            py,
            ClientErrorCode::ParameterCount,
            "prepared parameters exceed the native u16 position count",
        )
    })
}

fn validate_prepared_template(
    py: Python<'_>,
    template: &Template,
    limit: usize,
    unbound: bool,
    budget: &IoBudget,
) -> PyResult<Vec<u8>> {
    let mut writer = if unbound {
        template.prepare_unbound_request(limit)
    } else {
        template.prepare_request(limit)
    }
    .map_err(|error| errors::wire(py, error))?;
    loop {
        let step = py
            .detach(|| writer.advance())
            .map_err(|error| errors::wire(py, error))?;
        check(py, budget)?;
        if let Some(request) = step {
            return Ok(request);
        }
    }
}

/// Materialize final SQL text using the writer's connection codec, never
/// result metadata or a Python codec callback. UTF-8 has no native transcode.
pub(crate) fn display<'py>(
    py: Python<'py>,
    bytes: &[u8],
    codec: sqlaero_core::charset::codec::Codec,
) -> PyResult<Bound<'py, PyString>> {
    use sqlaero_core::charset::output::OutputCodec;
    // These strict routes are self-inverse over their complete valid domains:
    // UTF-8 encodes exactly its decoded scalar bytes, ASCII exactly its bytes.
    let route = match codec.name() {
        "utf8mb4" => return crate::output::text(py, bytes, OutputCodec::Utf8(codec), true, None),
        "ascii" => return crate::output::text(py, bytes, OutputCodec::Ascii(codec), true, None),
        "binary" => OutputCodec::ByteValues(codec),
        _ => OutputCodec::Mysql(codec),
    };
    let points = crate::output::points(py, bytes, route, true, None)?;
    const CHUNK: usize = 256 * 1024;
    let mut text = String::new();
    text.try_reserve_exact(points.len().saturating_mul(4).min(CHUNK + 4))
        .map_err(|_| errors::memory_error(py))?;
    let mut encoded = Vec::new();
    let mut consumed = 0;
    let mut verify = |text: &str| -> PyResult<()> {
        encoded.clear();
        codec
            .encode(text, &mut encoded, usize::MAX)
            .map_err(|error| match error {
                sqlaero_core::charset::codec::CodecError::Wire(error) => errors::wire(py, error),
                _ => display_error(py, codec.name()),
            })?;
        if !bytes
            .get(consumed..)
            .is_some_and(|remaining| remaining.starts_with(&encoded))
        {
            return Err(display_error(py, codec.name()));
        }
        consumed += encoded.len();
        py.check_signals()
    };
    for point in points.iter() {
        let point = char::from_u32(*point).ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "strict text decoder produced a non-scalar value",
            )
        })?;
        text.push(point);
        if text.len() >= CHUNK {
            verify(&text)?;
            text.clear();
        }
    }
    if !text.is_empty() {
        verify(&text)?;
    }
    if consumed != bytes.len() {
        return Err(display_error(py, codec.name()));
    }
    crate::output::codepoints(py, &points)
}

fn display_error(py: Python<'_>, charset: &str) -> PyErr {
    errors::builtin_message::<pyo3::exceptions::PyValueError>(
        py,
        format!(
            "auxiliary SQL text for charset {charset} cannot be represented losslessly as str: decoding and re-encoding changes bytes; no SQL was executed"
        ),
    )
}

/// Project the existing native input roles only at the public auxiliary
/// boundary. All final fragments are strings; no result-pool quota or SQL
/// timeout is inherited, and no output feeds back into execution.
pub(crate) fn escape<'py>(
    data: &Bound<'py, PyAny>,
    many: bool,
    itemize: bool,
    types: &ScalarTypes,
    context: &Context,
) -> PyResult<Bound<'py, PyAny>> {
    use sqlaero_core::transport::deadline::TimeoutKind;
    let py = data.py();
    let context = Context {
        limit: usize::MAX,
        ..*context
    };
    let budget = IoBudget::new(TimeoutKind::Operation, None, None, None);
    let mut rows = Rows::new(data, many, itemize, types, &budget)?;
    if rows.public_batch() {
        let output = crate::output::list(py, [])?;
        while let Some(row) = rows.next(types, &budget)? {
            output.append(row.project(py, types, &context, &budget)?)?;
        }
        check(py, &budget)?;
        Ok(output.into_any())
    } else {
        let row = rows.next(types, &budget)?.ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "root projection did not produce its logical row",
            )
        })?;
        let value = row.project(py, types, &context, &budget)?;
        check(py, &budget)?;
        Ok(value)
    }
}

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
    fn public_batch(&self) -> bool {
        self.outer.is_some()
            || self
                .native
                .as_ref()
                .is_some_and(|source| match source.shape() {
                    Shape::Table { .. } => true,
                    Shape::Vector { .. } => self.many,
                    Shape::Record { .. } => false,
                })
    }
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
        let (outer, single) = if many {
            (sequence, None)
        } else {
            (None, sequence)
        };
        let (count, empty) = match native.as_ref().map(|v| v.shape()) {
            Some(Shape::Vector { length }) => (if many { length.max(1) } else { 1 }, length == 0),
            Some(Shape::Record { .. }) => (1, false),
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
                outer: None,
                path,
                itemize: self.itemize,
                zero_positions: self.empty || matches!(shape, Shape::Table { columns: 0, .. }),
                input: RowInput::Native(NativeRow {
                    source: SourceRef::Borrowed(native.as_mut()),
                    row: native_row,
                }),
            });
        }
        let (value, outer, path) = match &self.outer {
            Some(outer) if !self.empty => {
                let value = outer.row_item(index)?;
                (value, Some(outer.identity()), smallvec![index])
            }
            _ => (self.root.clone(), None, SmallVec::new()),
        };
        let sequence = if !self.empty {
            if self.outer.is_none() {
                self.single.as_ref().map(SequenceRef::Borrowed)
            } else {
                Sequence::from(&value, types, &[])?.map(SequenceRef::Owned)
            }
        } else {
            None
        };
        let input = if let Some(sequence) = sequence {
            RowInput::Sequence(sequence)
        } else if !self.empty {
            match source::prepare(&value, types, budget)? {
                Some(source) => RowInput::Native(NativeRow {
                    source: SourceRef::Owned(source),
                    row: None,
                }),
                None => RowInput::Scalar,
            }
        } else {
            RowInput::Scalar
        };
        Ok(Row {
            value,
            input,
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

/// A single root strategy stays in Rows; a batch child owns its independently
/// prepared strategy. Re-encoding never clones an iterator or normalized Box.
enum SequenceRef<'r, 'py> {
    Borrowed(&'r Sequence<'py>),
    Owned(Sequence<'py>),
}
impl<'py> SequenceRef<'_, 'py> {
    fn sequence(&self) -> &Sequence<'py> {
        match self {
            Self::Borrowed(value) => value,
            Self::Owned(value) => value,
        }
    }
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

/// A row has one prepared strategy; ordinary scalars need neither provider.
enum RowInput<'r, 'py> {
    Scalar,
    Sequence(SequenceRef<'r, 'py>),
    Native(NativeRow<'r, 'py>),
}

pub(crate) struct Row<'r, 'py> {
    value: Bound<'py, PyAny>,
    input: RowInput<'r, 'py>,
    outer: Option<usize>,
    path: SmallVec<[usize; 4]>,
    itemize: bool,
    zero_positions: bool,
}
impl Row<'_, '_> {
    fn public_row(&self) -> bool {
        self.zero_positions
            || (self.itemize
                && match &self.input {
                    RowInput::Sequence(_) => true,
                    RowInput::Native(native) => matches!(
                        (native.source.shape(), native.row),
                        (Shape::Vector { .. }, None)
                            | (Shape::Record { .. }, _)
                            | (Shape::Table { .. }, Some(_))
                    ),
                    RowInput::Scalar => false,
                })
    }

    fn project<'py>(
        self,
        py: Python<'py>,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<Bound<'py, PyAny>> {
        let row = self.public_row();
        let mut values = Vec::new();
        let mut encoded = Vec::new();
        self.walk::<false>(
            types,
            context,
            budget,
            &mut encoded,
            None,
            |_, output| {
                output.clear();
                Ok(false)
            },
            |_, output, _| {
                let value = display(py, output, context.codec)?.into_any().unbind();
                values
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                values.push(value);
                Ok(())
            },
        )?;
        if row {
            crate::output::tuple_from_ready_vec(py, values).map(Bound::into_any)
        } else {
            values
                .pop()
                .map(|value| value.into_bound(py))
                .ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "scalar projection did not produce a value",
                    )
                })
        }
    }

    pub(crate) fn width(&self) -> usize {
        if self.zero_positions {
            return 0;
        }
        if !self.itemize {
            return 1;
        }
        match &self.input {
            RowInput::Native(native) => match (native.source.shape(), native.row) {
                (Shape::Vector { length }, None) => length,
                (Shape::Record { fields }, _) => fields,
                (Shape::Table { columns, .. }, Some(_)) => columns,
                _ => 1,
            },
            RowInput::Sequence(value) => value.sequence().len(),
            RowInput::Scalar => 1,
        }
    }

    /// Adapt one logical row directly into native prepared parameters.
    /// Container roles are the same roles used by positional text binding.
    pub(crate) fn write_parameters<
        S: sqlaero_core::bind::value::ValueSink<Error = sqlaero_core::bind::value::ValueError>,
    >(
        self,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        sink: &mut S,
    ) -> PyResult<()> {
        self.write_parameters_checked(types, context, budget, sink, None, |_, _| Ok(()))
    }

    fn write_parameters_checked<
        S: sqlaero_core::bind::value::ValueSink<Error = sqlaero_core::bind::value::ValueError>,
    >(
        mut self,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        sink: &mut S,
        template: Option<&Template>,
        mut after: impl FnMut(usize, &S) -> PyResult<()>,
    ) -> PyResult<()> {
        let py = self.value.py();
        check(py, budget)?;
        let width = self.width();
        let mut visit = Visit {
            types,
            context: *context,
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
        let sequence = match std::mem::replace(&mut self.input, RowInput::Scalar) {
            RowInput::Sequence(sequence) => Some(sequence),
            input => {
                self.input = input;
                None
            }
        };
        let sequence = sequence
            .as_ref()
            .filter(|_| self.itemize)
            .map(SequenceRef::sequence);
        if let Some(sequence) = sequence {
            visit.enter_sequence(py, sequence)?;
        }
        let mut native = match &mut self.input {
            RowInput::Native(native) => Some(native),
            _ => None,
        };
        if let Some(native) = native.as_ref() {
            visit.enter(py, self.value.as_ptr() as usize)?;
            if matches!(
                (native.source.shape(), native.row),
                (Shape::Table { .. }, Some(_))
            ) {
                visit.enter_native(py)?;
            }
        }
        for index in 0..width {
            let effective = slot_context(template, index, context);
            visit.context = effective;
            let result = if let Some(native) = native.as_deref_mut() {
                let shape = native.source.shape();
                let coordinates = match (self.itemize, shape, native.row) {
                    (_, Shape::Vector { .. }, Some(row)) => Some((row, 0, false)),
                    (true, Shape::Vector { .. }, None) => Some((index, 0, true)),
                    (true, Shape::Record { .. }, _) => Some((0, index, true)),
                    (true, Shape::Table { .. }, Some(row)) => Some((row, index, true)),
                    _ => None,
                };
                if let Some((row, column, positional)) = coordinates {
                    if positional {
                        visit.push_index(py, index)?;
                    }
                    let result = native
                        .source
                        .source()
                        .write_parameter_cell(row, column, sink, &effective, budget, &mut visit);
                    if result.is_ok() && positional {
                        visit.pop_index();
                    }
                    result
                } else {
                    Err(crate::input::sink::fragment_error(py))
                }
            } else {
                let value = match sequence {
                    Some(sequence) => sequence.row_item(index).inspect_err(|error| {
                        errors::note_format(
                            py,
                            error,
                            format_args!(
                                "while binding {}[{index}]; current request was not sent",
                                path_text(&visit.path),
                            ),
                        );
                    })?,
                    None => self.value.clone(),
                };
                visit.native_python(&value, sink, sequence.map(|_| index))
            };
            if let Err(error) = result {
                note_slot_context(py, &error, template, index, context);
                errors::note_format(
                    py,
                    &error,
                    format_args!(
                        "while binding {}; current request was not sent",
                        path_text(&visit.path)
                    ),
                );
                return Err(error);
            }
            after(index, sink)?;
        }
        check(py, budget)
    }
    fn parameter_count(&self, expected: usize) -> PyResult<u16> {
        let py = self.value.py();
        let width = self.width();
        if width != expected {
            return Err(errors::client_format(
                py,
                ClientErrorCode::ParameterCount,
                format_args!(
                    "SQL expects {expected} positions but {} provides {width}; current request was not sent",
                    path_text(&self.path)
                ),
            ));
        }
        prepared_count(py, expected)
    }

    fn native_parameters(
        self,
        template: &Template,
        expected: u16,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<Vec<u8>> {
        let py = self.value.py();
        self.parameter_count(usize::from(expected))?;
        let mut sink =
            sqlaero_core::protocol::prepared::Parameters::new(0, expected, context.limit)
                .map_err(|error| crate::input::sink::failure(py, error))?;
        self.write_parameters_checked(
            types,
            context,
            budget,
            &mut sink,
            Some(template),
            |_, _| Ok(()),
        )?;
        sink.finish()
            .map_err(|error| crate::input::sink::failure(py, error))
    }

    fn display_parameters(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
    ) -> PyResult<Vec<u8>> {
        let py = self.value.py();
        let expected = self.parameter_count(template.slots())?;
        let mut sink = prepared_display::DisplaySink::new(template, expected, context.limit)
            .map_err(|error| crate::input::sink::failure(py, error))?;
        self.write_parameters_checked(
            types,
            &Context {
                limit: usize::MAX,
                ..*context
            },
            budget,
            &mut sink,
            Some(template),
            |_, _| Ok(()),
        )?;
        sink.finish()
            .map_err(|error| crate::input::sink::failure(py, error))
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
        self.fill::<false>(template, types, context, budget, 0..end, &mut output)?;
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
        self.fill::<false>(template, types, context, budget, range, output)
            .map(|_| ())
    }

    /// Bind one repeated fragment and publish its writer-proven row role.
    fn bind_repeated(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        layout: &Layout,
        output: &mut Vec<u8>,
    ) -> PyResult<FragmentRole> {
        output.clear();
        if layout.whole_row {
            self.fill::<true>(template, types, context, budget, layout.row.clone(), output)
        } else {
            self.fill::<false>(template, types, context, budget, layout.row.clone(), output)
                .map(|_| FragmentRole::Row)
        }
    }

    fn fill<const TRACK_ROLE: bool>(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        range: Range<usize>,
        output: &mut Vec<u8>,
    ) -> PyResult<FragmentRole> {
        let py = self.value.py();
        check(py, budget)?;
        let width = self.width();
        if width != template.slots() {
            return Err(errors::client_format(
                py,
                ClientErrorCode::ParameterCount,
                format_args!(
                    "SQL expects {} positions but {} provides {width}; current request was not sent",
                    template.slots(),
                    path_text(&self.path)
                ),
            ));
        }
        let mut cursor = 0;
        let role = self.walk::<TRACK_ROLE>(
            types,
            context,
            budget,
            output,
            Some(template),
            |_, output| {
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
                Ok(false)
            },
            |_, _, _| Ok(()),
        )?;
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
        Ok(role)
    }

    /// Traverse each existing positional role exactly once. Binder framing and
    /// final public projection are static consumers around the same writes.
    #[expect(
        clippy::too_many_arguments,
        reason = "One traversal shares projection, text and CALL framing; the optional template supplies per-slot facts without another visitor or allocation."
    )]
    fn walk<const TRACK_ROLE: bool>(
        mut self,
        types: &ScalarTypes,
        context: &Context,
        budget: &IoBudget,
        output: &mut Vec<u8>,
        template: Option<&Template>,
        mut before: impl FnMut(usize, &mut Vec<u8>) -> PyResult<bool>,
        mut after: impl FnMut(usize, &mut Vec<u8>, FragmentRole) -> PyResult<()>,
    ) -> PyResult<FragmentRole> {
        let py = self.value.py();
        check(py, budget)?;
        let width = self.width();
        let mut visit = Visit {
            types,
            context: *context,
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
        // Keep Sequence's original local-drop lifetime and NativeRow's
        // original value-before-provider field-drop order.
        let sequence = match std::mem::replace(&mut self.input, RowInput::Scalar) {
            RowInput::Sequence(sequence) => Some(sequence),
            input => {
                self.input = input;
                None
            }
        };
        let row = if self.itemize {
            sequence.as_ref().map(SequenceRef::sequence)
        } else {
            None
        };
        if let Some(row) = row {
            visit.enter_sequence(py, row)?;
        }
        let mut native = match &mut self.input {
            RowInput::Native(native) => Some(native),
            _ => None,
        };
        if let Some(native) = native.as_ref() {
            visit.enter(py, self.value.as_ptr() as usize)?;
            if matches!(
                (native.source.shape(), native.row),
                (Shape::Table { .. }, Some(_))
            ) {
                // A natural table retains its root while one logical row is
                // bound. Itemizing that row does not remove its depth level.
                visit.enter_native(py)?;
            }
        }
        let mut role = FragmentRole::Scalar;
        for index in 0..width {
            let effective = slot_context(template, index, context);
            visit.context = effective;
            let source = if let Some(row) = row {
                row.row_item(index).inspect_err(|error| {
                    errors::note_format(
                        py,
                        error,
                        format_args!(
                            "while binding {}[{index}]; current request was not sent",
                            path_text(&visit.path)
                        ),
                    );
                })?
            } else {
                self.value.clone()
            };
            let out = before(index, output)?;
            let result = if out {
                let mut guard = call::NullGuard::default();
                if let Some(native) = native.as_deref_mut() {
                    let (source_row, column) = match (native.source.shape(), native.row) {
                        (Shape::Vector { .. }, Some(row)) => (row, 0),
                        (Shape::Vector { .. }, None) => (index, 0),
                        (Shape::Record { .. }, _) => (0, index),
                        (Shape::Table { .. }, Some(row)) => (row, index),
                        _ => return Err(crate::input::sink::fragment_error(py)),
                    };
                    visit.push_index(py, index)?;
                    native.source.source().write_parameter_cell(
                        source_row, column, &mut guard, &effective, budget, &mut visit,
                    )?;
                    visit.pop_index();
                } else {
                    visit.native_python(&source, &mut guard, row.map(|_| index))?;
                }
                guard.validate(py, index)?;
                writer::append(output, b"NULL", context.limit)
                    .map_err(|error| errors::wire(py, error))?;
                Ok(FragmentRole::Scalar)
            } else if let Some(native) = native.as_deref_mut() {
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
                        visit
                            .native_value(native.source.source(), output)
                            .map(|()| FragmentRole::Row)
                    } else {
                        let positional =
                            !matches!((shape, source_row), (Shape::Vector { .. }, Some(_)));
                        if positional {
                            visit.push_index(py, index)?;
                        }
                        let result = visit.native_cell::<TRACK_ROLE>(
                            native.source.source(),
                            coordinates.0,
                            coordinates.1,
                            output,
                        );
                        if result.is_ok() && positional {
                            visit.pop_index();
                        }
                        result
                    }
                } else if let Some(row) = source_row {
                    match shape {
                        Shape::Vector { .. } => {
                            visit.native_cell::<TRACK_ROLE>(native.source.source(), row, 0, output)
                        }
                        Shape::Table { columns, .. } => visit
                            .native_row(native.source.source(), row, columns, output)
                            .map(|()| FragmentRole::Row),
                        Shape::Record { .. } => visit
                            .native_value(native.source.source(), output)
                            .map(|()| FragmentRole::Row),
                    }
                } else {
                    visit
                        .native_value(native.source.source(), output)
                        .map(|()| FragmentRole::Row)
                }
            } else if let Some(sequence) = sequence.as_ref().filter(|_| !self.itemize) {
                visit
                    .sequence(sequence.sequence(), output, None)
                    .map(|()| FragmentRole::Row)
            } else {
                if TRACK_ROLE {
                    visit.value_role(&source, output, row.map(|_| index))
                } else {
                    visit
                        .value(&source, output, row.map(|_| index))
                        .map(|()| FragmentRole::Scalar)
                }
            };
            role = match result {
                Ok(role) => {
                    after(index, output, role)?;
                    role
                }
                Err(error) => {
                    note_slot_context(py, &error, template, index, context);
                    errors::note_format(
                        py,
                        &error,
                        format_args!(
                            "while binding {}; current request was not sent",
                            path_text(&visit.path)
                        ),
                    );
                    return Err(error);
                }
            };
        }
        check(py, budget)?;
        Ok(role)
    }
}

fn slot_context(template: Option<&Template>, slot: usize, base: &Context) -> Context {
    Context {
        utc_confirmed: base.utc_confirmed
            && template.is_none_or(|template| template.utc_override(slot).is_none()),
        ..*base
    }
}

#[cold]
fn note_slot_context(
    py: Python<'_>,
    error: &PyErr,
    template: Option<&Template>,
    slot: usize,
    context: &Context,
) {
    let Some(template) = template.filter(|template| template.utc_override(slot).is_some()) else {
        return;
    };
    match template.utc_override_character(slot, context.codec) {
        Ok(Some(position)) => errors::note_format(
            py,
            error,
            format_args!(
                "SQL parameter {slot} belongs to a statement with a SET_VAR hint at SQL character {position}; its effective time_zone cannot be certified as UTC. Aware inputs require reliably confirmed UTC. Session facts were not changed and the current request was not sent."
            ),
        ),
        _ => errors::note_format(
            py,
            error,
            format_args!(
                "SQL parameter {slot} belongs to a statement with a SET_VAR hint; its effective time_zone cannot be certified as UTC. The hint's Unicode position could not be reconstructed. The original failure is preserved; the current request was not sent."
            ),
        ),
    }
}

struct Visit<'a> {
    types: &'a ScalarTypes,
    // Copy only the native policy, so one SQL position can refine its temporal
    // precondition without mutating session facts or another position.
    context: Context,
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
        context: *context,
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
    fn native_cell<const TRACK_ROLE: bool>(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
    ) -> PyResult<FragmentRole> {
        let context = self.context;
        if TRACK_ROLE {
            source.write_role_cell(row, column, output, &context, self.budget, self)
        } else {
            source
                .write_cell(row, column, output, &context, self.budget, self)
                .map(|()| FragmentRole::Scalar)
        }
    }

    fn enter(&mut self, py: Python<'_>, identity: usize) -> PyResult<()> {
        if self.active_depth + self.native_depth >= 64 || self.active.contains(&identity) {
            return Err(errors::client_format(
                py,
                ClientErrorCode::ContainerNesting,
                format_args!(
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
        self.value_role(value, output, child).map(|_| ())
    }
    fn value_role(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<FragmentRole> {
        let py = value.py();
        self.work += 1;
        if self.work >= 16384 {
            if let Err(error) = check(py, self.budget) {
                if let Some(index) = child
                    && self.path.try_reserve(1).is_ok()
                {
                    self.path.push(index);
                }
                return Err(error);
            }
            self.work = 0;
        }
        // Preserve the two exact-container checks in front of primitive
        // writes. Extended families are resolved only after scalar rejection.
        let sequence = if value.is_exact_instance_of::<pyo3::types::PyList>() {
            Some(Sequence::List(value.cast::<pyo3::types::PyList>()?.clone()))
        } else if value.is_exact_instance_of::<pyo3::types::PyTuple>() {
            Some(Sequence::Tuple(
                value.cast::<pyo3::types::PyTuple>()?.clone(),
            ))
        } else {
            match self
                .types
                .try_write(value, output, &self.context, Some(self.budget))
            {
                Ok(true) => return Ok(FragmentRole::Scalar),
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
                .map(|()| FragmentRole::Row)
        } else {
            self.native_or_unsupported(value, output, child)
                .map(|()| FragmentRole::Row)
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

    // Keep canonical entry/exit on its existing inlined traversal path.
    // Ownership and entry/exit stay here; the item loop remains shared below.
    #[inline(always)]
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
        // Exact containers have one identity and cannot carry a normalized
        // alias. Keep their entry on the original checked enter path without
        // the generic normalization wrapper. A Sequence's strategy is immutable
        // during traversal, so retain its alias fact for the matching exit.
        let aliased = match sequence {
            Sequence::List(value) => {
                self.enter(py, value.as_ptr() as usize)?;
                false
            }
            Sequence::Tuple(value) => {
                self.enter(py, value.as_ptr() as usize)?;
                false
            }
            _ => {
                self.enter_sequence(py, sequence)?;
                sequence.alias().is_some()
            }
        };
        let result = (|| {
            writer::append(output, b"(", self.context.limit)
                .map_err(|error| errors::wire(py, error))?;
            // Select one static getter for this container, while preserving
            // the original length observation after the opening write.
            match sequence {
                Sequence::List(value) => {
                    self.sequence_items(py, value.len(), output, |visit, index, output| {
                        let item = value.get_item(index).inspect_err(|_| {
                            visit.path.push(index);
                        })?;
                        visit.value(&item, output, Some(index))
                    })
                }
                Sequence::Tuple(value) => {
                    self.sequence_items(py, value.len(), output, |visit, index, output| {
                        let item = value.get_borrowed_item(index).inspect_err(|_| {
                            visit.path.push(index);
                        })?;
                        visit.value(&item, output, Some(index))
                    })
                }
                _ => self.sequence_items(py, sequence.len(), output, |visit, index, output| {
                    sequence.with_item(index, |value| {
                        let value = value.inspect_err(|_| {
                            visit.path.push(index);
                        })?;
                        visit.value(value, output, Some(index))
                    })
                }),
            }
        })();
        if aliased {
            self.active.pop();
        }
        self.active.pop();
        self.active_depth -= 1;
        if result.is_ok() && child.is_some() {
            self.path.pop();
        }
        result
    }

    fn sequence_items(
        &mut self,
        py: Python<'_>,
        width: usize,
        output: &mut Vec<u8>,
        mut value: impl FnMut(&mut Self, usize, &mut Vec<u8>) -> PyResult<()>,
    ) -> PyResult<()> {
        for index in 0..width {
            if index != 0 {
                writer::append(output, b",", self.context.limit)
                    .map_err(|error| errors::wire(py, error))?;
            }
            value(self, index, output)?;
        }
        writer::append(output, b")", self.context.limit).map_err(|error| errors::wire(py, error))
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
        let context = self.context;
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
                    source.write_cell(index, 0, output, &context, self.budget, self)?;
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
        let context = self.context;
        writer::append(output, b"(", self.context.limit).map_err(|e| errors::wire(py, e))?;
        for column in 0..columns {
            if column != 0 {
                writer::append(output, b",", self.context.limit)
                    .map_err(|e| errors::wire(py, e))?;
            }
            self.push_index(py, column)?;
            source.write_cell(row, column, output, &context, self.budget, self)?;
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
            if self.path.try_reserve(1).is_err() {
                return;
            }
            self.path.push(index);
        }
        if let Ok(summary) = self.types.diagnostic(value) {
            errors::note_format(
                value.py(),
                error,
                format_args!("failed escape input: {summary}"),
            );
        }
    }
}

impl CellWriter for Visit<'_> {
    fn python_role(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<FragmentRole> {
        self.value_role(value, output, child)
    }

    fn native_python(
        &mut self,
        value: &Bound<'_, PyAny>,
        sink: &mut dyn sqlaero_core::bind::value::ValueSink<Error = sqlaero_core::bind::value::ValueError>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let py = value.py();
        let result = (|| {
            self.checkpoint(py, 1)?;
            if self.types.try_write_to(
                value,
                &mut crate::input::sink::Native(sink),
                &self.context,
                Some(self.budget),
            )? {
                return Ok(());
            }
            if Sequence::from(value, self.types, &self.active)?.is_some()
                || source::prepare(value, self.types, self.budget)?.is_some()
            {
                return Err(crate::input::sink::fragment_error(py));
            }
            Err(crate::input::unsupported(value))
        })();
        if let Err(error) = &result {
            self.failed_value(value, child, error);
        }
        result
    }

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
            return Err(errors::client_format(
                py,
                ClientErrorCode::ContainerNesting,
                format_args!(
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

struct InputPath<'a>(&'a [usize]);

impl std::fmt::Display for InputPath<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("args")?;
        for index in self.0 {
            write!(formatter, "[{index}]")?;
        }
        Ok(())
    }
}

fn path_text(path: &[usize]) -> InputPath<'_> {
    InputPath(path)
}
pub(crate) fn template_error(py: Python<'_>, error: TemplateError) -> PyErr {
    match error {
        TemplateError::Wire(error) => errors::wire(py, error),
        error => errors::client_format(
            py,
            ClientErrorCode::TemplateBinding,
            format_args!("{error}"),
        ),
    }
}
