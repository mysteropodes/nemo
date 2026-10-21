//! Pure frame reconstruction and original owner-control read collection.
use crate::evaluation::object_component_kernels::object_context_admission::admit_frame_context;
use crate::history::NativeObjectHistory;
use crate::object_expression_runtime::motion_frame::commit::{
    batch_limit, same_source, selector_matches, ObjectMotionFrameControl, RecordedFrame,
};
use crate::object_expression_runtime::motion_frame::{motion_limits, verify, EvaluatedMotion};
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

pub(super) fn replay(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    record: &RecordedFrame,
) -> Result<ObjectFramePacket, String> {
    verify(owner, snapshot).map_err(|error| format!("Motion replay owner: {error:?}"))?;
    if !selector_matches(snapshot, selector)
        || !record.matches(snapshot, selector)
        || !record.candidate
    {
        return Err(format!(
            "Motion replay source/frame unavailable: {}",
            record.reason
        ));
    }
    let mut motion = EvaluatedMotion::new(
        owner,
        snapshot,
        selector.context_id,
        f64::from(selector.frame),
        motion_limits()?,
    );
    motion.replay = Some(&record.effects);
    let packet =
        crate::object_frame_packet::duplicators::prepare_with_motion(snapshot, selector, &motion)
            .map_err(|error| format!("Motion replay frame: {error:?}"))?;
    if motion.replay_cursor.get() != record.effects.len() {
        return Err("Motion replay did not consume the exact retained numeric sequence".into());
    }
    verify(owner, snapshot).map_err(|error| format!("Motion replay retirement: {error:?}"))?;
    Ok(packet)
}

/// Pure existing-source fallback, for the host's already-admitted pending-proof
/// preview only. No guest, revision, diagnostic publication or receipt draining.
pub fn prepare_object_motion_fallback_frame_for_control(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    control: &ObjectMotionFrameControl,
) -> Result<ObjectFramePacket, String> {
    let record = control
        .pending
        .iter()
        .chain(&control.ready)
        .find(|record| record.matches(snapshot, selector))
        .ok_or("Motion fallback frame was not visited by this retained read")?;
    replay(owner, snapshot, selector, record)
}

/// The actual host prepares every original request frame before staging output.
/// Clean values can paint; non-clean candidates wait for the explicit callback.
pub fn prepare_object_motion_exposure_frame_for_control(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    control: &mut ObjectMotionFrameControl,
) -> Result<ObjectFramePacket, String> {
    verify(owner, snapshot).map_err(|error| format!("Motion control owner: {error:?}"))?;
    if !selector_matches(snapshot, selector) {
        return Err("Motion control selector does not name the exact owner snapshot".into());
    }
    // Empty captures have no effects to finalize. Retire only their obsolete
    // source cache after exact live-owner admission; every real effect
    // (including successful diagnostic clears) remains pending even if stale.
    if control.committed_without_source.is_none() && admit_frame_context(snapshot, selector).is_ok()
    {
        control.pending.retain(|record| {
            !record.effects.is_empty()
                || record.source.instance_id() != snapshot.instance_id()
                || record.source.document_id() != snapshot.document_id()
                || record.context != selector.context_id
                || record.scope != selector.scope_kind
                || same_source(&record.source, snapshot)
        });
    }
    // Only finalized projections can retire here. Pending effects stay intact.
    control.retire_finalized_diagnostics(snapshot);
    if let Some(record) = control
        .pending
        .iter()
        .find(|record| record.matches(snapshot, selector))
    {
        return if record.non_clean() {
            Err(record.reason.clone())
        } else {
            replay(owner, snapshot, selector, record)
        };
    }
    if let Some(record) = control
        .ready
        .iter()
        .find(|record| record.matches(snapshot, selector))
    {
        return replay(owner, snapshot, selector, record);
    }
    if control.pending.iter().any(|record| {
        !same_source(&record.source, snapshot)
            || record.context != selector.context_id
            || record.scope != selector.scope_kind
    }) {
        return Err(
            "Motion control retains unresolved effects from a different source/context".into(),
        );
    }
    if control.pending.len() >= batch_limit() {
        return Err("Motion control exceeds the existing original frame-batch bound".into());
    }
    // These receipts already committed. Starting a different read may retire
    // their numeric cache; its transient diagnostic ledger remains at the owner.
    if control.pending.is_empty() {
        control.ready.clear();
    }
    let mut motion = EvaluatedMotion::new(
        owner,
        snapshot,
        selector.context_id,
        f64::from(selector.frame),
        motion_limits()?,
    );
    motion.capture_all = true;
    let prepared =
        crate::object_frame_packet::duplicators::prepare_with_motion(snapshot, selector, &motion);
    let current = verify(owner, snapshot);
    let refusal = motion.refusal.into_inner();
    let effects = motion.effects.into_inner();
    let candidate = prepared.is_ok() && current.is_ok() && refusal.is_none();
    let reason = format!(
        "Motion effects await owner finalization: frame {}; refusal={refusal:?}; frame={:?}; owner={current:?}; effects={effects:?}",
        selector.frame, prepared.as_ref().err(),
    );
    if effects.is_empty() && !candidate {
        current.map_err(|error| format!("Motion control retirement: {error:?}"))?;
        return prepared.map_err(|error| format!("Motion control frame: {error:?}"));
    }
    let record = RecordedFrame {
        origin: snapshot.clone(),
        source: snapshot.clone(),
        context: selector.context_id.to_owned(),
        scope: selector.scope_kind.clone(),
        frame: selector.frame,
        effects,
        candidate,
        reason,
    };
    let refused = !candidate || record.non_clean();
    let message = record.reason.clone();
    control.pending.push(record);
    if refused {
        Err(message)
    } else {
        prepared.map_err(|error| format!("Motion control frame: {error:?}"))
    }
}
