//! Monotonic total deadlines and progress-based network waiting limits.

use std::time::{Duration, Instant};

/// Maximum native readiness wait before returning to the binding boundary.
pub const CONTROL_INTERVAL: Duration = Duration::from_millis(100);

/// Direction of an actual socket readiness wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Waiting for network input.
    Read,
    /// Waiting for network output capacity or connection completion.
    Write,
}

/// The applicable timeout that expired, without implying server cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TimeoutKind {
    /// The connection's original establishment budget expired.
    #[error("connection establishment deadline expired")]
    Connect,
    /// One operation's original total budget expired.
    #[error("operation deadline expired")]
    Operation,
    /// No network input progress occurred within the allowed wait interval.
    #[error("network read timed out without progress")]
    Read,
    /// No network output progress occurred within the allowed wait interval.
    #[error("network write timed out without progress")]
    Write,
}

/// One immutable timeout snapshot shared by every batch and internal request
/// in an operation. Readiness events alone never reset a progress interval.
#[derive(Debug)]
pub struct IoBudget {
    started: Instant,
    total: Option<Duration>,
    total_kind: TimeoutKind,
    read: Option<Duration>,
    write: Option<Duration>,
    read_wait: Option<Instant>,
    write_wait: Option<Instant>,
}

impl IoBudget {
    /// Start an establishment or operation budget at the calling boundary,
    /// before DNS, SQL binding or other work covered by that deadline.
    #[must_use]
    pub fn new(
        total_kind: TimeoutKind,
        total: Option<Duration>,
        read: Option<Duration>,
        write: Option<Duration>,
    ) -> Self {
        Self::starting_at(Instant::now(), total_kind, total, read, write)
    }

    /// Retain the public call's original monotonic start while its typed
    /// configuration is being prepared. No stage receives a new total window.
    #[must_use]
    pub fn starting_at(
        started: Instant,
        total_kind: TimeoutKind,
        total: Option<Duration>,
        read: Option<Duration>,
        write: Option<Duration>,
    ) -> Self {
        Self {
            started,
            total,
            total_kind,
            read,
            write,
            read_wait: None,
            write_wait: None,
        }
    }

    /// Check the fixed total deadline without starting a network wait.
    ///
    /// # Errors
    /// Returns the original connection/operation timeout on expiry.
    pub fn check(&self) -> Result<(), TimeoutKind> {
        self.total_remaining(Instant::now()).map(|_| ())
    }

    /// Check an existing directional wait without starting or resetting one.
    ///
    /// # Errors
    /// Returns an expired total or active no-progress timeout before new I/O.
    pub fn check_io(&self, direction: Direction) -> Result<(), TimeoutKind> {
        let now = Instant::now();
        self.total_remaining(now)?;
        let (started, maximum, kind) = match direction {
            Direction::Read => (self.read_wait, self.read, TimeoutKind::Read),
            Direction::Write => (self.write_wait, self.write, TimeoutKind::Write),
        };
        if let (Some(started), Some(maximum)) = (started, maximum)
            && now.saturating_duration_since(started) >= maximum
        {
            return Err(kind);
        }
        Ok(())
    }

    fn total_remaining(&self, now: Instant) -> Result<Option<Duration>, TimeoutKind> {
        self.total
            .map(|total| {
                total
                    .checked_sub(now.saturating_duration_since(self.started))
                    .filter(|left| !left.is_zero())
                    .ok_or(self.total_kind)
            })
            .transpose()
    }

    /// Record an actual positive byte transfer, not a readiness notification.
    pub fn progress(&mut self, direction: Direction) {
        match direction {
            Direction::Read => self.read_wait = None,
            Direction::Write => self.write_wait = None,
        }
    }

    /// Bound a readiness wait by the total budget, continuous no-progress
    /// interval and the control-check cadence. Repeated waits retain their
    /// original start unless bytes were transferred.
    ///
    /// # Errors
    /// Returns the applicable total or network-wait timeout on expiry.
    pub fn wait_duration(&mut self, direction: Direction) -> Result<Duration, TimeoutKind> {
        self.wait_at(direction, Instant::now())
    }

    fn wait_at(&mut self, direction: Direction, now: Instant) -> Result<Duration, TimeoutKind> {
        let mut duration = self
            .total_remaining(now)?
            .unwrap_or(CONTROL_INTERVAL)
            .min(CONTROL_INTERVAL);
        let (started, maximum, kind) = match direction {
            Direction::Read => (&mut self.read_wait, self.read, TimeoutKind::Read),
            Direction::Write => (&mut self.write_wait, self.write, TimeoutKind::Write),
        };
        let started = *started.get_or_insert(now);
        if let Some(maximum) = maximum {
            let left = maximum
                .checked_sub(now.saturating_duration_since(started))
                .filter(|left| !left.is_zero())
                .ok_or(kind)?;
            duration = duration.min(left);
        }
        Ok(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_or_interruption_does_not_reset_the_wait() {
        let mut budget = IoBudget::new(
            TimeoutKind::Operation,
            None,
            Some(Duration::from_secs(1)),
            None,
        );
        let start = budget.started;
        budget.wait_at(Direction::Read, start).unwrap();
        assert_eq!(
            budget.wait_at(Direction::Read, start + Duration::from_secs(1)),
            Err(TimeoutKind::Read)
        );
        budget.progress(Direction::Read);
        assert!(
            budget
                .wait_at(Direction::Read, start + Duration::from_secs(1))
                .is_ok()
        );
    }

    #[test]
    fn progress_never_extends_total_deadline_and_unlimited_still_yields_control() {
        let mut budget = IoBudget::new(
            TimeoutKind::Connect,
            Some(Duration::from_secs(2)),
            None,
            None,
        );
        let start = budget.started;
        budget.progress(Direction::Write);
        assert_eq!(
            budget.wait_at(Direction::Write, start + Duration::from_secs(2)),
            Err(TimeoutKind::Connect)
        );
        let mut unlimited = IoBudget::new(TimeoutKind::Operation, None, None, None);
        assert_eq!(
            unlimited.wait_duration(Direction::Read).unwrap(),
            CONTROL_INTERVAL
        );
    }
}
