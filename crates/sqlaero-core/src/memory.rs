//! Allocation-bound native result quotas, shared without locking the wire.

use std::collections::TryReserveError;
use std::mem::size_of;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A real allocation or configured capacity failure, never an RSS estimate.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// This result and its still-owned metadata exceeded their own limit.
    #[error("native result requires {required} bytes, exceeding its {limit}-byte memory limit")]
    Result {
        /// Simultaneously managed native capacity required by this operation.
        required: usize,
        /// Configured per-result limit.
        limit: usize,
    },
    /// Other live results still own part of the connection's capacity.
    #[error(
        "connection native results require {required} bytes, exceeding the {limit}-byte total result memory limit"
    )]
    Total {
        /// Total simultaneous native capacity requested.
        required: usize,
        /// Configured per-connection limit.
        limit: usize,
    },
    /// The native allocator rejected the checked reservation.
    #[error("cannot allocate native result storage: {0}")]
    Allocation(#[from] TryReserveError),
}

/// One connection's aggregate native ownership. No result registry, background
/// worker or connection-command mutex is involved in local allocation/release.
pub struct Pool {
    limit: usize,
    used: AtomicUsize,
}
impl Pool {
    /// Create the connection's fixed aggregate bound.
    #[must_use]
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: AtomicUsize::new(0),
        })
    }
    /// Read actual currently charged capacity, including live growth buffers.
    #[must_use]
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }
    fn acquire(&self, bytes: usize) -> Result<(), MemoryError> {
        acquire(&self.used, bytes, self.limit).map_err(|required| MemoryError::Total {
            required,
            limit: self.limit,
        })
    }
    fn release(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// A result's quota identity. Metadata and row storage share it, so exported
/// column snapshots keep only their own allocations alive after rows are gone.
pub struct Scope {
    pool: Arc<Pool>,
    limit: usize,
    used: AtomicUsize,
}
impl Scope {
    /// Reserve this scope's own native allocation before creating it.
    ///
    /// # Errors
    /// Returns the real per-result or connection bound that cannot be met.
    pub fn new(pool: Arc<Pool>, limit: usize) -> Result<Arc<Self>, MemoryError> {
        let own = size_of::<Self>() + 2 * size_of::<usize>();
        if own > limit {
            return Err(MemoryError::Result {
                required: own,
                limit,
            });
        }
        pool.acquire(own)?;
        Ok(Arc::new(Self {
            pool,
            limit,
            used: AtomicUsize::new(own),
        }))
    }
    /// Currently owned capacity for this result, not merely its payload length.
    #[must_use]
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }
    /// Charge a concrete allocation or fixed native owner.
    ///
    /// # Errors
    /// Returns a per-result/aggregate limit; a failed charge changes neither.
    pub fn charge(self: &Arc<Self>, bytes: usize) -> Result<Reservation, MemoryError> {
        self.acquire(bytes)?;
        Ok(Reservation {
            scope: Arc::clone(self),
            bytes,
        })
    }
    fn acquire(&self, bytes: usize) -> Result<(), MemoryError> {
        acquire(&self.used, bytes, self.limit).map_err(|required| MemoryError::Result {
            required,
            limit: self.limit,
        })?;
        if let Err(error) = self.pool.acquire(bytes) {
            self.used.fetch_sub(bytes, Ordering::Relaxed);
            return Err(error);
        }
        Ok(())
    }
    fn release(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
        self.pool.release(bytes);
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.pool
            .release(size_of::<Self>() + 2 * size_of::<usize>());
    }
}

fn acquire(counter: &AtomicUsize, bytes: usize, limit: usize) -> Result<(), usize> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            old.checked_add(bytes).filter(|new| *new <= limit)
        })
        .map(|_| ())
        .map_err(|old| old.saturating_add(bytes))
}

/// Capacity credit owned for exactly as long as the corresponding allocation.
pub struct Reservation {
    scope: Arc<Scope>,
    bytes: usize,
}
impl Reservation {
    /// Borrow the same quota identity for a necessary dependent allocation.
    #[must_use]
    pub fn scope(&self) -> &Arc<Scope> {
        &self.scope
    }
    fn resize(&mut self, bytes: usize) -> Result<(), MemoryError> {
        if bytes > self.bytes {
            self.scope.acquire(bytes - self.bytes)?;
        } else {
            self.scope.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.scope.release(self.bytes);
    }
}

/// An owned Vec whose actual capacity is charged. Growth uses one explicitly
/// allocated replacement, so old plus new capacities are both covered during
/// the real transient. The old allocation is freed before its credit returns.
pub struct Buffer<T> {
    // Drop order matters: release allocation before returning its quota.
    values: Vec<T>,
    charge: Option<Reservation>,
}
impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            charge: None,
        }
    }
}
impl<T> Buffer<T> {
    /// Untracked storage for protocol stages before result collection exists.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Empty result-owned storage; its first actual allocation is charged.
    #[must_use]
    pub fn tracked(scope: &Arc<Scope>) -> Self {
        Self {
            values: Vec::new(),
            charge: Some(Reservation {
                scope: Arc::clone(scope),
                bytes: 0,
            }),
        }
    }
    /// Actual owned capacity, including unused reserved slots.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }
    /// Borrow the initialized values without exposing unchecked Vec mutation.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
    /// Release initialized values while keeping the charged reusable capacity.
    pub fn clear(&mut self) {
        self.values.clear();
    }
    /// Shorten initialized data; this does not release allocated capacity.
    pub fn truncate(&mut self, length: usize) {
        self.values.truncate(length);
    }
    /// Borrow initialized data for bounded protocol reads.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.values
    }

    /// Reserve actual additional slots, with geometric growth when affordable.
    ///
    /// # Errors
    /// Returns the actual quota or allocation failure, preserving old values.
    pub fn reserve(&mut self, additional: usize) -> Result<(), MemoryError> {
        let required = self
            .values
            .len()
            .checked_add(additional)
            .ok_or(MemoryError::Result {
                required: usize::MAX,
                limit: usize::MAX,
            })?;
        if required <= self.values.capacity() {
            return Ok(());
        }
        let Some(charge) = &self.charge else {
            self.values.try_reserve(additional)?;
            return Ok(());
        };
        let exact = required
            .checked_mul(size_of::<T>())
            .ok_or(MemoryError::Result {
                required: usize::MAX,
                limit: charge.scope.limit,
            })?;
        let target = required.max(self.values.capacity().saturating_mul(2).max(8));
        let bytes = target.saturating_mul(size_of::<T>());
        let (capacity, mut pending) = match charge.scope.charge(bytes) {
            Ok(pending) => (target, pending),
            Err(_) if target != required => (required, charge.scope.charge(exact)?),
            Err(error) => return Err(error),
        };
        let mut replacement = Vec::new();
        replacement.try_reserve_exact(capacity)?;
        // Vec may report more slots than requested. Use the actual capacity,
        // never assume the allocator's requested size was the returned size.
        pending.resize(replacement.capacity().checked_mul(size_of::<T>()).ok_or(
            MemoryError::Result {
                required: usize::MAX,
                limit: charge.scope.limit,
            },
        )?)?;
        replacement.append(&mut self.values);
        drop(std::mem::replace(&mut self.values, replacement));
        self.charge = Some(pending);
        Ok(())
    }
    /// Append one value after the same capacity check used by bulk writes.
    ///
    /// # Errors
    /// Returns the actual capacity or allocation failure.
    pub fn push(&mut self, value: T) -> Result<(), MemoryError> {
        self.reserve(1)?;
        self.values.push(value);
        Ok(())
    }
    /// Insert at a previously checked sorted-cache position.
    ///
    /// # Errors
    /// Returns the actual capacity or allocation failure.
    pub fn insert(&mut self, index: usize, value: T) -> Result<(), MemoryError> {
        self.reserve(1)?;
        self.values.insert(index, value);
        Ok(())
    }
}
impl<T: Clone> Buffer<T> {
    /// Append initialized values, charging a growth only at its real boundary.
    ///
    /// # Errors
    /// Returns the actual capacity or allocation failure.
    pub fn extend_from_slice(&mut self, values: &[T]) -> Result<(), MemoryError> {
        self.reserve(values.len())?;
        self.values.extend_from_slice(values);
        Ok(())
    }
    /// Initialize additional slots for an immediately following bounded read.
    ///
    /// # Errors
    /// Returns the actual capacity or allocation failure.
    pub fn resize(&mut self, length: usize, value: T) -> Result<(), MemoryError> {
        self.reserve(length.saturating_sub(self.values.len()))?;
        self.values.resize(length, value);
        Ok(())
    }
}
impl<T> Deref for Buffer<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for Buffer<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(formatter)
    }
}
impl<T: PartialEq> PartialEq<Vec<T>> for Buffer<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.values == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_and_transient_growth_are_real_and_release_is_owned() {
        let pool = Pool::new(10_000);
        let scope = Scope::new(Arc::clone(&pool), 10_000).unwrap();
        let own = scope.used();
        let mut bytes = Buffer::tracked(&scope);
        bytes.extend_from_slice(&[1_u8; 9]).unwrap();
        assert_eq!(scope.used(), own + bytes.capacity());
        bytes.clear();
        assert_eq!(scope.used(), own + bytes.capacity());
        bytes.extend_from_slice(&[2_u8; 40]).unwrap();
        assert_eq!(scope.used(), own + bytes.capacity());
        drop(scope);
        assert_ne!(pool.used(), 0);
        drop(bytes);
        assert_eq!(pool.used(), 0);
    }
    #[test]
    fn an_old_result_can_block_growth_without_losing_either_result() {
        let pool = Pool::new(1024);
        let first = Scope::new(Arc::clone(&pool), 1024).unwrap();
        let second = Scope::new(Arc::clone(&pool), 1024).unwrap();
        let mut a = Buffer::tracked(&first);
        a.extend_from_slice(&[7_u8; 400]).unwrap();
        let mut b = Buffer::tracked(&second);
        assert!(b.extend_from_slice(&[8_u8; 600]).is_err());
        assert_eq!(a.as_slice(), &[7_u8; 400]);
        assert!(b.is_empty());
        drop(a);
        b.extend_from_slice(&[8_u8; 600]).unwrap();
        assert_eq!(b.len(), 600);
    }
    #[test]
    fn final_size_fitting_does_not_hide_a_real_transient_overflow() {
        let own = size_of::<Scope>() + 2 * size_of::<usize>();
        let pool = Pool::new(own + 70);
        let scope = Scope::new(Arc::clone(&pool), own + 70).unwrap();
        let mut buffer = Buffer::tracked(&scope);
        buffer.extend_from_slice(&[1_u8; 32]).unwrap();
        assert!(buffer.extend_from_slice(&[2_u8; 16]).is_err());
        assert_eq!(buffer.as_slice(), &[1_u8; 32]);
        assert_eq!(scope.used(), own + buffer.capacity());
    }
}
