//! Persistent native response ownership between public streaming calls.
//!
//! The shared Query parses every packet. This owner holds no Python input,
//! callback or lock guard, and releases its wire token only at real command end.

use crate::{
    connection::{CommandRole, Connection},
    errors,
    reader::StreamRows,
};
use pyo3::prelude::*;
use sqlaero_core::{
    charset::output::OutputCodec,
    error::ClientErrorCode,
    result::{
        decode::Options,
        text::{CommandConfirmation, Completion, StreamEvent},
    },
    session::{
        query::{ConfirmedPrefix, Query},
        shared::{CommandLease, OwnedCommand},
    },
    transport::deadline::IoBudget,
};
use std::{num::NonZeroUsize, ops::Range, time::Instant};

/// An actual event and, only for a header, its independently confirmed label
/// codec. A later terminator cannot rewrite that header's saved representation.
pub(crate) struct Event {
    pub(crate) value: StreamEvent,
    pub(crate) codec: Option<OutputCodec>,
}

pub(crate) struct Stream {
    owner: Option<OwnedCommand>,
    job: Query,
    pub(crate) budget: IoBudget,
    returned: Option<Instant>,
    pub(crate) rows: Option<StreamRows>,
    pub(crate) pending: Option<Event>,
    pub(crate) options: Options,
    pub(crate) request_index: usize,
    pub(crate) set_index: usize,
    source_rows: Range<usize>,
    prefix: ConfirmedPrefix,
    base_count: u128,
    first: Option<CommandConfirmation>,
    pub(crate) batch: Option<u128>,
    pub(crate) failed: bool,
    pub(crate) statement_invalidated: bool,
}

impl Stream {
    #[expect(
        clippy::too_many_arguments,
        reason = "one final request retains the existing operation's concrete owners"
    )]
    pub(crate) fn start(
        py: Python<'_>,
        connection: &Connection,
        lease: &mut CommandLease<'_>,
        job: Query,
        budget: IoBudget,
        options: Options,
        source_rows: Range<usize>,
        prefix: ConfirmedPrefix,
        base_count: u128,
    ) -> PyResult<Self> {
        let owner = connection
            .native
            .retain_command(lease)
            .map_err(|error| errors::access(py, error))?;
        Ok(Self {
            owner: Some(owner),
            job,
            budget,
            returned: None,
            rows: None,
            pending: None,
            options,
            request_index: prefix.commands,
            set_index: 0,
            source_rows,
            prefix,
            base_count,
            first: None,
            batch: None,
            failed: false,
            statement_invalidated: false,
        })
    }

    /// Charge all library work while excluding exactly the measured interval
    /// after the previous public return. Configuration is never refreshed here.
    pub(crate) fn enter(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(returned) = self.returned.take() {
            self.budget
                .exclude_idle(returned.elapsed())
                .map_err(|error| errors::timeout(py, error))?;
        }
        Ok(())
    }
    pub(crate) fn returned(&mut self) {
        self.returned = Some(Instant::now());
    }
    pub(crate) fn finished(&self) -> bool {
        self.job.is_finished()
    }
    pub(crate) fn discard_current(&mut self, py: Python<'_>) -> PyResult<()> {
        self.job
            .discard_current_rows()
            .map_err(|error| errors::query(py, error))
    }
    pub(crate) fn abort(&mut self, connection: &Connection) {
        self.failed = true;
        connection.native.retire();
        self.owner.take();
    }

    /// Reserve the next accumulation slot before Query starts another message.
    /// This local failure leaves all cached rows and the synchronized message
    /// boundary intact for a smaller subsequent fetch to consume and release.
    pub(crate) fn reserve(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(rows) = &mut self.rows {
            rows.reserve_batch()
                .map_err(|error| crate::output::memory(py, error))?;
        }
        Ok(())
    }

    pub(crate) fn advance(
        &mut self,
        py: Python<'_>,
        connection: &Connection,
        count: usize,
    ) -> PyResult<Event> {
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }
        let limit = NonZeroUsize::new(count.max(1)).ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "stream row boundary is zero",
            )
        })?;
        let owner = self.owner.as_ref().ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::SessionState,
                "The streaming command already ended; no later response is available",
            )
        })?;
        let mut lease = owner.resume().map_err(|error| errors::access(py, error))?;
        let result = connection.drive_stream(
            py,
            &mut lease,
            &mut self.job,
            &mut self.budget,
            CommandRole::UserSql,
            limit,
        );
        let synchronized = result
            .as_ref()
            .map_or_else(|failure| failure.synchronized(), |_| self.job.is_finished());
        let released = if synchronized && !connection.native.is_closed() {
            lease
                .finish_owned(owner)
                .map_err(|error| errors::access(py, error))
        } else {
            Ok(())
        };
        drop(lease);
        if synchronized || connection.native.is_closed() {
            self.owner.take();
        }
        let event = match result {
            Ok(event) => {
                released?;
                event
            }
            Err(failure) => {
                self.failed = true;
                self.statement_invalidated |= failure
                    .native_failure()
                    .is_some_and(|failure| crate::statement::invalidates(&failure.error));
                let error: PyErr = failure.into();
                errors::note_format(
                    py,
                    &error,
                    format_args!(
                        "while receiving streaming request {}, result set {}; source rows {}..{}; no request was replayed",
                        self.request_index,
                        self.set_index,
                        self.source_rows.start,
                        self.source_rows.end
                    ),
                );
                errors::note(py, &error, self.prefix.diagnostic());
                return Err(error);
            }
        };
        if let StreamEvent::End {
            completion,
            total_rows,
            ..
        } = &event
        {
            self.completed(py, completion, *total_rows)?;
        }
        let codec = if matches!(event, StreamEvent::Header(_)) {
            connection
                .native
                .facts
                .lock()
                .map_err(|_| {
                    errors::client(
                        py,
                        ClientErrorCode::InternalState,
                        "stream metadata facts are unavailable",
                    )
                })?
                .metadata_codec()
        } else {
            None
        };
        Ok(Event {
            value: event,
            codec,
        })
    }

    /// Account only actual set terminators. Each concrete request contributes
    /// its first set once, irrespective of later CALL or multi-SQL results.
    pub(crate) fn completed(
        &mut self,
        py: Python<'_>,
        completion: &Completion,
        rows: Option<usize>,
    ) -> PyResult<()> {
        if self.first.is_none() {
            self.first = Some(CommandConfirmation::from_completion(completion, rows));
            let count = rows.map_or_else(
                || u128::from(completion.affected_rows()),
                |rows| rows as u128,
            );
            self.batch = Some(self.base_count.checked_add(count).ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "batch first-result row count overflow",
                )
            })?);
        }
        if self.job.is_finished() {
            let first = self.first.ok_or_else(|| {
                errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "stream command ended without a first result",
                )
            })?;
            self.prefix
                .record_confirmation(self.source_rows.clone(), first)
                .map_err(|error| errors::wire(py, error))?;
        }
        self.set_index = self.set_index.checked_add(1).ok_or_else(|| {
            errors::client(
                py,
                ClientErrorCode::InternalState,
                "stream result index overflow",
            )
        })?;
        Ok(())
    }
}
