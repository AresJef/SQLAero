//! Thin public Cursor over one native incremental request and buffered Reader.

use crate::{
    binding,
    config::{self, Argument},
    connection::{CommandRole, Connection},
    errors,
    input::{self, ScalarTypes},
    metadata,
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
    error::ClientErrorCode,
    result::{decode::Options, text::Response},
    session::query::ConfirmedPrefix,
};
use std::sync::{Mutex, MutexGuard};

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
}
impl Cursor {
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
        }
    }
    fn settings(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Settings>> {
        self.settings.lock().map_err(|_| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "Cursor configuration is unavailable after a native failure",
            )
        })
    }
    fn counters(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, Option<Counts>>> {
        self.counts.lock().map_err(|_| {
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
    fn handoff(&self, py: Python<'_>, begun: &mut bool) -> PyResult<()> {
        if !*begun {
            self.reader.clear_result(py)?;
            *self.counters(py)? = None;
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
            pyo3::exceptions::PyTypeError::new_err(
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
        let mut prefix = ConfirmedPrefix::default();
        let mut begun = false;
        let result = (|| {
            connection.ensure_context(py, &mut lease, &mut budget)?;
            let mut last = None;
            if let Some(args) = args.0 {
                let mut rows = binding::Rows::new(&args, many, itemize);
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
                            py, sql, &context, ansi, many, itemize, &budget,
                        )?);
                    }
                    let prepared = prepared.as_mut().ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::InternalState,
                            "SQL preparation was not published",
                        )
                    })?;
                    let start = prefix.source_rows;
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
                                    rows.at(index)?.bind_fragment(
                                        &prepared.template,
                                        &types,
                                        &context,
                                        &budget,
                                        layout.row.clone(),
                                        &mut fragment,
                                    )?;
                                }
                            } else {
                                let Some(row) = rows.next()? else { break };
                                index = rows.position() - 1;
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
                        let Some(row) = rows.next()? else { break };
                        let end = if empty { 0 } else { rows.position() };
                        (
                            row.bind(&prepared.template, &types, &context, &budget)?,
                            end,
                        )
                    };
                    let final_request = pending.is_none() && rows.remaining() == 0;
                    let job = connection.query_job(py, &mut lease, request, !final_request)?;
                    input::check(py, &budget)?;
                    self.handoff(py, &mut begun)?;
                    let response = connection
                        .drive(py, &mut lease, job, &mut budget, CommandRole::UserSql)
                        .map_err(|failure| failure.record_prefix(py, &mut prefix, start..end))?;
                    prefix
                        .record(start..end, &response)
                        .map_err(|error| errors::wire(py, error))?;
                    last = Some((response, names_codec));
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
                let response = connection
                    .drive(py, &mut lease, job, &mut budget, CommandRole::UserSql)
                    .map_err(|failure| failure.record_prefix(py, &mut prefix, 0..0))?;
                prefix
                    .record(0..0, &response)
                    .map_err(|error| errors::wire(py, error))?;
                last = Some((response, names_codec));
            }
            let Some((response, codec)) = last else {
                return Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "an execution produced no actual command",
                ));
            };
            input::check(py, &budget)?;
            let prepared = if let Response::Rows(rows) = response {
                Some(Reader::prepare(py, rows, options, codec)?)
            } else {
                None
            };
            let returned_count = prefix.row_count.into_pyobject(py)?.into_any();
            input::check(py, &budget)?;
            if let Some(prepared) = prepared {
                self.reader.publish(py, prepared)?;
            }
            *self.counters(py)? = Some(Counts {
                rows: prefix.row_count,
                last_id: prefix.last_insert_id,
                warnings: prefix.warnings,
            });
            Ok(returned_count)
        })();
        if let Err(error) = &result
            && begun
        {
            // Publication is all-or-nothing, including decode-plan/resource
            // preparation after a completely consumed final response.
            self.reader.clear_result(py)?;
            *self.counters(py)? = None;
            errors::note(py, error, prefix.diagnostic());
        }
        result
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
            #[pyo3(signature=(sql,args=Argument::default(),*,many=Argument::default(),itemize=Argument::default()))]
            #[pyo3(text_signature="($self, sql, args=..., *, many=False, itemize=True)")]
            fn execute<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:Argument<'_>,many:Argument<'_>,itemize:Argument<'_>)->PyResult<Bound<'py,PyAny>> {self.run(py,sql,args,&many,&itemize).map_err(|error|errors::at(py,error,"cursor.execute"))}
            /// Execute using the same native many=True,itemize=True operation.
            fn executemany<'py>(&self,py:Python<'py>,sql:&Bound<'_,PyAny>,args:&Bound<'_,PyAny>)->PyResult<Bound<'py,PyAny>> {
                let many=true.into_pyobject(py)?.to_owned().into_any();
                self.run(py,sql,Argument(Some(args.clone())),&Argument(Some(many.clone())),&Argument(Some(many)))
                    .map_err(|error|errors::at(py,error,"cursor.executemany"))
            }
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
            pub(crate) fn close(&self,py:Python<'_>)->PyResult<()> {self.reader.close(py).map_err(|error|errors::at(py,error,"cursor.close"))}
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
                match self.counters(py)?.as_ref() {
                    None=>Ok((-1i32).into_pyobject(py)?.into_any()),
                    Some(value)=>Ok(value.rows.into_pyobject(py)?.into_any()),
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
    (decode_json,set_decode_json,"Decode JSON through the native parser. Default copied from Connection. Exact bool assignment sends no SQL and applies only to subsequent executions; old results keep their snapshot. Modification fails after Cursor.close; saved values stay readable."),
    (decode_set,set_decode_set,"Decode MySQL SET as ordered tuple[str,...] instead of text. Default copied from Connection. Exact bool assignment affects only subsequent executions and sends no SQL. Existing results stay unchanged; modification fails after Cursor.close."),
    (decode_bit,set_decode_bit,"Decode MySQL BIT as unsigned int instead of bytes. Default copied from Connection. Exact bool assignment affects only subsequent executions, sends no SQL and fails after Cursor.close. Existing result policy and saved values remain readable."),
    (use_decimal,set_use_decimal,"Use exact decimal.Decimal for DECIMAL when True, binary64 float when False. Default copied from Connection. Exact bool assignment affects only subsequent executions, sends no SQL and fails after Cursor.close; old results retain their policy."),
    (use_timedelta,set_use_timedelta,"Use timedelta for MySQL TIME when True, time when False. Values outside clock range fail in time mode. Default copied from Connection. Exact bool assignment affects only subsequent executions, sends no SQL and fails after Cursor.close."),
    (decode_text_strict,set_decode_text_strict,"Reject invalid text when True, preserve invalid bytes with surrogateescape when False. Default copied from Connection. Exact bool assignment affects only subsequent executions and sends no SQL. Old results retain their policy; modification fails after Cursor.close."),
    (decode_date_strict,set_decode_date_strict,"Reject invalid calendar dates when True, return their complete original text when False. Default copied from Connection. Exact bool assignment affects only subsequent executions, sends no SQL and fails after Cursor.close; old results retain their policy."),
}
