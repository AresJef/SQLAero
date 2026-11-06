//! One CALL's attached input preparation and owned native target requests.

use super::{PlanMode, PreparedKey, PreparedTemplate, Request, Rows, ScalarTypes};
use crate::{errors, input};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple};
use sqlaero_core::bind::value::{ValueError, ValueRef, ValueSink};
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::protocol::prepared::Parameters;
use sqlaero_core::transport::deadline::IoBudget;
use std::{cell::Cell, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    In,
    Out,
    InOut,
}

/// One shared prefix for generated slot names and their trusted validation.
pub(crate) const SLOT_NAMESPACE: &str = "_sqlaero_call_";

/// Complete owned requests for one logical CALL. The execution owner decides
/// combined versus segmented delivery before sending, keeps private responses
/// private, and owns the slot namespace, output readiness and cleanup lifetime.
pub(crate) struct NativeCall {
    pub(crate) directions: Vec<Direction>,
    pub(crate) output_positions: Vec<usize>,
    pub(crate) initialize: Option<Vec<u8>>,
    pub(crate) call: Request,
    pub(crate) select: Option<Vec<u8>>,
    pub(crate) cleanup: Option<Vec<u8>>,
}

impl NativeCall {
    #[expect(
        clippy::too_many_arguments,
        reason = "one attached CALL boundary supplies explicit input and native execution snapshots"
    )]
    pub(crate) fn new(
        proc_name: &Bound<'_, PyString>,
        args: Option<&Bound<'_, PyAny>>,
        database: Option<&Bound<'_, PyAny>>,
        parameter_modes: Option<&Bound<'_, PyAny>>,
        slot_prefix: &str,
        mode: PlanMode,
        types: &ScalarTypes,
        context: &Context,
        ansi: bool,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        let py = proc_name.py();
        input::check(py, budget)?;
        if mode == PlanMode::PreparedDisplay {
            return Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "CALL cannot use a display-only execution mode",
            ));
        }
        let mut target = Vec::new();
        append(py, &mut target, b"CALL ", context.limit)?;
        if let Some(database) = database.filter(|value| !value.is_none()) {
            input::control::identifier(database, "database", &mut target, context, budget)?;
            append(py, &mut target, b".", context.limit)?;
        }
        input::control::identifier(
            proc_name.as_any(),
            "proc_name",
            &mut target,
            context,
            budget,
        )?;
        let mut rows = args
            .map(|args| Rows::new(args, false, true, types, budget))
            .transpose()?;
        if rows.as_ref().is_some_and(|rows| rows.remaining() > 1) {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "callproc accepts one parameter row; a multirow source is not supported",
            ));
        }
        let row = rows
            .as_mut()
            .map(|rows| rows.next(types, budget))
            .transpose()?
            .flatten();
        let width = row.as_ref().map_or(0, |row| row.width());
        let directions = modes(py, parameter_modes, width, budget)?;
        let mut output_positions = Vec::new();
        for (position, direction) in directions.iter().enumerate() {
            if position.is_multiple_of(4096) {
                input::check(py, budget)?;
            }
            if *direction != Direction::In {
                output_positions
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                output_positions.push(position);
            }
        }
        let source_rows = 0..usize::from(args.is_some());
        if mode == PlanMode::PreparedNative {
            let parameters = super::prepared_count(py, width)?;
            let mut sink = Tracked {
                inner: Parameters::new(0, parameters, context.limit)
                    .map_err(|error| input::sink::failure(py, error))?,
                null: false,
            };
            if let Some(row) = row {
                row.write_parameters_checked(types, context, budget, &mut sink, |index, sink| {
                    let direction = directions.get(index).ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::ParameterCount,
                            "CALL parameter row changed width during preparation",
                        )
                    })?;
                    validate_out(py, index, *direction, sink.null)
                })?;
            }
            let bytes = sink
                .inner
                .finish()
                .map_err(|error| input::sink::failure(py, error))?;
            let mut request = Vec::new();
            append(py, &mut request, &[0x16], context.limit)?;
            append_chunks(py, &mut request, &target, context.limit, budget)?;
            append(py, &mut request, b"(", context.limit)?;
            for index in 0..width {
                if index != 0 {
                    append(py, &mut request, b",", context.limit)?;
                }
                append(py, &mut request, b"?", context.limit)?;
                input::check(py, budget)?;
            }
            append(py, &mut request, b")", context.limit)?;
            input::check(py, budget)?;
            return Ok(Self {
                directions,
                output_positions,
                initialize: None,
                select: None,
                cleanup: None,
                call: Request {
                    bytes,
                    source_rows,
                    final_request: true,
                    prepared: Some(PreparedTemplate {
                        request: Arc::new(request),
                        parameters,
                        key: PreparedKey::new(context, ansi, true),
                    }),
                },
            });
        }
        let mut values = Vec::new();
        let mut ranges = Vec::new();
        ranges
            .try_reserve_exact(width)
            .map_err(|_| errors::memory_error(py))?;
        if let Some(row) = row {
            let start = Cell::new(0);
            // This single native buffer stores stable text fragments, never
            // per-value Python SQL strings. IN and INOUT occupy different
            // final requests, so their combined capture may exceed one packet.
            row.walk::<false>(
                types,
                &Context {
                    limit: context.limit.saturating_mul(2),
                    ..*context
                },
                budget,
                &mut values,
                |index, output| {
                    start.set(output.len());
                    let direction = directions.get(index).ok_or_else(|| {
                        errors::client(
                            py,
                            ClientErrorCode::ParameterCount,
                            "CALL parameter row changed width during preparation",
                        )
                    })?;
                    Ok(*direction == Direction::Out)
                },
                |_, output, _| {
                    ranges.push(start.get()..output.len());
                    Ok(())
                },
            )?;
        }
        if ranges.len() != width {
            return Err(errors::client(
                py,
                ClientErrorCode::ParameterCount,
                "CALL parameter row changed width during preparation",
            ));
        }
        if !output_positions.is_empty() {
            validate_prefix(py, slot_prefix, output_positions.len())?;
        }
        let mut call = Vec::new();
        append(py, &mut call, &[3], context.limit)?;
        append_chunks(py, &mut call, &target, context.limit, budget)?;
        append(py, &mut call, b"(", context.limit)?;
        let mut slot = 0;
        for (index, direction) in directions.iter().enumerate() {
            if index != 0 {
                append(py, &mut call, b",", context.limit)?;
            }
            if *direction == Direction::In {
                append_chunks(
                    py,
                    &mut call,
                    &values[ranges[index].clone()],
                    context.limit,
                    budget,
                )?;
            } else {
                append_slot(py, &mut call, slot_prefix, slot, context.limit)?;
                slot += 1;
            }
            input::check(py, budget)?;
        }
        append(py, &mut call, b")", context.limit)?;
        let (initialize, select, cleanup) = if output_positions.is_empty() {
            (None, None, None)
        } else {
            let mut initialize = Vec::new();
            let mut select = Vec::new();
            let mut cleanup = Vec::new();
            append(py, &mut initialize, b"\x03SET ", context.limit)?;
            append(py, &mut select, b"\x03SELECT ", context.limit)?;
            append(py, &mut cleanup, b"\x03SET ", context.limit)?;
            for (slot, position) in output_positions.iter().copied().enumerate() {
                for output in [&mut initialize, &mut select, &mut cleanup] {
                    if slot != 0 {
                        append(py, output, b",", context.limit)?;
                    }
                    append_slot(py, output, slot_prefix, slot, context.limit)?;
                }
                append(py, &mut initialize, b"=", context.limit)?;
                append_chunks(
                    py,
                    &mut initialize,
                    &values[ranges[position].clone()],
                    context.limit,
                    budget,
                )?;
                append(py, &mut cleanup, b"=NULL", context.limit)?;
            }
            (Some(initialize), Some(select), Some(cleanup))
        };
        input::check(py, budget)?;
        Ok(Self {
            directions,
            output_positions,
            initialize,
            select,
            cleanup,
            call: Request {
                bytes: call,
                source_rows,
                final_request: true,
                prepared: None,
            },
        })
    }

    /// Preflight the one optional combined text request. A capacity miss means
    /// segmented delivery; actual allocation, signal and timeout errors remain
    /// failures. This method never changes a path after any request was sent.
    pub(crate) fn combined(
        &self,
        py: Python<'_>,
        limit: usize,
        budget: &IoBudget,
    ) -> PyResult<Option<Vec<u8>>> {
        input::check(py, budget)?;
        let (Some(initialize), Some(select), Some(cleanup)) =
            (&self.initialize, &self.select, &self.cleanup)
        else {
            return Ok(None);
        };
        let stages = [initialize, &self.call.bytes, select, cleanup];
        // Four command prefixes become one prefix and three semicolons, so
        // the complete combined request has the sum of the original lengths.
        let required = stages
            .iter()
            .try_fold(0_usize, |sum, stage| sum.checked_add(stage.len()));
        if required.is_none_or(|required| required > limit) {
            return Ok(None);
        }
        let mut output = Vec::new();
        for (index, stage) in stages.iter().enumerate() {
            append(
                py,
                &mut output,
                if index == 0 { b"\x03" } else { b";" },
                limit,
            )?;
            append_chunks(py, &mut output, &stage[1..], limit, budget)?;
        }
        input::check(py, budget)?;
        Ok(Some(output))
    }
}

fn modes(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
    width: usize,
    budget: &IoBudget,
) -> PyResult<Vec<Direction>> {
    let mut modes = Vec::new();
    modes
        .try_reserve_exact(width)
        .map_err(|_| errors::memory_error(py))?;
    let Some(value) = value.filter(|value| !value.is_none()) else {
        for index in 0..width {
            if index.is_multiple_of(4096) {
                input::check(py, budget)?;
            }
            modes.push(Direction::In);
        }
        return Ok(modes);
    };
    if !value.is_exact_instance_of::<PyList>() && !value.is_exact_instance_of::<PyTuple>() {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "parameter_modes must be an exact list or tuple of IN, OUT, or INOUT strings, or None",
        ));
    }
    if value.len()? != width {
        return Err(errors::builtin_format::<PyValueError>(
            py,
            format_args!("parameter_modes must contain exactly {width} directions"),
        ));
    }
    for index in 0..width {
        input::check(py, budget)?;
        let item = if let Ok(list) = value.cast::<PyList>() {
            list.get_item(index)?
        } else {
            value
                .cast::<PyTuple>()?
                .get_borrowed_item(index)?
                .to_owned()
        };
        let source = item.cast::<PyString>().map_err(|_| {
            errors::builtin_format::<PyTypeError>(
                py,
                format_args!("parameter_modes[{index}] must be str"),
            )
        })?;
        let text = source.to_str()?;
        let direction = if text.eq_ignore_ascii_case("IN") {
            Direction::In
        } else if text.eq_ignore_ascii_case("OUT") {
            Direction::Out
        } else if text.eq_ignore_ascii_case("INOUT") {
            Direction::InOut
        } else {
            return Err(errors::builtin_format::<PyValueError>(
                py,
                format_args!("parameter_modes[{index}] must be IN, OUT, or INOUT"),
            ));
        };
        modes.push(direction);
    }
    Ok(modes)
}
fn validate_out(py: Python<'_>, index: usize, direction: Direction, null: bool) -> PyResult<()> {
    if direction == Direction::Out && !null {
        return Err(errors::builtin_format::<PyValueError>(
            py,
            format_args!("args[{index}] is OUT and must be None (a source NULL)"),
        ));
    }
    Ok(())
}
fn validate_prefix(py: Python<'_>, prefix: &str, count: usize) -> PyResult<()> {
    let digits = count
        .saturating_sub(1)
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1);
    if !prefix.starts_with(SLOT_NAMESPACE)
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || prefix
            .len()
            .checked_add(digits)
            .is_none_or(|length| length > 64)
    {
        return Err(errors::client(
            py,
            ClientErrorCode::InternalState,
            "invalid internal CALL variable namespace",
        ));
    }
    Ok(())
}
fn append_slot(
    py: Python<'_>,
    output: &mut Vec<u8>,
    prefix: &str,
    index: usize,
    limit: usize,
) -> PyResult<()> {
    append(py, output, b"@", limit)?;
    append(py, output, prefix.as_bytes(), limit)?;
    scalar::integer(output, index, limit).map_err(|error| errors::wire(py, error))
}
fn append(py: Python<'_>, output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> PyResult<()> {
    writer::append(output, bytes, limit).map_err(|error| errors::wire(py, error))
}
fn append_chunks(
    py: Python<'_>,
    output: &mut Vec<u8>,
    bytes: &[u8],
    limit: usize,
    budget: &IoBudget,
) -> PyResult<()> {
    for bytes in bytes.chunks(256 * 1024) {
        input::check(py, budget)?;
        append(py, output, bytes, limit)?;
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct NullGuard {
    null: bool,
}
impl NullGuard {
    pub(super) fn validate(&self, py: Python<'_>, index: usize) -> PyResult<()> {
        validate_out(py, index, Direction::Out, self.null)
    }
}
impl ValueSink for NullGuard {
    type Error = ValueError;
    fn write(&mut self, value: ValueRef<'_>, _: &Context) -> Result<(), ValueError> {
        self.null = matches!(value, ValueRef::Null);
        Ok(())
    }
    fn text_start(&mut self, _: usize, _: &Context) -> Result<(), ValueError> {
        self.null = false;
        Ok(())
    }
    fn text_fragment(&mut self, _: &str, _: &Context) -> Result<(), ValueError> {
        Ok(())
    }
    fn text_end(&mut self, _: &Context) -> Result<(), ValueError> {
        Ok(())
    }
    fn bytes_start(&mut self, _: usize, _: &Context) -> Result<(), ValueError> {
        self.null = false;
        Ok(())
    }
    fn bytes_fragment(&mut self, _: &[u8], _: &Context) -> Result<(), ValueError> {
        Ok(())
    }
    fn bytes_end(&mut self, _: &Context) -> Result<(), ValueError> {
        Ok(())
    }
}
struct Tracked<S> {
    inner: S,
    null: bool,
}
impl<S: ValueSink<Error = ValueError>> ValueSink for Tracked<S> {
    type Error = ValueError;
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.null = matches!(value, ValueRef::Null);
        self.inner.write(value, context)
    }
    fn text_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.null = false;
        self.inner.text_start(length, context)
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        self.inner.text_fragment(text, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.inner.text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.null = false;
        self.inner.bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        self.inner.bytes_fragment(bytes, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.inner.bytes_end(context)
    }
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
