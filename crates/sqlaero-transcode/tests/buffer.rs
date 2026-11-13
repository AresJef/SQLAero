//! Checked byte writes preserve existing content when capacity is unavailable.

use sqlaero_transcode::buffer::{CapacityError, append, reserve};

#[test]
fn append_accepts_the_exact_logical_bound() {
    let mut output = b"old:".to_vec();
    append(&mut output, b"new", 7).unwrap();
    assert_eq!(output, b"old:new");
}

#[test]
fn append_rejects_excess_length_without_modifying_content() {
    let mut output = b"old:".to_vec();
    let error = append(&mut output, b"new", 6).unwrap_err();
    assert!(matches!(
        error,
        CapacityError::Limit {
            required: 7,
            limit: 6
        }
    ));
    assert_eq!(output, b"old:");
}

#[test]
fn logical_limit_is_checked_even_when_existing_capacity_is_larger() {
    let mut output = Vec::with_capacity(32);
    output.extend_from_slice(b"old:");
    let error = append(&mut output, b"new", 6).unwrap_err();
    assert!(matches!(
        error,
        CapacityError::Limit {
            required: 7,
            limit: 6
        }
    ));
    assert_eq!(output, b"old:");
}

#[test]
fn empty_append_is_valid_at_the_bound() {
    let mut output = b"old:".to_vec();
    append(&mut output, b"", 4).unwrap();
    assert_eq!(output, b"old:");
}

#[test]
fn checked_length_overflow_is_reported_before_allocation() {
    let mut output = vec![7];
    let error = reserve(&mut output, usize::MAX, usize::MAX).unwrap_err();
    assert!(matches!(
        error,
        CapacityError::Limit {
            required: usize::MAX,
            limit: usize::MAX
        }
    ));
    assert_eq!(output, [7]);
}

#[test]
fn allocator_capacity_failure_retains_its_native_cause() {
    let mut output = Vec::new();
    let error = reserve(&mut output, usize::MAX, usize::MAX).unwrap_err();
    assert!(matches!(&error, CapacityError::Allocation(_)));
    assert!(std::error::Error::source(&error).is_some());
    assert!(output.is_empty());
}
