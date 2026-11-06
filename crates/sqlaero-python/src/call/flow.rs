//! One CALL's native request sequence; public result ownership stays outside.

use super::Output;
use crate::binding::call::{Direction, NativeCall};
use crate::connection::{CommandRole, Connection, DriveFailure};
use crate::{errors, input};
use pyo3::prelude::*;
use sqlaero_core::charset::output::OutputCodec;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Reservation, Scope};
use sqlaero_core::protocol::StatusFlags;
use sqlaero_core::protocol::framing::{MessageWriter, WireError};
use sqlaero_core::result::chain::Failure;
use sqlaero_core::result::text::{CommandConfirmation, Response, RowBatch, StreamEvent};
use sqlaero_core::session::query::{Managed, Query, RequestProgress};
use sqlaero_core::session::shared::CommandLease;
use sqlaero_core::transport::deadline::IoBudget;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Confirmed operation policy. No live setting is refreshed between requests.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub(crate) multi_statements: bool,
    pub(crate) streaming: bool,
    pub(crate) send_limit: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Single,
    Native,
    Combined,
    Split,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Initialize,
    Call,
    Select,
    Cleanup,
    Done,
    Failed,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Initialize => "initializing private CALL slots",
            Self::Call => "receiving CALL business results",
            Self::Select => "receiving private CALL output",
            Self::Cleanup => "clearing private CALL slots",
            Self::Done => "completed CALL",
            Self::Failed => "failed CALL",
        }
    }
}

/// Only CALL pays for this fixed, checked allocation. No Python input, live
/// exception, Connection, lease or IoBudget is retained across public returns.
pub(crate) struct Flow {
    state: Vec<State>,
    _owner: Reservation,
    _extra_capacity: Option<Reservation>,
}
struct State {
    layout: Layout,
    phase: Phase,
    current: Query,
    call: Option<Query>,
    select: Option<Query>,
    cleanup: Option<Query>,
    current_streaming: bool,
    streaming: bool,
    prepared: bool,
    expected: usize,
    output: Arc<Output>,
    private_row: Option<RowBatch>,
    pending_native: Option<RowBatch>,
    native_codec: Option<OutputCodec>,
    output_saved: bool,
    private_header: bool,
    select_codec: Option<OutputCodec>,
    live_codec: Option<OutputCodec>,
    public_codec: Option<OutputCodec>,
    boundary: bool,
    variables_touched: bool,
    call_started: bool,
    call_complete: bool,
    call_confirmation: Option<CommandConfirmation>,
    output_received: bool,
    cleanup_attempted: bool,
    cleanup_confirmed: bool,
    last_synchronized: bool,
    failure: Option<Failure>,
    statement_invalidated: bool,
}

fn internal(py: Python<'_>, message: &'static str) -> PyErr {
    errors::client(py, ClientErrorCode::InternalState, message)
}
fn codec(py: Python<'_>, connection: &Connection) -> PyResult<Option<OutputCodec>> {
    match connection.native.facts.lock() {
        Ok(facts) => Ok(facts.metadata_codec()),
        Err(error) => {
            drop(error.into_inner());
            connection.native.retire();
            Err(internal(py, "CALL metadata facts are unavailable"))
        }
    }
}
fn out(status: StatusFlags, metadata: Option<StatusFlags>) -> bool {
    (status | metadata.unwrap_or_default()).contains(StatusFlags::SERVER_PS_OUT_PARAMS)
}
fn query(
    py: Python<'_>,
    connection: &Connection,
    lease: &mut CommandLease<'_>,
    bytes: Vec<u8>,
    policy: Policy,
    streaming: bool,
    outputs: usize,
) -> PyResult<Query> {
    MessageWriter::new(bytes.len(), policy.send_limit).map_err(|error| errors::wire(py, error))?;
    let mut query = connection.query_job(py, lease, bytes, false)?;
    if streaming {
        query = query
            .with_streaming()
            .map_err(|error| errors::query(py, error))?;
    }
    if outputs != 0 {
        query = query
            .with_managed_outputs(outputs)
            .map_err(|error| errors::query(py, error))?;
    }
    Ok(query)
}

impl Flow {
    #[expect(
        clippy::too_many_arguments,
        reason = "one CALL supplies owned requests and the existing operation's policy and allocation owner"
    )]
    pub(crate) fn new(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        native: NativeCall,
        output: Arc<Output>,
        policy: Policy,
        scope: &Arc<Scope>,
        budget: &IoBudget,
    ) -> PyResult<Self> {
        input::check(py, budget)?;
        let prepared = native.call.bytes.first() == Some(&0x17);
        if !matches!(native.call.bytes.first(), Some(3 | 0x17)) {
            return Err(internal(py, "CALL request has an unexpected command byte"));
        }
        let expected = native
            .directions
            .iter()
            .filter(|direction| **direction != Direction::In)
            .count();
        if expected != native.output_positions.len() {
            return Err(internal(
                py,
                "CALL output directions and positions disagree",
            ));
        }
        let combined = if !prepared && expected != 0 && policy.multi_statements {
            native.combined(py, policy.send_limit, budget)?
        } else {
            None
        };
        let NativeCall {
            directions: _,
            output_positions: _,
            initialize,
            call,
            select,
            cleanup,
        } = native;
        let (layout, phase, current, later_call, later_select, later_cleanup, current_streaming) =
            if prepared || expected == 0 {
                if initialize.is_some() || select.is_some() || cleanup.is_some() {
                    return Err(internal(
                        py,
                        "CALL preparation mixed native or IN-only execution with private text requests",
                    ));
                }
                let current = query(
                    py,
                    connection,
                    lease,
                    call.bytes,
                    policy,
                    policy.streaming,
                    if prepared { expected } else { 0 },
                )?;
                (
                    if prepared {
                        Layout::Native
                    } else {
                        Layout::Single
                    },
                    Phase::Call,
                    current,
                    None,
                    None,
                    None,
                    policy.streaming,
                )
            } else {
                let (Some(initialize), Some(select), Some(cleanup)) = (initialize, select, cleanup)
                else {
                    return Err(internal(
                        py,
                        "Text CALL output preparation omitted a required private request",
                    ));
                };
                // All possible requests, including the sole safe rescue cleanup,
                // are owned and checked before the first request can be sent.
                let cleanup = query(py, connection, lease, cleanup, policy, false, 0)?;
                if let Some(combined) = combined {
                    let current =
                        query(py, connection, lease, combined, policy, policy.streaming, 0)?;
                    (
                        Layout::Combined,
                        Phase::Initialize,
                        current,
                        None,
                        None,
                        Some(cleanup),
                        policy.streaming,
                    )
                } else {
                    let current = query(py, connection, lease, initialize, policy, false, 0)?;
                    let call = query(
                        py,
                        connection,
                        lease,
                        call.bytes,
                        policy,
                        policy.streaming,
                        0,
                    )?;
                    let select = query(py, connection, lease, select, policy, false, 0)?;
                    (
                        Layout::Split,
                        Phase::Initialize,
                        current,
                        Some(call),
                        Some(select),
                        Some(cleanup),
                        false,
                    )
                }
            };
        let live_codec = codec(py, connection)?;
        let owner = scope
            .charge(std::mem::size_of::<State>())
            .map_err(|error| crate::output::memory(py, error))?;
        let mut state = Vec::new();
        state
            .try_reserve_exact(1)
            .map_err(|_| errors::memory_error(py))?;
        let actual = state
            .capacity()
            .checked_mul(std::mem::size_of::<State>())
            .ok_or_else(|| internal(py, "CALL control capacity overflow"))?;
        let extra = actual
            .checked_sub(std::mem::size_of::<State>())
            .ok_or_else(|| internal(py, "CALL control allocation has no slot"))?;
        let extra_capacity = if extra == 0 {
            None
        } else {
            Some(
                scope
                    .charge(extra)
                    .map_err(|error| crate::output::memory(py, error))?,
            )
        };
        state.push(State {
            layout,
            phase,
            current,
            call: later_call,
            select: later_select,
            cleanup: later_cleanup,
            current_streaming,
            streaming: policy.streaming,
            prepared,
            expected,
            output,
            private_row: None,
            pending_native: None,
            native_codec: None,
            output_saved: false,
            private_header: false,
            select_codec: None,
            live_codec,
            public_codec: None,
            boundary: false,
            variables_touched: false,
            call_started: false,
            call_complete: false,
            call_confirmation: None,
            output_received: false,
            cleanup_attempted: false,
            cleanup_confirmed: false,
            last_synchronized: true,
            failure: None,
            statement_invalidated: false,
        });
        Ok(Self {
            state,
            _owner: owner,
            _extra_capacity: extra_capacity,
        })
    }
    fn inner(&self) -> &State {
        &self.state[0]
    }
    fn inner_mut(&mut self) -> &mut State {
        &mut self.state[0]
    }
    pub(crate) fn is_finished(&self) -> bool {
        matches!(self.inner().phase, Phase::Done | Phase::Failed)
    }
    pub(crate) fn call_completed(&self) -> bool {
        self.inner().call_complete
    }
    pub(crate) fn call_confirmation(&self) -> Option<CommandConfirmation> {
        self.inner().call_confirmation
    }
    pub(crate) fn invalidates_statement(&self) -> bool {
        self.inner().statement_invalidated
    }
    pub(crate) fn synchronized(&self) -> bool {
        let state = self.inner();
        self.is_finished()
            && state.last_synchronized
            && (!state.variables_touched || state.cleanup_confirmed)
    }
    pub(crate) fn metadata_codec(&self) -> Option<OutputCodec> {
        self.inner().public_codec
    }
    pub(crate) fn at_stream_boundary(&self) -> bool {
        let state = self.inner();
        state.phase == Phase::Call && state.boundary && state.current.at_stream_boundary()
    }
    pub(crate) fn take_native_failure(&mut self) -> Option<Failure> {
        self.inner_mut().failure.take()
    }
    pub(crate) fn discard_current_rows(&mut self, py: Python<'_>) -> PyResult<()> {
        if !self.at_stream_boundary() {
            return Err(internal(
                py,
                "CALL discard requires a current public row boundary",
            ));
        }
        self.inner_mut()
            .current
            .discard_current_rows()
            .map_err(|error| errors::query(py, error))
    }
    pub(crate) fn advance_buffered(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<Option<Response>> {
        if self.inner().streaming {
            return Err(internal(py, "Streaming CALL requires streaming delivery"));
        }
        let result = self.inner_mut().buffered(py, connection, lease, budget);
        result.map_err(|error| {
            self.inner_mut()
                .failed(py, connection, lease, budget, error)
        })
    }
    pub(crate) fn advance_stream(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
        row_limit: NonZeroUsize,
    ) -> PyResult<Option<StreamEvent>> {
        if !self.inner().streaming {
            return Err(internal(py, "Buffered CALL requires buffered delivery"));
        }
        self.inner_mut().boundary = false;
        let result = self
            .inner_mut()
            .stream(py, connection, lease, budget, row_limit);
        result.map_err(|error| {
            self.inner_mut()
                .failed(py, connection, lease, budget, error)
        })
    }
}

impl State {
    fn progress(&mut self) {
        if self.current.progress() != RequestProgress::NotSent {
            if self.layout == Layout::Combined || self.phase == Phase::Initialize {
                self.variables_touched = true;
            }
            if self.layout == Layout::Combined || self.phase == Phase::Call {
                self.call_started = true;
            }
        }
    }
    fn received<T>(&mut self, result: Result<T, DriveFailure>) -> PyResult<T> {
        self.progress();
        match result {
            Ok(value) => {
                self.last_synchronized = self.current.is_finished();
                Ok(value)
            }
            Err(failure) => {
                self.last_synchronized = failure.synchronized();
                if self.phase == Phase::Call
                    && let Some(confirmation) = failure
                        .command_set_confirmation()
                        .or_else(|| failure.confirmation().filter(|value| !value.has_rows()))
                {
                    self.call_complete = true;
                    self.call_confirmation = Some(confirmation);
                }
                if let Some(native) = failure.native_failure() {
                    self.statement_invalidated |=
                        self.prepared && crate::statement::invalidates(&native.error);
                    if self.failure.is_none() {
                        self.failure = Some(native);
                    }
                }
                Err(failure.into())
            }
        }
    }
    fn track(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        status: StatusFlags,
    ) -> PyResult<()> {
        if status.contains(StatusFlags::SERVER_SESSION_STATE_CHANGED) {
            self.live_codec = codec(py, connection)?;
        }
        Ok(())
    }
    fn more(&mut self, py: Python<'_>, connection: &Connection, expected: bool) -> PyResult<()> {
        if self.current.is_finished() == expected {
            // A structurally valid packet cannot prove an omitted known stage
            // executed. Do not guess a safe replay of its cleanup request.
            self.last_synchronized = false;
            connection.native.retire();
            return Err(errors::wire(
                py,
                WireError::Malformed(
                    "CALL response ended before or beyond its known request stages",
                ),
            ));
        }
        Ok(())
    }
    fn initialized(&mut self, py: Python<'_>, connection: &Connection) -> PyResult<()> {
        self.more(py, connection, self.layout == Layout::Combined)?;
        self.phase = Phase::Call;
        if self.layout == Layout::Split {
            self.current = self
                .call
                .take()
                .ok_or_else(|| internal(py, "CALL request owner is missing"))?;
            self.current_streaming = self.streaming;
        }
        Ok(())
    }
    fn called(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        confirmation: CommandConfirmation,
    ) -> PyResult<()> {
        self.call_complete = true;
        self.call_confirmation = Some(confirmation);
        self.more(py, connection, self.layout == Layout::Combined)?;
        if self.layout == Layout::Combined || self.layout == Layout::Split {
            self.phase = Phase::Select;
            if self.layout == Layout::Split {
                self.current = self
                    .select
                    .take()
                    .ok_or_else(|| internal(py, "CALL output request owner is missing"))?;
                self.current_streaming = false;
            }
        } else {
            if self.expected != 0 && !self.output_received {
                return Err(errors::client(
                    py,
                    ClientErrorCode::ParameterCount,
                    "The completed prepared CALL did not return its declared OUT/INOUT values",
                ));
            }
            // The OUT carrier precedes the actual CALL tail. Only this real
            // successful tail permits publication into the ready-capable owner.
            if self.layout == Layout::Native {
                self.commit_native(py)?;
            }
            self.phase = Phase::Done;
            self.output.finish();
        }
        Ok(())
    }
    fn selected(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        rows: RowBatch,
        codec: Option<OutputCodec>,
    ) -> PyResult<()> {
        // A local save failure may still consume the already ordered cleanup.
        self.phase = Phase::Cleanup;
        self.output.save(py, rows, codec)?;
        self.output_received = true;
        self.output_saved = true;
        if self.layout == Layout::Combined {
            self.more(py, connection, true)?;
        } else {
            self.more(py, connection, false)?;
        }
        if self.layout == Layout::Split {
            self.current = self
                .cleanup
                .take()
                .ok_or_else(|| internal(py, "CALL cleanup request owner is missing"))?;
            self.current_streaming = false;
        }
        Ok(())
    }
    fn received_native(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(rows) = self.current.take_call_output() {
            if self.pending_native.is_some() || self.output_saved {
                return Err(internal(py, "CALL returned duplicate native output"));
            }
            self.pending_native = Some(rows);
            self.native_codec = self.live_codec;
            self.output_received = true;
        }
        Ok(())
    }
    fn commit_native(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(rows) = self.pending_native.take() {
            self.output.save(py, rows, self.native_codec)?;
            self.output_saved = true;
        }
        Ok(())
    }
    fn no_unmanaged_output(
        &self,
        py: Python<'_>,
        status: StatusFlags,
        metadata: Option<StatusFlags>,
    ) -> PyResult<()> {
        if self.prepared && self.expected == 0 && out(status, metadata) {
            return Err(errors::client(
                py,
                ClientErrorCode::ParameterCount,
                "The prepared CALL returned native OUT/INOUT values, but parameter_modes declared no outputs; the CALL was not replayed",
            ));
        }
        Ok(())
    }
    fn cleanup_done(&mut self, py: Python<'_>, connection: &Connection) -> PyResult<()> {
        self.more(py, connection, false)?;
        self.cleanup_confirmed = true;
        self.last_synchronized = true;
        self.phase = Phase::Done;
        self.output.finish();
        Ok(())
    }
    fn private_buffered(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        let phase = self.phase;
        if phase == Phase::Cleanup {
            self.cleanup_attempted = true;
        }
        let result = connection.drive_set(
            py,
            lease,
            &mut self.current,
            budget,
            CommandRole::InternalControl,
        );
        let response = self.received(result)?;
        self.track(py, connection, response.completion().status_flags())?;
        match (phase, response) {
            (Phase::Initialize, Response::Command(_)) => self.initialized(py, connection),
            (Phase::Select, Response::Rows(rows)) => {
                self.selected(py, connection, rows.rows, self.live_codec)
            }
            (Phase::Cleanup, Response::Command(_)) => self.cleanup_done(py, connection),
            _ => Err(errors::wire(
                py,
                WireError::Malformed("unexpected private CALL response shape"),
            )),
        }
    }
    fn buffered(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<Option<Response>> {
        loop {
            match self.phase {
                Phase::Done => return Ok(None),
                Phase::Failed => {
                    return Err(internal(py, "Failed CALL cannot be resumed or replayed"));
                }
                Phase::Call => {
                    let response = if self.layout == Layout::Native && self.expected != 0 {
                        let result = connection.drive_managed_set(
                            py,
                            lease,
                            &mut self.current,
                            budget,
                            CommandRole::UserSql,
                        );
                        match self.received(result)? {
                            Managed::Public(response) => response,
                            Managed::InternalOutput { completion, .. } => {
                                self.track(py, connection, completion.status_flags())?;
                                self.received_native(py)?;
                                continue;
                            }
                        }
                    } else {
                        let result = connection.drive_set(
                            py,
                            lease,
                            &mut self.current,
                            budget,
                            CommandRole::UserSql,
                        );
                        self.received(result)?
                    };
                    let metadata = match &response {
                        Response::Rows(rows) => rows.metadata_status,
                        _ => None,
                    };
                    self.no_unmanaged_output(py, response.completion().status_flags(), metadata)?;
                    if matches!(response, Response::Command(_)) {
                        self.called(
                            py,
                            connection,
                            CommandConfirmation::from_response(&response),
                        )?;
                    }
                    self.track(py, connection, response.completion().status_flags())?;
                    if matches!(response, Response::Rows(_)) {
                        self.public_codec = self.live_codec;
                    }
                    return Ok(Some(response));
                }
                _ => self.private_buffered(py, connection, lease, budget)?,
            }
        }
    }
    fn stream(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
        limit: NonZeroUsize,
    ) -> PyResult<Option<StreamEvent>> {
        loop {
            if self.phase == Phase::Done {
                return Ok(None);
            }
            if self.phase == Phase::Failed {
                return Err(internal(py, "Failed CALL cannot be resumed or replayed"));
            }
            if self.phase != Phase::Call && !self.current_streaming {
                self.private_buffered(py, connection, lease, budget)?;
                continue;
            }
            let public = self.phase == Phase::Call;
            if self.phase == Phase::Cleanup {
                self.cleanup_attempted = true;
            }
            let role = if public {
                CommandRole::UserSql
            } else {
                CommandRole::InternalControl
            };
            let event = if self.layout == Layout::Native && self.expected != 0 {
                let result = connection.drive_managed_stream(
                    py,
                    lease,
                    &mut self.current,
                    budget,
                    role,
                    limit,
                );
                match self.received(result)? {
                    Managed::Public(event) => event,
                    Managed::InternalOutput { completion, .. } => {
                        self.track(py, connection, completion.status_flags())?;
                        self.received_native(py)?;
                        continue;
                    }
                }
            } else {
                let result = connection.drive_stream(
                    py,
                    lease,
                    &mut self.current,
                    budget,
                    role,
                    if public { limit } else { NonZeroUsize::MIN },
                );
                self.received(result)?
            };
            if public {
                match &event {
                    StreamEvent::Header(header) => {
                        self.no_unmanaged_output(py, StatusFlags::empty(), header.metadata_status)?;
                        self.public_codec = self.live_codec;
                    }
                    StreamEvent::End {
                        completion,
                        metadata_status,
                        total_rows,
                        ..
                    } => {
                        self.no_unmanaged_output(py, completion.status_flags(), *metadata_status)?;
                        if total_rows.is_none() {
                            self.called(
                                py,
                                connection,
                                CommandConfirmation::from_completion(completion, None),
                            )?;
                        }
                        self.track(py, connection, completion.status_flags())?;
                    }
                    StreamEvent::Rows(_) => (),
                }
                self.boundary = self.phase == Phase::Call;
                return Ok(Some(event));
            }
            match (self.phase, event) {
                (
                    Phase::Initialize,
                    StreamEvent::End {
                        total_rows: None,
                        completion,
                        ..
                    },
                ) => {
                    self.track(py, connection, completion.status_flags())?;
                    self.initialized(py, connection)?;
                }
                (Phase::Select, StreamEvent::Header(_)) => {
                    self.private_header = true;
                    self.select_codec = self.live_codec;
                }
                (Phase::Select, StreamEvent::Rows(rows)) => {
                    if self.private_row.is_some() || rows.row_count != 1 {
                        return Err(errors::wire(
                            py,
                            WireError::Malformed("private CALL SELECT returned more than one row"),
                        ));
                    }
                    self.private_row = Some(rows);
                }
                (
                    Phase::Select,
                    StreamEvent::End {
                        total_rows: Some(1),
                        tail,
                        completion,
                        ..
                    },
                ) => {
                    self.track(py, connection, completion.status_flags())?;
                    if let Some(tail) = tail {
                        if self.private_row.is_some() || tail.row_count != 1 {
                            return Err(errors::wire(
                                py,
                                WireError::Malformed(
                                    "private CALL SELECT returned more than one row",
                                ),
                            ));
                        }
                        self.private_row = Some(tail);
                    }
                    if !self.private_header {
                        return Err(internal(py, "CALL output has no complete metadata header"));
                    }
                    let rows = self.private_row.take().ok_or_else(|| {
                        internal(py, "CALL output SELECT returned no complete row")
                    })?;
                    self.selected(py, connection, rows, self.select_codec)?;
                }
                (
                    Phase::Cleanup,
                    StreamEvent::End {
                        total_rows: None,
                        completion,
                        ..
                    },
                ) => {
                    self.track(py, connection, completion.status_flags())?;
                    self.cleanup_done(py, connection)?;
                }
                _ => {
                    return Err(errors::wire(
                        py,
                        WireError::Malformed("unexpected private CALL streaming response"),
                    ));
                }
            }
        }
    }
    fn clean_once(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
    ) -> PyResult<()> {
        input::check(py, budget)?;
        if self.cleanup_attempted {
            return Err(internal(
                py,
                "CALL cleanup was already attempted and is not replayed",
            ));
        }
        if self.phase != Phase::Cleanup
            || self.layout != Layout::Combined
            || self.current.is_finished()
        {
            if !self.last_synchronized {
                return Err(internal(
                    py,
                    "CALL cleanup cannot start with an unread response",
                ));
            }
            self.current = self
                .cleanup
                .take()
                .ok_or_else(|| internal(py, "CALL cleanup request is unavailable"))?;
            self.current_streaming = false;
        }
        self.phase = Phase::Cleanup;
        if self.current_streaming {
            // This is the existing combined request's final response, not a
            // replacement request. The common parser retains its sequence.
            self.cleanup_attempted = true;
            let result = connection.drive_stream(
                py,
                lease,
                &mut self.current,
                budget,
                CommandRole::InternalControl,
                NonZeroUsize::MIN,
            );
            let event = self.received(result)?;
            match event {
                StreamEvent::End {
                    total_rows: None,
                    completion,
                    ..
                } => {
                    self.track(py, connection, completion.status_flags())?;
                    self.cleanup_done(py, connection)
                }
                _ => Err(errors::wire(
                    py,
                    WireError::Malformed("unexpected CALL cleanup response"),
                )),
            }
        } else {
            self.private_buffered(py, connection, lease, budget)
        }
    }
    fn failed(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        budget: &mut IoBudget,
        primary: PyErr,
    ) -> PyErr {
        let stage = self.phase;
        self.progress();
        // An OUT carrier alone does not prove CALL success. A host failure
        // can still leave its completely received row in the native Query.
        if let Some(rows) = self.current.take_call_output() {
            self.output_received = true;
            self.pending_native = Some(rows);
            self.native_codec = connection
                .native
                .facts
                .lock()
                .ok()
                .and_then(|facts| facts.metadata_codec());
        }
        if self.call_complete && self.last_synchronized {
            if let Err(secondary) = self.commit_native(py) {
                errors::additional(py, &primary, secondary, "retaining confirmed CALL output");
            }
        } else {
            self.pending_native = None;
        }
        let needs_cleanup = self.variables_touched && !self.cleanup_confirmed;
        if needs_cleanup
            && !self.cleanup_attempted
            && !connection.native.is_closed()
            && budget.check().is_ok()
            && (self.last_synchronized
                || self.phase == Phase::Cleanup && self.layout == Layout::Combined)
            && let Err(secondary) = self.clean_once(py, connection, lease, budget)
        {
            errors::additional(py, &primary, secondary, "clearing private CALL slots");
        }
        if needs_cleanup && !self.cleanup_confirmed || !self.last_synchronized {
            connection.native.retire();
        }
        self.output.fail();
        self.phase = Phase::Failed;
        self.boundary = false;
        errors::note_format(
            py,
            &primary,
            format_args!(
                "while {}; CALL sending started={}, CALL completion confirmed={}, complete OUT carrier received={}, output committed={}, private cleanup confirmed={}; no CALL request was replayed; these facts do not imply transaction commit",
                stage.name(),
                self.call_started,
                self.call_complete,
                self.output_received,
                self.output_saved,
                self.cleanup_confirmed
            ),
        );
        primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn call_job_keeps_only_an_opt_in_control_handle_inline() {
        assert!(std::mem::size_of::<Flow>() <= 64);
    }

    use crate::binding::PlanMode;
    use pyo3::types::{PyDict, PyModule, PyString};
    use sqlaero_core::result::decode::Options;
    use sqlaero_core::result::value::{FieldValue, RowEncoding};
    use sqlaero_core::session::query::QueryError;
    use sqlaero_core::transport::deadline::TimeoutKind;
    use std::path::PathBuf;
    use std::time::Duration;

    const LIMIT: usize = 16 * 1024 * 1024;
    fn budget() -> IoBudget {
        IoBudget::new(
            TimeoutKind::Operation,
            Some(Duration::from_secs(5)),
            None,
            None,
        )
    }
    fn connected(py: Python<'_>, label: &str, multi: bool) -> Py<Connection> {
        let root =
            PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").expect("owned fixture root"));
        let state = py
            .import("json")
            .unwrap()
            .call_method1(
                "loads",
                (std::fs::read_to_string(root.join(label).join("state.json")).unwrap(),),
            )
            .unwrap();
        let module = PyModule::new(py, "_call_flow_test_native").unwrap();
        crate::_sqlaero(&module).unwrap();
        let kwargs = PyDict::new(py);
        kwargs.set_item("user", "sqlaero").unwrap();
        kwargs
            .set_item("password", state.get_item("test_password").unwrap())
            .unwrap();
        kwargs.set_item("database", "sqlaero_test").unwrap();
        kwargs
            .set_item(
                "unix_socket",
                root.join(format!("{label}.sock")).to_str().unwrap(),
            )
            .unwrap();
        kwargs.set_item("operation_timeout", 5).unwrap();
        kwargs.set_item("multi_statements", multi).unwrap();
        module
            .getattr("connect")
            .unwrap()
            .call((), Some(&kwargs))
            .unwrap()
            .extract()
            .unwrap()
    }
    fn sql(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        source: &str,
    ) -> Response {
        let mut request = vec![3];
        request.extend_from_slice(source.as_bytes());
        let job = connection.query_job(py, lease, request, false).unwrap();
        connection
            .drive(py, lease, job, &mut budget(), CommandRole::UserSql)
            .map_err(PyErr::from)
            .unwrap()
    }
    fn option_counter(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
    ) -> u64 {
        let Response::Rows(rows) = sql(
            py,
            connection,
            lease,
            "SHOW SESSION STATUS LIKE 'Com_set_option'",
        ) else {
            panic!("option counter")
        };
        std::str::from_utf8(rows.row(0).unwrap()[1].get(&rows.bytes).unwrap().unwrap())
            .unwrap()
            .parse()
            .unwrap()
    }
    fn native_call(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        name: &str,
        prepared: bool,
        budget: &mut IoBudget,
    ) -> (NativeCall, Option<crate::statement::Cached>, String) {
        let prefix = if prepared {
            String::new()
        } else {
            super::super::slot_prefix(py, connection)
                .unwrap()
                .to_owned()
        };
        let proc_name = PyString::new(py, name);
        let args = py.eval(c"(5,None,10)", None, None).unwrap();
        let modes = py.eval(c"('IN','OUT','INOUT')", None, None).unwrap();
        let (context, ansi, _) = connection.context(py).unwrap();
        let mut native = {
            let types = connection.types.borrow(py);
            NativeCall::new(
                &proc_name,
                Some(&args),
                None,
                Some(&modes),
                &prefix,
                if prepared {
                    PlanMode::PreparedNative
                } else {
                    PlanMode::Text
                },
                &types,
                &context,
                ansi,
                budget,
            )
            .unwrap()
        };
        let mut slot = None;
        if prepared {
            let template = native.call.prepared.as_ref().unwrap().clone();
            let id = crate::statement::resolve(py, connection, lease, template, budget, &mut slot)
                .unwrap();
            native.call.bytes[1..5].copy_from_slice(&id.to_le_bytes());
        }
        (native, slot, prefix)
    }
    fn numbers(rows: &RowBatch) -> Vec<Vec<i64>> {
        (0..rows.row_count)
            .map(|index| {
                rows.columns
                    .iter()
                    .zip(rows.row(index).unwrap())
                    .map(|(column, cell)| {
                        let raw = cell.get(&rows.bytes).unwrap().unwrap();
                        if rows.encoding == RowEncoding::Text {
                            std::str::from_utf8(raw).unwrap().parse().unwrap()
                        } else {
                            match sqlaero_core::protocol::binary::value(column, raw).unwrap() {
                                FieldValue::Signed(value) => value,
                                FieldValue::Unsigned(value) => i64::try_from(value).unwrap(),
                                _ => panic!("numeric native field"),
                            }
                        }
                    })
                    .collect()
            })
            .collect()
    }
    fn drain(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        flow: &mut Flow,
        budget: &mut IoBudget,
        streaming: bool,
        discard: bool,
    ) -> PyResult<Vec<Option<usize>>> {
        let mut ends = Vec::new();
        loop {
            if streaming {
                let Some(event) = flow.advance_stream(
                    py,
                    connection,
                    lease,
                    budget,
                    NonZeroUsize::new(4).unwrap(),
                )?
                else {
                    break;
                };
                match event {
                    StreamEvent::Header(_) => {
                        assert!(flow.at_stream_boundary());
                        if discard {
                            flow.discard_current_rows(py)?;
                        }
                    }
                    StreamEvent::Rows(_) => assert!(!discard),
                    StreamEvent::End { total_rows, .. } => ends.push(total_rows),
                }
            } else {
                let Some(response) = flow.advance_buffered(py, connection, lease, budget)? else {
                    break;
                };
                ends.push(match response {
                    Response::Rows(rows) => Some(rows.row_count),
                    Response::Command(_) => None,
                    Response::Counted { .. } => panic!("no counted-only CALL set"),
                });
            }
        }
        Ok(ends)
    }

    #[test]
    #[ignore = "requires C's two owned MySQL fixtures and CP313 embedded test runner"]
    fn real_text_combined_split_and_prepared_preserve_business_output_and_cleanup() {
        Python::initialize();
        Python::attach(|py| {
            for label in ["84", "97"] {
                for prepared in [false, true] {
                    for multi in [false, true] {
                        for streaming in [false, true] {
                            let owner = connected(py, label, multi);
                            let connection = owner.borrow(py);
                            let mut operation = budget();
                            let mut lease = connection.acquire(py, &mut operation).unwrap();
                            sql(
                                py,
                                &connection,
                                &mut lease,
                                "DROP PROCEDURE IF EXISTS c_flow_ok",
                            );
                            sql(
                                py,
                                &connection,
                                &mut lease,
                                "CREATE PROCEDURE c_flow_ok(IN a BIGINT,OUT b BIGINT,INOUT c BIGINT) BEGIN SELECT a AS n WHERE FALSE; SELECT a AS n; SELECT a AS n UNION ALL SELECT a+1 UNION ALL SELECT a+2; SET b=-9223372036854775808; SET c=c+1; END",
                            );
                            let (native, slot, prefix) = native_call(
                                py,
                                &connection,
                                &mut lease,
                                "c_flow_ok",
                                prepared,
                                &mut operation,
                            );
                            let output = Output::new(
                                py,
                                &native.output_positions,
                                Options::default(),
                                Arc::clone(&connection.native.result_pool),
                                LIMIT,
                            )
                            .unwrap();
                            let scope =
                                Scope::new(Arc::clone(&connection.native.result_pool), LIMIT)
                                    .unwrap();
                            let mut flow = Flow::new(
                                py,
                                &connection,
                                &mut lease,
                                native,
                                Arc::clone(&output),
                                Policy {
                                    multi_statements: multi,
                                    streaming,
                                    send_limit: connection.context(py).unwrap().0.limit,
                                },
                                &scope,
                                &operation,
                            )
                            .unwrap();
                            assert_eq!(flow.inner().layout == Layout::Combined, !prepared && multi);
                            assert_eq!(
                                drain(
                                    py,
                                    &connection,
                                    &mut lease,
                                    &mut flow,
                                    &mut operation,
                                    streaming,
                                    streaming
                                )
                                .unwrap(),
                                [Some(0), Some(1), Some(3), None]
                            );
                            assert!(
                                flow.is_finished() && flow.synchronized() && flow.call_completed()
                            );
                            assert!(flow.call_confirmation().is_some());
                            assert!(flow.inner().output_received && output.is_ready());
                            assert_eq!(numbers(output.rows.get().unwrap()), [vec![i64::MIN, 11]]);
                            if !prepared {
                                assert!(flow.inner().cleanup_confirmed);
                                let Response::Rows(rows) = sql(
                                    py,
                                    &connection,
                                    &mut lease,
                                    &format!("SELECT @{prefix}0 IS NULL,@{prefix}1 IS NULL"),
                                ) else {
                                    panic!("slot check")
                                };
                                assert_eq!(numbers(&rows), [vec![1, 1]]);
                            }
                            assert_eq!(
                                flow.metadata_codec().map(OutputCodec::name),
                                Some("utf8mb4")
                            );
                            assert!(!connection.native.is_closed());
                            assert!(flow.take_native_failure().is_none());
                            assert!(!flow.invalidates_statement());
                            eprintln!(
                                "MySQL {label} prepared={prepared} multi={multi} streaming={streaming}: business 0/1/3+tail, exact native outputs, cleanup confirmed"
                            );
                            drop(slot);
                            sql(py, &connection, &mut lease, "DROP PROCEDURE c_flow_ok");
                            drop(lease);
                            drop(connection);
                            owner.bind(py).call_method0("close").unwrap();
                        }
                    }
                }
            }
        });
    }

    #[test]
    #[ignore = "requires C's two owned MySQL fixtures and CP313 embedded test runner"]
    fn real_call_error_cleans_once_and_cleanup_error_preserves_confirmed_values() {
        Python::initialize();
        Python::attach(|py| {
            for label in ["84", "97"] {
                for multi in [false, true] {
                    for cleanup_error in [false, true] {
                        let owner = connected(py, label, multi);
                        let connection = owner.borrow(py);
                        let mut operation = budget();
                        let mut lease = connection.acquire(py, &mut operation).unwrap();
                        sql(
                            py,
                            &connection,
                            &mut lease,
                            "DROP PROCEDURE IF EXISTS c_flow_error",
                        );
                        let body = if cleanup_error {
                            "SET b=99; SET c=c+1;"
                        } else {
                            "SET b=99; SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT='c flow primary';"
                        };
                        sql(
                            py,
                            &connection,
                            &mut lease,
                            &format!(
                                "CREATE PROCEDURE c_flow_error(IN a BIGINT,OUT b BIGINT,INOUT c BIGINT) BEGIN {body} END"
                            ),
                        );
                        let (mut native, _, prefix) = native_call(
                            py,
                            &connection,
                            &mut lease,
                            "c_flow_error",
                            false,
                            &mut operation,
                        );
                        if cleanup_error {
                            native.cleanup = Some(b"\x03THIS IS NOT SQL".to_vec());
                        }
                        let output = Output::new(
                            py,
                            &native.output_positions,
                            Options::default(),
                            Arc::clone(&connection.native.result_pool),
                            LIMIT,
                        )
                        .unwrap();
                        let scope =
                            Scope::new(Arc::clone(&connection.native.result_pool), LIMIT).unwrap();
                        let mut flow = Flow::new(
                            py,
                            &connection,
                            &mut lease,
                            native,
                            Arc::clone(&output),
                            Policy {
                                multi_statements: multi,
                                streaming: false,
                                send_limit: LIMIT,
                            },
                            &scope,
                            &operation,
                        )
                        .unwrap();
                        let error = drain(
                            py,
                            &connection,
                            &mut lease,
                            &mut flow,
                            &mut operation,
                            false,
                            false,
                        )
                        .unwrap_err();
                        assert_eq!(
                            error
                                .value(py)
                                .getattr("code")
                                .unwrap()
                                .extract::<u16>()
                                .unwrap(),
                            if cleanup_error { 1064 } else { 1644 }
                        );
                        assert_eq!(flow.call_completed(), cleanup_error);
                        assert_eq!(output.is_ready(), cleanup_error);
                        assert_eq!(connection.native.is_closed(), cleanup_error);
                        assert!(flow.inner().cleanup_attempted);
                        assert_eq!(flow.inner().cleanup_confirmed, !cleanup_error);
                        assert!(flow.take_native_failure().is_some());
                        if cleanup_error {
                            assert_eq!(numbers(output.rows.get().unwrap()), [vec![99, 11]]);
                        } else {
                            assert!(flow.synchronized());
                            let Response::Rows(rows) = sql(
                                py,
                                &connection,
                                &mut lease,
                                &format!("SELECT @{prefix}0 IS NULL,@{prefix}1 IS NULL"),
                            ) else {
                                panic!("cleaned slots")
                            };
                            assert_eq!(numbers(&rows), [vec![1, 1]]);
                            sql(py, &connection, &mut lease, "DROP PROCEDURE c_flow_error");
                        }
                        drop(lease);
                        drop(connection);
                        owner.bind(py).call_method0("close").unwrap();
                        if cleanup_error {
                            let observer = connected(py, label, false);
                            let connection = observer.borrow(py);
                            let mut operation = budget();
                            let mut lease = connection.acquire(py, &mut operation).unwrap();
                            sql(py, &connection, &mut lease, "DROP PROCEDURE c_flow_error");
                            drop(lease);
                            drop(connection);
                            observer.bind(py).call_method0("close").unwrap();
                        }
                    }
                }
            }
        });
    }

    #[test]
    #[ignore = "requires C's fixture for host state and a controlled native socket peer"]
    fn complete_native_out_with_tail_err_or_truncation_never_readies_output() {
        use sqlaero_core::protocol::framing::{self, Sequence};
        use sqlaero_core::transport::channel::{Channel, Step};
        use sqlaero_core::transport::deadline::Direction;
        use sqlaero_core::transport::socket::Transport;
        use std::io::Write;
        use std::net::TcpListener;
        Python::initialize();
        Python::attach(|py| {
            for streaming in [false, true] {
                for truncated in [false, true] {
                    let owner = connected(py, "84", false);
                    let connection = owner.borrow(py);
                    let mut operation = budget();
                    let mut lease = connection.acquire(py, &mut operation).unwrap();
                    let (context, ansi, _) = connection.context(py).unwrap();
                    let proc_name = PyString::new(py, "c_flow_controlled");
                    let args = py.eval(c"(5,None,10)", None, None).unwrap();
                    let modes = py.eval(c"('IN','OUT','INOUT')", None, None).unwrap();
                    let mut native = NativeCall::new(
                        &proc_name,
                        Some(&args),
                        None,
                        Some(&modes),
                        "",
                        PlanMode::PreparedNative,
                        &connection.types.borrow(py),
                        &context,
                        ansi,
                        &operation,
                    )
                    .unwrap();
                    native.call.bytes[1..5].copy_from_slice(&42_u32.to_le_bytes());
                    let request = native.call.bytes.clone();
                    let output = Output::new(
                        py,
                        &native.output_positions,
                        Options::default(),
                        Arc::clone(&connection.native.result_pool),
                        LIMIT,
                    )
                    .unwrap();
                    let scope =
                        Scope::new(Arc::clone(&connection.native.result_pool), LIMIT).unwrap();
                    let mut flow = Flow::new(
                        py,
                        &connection,
                        &mut lease,
                        native,
                        Arc::clone(&output),
                        Policy {
                            multi_statements: false,
                            streaming,
                            send_limit: LIMIT,
                        },
                        &scope,
                        &operation,
                    )
                    .unwrap();
                    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
                    let address = listener.local_addr().unwrap();
                    let server = std::thread::spawn(move || {
                        let (mut socket, _) = listener.accept().unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut sequence = Sequence::default();
                        assert_eq!(
                            framing::read_message(&mut socket, &mut sequence, LIMIT).unwrap(),
                            request
                        );
                        framing::write_message(&mut socket, &[2], &mut sequence, LIMIT).unwrap();
                        for name in [b"b", b"c"] {
                            let mut column = Vec::new();
                            for label in [b"def".as_slice(), b"", b"", b"", name.as_slice(), b""] {
                                column.push(label.len() as u8);
                                column.extend_from_slice(label);
                            }
                            column.extend_from_slice(&[12, 63, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
                            framing::write_message(&mut socket, &column, &mut sequence, LIMIT)
                                .unwrap();
                        }
                        let mut row = vec![0, 0];
                        row.extend_from_slice(&99_i64.to_le_bytes());
                        row.extend_from_slice(&11_i64.to_le_bytes());
                        framing::write_message(&mut socket, &row, &mut sequence, LIMIT).unwrap();
                        let flags = (StatusFlags::SERVER_STATUS_AUTOCOMMIT
                            | StatusFlags::SERVER_MORE_RESULTS_EXISTS
                            | StatusFlags::SERVER_PS_OUT_PARAMS)
                            .bits()
                            .to_le_bytes();
                        framing::write_message(
                            &mut socket,
                            &[0xfe, 0, 0, flags[0], flags[1], 0, 0, 0],
                            &mut sequence,
                            LIMIT,
                        )
                        .unwrap();
                        if truncated {
                            socket
                                .write_all(&[10, 0, 0, sequence.next(), 0xfe, 0])
                                .unwrap();
                        } else {
                            framing::write_message(
                                &mut socket,
                                b"\xff\x28\x04#42000controlled CALL tail error",
                                &mut sequence,
                                LIMIT,
                            )
                            .unwrap();
                        }
                    });
                    let mut channel = Channel::new(Transport::tcp(address).unwrap(), false);
                    while !channel.connected().unwrap() {
                        channel.wait(Direction::Write, &mut operation).unwrap();
                    }
                    loop {
                        let internal = if streaming {
                            match flow
                                .inner_mut()
                                .current
                                .advance_managed_stream(
                                    &mut channel,
                                    &mut operation,
                                    NonZeroUsize::MIN,
                                )
                                .unwrap()
                            {
                                Step::Ready(Managed::InternalOutput { .. }) => true,
                                Step::Control => false,
                                _ => panic!("carrier remains internal"),
                            }
                        } else {
                            match flow
                                .inner_mut()
                                .current
                                .advance_managed(&mut channel, &mut operation)
                                .unwrap()
                            {
                                Step::Ready(Managed::InternalOutput { .. }) => true,
                                Step::Control => false,
                                _ => panic!("carrier remains internal"),
                            }
                        };
                        if internal {
                            break;
                        }
                    }
                    flow.inner_mut().received_native(py).unwrap();
                    assert!(flow.inner().output_received && flow.inner().pending_native.is_some());
                    assert!(!flow.call_completed());
                    assert!(!output.is_ready() && output.rows.get().is_none());
                    assert!(
                        output.get(py, &connection.types).unwrap().is_none(),
                        "getter before CALL tail"
                    );
                    let error = loop {
                        let result = if streaming {
                            flow.inner_mut()
                                .current
                                .advance_managed_stream(
                                    &mut channel,
                                    &mut operation,
                                    NonZeroUsize::MIN,
                                )
                                .map(|step| match step {
                                    Step::Control => (),
                                    _ => panic!("no successful CALL tail"),
                                })
                        } else {
                            flow.inner_mut()
                                .current
                                .advance_managed(&mut channel, &mut operation)
                                .map(|step| match step {
                                    Step::Control => (),
                                    _ => panic!("no successful CALL tail"),
                                })
                        };
                        if let Err(error) = result {
                            break error;
                        }
                    };
                    assert_eq!(
                        matches!(
                            error,
                            QueryError::Result(sqlaero_core::result::text::ResultError::Server(_))
                        ),
                        !truncated
                    );
                    let error = Arc::new(error);
                    let primary = errors::query_with_codec(py, error.as_ref(), None);
                    flow.inner_mut().last_synchronized = flow.inner().current.received_server_err();
                    flow.inner_mut().failure = Some(Failure { error, codec: None });
                    let _original = flow.inner_mut().failed(
                        py,
                        &connection,
                        &mut lease,
                        &mut operation,
                        primary,
                    );
                    assert!(
                        flow.inner().output_received,
                        "actual carrier observation is retained"
                    );
                    assert!(
                        flow.inner().pending_native.is_none(),
                        "unconfirmed row is discarded"
                    );
                    assert!(!flow.call_completed() && flow.call_confirmation().is_none());
                    assert!(!output.is_ready() && output.rows.get().is_none());
                    assert!(
                        output.get(py, &connection.types).unwrap().is_none(),
                        "getter after failed/unconfirmed CALL"
                    );
                    server.join().unwrap();
                    drop(lease);
                    drop(connection);
                    owner.bind(py).call_method0("close").unwrap();
                }
            }
        });
    }

    #[test]
    #[ignore = "requires C's two owned MySQL fixtures"]
    fn capacity_preflight_splits_without_toggling_multi_statements() {
        Python::initialize();
        Python::attach(|py| {
            for label in ["84", "97"] {
                for streaming in [false, true] {
                    let owner = connected(py, label, true);
                    let connection = owner.borrow(py);
                    let mut operation = budget();
                    let mut lease = connection.acquire(py, &mut operation).unwrap();
                    sql(
                        py,
                        &connection,
                        &mut lease,
                        "DROP PROCEDURE IF EXISTS c_flow_capacity",
                    );
                    sql(
                        py,
                        &connection,
                        &mut lease,
                        "CREATE PROCEDURE c_flow_capacity(IN a BIGINT,OUT b BIGINT,INOUT c BIGINT) BEGIN SET b=a; SET c=c+1; END",
                    );
                    let options_before = option_counter(py, &connection, &mut lease);
                    let (native, _, _) = native_call(
                        py,
                        &connection,
                        &mut lease,
                        "c_flow_capacity",
                        false,
                        &mut operation,
                    );
                    let limit = [
                        native.initialize.as_ref().unwrap().len(),
                        native.call.bytes.len(),
                        native.select.as_ref().unwrap().len(),
                        native.cleanup.as_ref().unwrap().len(),
                    ]
                    .into_iter()
                    .max()
                    .unwrap();
                    assert!(native.combined(py, limit, &operation).unwrap().is_none());
                    let output = Output::new(
                        py,
                        &native.output_positions,
                        Options::default(),
                        Arc::clone(&connection.native.result_pool),
                        LIMIT,
                    )
                    .unwrap();
                    let scope =
                        Scope::new(Arc::clone(&connection.native.result_pool), LIMIT).unwrap();
                    let mut flow = Flow::new(
                        py,
                        &connection,
                        &mut lease,
                        native,
                        Arc::clone(&output),
                        Policy {
                            multi_statements: true,
                            streaming,
                            send_limit: limit,
                        },
                        &scope,
                        &operation,
                    )
                    .unwrap();
                    assert!(flow.inner().layout == Layout::Split);
                    assert_eq!(
                        drain(
                            py,
                            &connection,
                            &mut lease,
                            &mut flow,
                            &mut operation,
                            streaming,
                            false
                        )
                        .unwrap(),
                        [None]
                    );
                    assert_eq!(numbers(output.rows.get().unwrap()), [vec![5, 11]]);
                    assert_eq!(
                        connection.native.facts.lock().unwrap().multi_statements,
                        Some(true)
                    );
                    // Both private SQL SET statements share this server counter
                    // with wire COM_SET_OPTION. The exact +2 excludes a hidden
                    // capability toggle in addition to init and cleanup.
                    assert_eq!(
                        option_counter(py, &connection, &mut lease),
                        options_before + 2
                    );
                    sql(
                        py,
                        &connection,
                        &mut lease,
                        "DROP PROCEDURE c_flow_capacity",
                    );
                    drop(lease);
                    drop(connection);
                    owner.bind(py).call_method0("close").unwrap();
                }
            }
        });
    }

    #[test]
    #[ignore = "requires C's two owned MySQL fixtures"]
    fn private_codec_changes_do_not_reinterpret_the_public_header() {
        Python::initialize();
        Python::attach(|py| {
            for label in ["84", "97"] {
                for multi in [false, true] {
                    for streaming in [false, true] {
                        let owner = connected(py, label, multi);
                        let connection = owner.borrow(py);
                        let mut operation = budget();
                        let mut lease = connection.acquire(py, &mut operation).unwrap();
                        sql(
                            py,
                            &connection,
                            &mut lease,
                            "DROP PROCEDURE IF EXISTS c_flow_codec",
                        );
                        sql(
                            py,
                            &connection,
                            &mut lease,
                            "CREATE PROCEDURE c_flow_codec(OUT b TEXT) BEGIN SELECT 'é' AS `é`; SET SESSION character_set_results='latin1'; SET b='ö'; END",
                        );
                        let prefix = super::super::slot_prefix(py, &connection).unwrap();
                        let name = PyString::new(py, "c_flow_codec");
                        let args = py.eval(c"(None,)", None, None).unwrap();
                        let modes = py.eval(c"('OUT',)", None, None).unwrap();
                        let (context, ansi, _) = connection.context(py).unwrap();
                        let native = NativeCall::new(
                            &name,
                            Some(&args),
                            None,
                            Some(&modes),
                            prefix,
                            PlanMode::Text,
                            &connection.types.borrow(py),
                            &context,
                            ansi,
                            &operation,
                        )
                        .unwrap();
                        let output = Output::new(
                            py,
                            &native.output_positions,
                            Options::default(),
                            Arc::clone(&connection.native.result_pool),
                            LIMIT,
                        )
                        .unwrap();
                        let scope =
                            Scope::new(Arc::clone(&connection.native.result_pool), LIMIT).unwrap();
                        let mut flow = Flow::new(
                            py,
                            &connection,
                            &mut lease,
                            native,
                            Arc::clone(&output),
                            Policy {
                                multi_statements: multi,
                                streaming,
                                send_limit: LIMIT,
                            },
                            &scope,
                            &operation,
                        )
                        .unwrap();
                        assert_eq!(
                            drain(
                                py,
                                &connection,
                                &mut lease,
                                &mut flow,
                                &mut operation,
                                streaming,
                                false
                            )
                            .unwrap(),
                            [Some(1), None]
                        );
                        assert_eq!(flow.metadata_codec().unwrap().name(), "utf8mb4");
                        assert_eq!(output.codec.get().unwrap().name(), "latin1");
                        let rows = output.rows.get().unwrap();
                        assert_eq!(
                            rows.row(0).unwrap()[0].get(&rows.bytes).unwrap().unwrap(),
                            b"\xf6"
                        );
                        assert!(output.is_ready());
                        sql(py, &connection, &mut lease, "DROP PROCEDURE c_flow_codec");
                        drop(lease);
                        drop(connection);
                        owner.bind(py).call_method0("close").unwrap();
                    }
                }
            }
        });
    }
}
