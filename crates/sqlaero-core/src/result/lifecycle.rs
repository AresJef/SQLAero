//! Linearizable handoff between local result operations and forced retirement.

use std::sync::atomic::{AtomicU8, Ordering};

const ACTIVE: u8 = 1;
const RETIRED: u8 = 2;

/// One result owner's operation state; it contains no Python objects or I/O.
///
/// Retirement and operation exit update the same atomic word. Whichever
/// observes the other event claims cleanup, so neither can lose the handoff.
/// The caller releases retained rows outside its state lock; in-flight owned
/// references remain valid until their ordinary owners release them.
#[derive(Default)]
pub struct Lifecycle(AtomicU8);

impl Lifecycle {
    /// Acquire the local operation slot, including for an idempotent close.
    /// A successful caller must pair this with `finish`, even after failure.
    /// Retired owners still reject new work through their ordinary open check.
    #[must_use]
    pub fn try_begin(&self) -> bool {
        self.0.fetch_or(ACTIVE, Ordering::Acquire) & ACTIVE == 0
    }

    /// Release the previously acquired operation slot and claim row cleanup
    /// when retirement already happened. Call exactly once per acquired slot.
    #[must_use]
    pub fn finish(&self) -> bool {
        self.0.fetch_and(!ACTIVE, Ordering::AcqRel) & RETIRED != 0
    }

    /// Permanently retire the owner and claim cleanup if no operation is live.
    /// Cleanup is idempotent: repeated calls may claim an already empty owner.
    #[must_use]
    pub fn retire(&self) -> bool {
        self.0.fetch_or(RETIRED, Ordering::AcqRel) & ACTIVE == 0
    }

    /// Read the permanent closed state without acquiring the operation slot.
    #[must_use]
    pub fn is_retired(&self) -> bool {
        self.0.load(Ordering::Acquire) & RETIRED != 0
    }
}

#[cfg(test)]
mod tests {
    use super::Lifecycle;
    use crate::memory::{Pool, Reservation, Scope};
    use std::sync::{Arc, Barrier, Mutex};

    fn release(rows: &Mutex<Option<Arc<Reservation>>>, claim: bool) {
        if claim {
            let released = rows.lock().unwrap().take();
            drop(released);
        }
    }

    #[test]
    fn either_order_hands_off_cleanup_without_a_second_operation() {
        for retire_first in [false, true] {
            let state = Lifecycle::default();
            assert!(state.try_begin());
            assert!(!state.try_begin(), "the active owner remains exclusive");
            if retire_first {
                assert!(!state.retire());
                assert!(state.finish());
            } else {
                assert!(!state.finish());
                assert!(state.retire());
            }
            assert!(state.is_retired());
            assert!(state.try_begin(), "idempotent close can still own the slot");
            assert!(state.is_retired(), "acquisition cannot reopen the owner");
            assert!(state.finish());
        }
    }

    #[test]
    fn real_quota_is_returned_once_while_inflight_ownership_stays_valid() {
        for _ in 0..64 {
            let pool = Pool::new(4096);
            let scope = Scope::new(Arc::clone(&pool), 4096).unwrap();
            let metadata = pool.used();
            let data = Arc::new(scope.charge(512).unwrap());
            let inflight = Arc::clone(&data);
            let retained = Mutex::new(Some(data));
            let state = Lifecycle::default();
            assert!(state.try_begin());
            let start = Barrier::new(2);
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    start.wait();
                    release(&retained, state.retire());
                });
                start.wait();
                release(&retained, state.finish());
            });
            assert!(retained.lock().unwrap().is_none());
            assert_eq!(
                pool.used(),
                metadata + 512,
                "the in-flight Arc still owns its storage"
            );
            assert!(Arc::ptr_eq(inflight.scope(), &scope));
            drop(inflight);
            assert_eq!(
                pool.used(),
                metadata,
                "row capacity returned without losing metadata"
            );
            release(&retained, state.retire());
            release(&retained, state.retire());
            assert_eq!(
                pool.used(),
                metadata,
                "repeated cleanup does not release twice"
            );
            drop(scope);
            assert_eq!(pool.used(), 0);
        }
    }
}
