//! Explicit retry scheduling, independent of Python, connections and SQL replay.

use std::time::Duration;

/// Immutable validated timing/count policy; matching belongs to the boundary.
#[derive(Clone, Copy)]
pub struct Policy {
    maximum: Option<u64>,
    initial: f64,
    multiplier: f64,
    cap: f64,
}
impl Policy {
    /// Validate native timing/count inputs once, not on every attempt.
    ///
    /// # Errors
    /// Returns the invalid parameter name, without normalizing NaN or infinity.
    pub fn new(
        maximum: Option<u64>,
        initial: f64,
        multiplier: f64,
        cap: f64,
    ) -> Result<Self, &'static str> {
        if maximum == Some(0) {
            return Err("max_attempts");
        }
        for (name, value) in [("wait_interval", initial), ("max_wait_interval", cap)] {
            if !value.is_finite() || value < 0.0 || Duration::try_from_secs_f64(value).is_err() {
                return Err(name);
            }
        }
        if !multiplier.is_finite() || multiplier < 1.0 {
            return Err("wait_multiplier");
        }
        Ok(Self {
            maximum,
            initial: initial.min(cap),
            multiplier,
            cap,
        })
    }
    /// Start a fresh independent traversal; configurations do not share progress.
    #[must_use]
    pub fn run(self) -> Run {
        Run {
            policy: self,
            attempts: 0,
            base: self.initial,
            active: false,
            done: false,
        }
    }
}

/// One traversal's small native state; never stores an exception or history.
pub struct Run {
    policy: Policy,
    attempts: u64,
    base: f64,
    active: bool,
    done: bool,
}
impl Run {
    /// Reserve the next attempt and return its pre-attempt delay. The first is
    /// immediate. Saturating counting permits genuinely unbounded traversals.
    ///
    /// # Errors
    /// Returns a misuse error if the prior attempt has not completed.
    pub fn begin(&mut self) -> Result<Option<f64>, &'static str> {
        if self.active {
            return Err("the previous RetryAttempt has not exited");
        }
        if self.done || !self.can_continue() {
            return Ok(None);
        }
        let delay = if self.attempts == 0 {
            0.0
        } else {
            let current = self.base;
            // Capping infinity from an otherwise valid multiplication is safe;
            // jitter never feeds back into this deterministic base sequence.
            self.base = (self.base * self.policy.multiplier).min(self.policy.cap);
            current
        };
        self.attempts = self.attempts.saturating_add(1);
        self.active = true;
        Ok(Some(delay))
    }
    /// Complete one actual attempt; only an explicit matching failure can
    /// request continuation. Success, exhaustion and nonmatches end the run.
    ///
    /// # Errors
    /// Returns misuse when no active attempt owns the completion.
    pub fn finish(&mut self, matched_failure: bool) -> Result<bool, &'static str> {
        if !self.active {
            return Err("no active RetryAttempt can exit");
        }
        self.active = false;
        let retry = matched_failure && self.can_continue();
        self.done = !retry;
        Ok(retry)
    }
    /// Abandon an interrupted wait without retaining a partly granted attempt.
    pub fn cancel(&mut self) {
        self.active = false;
        self.done = true;
    }
    fn can_continue(&self) -> bool {
        self.policy
            .maximum
            .is_none_or(|maximum| self.attempts < maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_infinite_and_success_have_independent_exact_attempt_counts() {
        let policy = Policy::new(Some(3), 0.0, 1.0, 10.0).unwrap();
        let mut first = policy.run();
        let mut second = policy.run();
        for index in 0..3 {
            assert_eq!(first.begin(), Ok(Some(0.0)));
            assert_eq!(first.finish(true), Ok(index < 2));
        }
        assert_eq!(first.begin(), Ok(None));
        assert_eq!(second.begin(), Ok(Some(0.0)));
        assert!(!second.finish(false).unwrap());
        assert_eq!(second.begin(), Ok(None));
        let mut unlimited = Policy::new(None, 0.0, 1.0, 0.0).unwrap().run();
        for _ in 0..100 {
            unlimited.begin().unwrap().unwrap();
            assert!(unlimited.finish(true).unwrap());
        }
        unlimited.begin().unwrap().unwrap();
        assert!(!unlimited.finish(false).unwrap());
    }
    #[test]
    fn capped_exponential_delays_include_first_retry_and_handle_overflow() {
        let mut run = Policy::new(None, 1.0, 2.0, 10.0).unwrap().run();
        for delay in [0.0, 1.0, 2.0, 4.0, 8.0, 10.0, 10.0] {
            assert_eq!(run.begin().unwrap(), Some(delay));
            assert!(run.finish(true).unwrap());
        }
        let mut capped = Policy::new(None, 20.0, 1.0, 2.0).unwrap().run();
        assert_eq!(capped.begin().unwrap(), Some(0.0));
        capped.finish(true).unwrap();
        assert_eq!(capped.begin().unwrap(), Some(2.0));
        let mut huge = Policy::new(None, 1.0, f64::MAX, 10.0).unwrap().run();
        for delay in [0.0, 1.0, 10.0, 10.0] {
            assert_eq!(huge.begin().unwrap(), Some(delay));
            huge.finish(true).unwrap();
        }
        for invalid in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
            assert!(Policy::new(None, invalid, 1.0, 10.0).is_err());
        }
    }
}
