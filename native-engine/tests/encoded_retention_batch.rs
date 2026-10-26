use super::*;
use crate::revision::object_revision::encoded_retention_budget::EncodedRetentionBudget;

#[test]
fn tween_retention_batch_second_ticket_failure_releases_first_without_commit() {
    let a = EncodedRetentionBudget::new(64).unwrap();
    let b = EncodedRetentionBudget::new(64).unwrap();
    let retained = Arc::new(vec![0u8; 4]);
    a.admit(&retained).unwrap().commit().unwrap();
    let before = a.used().unwrap();
    let first = Arc::new(vec![1u8; 8]);
    let second = Arc::new(vec![2u8; 8]);
    let tickets = vec![a.admit(&first).unwrap(), b.admit(&second).unwrap()];
    assert_eq!(
        Admission::commit_all(tickets),
        Err(RetentionError::Unavailable)
    );
    // Keep both inputs alive: a partial commit would still charge the first.
    assert_eq!(a.used(), Ok(before));
    assert_eq!(b.used(), Ok(0));
}

#[test]
fn tween_retention_batch_deduplicates_identity_and_preserves_pending_adopters() {
    let budget = EncodedRetentionBudget::new(64).unwrap();
    let blob = Arc::new(vec![0u8; 8]);
    let pending = budget.admit(&blob).unwrap();
    Admission::commit_all(vec![
        budget.admit(&blob).unwrap(),
        budget.admit(&blob).unwrap(),
    ])
    .unwrap();
    drop(pending);
    assert_eq!(budget.used(), Ok(8));
    drop(blob);
    assert_eq!(budget.used(), Ok(0));
}
