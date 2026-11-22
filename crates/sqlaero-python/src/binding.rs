//! Incremental input-row adaptation and native positional request assembly.

use crate::errors;
use crate::input::control::Checkpoint;
use crate::input::sequence::{NativeIntegers, RangeSource, Sequence};
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
use std::ops::Range;
use std::sync::Arc;

pub(crate) mod call;
mod dictionary;
mod prepared_display;
#[cfg(test)]
mod prepared_native_tests;

fn tensor_error(py: Python<'_>) -> PyErr {
    errors::builtin_message::<pyo3::exceptions::PyValueError>(
        py,
        "this source's tensor axes are outside the supported MySQL parameter layout; use JSON(...) for one JSON text parameter",
    )
}

/// MySQL target admission consumes source shape facts, without a Python type catalog.
fn mysql_source<'py>(
    value: &Bound<'py, PyAny>,
    types: &ScalarTypes,
    budget: &dyn Checkpoint,
) -> PyResult<Option<Box<dyn PreparedSource<'py> + 'py>>> {
    let prepared = source::prepare(value, types, budget)?;
    if prepared
        .as_ref()
        .is_some_and(|source| matches!(source.shape(), Shape::Tensor { .. }))
    {
        return Err(tensor_error(value.py()));
    }
    Ok(prepared)
}

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
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
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
    budget: &dyn Checkpoint,
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
    let py = data.py();
    let context = Context {
        limit: usize::MAX,
        ..*context
    };
    let budget = crate::input::control::Control::unlimited();
    let mut rows = Rows::new(data, many, itemize, types, &budget)?;
    let mut scratch = ProjectionScratch::default();
    if rows.public_batch() {
        let output = crate::output::list(py, [])?;
        while let Some(row) = rows.next(types, &budget)? {
            output.append(row.project(py, types, &context, &budget, &mut scratch)?)?;
            scratch.release_large_buffers();
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
        let value = row.project(py, types, &context, &budget, &mut scratch)?;
        check(py, &budget)?;
        Ok(value)
    }
}

#[derive(Default)]
struct ProjectionScratch {
    encoded: Vec<u8>,
    values: Vec<Py<PyAny>>,
}

impl ProjectionScratch {
    fn release_large_buffers(&mut self) {
        // Keep ordinary row capacity only. A long field or wide row must
        // not pin a correspondingly large scratch allocation for the batch.
        const RETAIN_BYTES: usize = 4096;
        if self.encoded.capacity() > RETAIN_BYTES {
            self.encoded = Vec::new();
        }
        if self.values.capacity() > RETAIN_BYTES / std::mem::size_of::<Py<PyAny>>() {
            self.values = Vec::new();
        }
    }
}

/// Retain an input owner and advance one logical row at a time. Native typed
/// providers retain their own current chunk; neither path materializes a batch.
pub(crate) struct Rows<'py> {
    root: Bound<'py, PyAny>,
    outer: Option<Sequence<'py>>,
    single: Option<Sequence<'py>>,
    native: Option<Box<dyn PreparedSource<'py> + 'py>>,
    dictionary: Option<Box<dictionary::Cache>>,
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
                    Shape::Vector { .. } | Shape::Record { .. } => self.many,
                    Shape::Scalar | Shape::Tensor { .. } => false,
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
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        Self::prepare(root, many, itemize, types, budget)
            .inspect_err(|error| note_preparation(root.py(), error, None))
    }
    fn prepare(
        root: &Bound<'py, PyAny>,
        many: bool,
        itemize: bool,
        types: &ScalarTypes,
        budget: &dyn Checkpoint,
    ) -> PyResult<Self> {
        let sequence = Sequence::from(root, types, &[], Some(budget))?;
        let native = if sequence.is_none() {
            mysql_source(root, types, budget)?
        } else {
            None
        };
        let (outer, single) = if many {
            (sequence, None)
        } else {
            (None, sequence)
        };
        let (count, empty) = match native.as_ref().map(|v| v.shape()) {
            Some(Shape::Vector { length }) => {
                (if many { length.max(1) } else { 1 }, many && length == 0)
            }
            Some(Shape::Record { fields }) => {
                (if many { fields.max(1) } else { 1 }, many && fields == 0)
            }
            Some(Shape::Scalar) => (1, false),
            Some(Shape::Table { rows, .. }) => (rows.max(1), rows == 0),
            Some(Shape::Tensor { .. }) => return Err(tensor_error(root.py())),
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
            dictionary: None,
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
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
    ) -> PyResult<Row<'r, 'py>> {
        let py = self.root.py();
        let path = (self.public_batch() && !self.empty).then_some(index);
        self.prepare_at(index, types, budget)
            .inspect_err(|error| note_preparation(py, error, path))
    }
    fn prepare_at<'r>(
        &'r mut self,
        index: usize,
        types: &ScalarTypes,
        budget: &dyn Checkpoint,
    ) -> PyResult<Row<'r, 'py>> {
        let dictionary = &mut self.dictionary;
        if let Some(native) = self.native.as_mut() {
            let select = !self.empty
                && match native.shape() {
                    Shape::Table { .. } => true,
                    Shape::Vector { .. } | Shape::Record { .. } => self.many,
                    Shape::Scalar | Shape::Tensor { .. } => false,
                };
            let path = if select {
                smallvec![index]
            } else {
                SmallVec::new()
            };
            if select {
                let outer = Some(self.root.as_ptr() as usize);
                return match native.child(index, budget)? {
                    source::Child::Python(value) => {
                        Row::from_value(value, outer, path, self.itemize, types, budget, dictionary)
                    }
                    source::Child::Native(source) => Ok(Row {
                        value: self.root.clone(),
                        outer,
                        path,
                        itemize: self.itemize,
                        zero_positions: false,
                        dictionary,
                        input: RowInput::Native(NativeRow {
                            source: SourceRef::Selected(source),
                        }),
                    }),
                };
            }
            return Ok(Row {
                value: self.root.clone(),
                outer: None,
                path,
                itemize: self.itemize,
                zero_positions: self.empty,
                dictionary,
                input: RowInput::Native(NativeRow {
                    source: SourceRef::Borrowed(native.as_mut()),
                }),
            });
        }
        let (value, outer, path) = match &self.outer {
            Some(outer) if !self.empty => {
                let value = outer.row_item(index, Some(budget))?;
                (value, Some(outer.identity()), smallvec![index])
            }
            _ => (self.root.clone(), None, SmallVec::new()),
        };
        let sequence = if !self.empty {
            if self.outer.is_none() {
                self.single.as_ref().map(SequenceRef::Borrowed)
            } else {
                Sequence::from(&value, types, &[], Some(budget))?.map(SequenceRef::Owned)
            }
        } else {
            None
        };
        let input = if let Some(sequence) = sequence {
            RowInput::Sequence(sequence)
        } else if !self.empty {
            match mysql_source(&value, types, budget)? {
                Some(source) => RowInput::Native(NativeRow {
                    source: SourceRef::Owned(source),
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
            dictionary,
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
    Selected(source::Selected<'r, 'py>),
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
    fn source(&mut self) -> &mut (dyn PreparedSource<'py> + '_) {
        match self {
            Self::Borrowed(v) => *v,
            Self::Owned(v) => v.as_mut(),
            Self::Selected(v) => v,
        }
    }
    fn shape(&self) -> Shape {
        match self {
            Self::Borrowed(v) => v.shape(),
            Self::Owned(v) => v.shape(),
            Self::Selected(v) => v.shape(),
        }
    }
}
struct NativeRow<'r, 'py> {
    source: SourceRef<'r, 'py>,
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
    dictionary: &'r mut Option<Box<dictionary::Cache>>,
}
impl<'r, 'py> Row<'r, 'py> {
    fn from_value(
        value: Bound<'py, PyAny>,
        outer: Option<usize>,
        path: SmallVec<[usize; 4]>,
        itemize: bool,
        types: &ScalarTypes,
        budget: &dyn Checkpoint,
        dictionary: &'r mut Option<Box<dictionary::Cache>>,
    ) -> PyResult<Self> {
        let input = if let Some(sequence) = Sequence::from(&value, types, &[], Some(budget))? {
            RowInput::Sequence(SequenceRef::Owned(sequence))
        } else if let Some(source) = mysql_source(&value, types, budget)? {
            RowInput::Native(NativeRow {
                source: SourceRef::Owned(source),
            })
        } else {
            RowInput::Scalar
        };
        Ok(Self {
            value,
            input,
            outer,
            path,
            itemize,
            zero_positions: false,
            dictionary,
        })
    }

    fn public_row(&self) -> bool {
        self.zero_positions
            || (self.itemize
                && match &self.input {
                    RowInput::Sequence(_) => true,
                    RowInput::Native(native) => native.source.shape() != Shape::Scalar,
                    RowInput::Scalar => false,
                })
    }

    fn project<'out>(
        self,
        py: Python<'out>,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        scratch: &mut ProjectionScratch,
    ) -> PyResult<Bound<'out, PyAny>> {
        // Select the output shape once. Each emitted position already
        // belongs to that shape, so the sink has no per-value decision.
        if self.public_row() {
            self.project_values::<true>(py, types, context, budget, scratch)
        } else {
            self.project_values::<false>(py, types, context, budget, scratch)
        }
    }

    fn project_values<'out, const ROW: bool>(
        self,
        py: Python<'out>,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        scratch: &mut ProjectionScratch,
    ) -> PyResult<Bound<'out, PyAny>> {
        let ProjectionScratch { encoded, values } = scratch;
        let mut single = None;
        check(py, budget)?;
        self.walk::<false>(
            types,
            context,
            budget,
            encoded,
            None,
            |_, output| {
                output.clear();
                Ok(false)
            },
            |_, output, _| {
                let value = display(py, output, context.codec)?.into_any().unbind();
                if ROW {
                    values
                        .try_reserve(1)
                        .map_err(|_| errors::memory_error(py))?;
                    values.push(value);
                } else {
                    single = Some(value);
                }
                Ok(())
            },
        )?;
        check(py, budget)?;
        if ROW {
            crate::output::tuple_from_values(
                py,
                values.drain(..).map(|value| Ok(value.into_bound(py))),
            )
            .map(Bound::into_any)
        } else {
            single.map(|value| value.into_bound(py)).ok_or_else(|| {
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
            RowInput::Native(native) => match native.source.shape() {
                Shape::Vector { length } => length,
                Shape::Record { fields } => fields,
                Shape::Table { rows, .. } => rows,
                _ => 1,
            },
            RowInput::Sequence(value) => value.sequence().len(),
            RowInput::Scalar => 1,
        }
    }

    /// Adapt one logical row directly into native prepared parameters.
    /// Container roles are the same roles used by positional text binding.
    pub(crate) fn write_parameters<
        S: sqlaero_core::bind::value::ValueSink<
                Context = sqlaero_core::bind::Context,
                Error = sqlaero_core::bind::value::ValueError,
            >,
    >(
        self,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        sink: &mut S,
    ) -> PyResult<()> {
        self.write_parameters_checked(types, context, budget, sink, None, |_| Ok(false))
    }

    fn write_parameters_checked<
        S: sqlaero_core::bind::value::ValueSink<
                Context = sqlaero_core::bind::Context,
                Error = sqlaero_core::bind::value::ValueError,
            >,
    >(
        mut self,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        sink: &mut S,
        template: Option<&Template>,
        mut out: impl FnMut(usize) -> PyResult<bool>,
    ) -> PyResult<()> {
        let py = self.value.py();
        check(py, budget)?;
        let width = self.width();
        let progress = crate::input::control::Control::local(budget);
        let mut visit = Visit {
            dictionary: self.dictionary,
            types,
            context: *context,
            budget: &progress,
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
            visit.enter(py, sequence.identity())?;
        }
        let mut native = match &mut self.input {
            RowInput::Native(native) => Some(native),
            _ => None,
        };
        if let Some(native) = native.as_ref() {
            if matches!(native.source, SourceRef::Selected(_)) {
                if native.source.shape() != Shape::Scalar {
                    visit.enter_native(py)?;
                }
            } else {
                visit.enter_source(
                    py,
                    self.value.as_ptr() as usize,
                    native.source.shape() != Shape::Scalar,
                )?;
            }
        }
        for index in 0..width {
            let effective = slot_context(template, index, context);
            visit.context = effective;
            let require_none = out(index)?;
            let result = if let Some(native) = native.as_deref_mut() {
                let shape = native.source.shape();
                let coordinates = match (self.itemize, shape) {
                    (_, Shape::Scalar) => Some((0, 0, false)),
                    (true, Shape::Vector { .. }) => Some((index, 0, true)),
                    (true, Shape::Record { .. }) => Some((0, index, true)),
                    _ => None,
                };
                if let Some((row, column, positional)) = coordinates {
                    if positional {
                        visit.push_index(py, index)?;
                    }
                    let result =
                        if require_none {
                            visit.with_literal_none(py, index, |writer| {
                                native.source.source().write_parameter_cell(
                                    row, column, sink, &effective, budget, writer,
                                )
                            })
                        } else {
                            native.source.source().write_parameter_cell(
                                row, column, sink, &effective, budget, &mut visit,
                            )
                        };
                    if result.is_ok() && positional {
                        visit.pop_index();
                    }
                    result
                } else {
                    Err(crate::input::sink::fragment_error(py))
                }
            } else {
                let value =
                    match sequence {
                        Some(sequence) => sequence.row_item(index, Some(budget)).inspect_err(
                            |error| {
                                errors::note_format(
                                    py,
                                    error,
                                    format_args!(
                                        "while binding {}[{index}]; current request was not sent",
                                        path_text(&visit.path),
                                    ),
                                );
                            },
                        )?,
                        None => self.value.clone(),
                    };
                if require_none {
                    call::require_none(&value, index)
                        .and_then(|()| visit.native_python(&value, sink, sequence.map(|_| index)))
                } else {
                    visit.native_python(&value, sink, sequence.map(|_| index))
                }
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
        budget: &dyn Checkpoint,
    ) -> PyResult<Vec<u8>> {
        let py = self.value.py();
        self.parameter_count(usize::from(expected))?;
        let mut sink =
            sqlaero_core::protocol::prepared::Parameters::new(0, expected, context.limit)
                .map_err(|error| crate::input::sink::failure(py, error))?;
        self.write_parameters_checked(types, context, budget, &mut sink, Some(template), |_| {
            Ok(false)
        })?;
        sink.finish()
            .map_err(|error| crate::input::sink::failure(py, error))
    }

    fn display_parameters(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
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
            |_| Ok(false),
        )?;
        sink.finish()
            .map_err(|error| crate::input::sink::failure(py, error))
    }

    pub(crate) fn bind(
        self,
        template: &Template,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
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
        budget: &dyn Checkpoint,
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
    /// Callers check the original budget and signals immediately before and
    /// after successful traversal; Visit retains the dynamic work checkpoints.
    #[expect(
        clippy::too_many_arguments,
        reason = "One traversal shares projection, text and CALL framing; the optional template supplies per-slot facts without another visitor or allocation."
    )]
    fn walk<const TRACK_ROLE: bool>(
        self,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        output: &mut Vec<u8>,
        template: Option<&Template>,
        before: impl FnMut(usize, &mut Vec<u8>) -> PyResult<bool>,
        after: impl FnMut(usize, &mut Vec<u8>, FragmentRole) -> PyResult<()>,
    ) -> PyResult<FragmentRole> {
        // Completed template facts are immutable during this traversal. Keep
        // the common base context without refining it again for every value.
        if context.utc_confirmed && template.is_some_and(Template::has_utc_overrides) {
            self.walk_context::<TRACK_ROLE, true>(
                types, context, budget, output, template, before, after,
            )
        } else {
            self.walk_context::<TRACK_ROLE, false>(
                types, context, budget, output, template, before, after,
            )
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "One traversal retains framing and diagnostics while its immutable UTC policy is selected once."
    )]
    fn walk_context<const TRACK_ROLE: bool, const REFINE_UTC: bool>(
        mut self,
        types: &ScalarTypes,
        context: &Context,
        budget: &dyn Checkpoint,
        output: &mut Vec<u8>,
        template: Option<&Template>,
        mut before: impl FnMut(usize, &mut Vec<u8>) -> PyResult<bool>,
        mut after: impl FnMut(usize, &mut Vec<u8>, FragmentRole) -> PyResult<()>,
    ) -> PyResult<FragmentRole> {
        let py = self.value.py();
        let width = self.width();
        let progress = crate::input::control::Control::local(budget);
        let mut visit = Visit {
            dictionary: self.dictionary,
            types,
            context: *context,
            budget: &progress,
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
            visit.enter(py, row.identity())?;
        }
        let mut native = match &mut self.input {
            RowInput::Native(native) => Some(native),
            _ => None,
        };
        if let Some(native) = native.as_ref() {
            if matches!(native.source, SourceRef::Selected(_)) {
                if native.source.shape() != Shape::Scalar {
                    visit.enter_native(py)?;
                }
            } else {
                visit.enter_source(
                    py,
                    self.value.as_ptr() as usize,
                    native.source.shape() != Shape::Scalar,
                )?;
            }
        }
        let mut role = FragmentRole::Scalar;
        for index in 0..width {
            let effective = if REFINE_UTC {
                slot_context(template, index, context)
            } else {
                *context
            };
            if REFINE_UTC {
                visit.context = effective;
            }
            let source = if let Some(row) = row {
                row.row_item(index, Some(budget)).inspect_err(|error| {
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
                let mut sink = call::DiscardValue;
                if let Some(native) = native.as_deref_mut() {
                    let (source_row, column) = match native.source.shape() {
                        Shape::Scalar => (0, 0),
                        Shape::Vector { .. } if self.itemize => (index, 0),
                        Shape::Record { .. } if self.itemize => (0, index),
                        _ => return Err(crate::input::sink::fragment_error(py)),
                    };
                    visit.push_index(py, index)?;
                    visit.with_literal_none(py, index, |writer| {
                        native.source.source().write_parameter_cell(
                            source_row, column, &mut sink, &effective, budget, writer,
                        )
                    })?;
                    visit.pop_index();
                } else {
                    call::require_none(&source, index)?;
                    visit.native_python(&source, &mut sink, row.map(|_| index))?;
                }
                writer::append(output, b"NULL", context.limit)
                    .map_err(|error| errors::wire(py, error))?;
                Ok(FragmentRole::Scalar)
            } else if let Some(native) = native.as_deref_mut() {
                let shape = native.source.shape();
                if shape == Shape::Scalar {
                    visit.native_cell::<TRACK_ROLE>(native.source.source(), 0, 0, output)
                } else if self.itemize {
                    visit.push_index(py, index)?;
                    let result = match shape {
                        Shape::Vector { .. } => visit.native_cell::<TRACK_ROLE>(
                            native.source.source(),
                            index,
                            0,
                            output,
                        ),
                        Shape::Record { .. } => visit.native_cell::<TRACK_ROLE>(
                            native.source.source(),
                            0,
                            index,
                            output,
                        ),
                        Shape::Table { .. } => match native.source.source().child(index, budget)? {
                            source::Child::Native(mut child) => visit.source(&mut child, output),
                            source::Child::Python(value) => visit.value_role(&value, output, None),
                        },
                        Shape::Tensor { .. } => Err(tensor_error(py)),
                        Shape::Scalar => unreachable!("scalar handled before itemization"),
                    };
                    if result.is_ok() {
                        visit.pop_index();
                    }
                    result
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
    dictionary: &'a mut Option<Box<dictionary::Cache>>,
    types: &'a ScalarTypes,
    // Copy only the native policy, so one SQL position can refine its temporal
    // precondition without mutating session facts or another position.
    context: Context,
    budget: &'a dyn Checkpoint,
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
    budget: &dyn Checkpoint,
) -> PyResult<Vec<u8>> {
    check(value.py(), budget)?;
    let mut output = Vec::new();
    let progress = crate::input::control::Control::local(budget);
    let mut dictionary = None;
    Visit {
        dictionary: &mut dictionary,
        types,
        context: *context,
        budget: &progress,
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
    /// Check source identity only for an explicit OUT cell.
    fn with_literal_none(
        &mut self,
        py: Python<'_>,
        index: usize,
        write: impl FnOnce(&mut dyn CellWriter) -> PyResult<()>,
    ) -> PyResult<()> {
        let mut writer = call::LiteralNoneWriter::new(self, index);
        write(&mut writer)?;
        writer.validate(py)
    }

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
        self.enter_source(py, identity, true)
    }

    fn enter_source(&mut self, py: Python<'_>, identity: usize, container: bool) -> PyResult<()> {
        if (container && self.active_depth + self.native_depth >= 64)
            || crate::input::transparent::contains(py, &self.active, identity, Some(self.budget))?
        {
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
        self.active_depth += usize::from(container);
        Ok(())
    }

    fn enter_transparent(
        &mut self,
        source: &crate::input::transparent::Value<'_>,
    ) -> PyResult<usize> {
        let py = source.payload().py();
        if source.intersects(&self.active, Some(self.budget))? {
            return Err(errors::client(
                py,
                ClientErrorCode::ContainerNesting,
                "transparent source input contains a true cycle; current request was not sent",
            ));
        }
        let previous = self.active.len();
        self.active
            .try_reserve(source.len())
            .map_err(|_| errors::memory_error(py))?;
        if let Err(error) =
            source.for_each_identity(Some(self.budget), |identity| self.active.push(identity))
        {
            self.active.truncate(previous);
            return Err(error);
        }
        // These owners emit no container and do not consume output depth.
        Ok(previous)
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
        self.value_with(value, output, child, |_| ())
    }
    fn value_role(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<FragmentRole> {
        self.value_with(value, output, child, std::convert::identity)
    }

    /// Keep ordinary traversal unit-returning while shape consumers receive
    /// the role from the same visitor. The projection is static native code.
    fn value_with<T>(
        &mut self,
        value: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
        project: impl FnOnce(FragmentRole) -> T,
    ) -> PyResult<T> {
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
                Ok(true) => return Ok(project(FragmentRole::Scalar)),
                Err(error) => {
                    self.failed_value(value, child, &error);
                    return Err(error);
                }
                Ok(false) => (),
            }
            if RangeSource::recognizes(value) {
                return self
                    .range_source(value, output, child)
                    .map(|()| project(FragmentRole::Row));
            }
            match Sequence::from(value, self.types, &self.active, Some(self.budget)) {
                Ok(sequence) => sequence,
                Err(error) => {
                    self.failed_value(value, child, &error);
                    return Err(error);
                }
            }
        };
        if let Some(sequence) = sequence {
            self.sequence(&sequence, output, child)
                .map(|()| project(FragmentRole::Row))
        } else {
            self.native_or_unsupported(value, output, child)
                .map(project)
        }
    }

    #[inline(never)]
    fn range_source(
        &mut self,
        source: &Bound<'_, PyAny>,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        match RangeSource::prepare(source, &self.active, Some(self.budget)) {
            Ok(RangeSource::Native(integers)) => {
                self.native_integer_sequence(source, &integers, output, child)
            }
            Ok(RangeSource::Iterated(sequence)) => self.sequence(&sequence, output, child),
            Err(error) => {
                self.failed_value(source, child, &error);
                Err(error)
            }
        }
    }

    fn native_integer_sequence(
        &mut self,
        source: &Bound<'_, PyAny>,
        integers: &NativeIntegers,
        output: &mut Vec<u8>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let py = source.py();
        if let Some(index) = child {
            self.path.push(index);
        }
        self.enter(py, source.as_ptr() as usize)?;
        let (mut number, step) = integers.first_and_step();
        let result = (|| {
            writer::append(output, b"(", self.context.limit)
                .map_err(|error| errors::wire(py, error))?;
            self.sequence_items(py, integers.length, output, |visit, index, output| {
                if let Err(error) = visit.checkpoint(py, 1) {
                    if visit.path.try_reserve(1).is_ok() {
                        visit.path.push(index);
                    }
                    return Err(error);
                }
                if let Err(wire) =
                    sqlaero_core::bind::scalar::integer(output, number, visit.context.limit)
                {
                    let error = errors::wire(py, wire);
                    if let Ok(value) = NativeIntegers::diagnostic_value(py, number) {
                        visit.failed_value(&value, Some(index), &error);
                    } else if visit.path.try_reserve(1).is_ok() {
                        visit.path.push(index);
                    }
                    return Err(error);
                }
                // Only the unused post-final term can wrap. Each written
                // value was proved to fit when the range was qualified.
                number = number.wrapping_add(step);
                Ok(())
            })
        })();
        self.active.pop();
        self.active_depth -= 1;
        if result.is_ok() && child.is_some() {
            self.path.pop();
        }
        result
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
        self.enter(py, sequence.identity())?;
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
                    sequence.with_item(index, Some(visit.budget), |value| {
                        let value = value.inspect_err(|_| {
                            visit.path.push(index);
                        })?;
                        visit.value(value, output, Some(index))
                    })
                }),
            }
        })();
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
    ) -> PyResult<FragmentRole> {
        let py = value.py();
        if let Some(index) = child {
            self.push_index(py, index)?;
        }
        let result = (|| {
            if crate::input::transparent::contains(
                py,
                &self.active,
                value.as_ptr() as usize,
                Some(self.budget),
            )? {
                return Err(errors::client(
                    py,
                    ClientErrorCode::ContainerNesting,
                    "input contains a true cycle; current request was not sent",
                ));
            }
            if let Some(source) = self.types.transparent(value, Some(self.budget))? {
                let previous = self.enter_transparent(&source)?;
                let result = self.value_role(source.payload(), output, None);
                self.active.truncate(previous);
                return result;
            }
            let Some(mut source) = mysql_source(value, self.types, self.budget)? else {
                return Err(crate::input::unsupported(value));
            };
            let container = source.shape() != Shape::Scalar;
            self.enter_source(py, value.as_ptr() as usize, container)?;
            let result = if container {
                self.native_value(source.as_mut(), output)
                    .map(|()| FragmentRole::Row)
            } else {
                self.native_cell::<true>(source.as_mut(), 0, 0, output)
            };
            self.active.pop();
            self.active_depth -= usize::from(container);
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
            Shape::Scalar => source.write_cell(0, 0, output, &context, self.budget, self),
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
            Shape::Tensor { .. } => Err(tensor_error(py)),
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
    fn referenced_text(
        &mut self,
        py: Python<'_>,
        reference: crate::input::cell::TextReference<'_>,
        output: &mut Vec<u8>,
        budget: &dyn Checkpoint,
    ) -> PyResult<bool> {
        if !dictionary::Cache::eligible(&reference) {
            return Ok(false);
        }
        let mut cache = match self.dictionary.take() {
            Some(cache) => cache,
            None => crate::input::boxed(py, dictionary::Cache::new())?,
        };
        let context = self.context;
        let result = cache.write(py, reference, output, &context, budget, self);
        *self.dictionary = Some(cache);
        result.map(|()| true)
    }

    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        output: &mut Vec<u8>,
    ) -> PyResult<FragmentRole> {
        let py = source.owner().py();
        let container = source.shape() != Shape::Scalar;
        if container {
            self.enter_native(py)?;
        }
        let result = self.native_value(source, output);
        if container {
            self.leave_native();
        }
        result.map(|()| {
            if container {
                FragmentRole::Row
            } else {
                FragmentRole::Scalar
            }
        })
    }

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
        sink: &mut dyn sqlaero_core::bind::value::ValueSink<
            Context = sqlaero_core::bind::Context,
            Error = sqlaero_core::bind::value::ValueError,
        >,
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
            if let Some(source) = self.types.transparent(value, Some(self.budget))? {
                let previous = self.enter_transparent(&source)?;
                let result = self.native_python(source.payload(), sink, child);
                self.active.truncate(previous);
                return result;
            }
            if Sequence::from(value, self.types, &self.active, Some(self.budget))?.is_some()
                || mysql_source(value, self.types, self.budget)?.is_some()
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

fn path_text(path: &[usize]) -> sqlaero_transcode::diagnostic::Indices<'_> {
    sqlaero_transcode::diagnostic::Indices::new("args", path)
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
