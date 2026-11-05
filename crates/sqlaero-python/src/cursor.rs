//! Thin public Cursor over one native incremental request and buffered Reader.

use crate::{
    binding,
    config::{self, Argument},
    connection::{CommandRole, Connection},
    errors,
    input::{self, ScalarTypes},
    metadata, output,
    reader::Reader,
};
use num_bigint::BigUint;
use pyo3::exceptions::PyStopIteration;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::{
    bind::{
        Context,
        batch::{Blocks, Layout},
        template::{Compiler, Template},
    },
    charset::output::OutputCodec,
    error::ClientErrorCode,
    result::{
        chain::{Chain, Set as NativeSet},
        decode::Options,
        text::Response,
    },
    session::query::ConfirmedPrefix,
    session::{
        query::{CommandConfirmation, Query},
        shared::CommandLease,
    },
    transport::deadline::IoBudget,
};
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
struct Results {
    pending: Chain,
    batch: Option<u128>,
    failed: bool,
    started: bool,
    failure_note: Option<std::sync::Arc<errors::SavedFailure>>,
}
#[derive(Default)]
struct Collected {
    chain: Chain,
    prefix: ConfirmedPrefix,
    total: u128,
    total_unknown: bool,
    final_first_known: bool,
}

struct Settings {
    options: Options,
    arraysize: BigUint,
}
#[derive(Clone, Copy)]
struct Counts {
    rows: u128,
    last_id: Option<u64>,
    warnings: u128,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct BindingKey {
    charset: &'static str,
    no_backslash: bool,
    ansi: bool,
    utc: bool,
}
impl BindingKey {
    fn new(context: &Context, ansi: bool) -> Self {
        Self {
            charset: context.codec.name(),
            no_backslash: context.no_backslash_escapes,
            ansi,
            utc: context.utc_confirmed,
        }
    }
}
struct Prepared {
    key: BindingKey,
    template: Template,
    layout: Option<Layout>,
    blocks: Option<Blocks>,
}
impl Prepared {
    fn new(
        py: Python<'_>,
        sql: &Bound<'_, PyString>,
        context: &Context,
        ansi: bool,
        many: bool,
        itemize: bool,
        budget: &sqlaero_core::transport::deadline::IoBudget,
    ) -> PyResult<Self> {
        let mut encoded = Vec::new();
        input::text(
            sql,
            &mut encoded,
            &Context {
                limit: usize::MAX,
                ..*context
            },
            false,
            Some(budget),
        )?;
        let mut compiler = if many && itemize {
            Compiler::for_batch(encoded, context.codec, context.no_backslash_escapes, ansi)
        } else {
            Compiler::new(encoded, context.codec, context.no_backslash_escapes, ansi)
        };
        let mut template = loop {
            let step = py
                .detach(|| compiler.advance())
                .map_err(|error| binding::template_error(py, error))?;
            input::check(py, budget)?;
            if let Some(value) = step {
                break value;
            }
        };
        let layout = template.prepare_merge();
        let blocks = layout
            .as_ref()
            .map(|layout| Blocks::new(&template, layout, context.limit, context.limit))
            .transpose()
            .map_err(|error| errors::wire(py, error))?;
        Ok(Self {
            key: BindingKey::new(context, ansi),
            template,
            layout,
            blocks,
        })
    }
}

/// Independent buffered result and read position. Commands use their owning
/// Connection; local fetches never acquire the connection's network lease.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Cursor {
    pub(crate) connection: Py<Connection>,
    reader: Reader,
    settings: Mutex<Settings>,
    counts: Mutex<Option<Counts>>,
    results: Mutex<Results>,
}
impl Cursor {
    /// Stop future operations after mandatory transaction cleanup failed. Any
    /// already active Reader operation retains and releases its own data.
    pub(crate) fn retire(&self) {
        self.reader.retire();
        let old = match self.results.lock() {
            Ok(mut state) => { state.failure_note = None; std::mem::take(&mut state.pending) }
            Err(error) => std::mem::take(&mut error.into_inner().pending),
        };
        drop(old);
    }

    pub(crate) fn new(
        connection: Py<Connection>,
        types: Py<ScalarTypes>,
        options: Options,
    ) -> Self {
        Self {
            connection,
            reader: Reader::empty(types),
            settings: Mutex::new(Settings {
                options,
                arraysize: BigUint::from(1u8),
            }),
            counts: Mutex::new(None),
            results: Mutex::new(Results::default()),
        }
    }
    fn settings(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Settings>> {
        self.settings.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor configuration is unavailable after a native failure",
            )
        })
    }
    fn counters(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Option<Counts>>> {
        self.counts.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor counters are unavailable after a native failure",
            )
        })
    }
    fn modifying(&self, py: Python<'_>, property: &str) -> PyResult<()> {
        if self.reader.is_closed() {
            Err(errors::client(
                py,
                ClientErrorCode::SessionState,
                format!(
                    "Cannot modify Cursor.{property} because this cursor is closed or retired. Its saved value remains readable."
                ),
            ))
        } else {
            Ok(())
        }
    }
    fn results(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Results>> {
        self.results.lock().map_err(|error| {
            drop(error.into_inner());
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor results are unavailable after a native state failure",
            )
        })
    }
    fn publish_set(&self, py: Python<'_>, set: NativeSet) -> PyResult<()> {
        let completion = set.response.completion();
        let counts = Counts {
            rows: match &set.response {
                Response::Command(ok) => u128::from(ok.affected_rows()),
                Response::Rows(rows) => rows.row_count as u128,
                Response::Counted { rows, .. } => *rows as u128,
            },
            last_id: completion.last_insert_id(),
            warnings: u128::from(completion.warnings()),
        };
        match set.response {
            Response::Rows(rows) => self
                .reader
                .install(py, rows, set.options, set.names_codec)?,
            Response::Command(_) => self.reader.clear_result(py)?,
            Response::Counted { .. } => {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "A discarded internal result cannot be published",
                ));
            }
        }
        *self.counters(py)? = Some(counts);
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "one concrete request shares the operation's existing owners and policy"
    )]
    fn receive_request(
        &self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        mut job: Query,
        budget: &mut IoBudget,
        source_rows: std::ops::Range<usize>,
        final_request: bool,
        codec: OutputCodec,
        options: Options,
        collected: &mut Collected,
    ) -> PyResult<()> {
        let request_index = collected.prefix.commands;
        let mut first = None;
        let mut set_index = 0;
        loop {
            let response = match connection.drive_set(
                py,
                lease,
                &mut job,
                budget,
                CommandRole::UserSql,
            ) {
                Ok(response) => response,
                Err(failure) => {
                    if let Some(native) = failure.native_failure() {
                        collected.chain.fail(native);
                    }
                    collected.total_unknown |= !final_request || first.is_none();
                    let error =
                        failure.record_prefix(py, &mut collected.prefix, source_rows.clone());
                    errors::note_format(
                        py,
                        &error,
                        format_args!(
                            "while receiving request {request_index}, result set {set_index}; statement index is not inferred from result count"
                        ),
                    );
                    return Err(error);
                }
            };
            if first.is_none() {
                let count = match &response {
                    Response::Command(ok) => u128::from(ok.affected_rows()),
                    Response::Rows(rows) => rows.row_count as u128,
                    Response::Counted { rows, .. } => *rows as u128,
                };
                collected.total = collected.total.checked_add(count).ok_or_else(|| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "batch first-result row count overflow",
                    )
                })?;
                first = Some(CommandConfirmation::from_response(&response));
                collected.final_first_known |= final_request;
            }
            let result = collected.chain.push(NativeSet {
                response,
                options,
                names_codec: codec,
                request_index,
                set_index,
            });
            if let Err(error) = result {
                if !job.is_finished() {
                    lease.invalidate();
                }
                collected.total_unknown |= !final_request;
                return Err(output::memory(py, error));
            }
            if job.is_finished() {
                collected
                    .prefix
                    .record_confirmation(
                        source_rows,
                        first.ok_or_else(|| {
                            errors::client(
                                py,
                                ClientErrorCode::InternalState,
                                "a completed command has no first result",
                            )
                        })?,
                    )
                    .map_err(|error| errors::wire(py, error))?;
                return Ok(());
            }
            set_index += 1;
            if let Err(error) = input::check(py, budget) {
                lease.invalidate();
                collected.total_unknown |= !final_request;
                return Err(error);
            }
        }
    }
    fn handoff(&self, py: Python<'_>, begun: &mut bool) -> PyResult<()> {
        if !*begun {
            self.reader.clear_result(py)?;
            *self.counters(py)? = None;
            let old = std::mem::take(&mut *self.results(py)?);
            drop(old);
            *begun = true;
        }
        Ok(())
    }
    fn run<'py>(
        &self,
        py: Python<'py>,
        sql: &Bound<'_, PyAny>,
        args: Argument<'_>,
        many: &Argument<'_>,
        itemize: &Argument<'_>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _operation = self.reader.operation(py)?;
        self.reader.open(py)?;
        let connection = self.connection.borrow(py);
        let timeouts = connection.runtime(py)?.timeouts;
        let mut budget = timeouts.operation();
        let mut lease = connection
            .native
            .try_command()
            .map_err(|error| errors::access(py, error))?;
        let sql = sql.cast::<PyString>().map_err(|_| {
            crate::errors::builtin_message::<pyo3::exceptions::PyTypeError>(
                py,
                "sql must be str; already encoded bytes are not accepted",
            )
        })?;
        let many = many
            .0
            .as_ref()
            .map_or(Ok(false), |value| config::boolean(value, "many"))?;
        let itemize = itemize
            .0
            .as_ref()
            .map_or(Ok(true), |value| config::boolean(value, "itemize"))?;
        let options = self.settings(py)?.options;
        let types = connection.types.borrow(py);
        let mut collected = Collected::default();
        let mut begun = false;
        let result = (|| {
            connection.ensure_context(py, &mut lease, &mut budget)?;
            if let Some(args) = args.0 {
                let mut rows = binding::Rows::new(&args, many, itemize, &types, &budget)?;
                let batch = rows.is_batch();
                let empty = rows.empty_root();
                let mut prepared: Option<Prepared> = None;
                // Only an overflow row is retained here, as already encoded
                // native bytes. The original root remains owned by Rows.
                let mut pending: Option<(usize, Vec<u8>, BindingKey)> = None;
                loop {
                    input::check(py, &budget)?;
                    let (context, ansi, names_codec) = connection.context(py)?;
                    let key = BindingKey::new(&context, ansi);
                    if prepared.as_ref().is_none_or(|value| value.key != key) {
                        prepared = Some(Prepared::new(
                            py, sql, &context, ansi, batch, itemize, &budget,
                        )?);
                    }
                    let prepared = prepared.as_mut().ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "SQL preparation was not published",
                        )
                    })?;
                    let start = collected.prefix.source_rows;
                    let (request, end) = if let (Some(layout), Some(blocks)) =
                        (&prepared.layout, &mut prepared.blocks)
                    {
                        let mut fragment = Vec::new();
                        let mut end = start;
                        loop {
                            let index;
                            if let Some((saved, bytes, saved_key)) = pending.take() {
                                index = saved;
                                fragment = bytes;
                                if saved_key != key {
                                    rows.at(index, &types, &budget)?.bind_fragment(
                                        &prepared.template,
                                        &types,
                                        &context,
                                        &budget,
                                        layout.row.clone(),
                                        &mut fragment,
                                    )?;
                                }
                            } else {
                                index = rows.position();
                                let Some(row) = rows.next(&types, &budget)? else {
                                    break;
                                };
                                row.bind_fragment(
                                    &prepared.template,
                                    &types,
                                    &context,
                                    &budget,
                                    layout.row.clone(),
                                    &mut fragment,
                                )?;
                            }
                            if !blocks
                                .push(&fragment)
                                .map_err(|error| errors::wire(py, error))?
                            {
                                pending = Some((index, fragment, key));
                                break;
                            }
                            end = if empty { 0 } else { index + 1 };
                        }
                        let Some(request) =
                            blocks.take().map_err(|error| errors::wire(py, error))?
                        else {
                            break;
                        };
                        (request, end)
                    } else {
                        let end = if empty { 0 } else { rows.position() + 1 };
                        let Some(row) = rows.next(&types, &budget)? else {
                            break;
                        };
                        (
                            row.bind(&prepared.template, &types, &context, &budget)?,
                            end,
                        )
                    };
                    let final_request = pending.is_none() && rows.remaining() == 0;
                    let job = connection.query_job(py, &mut lease, request, false)?;
                    input::check(py, &budget)?;
                    self.handoff(py, &mut begun)?;
                    self.receive_request(
                        py,
                        &connection,
                        &mut lease,
                        job,
                        &mut budget,
                        start..end,
                        final_request,
                        names_codec,
                        options,
                        &mut collected,
                    )?;
                    if final_request {
                        break;
                    }
                }
            } else {
                let (context, _, names_codec) = connection.context(py)?;
                let mut request = vec![3];
                input::text(sql, &mut request, &context, false, Some(&budget))?;
                let job = connection.query_job(py, &mut lease, request, false)?;
                input::check(py, &budget)?;
                self.handoff(py, &mut begun)?;
                self.receive_request(
                    py,
                    &connection,
                    &mut lease,
                    job,
                    &mut budget,
                    0..0,
                    true,
                    names_codec,
                    options,
                    &mut collected,
                )?;
            }
            Ok(())
        })();
        if !begun {
            return result.map(|()| py.None().into_bound(py));
        }
        if let Err(error) = &result {
            errors::note(py, error, collected.prefix.diagnostic());
        }
        let failed = result.is_err();
        let failure_note = result.as_ref().err().and_then(|error|
            errors::SavedFailure::capture(py,error).ok().map(std::sync::Arc::new));
        let batch = (!collected.total_unknown && collected.final_first_known).then_some(collected.total);
        let first = collected.chain.take_next();
        let publication = first.map_or(Ok(()), |set| self.publish_set(py, set));
        *self.results(py)? = Results {
            pending: if self.reader.is_closed() {Chain::default()} else {collected.chain},
            batch,
            failed,
            started: true,
            failure_note,
        };
        match (result, publication) {
            (Err(error), Err(secondary)) => {
                errors::additional(py, &error, secondary, "cursor.publish_prefix");
                Err(error)
            }
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(collected.total.into_pyobject(py)?.into_any()),
        }
    }
}
macro_rules! cursor_api {
    ($(($name:ident,$set:ident,$doc:literal)),* $(,)?) => {
        #[pymethods]
        impl Cursor {
            $(#[getter] #[doc=$doc]
            fn $name(&self,py:Python<'_>)->PyResult<bool>{Ok(self.settings(py)?.options.$name)}
            #[setter] fn $set(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::boolean(value,stringify!($name))?;
                let _operation=self.reader.operation(py)?;self.modifying(py,stringify!($name))?;
                self.settings(py)?.options.$name=value;Ok(())
            })*
            /// Execute one SQL operation, returning its actual total row count.
            /// Omitted args sends original SQL; explicit None binds one NULL.
            /// many/itemize are exact bool and keyword-only. All request chunks
            /// share one deadline. Confirmed prefixes never imply a commit.
            /// Escape validates supported built-in values, approved real
            /// subclasses, containers and optional NumPy/Pandas/Arrow/Polars
            /// inputs. One-dimensional sources follow many/itemize; tables
            /// supply logical rows even when many=False. Providers are not
            /// imported by this API. bytearray and memoryview are rejected.
            /// datetime.time keeps its clock fields and microseconds, ignores
            /// tzinfo and never calls timezone methods; 00:30+01:00 remains
            /// 00:30:00. Aware datetime, in contrast, is normalized to UTC.
            /// Keep args and all nested mutable values unchanged until this
            /// call returns. Each constructed Rust request owns stable bytes;
            /// batch construction does not provide an atomic snapshot of all inputs.
            #[pyo3(signature=(sql,args=Argument::default(),*,many=Argument::default(),itemize=Argument::default()))]
            #[pyo3(text_signature="($self, sql, args=..., *, many=False, itemize=True)")]
            fn execute<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:Argument<'_>,many:Argument<'_>,itemize:Argument<'_>)->PyResult<Bound<'py,PyAny>> {self.run(py,sql,args,&many,&itemize).map_err(|error|errors::at(py,error,"cursor.execute"))}
            /// Execute using the same native many=True,itemize=True operation.
            /// Input value and timezone rules are the same as execute().
            /// Keep args and all nested mutable values unchanged until this
            /// call returns. Each constructed Rust request owns stable bytes;
            /// batch construction does not provide an atomic snapshot of all inputs.
            fn executemany<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:&Bound<'_,PyAny>)->PyResult<Bound<'py,PyAny>> {
                let many=true.into_pyobject(py)?.to_owned().into_any();
                self.run(py,sql,Argument(Some(args.clone())),&Argument(Some(many.clone())),&Argument(Some(many)))
                    .map_err(|error|errors::at(py,error,"cursor.executemany"))
            }
            /// Advance to the next actual result set, discarding unread rows of
            /// the current set. True selects a set; None confirms normal end.
            /// Buffered sets are already native-owned. A stored failure boundary
            /// raises its original cause rather than claiming natural exhaustion.
            fn nextset(&self,py:Python<'_>)->PyResult<Option<bool>> {
                let _operation=self.reader.operation(py)?;
                self.reader.open(py)?;
                if !self.results(py)?.started {return Err(errors::client(py,ClientErrorCode::SessionState,"Cursor has no result sequence; execute a command before nextset"));}
                let next=self.results(py)?.pending.take_next();
                if let Some(set)=next { self.publish_set(py,set)?; return Ok(Some(true)); }
                self.reader.end_set(py)?;
                let native=self.results(py)?.pending.failure().map(|failure|
                    (std::sync::Arc::clone(&failure.error),failure.codec));
                if let Some((error,codec))=native {
                    return Err(errors::at(py,errors::query_with_codec(py,error.as_ref(),codec),"cursor.nextset"));
                }
                let (failed, snapshot) = {
                    let state=self.results(py)?;
                    (state.failed,state.failure_note.as_ref().map(std::sync::Arc::clone))
                };
                if let Some(snapshot)=snapshot {return Err(errors::at(py,snapshot.restore(py),"cursor.nextset"));}
                if failed {return Err(errors::client(py,ClientErrorCode::SessionState,
                    "The previous operation failed before a trustworthy diagnostic snapshot could be retained"));}
                Ok(None)
            }
            /// Sum only the first result count of each actual request. None
            /// means a contribution or an unattempted request remains unknown.
            #[getter] fn batch_row_count(&self,py:Python<'_>)->PyResult<Option<u128>> {Ok(self.results(py)?.batch)}
            /// Move within the current result. Buffered results allow both
            /// directions, including EOF. Invalid positions raise IndexError.
            #[pyo3(signature=(value,mode="relative"))]
            fn scroll(&self,py:Python<'_>,value:&Bound<'_,PyAny>,mode:&str)->PyResult<()> {self.reader.scroll(py,value,mode)}
            /// Return the next tuple row, or None when a valid result is exhausted.
            /// Buffered fetching remains valid after its Connection is closed,
            /// as long as this Cursor stays open. This sends no network request.
            fn fetchone<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {self.reader.fetchone(py)}
            /// Return at most size tuple rows atomically; None uses arraysize.
            /// Zero preserves the position but still requires a current result.
            /// Failed conversion leaves the complete original position unchanged.
            #[pyo3(signature=(size=None))]
            fn fetchmany<'py>(&self,py:Python<'py>,size:Option<&Bound<'py,PyAny>>)->PyResult<Bound<'py,PyAny>> {self.reader.fetchmany(py,size)}
            /// Return all remaining tuple rows atomically, without re-querying.
            fn fetchall<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {self.reader.fetchall(py)}
            /// Close only this Cursor, retaining readable local metadata,
            /// counters and configuration. Its Connection remains usable.
            pub(crate) fn close(&self,py:Python<'_>)->PyResult<()> {
                self.reader.close(py).map_err(|error|errors::at(py,error,"cursor.close"))?;
                let old={let mut state=self.results(py)?;state.failure_note=None;std::mem::take(&mut state.pending)};
                drop(old);Ok(())
            }
            /// True after this Cursor has been permanently retired locally.
            #[getter] fn closed(&self)->bool {self.reader.is_closed()}
            /// Positive default row count for fetchmany(None); initial value 1.
            /// Local assignment sends no SQL, never changes existing decode
            /// policy, and fails after this Cursor is closed. Readable after close.
            #[getter] fn arraysize(&self,py:Python<'_>)->PyResult<BigUint>{Ok(self.settings(py)?.arraysize.clone())}
            #[setter] fn set_arraysize(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()> {
                let value=config::positive(value,"arraysize")?;
                let count=usize::try_from(value.clone()).unwrap_or(usize::MAX);
                let _operation=self.reader.operation(py)?; self.modifying(py,"arraysize")?;
                self.reader.set_arraysize(py,count)?;self.settings(py)?.arraysize=value;Ok(())
            }
            /// PEP 249 row count, or -1 if not known. Readable after close.
            #[getter] fn rowcount<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                let rows=self.counters(py)?.as_ref().map(|value|value.rows);
                match rows {
                    None=>Ok((-1i32).into_pyobject(py)?.into_any()),
                    Some(rows)=>Ok(rows.into_pyobject(py)?.into_any()),
                }
            }
            /// Actual row count, or None if unknown; alias of affected_rows.
            #[getter] fn row_count(&self,py:Python<'_>)->PyResult<Option<u128>>{Ok(self.counters(py)?.as_ref().map(|value|value.rows))}
            /// Actual affected/result row count, or None if unknown.
            #[getter] fn affected_rows(&self,py:Python<'_>)->PyResult<Option<u128>>{self.row_count(py)}
            /// Actual final command insert ID, or None; no ID range is inferred.
            #[getter] fn lastrowid(&self,py:Python<'_>)->PyResult<Option<u64>>{Ok(self.counters(py)?.as_ref().and_then(|value|value.last_id))}
            /// Alias of lastrowid, reliably readable after close.
            #[getter] fn last_row_id(&self,py:Python<'_>)->PyResult<Option<u64>>{self.lastrowid(py)}
            /// Actual warnings summed once per completed command, or None if
            /// this call's overall result is unknown. Readable after close.
            #[getter] fn warning_count(&self,py:Python<'_>)->PyResult<Option<u128>>{Ok(self.counters(py)?.as_ref().map(|value|value.warnings))}
            /// Immutable current-result metadata, available lazily after close.
            #[getter] fn columns(&self,py:Python<'_>)->PyResult<Py<metadata::Columns>>{self.reader.columns(py)}
            /// Seven-field PEP 249 metadata, or None without a row result.
            #[getter] fn description<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.reader.description(py)}
            fn __iter__(slf:PyRef<'_,Self>)->PyRef<'_,Self>{slf}
            fn __next__<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {
                let row=self.reader.fetchone(py)?; if row.is_none(){Err(PyStopIteration::new_err(()))}else{Ok(row)}
            }
            fn __enter__<'py>(slf:PyRef<'py,Self>,py:Python<'py>)->PyResult<PyRef<'py,Self>>{slf.reader.open(py)?;Ok(slf)}
            fn __exit__(&self,py:Python<'_>,_typ:&Bound<'_,PyAny>,value:&Bound<'_,PyAny>,_traceback:&Bound<'_,PyAny>)->PyResult<bool>{
                match self.close(py) {
                    Ok(())=>Ok(false),
                    Err(error) if !value.is_none()=>{ errors::additional(py,&PyErr::from_value(value.clone()),error,"cursor.close");Ok(false) },
                    Err(error)=>Err(error),
                }
            }
        }
    }
}
cursor_api! {
    (decode_json,set_decode_json,"Default False. False returns the original JSON text as str. True parses the complete JSON into Python values; malformed, incomplete or out-of-range content raises DataError without text fallback. This applies to JSON fields, not JSON-looking TEXT. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_set,set_decode_set,"Default False. False returns SET text as str. True returns an ordered tuple[str, ...]; the empty SET is (), and an empty member is preserved. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_bit,set_decode_bit,"Default False. False returns the exact BIT bytes, including leading zero bytes. True returns an unsigned big-endian int, never a bool. Invalid BIT contents raise DataError. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_decimal,set_use_decimal,"Default True. True returns exact decimal.Decimal values for DECIMAL. False uses the native correctly rounded binary64 parser and returns float, potentially losing decimal precision; invalid or non-finite results raise DataError. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (use_timedelta,set_use_timedelta,"Default True. True returns datetime.timedelta for MySQL TIME, including negative and multi-day durations. False returns datetime.time and raises DataError for negative values or durations at least 24 hours. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_text_strict,set_decode_text_strict,"Default True. True raises UnicodeDecodeError for invalid text bytes. False preserves undecodable bytes through surrogateescape, without replacement characters. Binary fields remain bytes and input encoding is unaffected. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
    (decode_date_strict,set_decode_date_strict,"Default True. True raises DataError for zero or otherwise unrepresentable DATE/DATETIME values. False returns their original calendar text as str. Valid values retain date/datetime output in both modes. SQL NULL remains None.\n\nThe initial value is copied from Connection. Changes apply only to subsequent executions; existing results keep their execution-time snapshot. Exact bool assignment only; other types raise TypeError. Reads and assignments send no SQL. Saved values remain readable after close, but assignment after close raises InterfaceError."),
}
