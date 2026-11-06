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
    last_confirmation: Option<CommandConfirmation>,
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
            last_confirmation: None,
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
    pub(crate) fn output_received(&self) -> bool {
        self.inner().output_received
    }
    pub(crate) fn cleanup_confirmed(&self) -> bool {
        self.inner().cleanup_confirmed
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
    /// Only actual native whole-command facts; never a hidden set's completion.
    pub(crate) fn take_confirmation(&mut self) -> Option<CommandConfirmation> {
        let state = self.inner_mut();
        state
            .last_confirmation
            .take()
            .or_else(|| state.current.take_confirmation())
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
                        .or_else(|| failure.confirmation())
                {
                    self.call_complete = true;
                    self.call_confirmation = Some(confirmation);
                }
                if let Some(confirmation) = failure.confirmation() {
                    self.last_confirmation = Some(confirmation);
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
}
