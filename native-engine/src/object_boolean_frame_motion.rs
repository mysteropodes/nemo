//! Snapshot-pinned query adapter; document geometry belongs to evaluation.
use crate::revision::ObjectSnapshot;

#[path = "object_boolean_motion_eval.rs"]
mod evaluator;

pub(super) fn from_snapshot(
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> impl evaluator::ElementMotionSource + '_ {
    evaluator::SnapshotMotion { snapshot, frame }
}
