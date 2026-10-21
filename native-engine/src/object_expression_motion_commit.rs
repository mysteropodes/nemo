//! Original evaluated-Motion effects, finalized only by the trusted owner callback.
use crate::commands::ObjectFillOwner;
use crate::evaluation::property_expression_runtime::Diagnostic;
use crate::history::NativeObjectHistory;
use crate::object_document::{FrameScopeKind, PropertyHolderTarget};
use crate::object_expression_runtime::motion_frame::{verify, MotionEffect};
use crate::object_frame_packet::ObjectFrameSelector;
use crate::revision::ObjectSnapshot;

#[path = "object_expression_motion_diagnostics.rs"]
mod diagnostics;
use crate::object_expression_runtime::motion_frame::commit::diagnostics::{
    expression_code, publish_committed_timeouts, publish_records, PublishedDiagnostic,
};
#[path = "object_expression_motion_replay.rs"]
mod replay;
pub use crate::object_expression_runtime::motion_frame::commit::replay::{
    prepare_object_motion_exposure_frame_for_control,
    prepare_object_motion_fallback_frame_for_control,
};

#[derive(Debug)]
struct RecordedFrame {
    origin: ObjectSnapshot,
    source: ObjectSnapshot,
    context: String,
    scope: FrameScopeKind,
    frame: u32,
    effects: Vec<MotionEffect>,
    candidate: bool,
    reason: String,
}
impl RecordedFrame {
    fn matches(&self, snapshot: &ObjectSnapshot, selector: &ObjectFrameSelector<'_>) -> bool {
        same_source(&self.source, snapshot)
            && self.context == selector.context_id
            && self.scope == selector.scope_kind
            && self.frame == selector.frame
    }
    fn non_clean(&self) -> bool {
        self.effects.iter().any(|effect| match effect {
            MotionEffect::Evaluated { result, .. } => {
                result.diagnostic.is_some()
                    || result.disable.is_some()
                    || result
                        .dependencies
                        .iter()
                        .any(|effect| effect.diagnostic.is_some() || effect.disable)
            }
            MotionEffect::Refused { .. } => true,
        })
    }
}

/// One non-Clone original owner holds this whole state, including parked tabs.
/// Pending effects are never erased by a new frame, stale request or retry.
#[derive(Debug, Default)]
pub struct ObjectMotionFrameControl {
    pending: Vec<RecordedFrame>,
    ready: Vec<RecordedFrame>,
    diagnostics: Vec<PublishedDiagnostic>,
    committed_without_source: Option<ObjectMotionEffectsCommit>,
}
impl ObjectMotionFrameControl {
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn pending_frames(&self) -> Vec<u32> {
        self.pending.iter().map(|record| record.frame).collect()
    }
    pub fn requires_revision(&self) -> bool {
        self.committed_without_source.is_none()
            && self.pending.iter().any(|record| {
                record.effects.iter().any(|effect| match effect {
                    MotionEffect::Evaluated { result, .. } => {
                        result.disable.is_some()
                            || result.dependencies.iter().any(|effect| effect.disable)
                    }
                    MotionEffect::Refused { dependencies, .. } => {
                        dependencies.iter().any(|effect| effect.disable)
                    }
                })
            })
    }
    /// Rust test/debug projection only; production queries use typed accessors.
    pub fn diagnostics(&self) -> String {
        format!("{:?}", self.diagnostics)
    }
}

#[derive(Debug, Clone)]
pub struct ObjectMotionEffectsCommit {
    before_revision: u64,
    content_revision: u64,
    document_snapshot_id: String,
    frame_ready: bool,
}
impl ObjectMotionEffectsCommit {
    pub fn before_revision(&self) -> u64 {
        self.before_revision
    }
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub fn document_snapshot_id(&self) -> &str {
        &self.document_snapshot_id
    }
    pub fn frame_ready(&self) -> bool {
        self.frame_ready
    }
    pub fn changed(&self) -> bool {
        self.before_revision != self.content_revision
    }
}

fn same_source(a: &ObjectSnapshot, b: &ObjectSnapshot) -> bool {
    a.instance_id() == b.instance_id()
        && a.document_id() == b.document_id()
        && a.content_revision() == b.content_revision()
        && a.snapshot_id() == b.snapshot_id()
        && std::ptr::eq(a.document(), b.document())
}
fn selector_matches(snapshot: &ObjectSnapshot, selector: &ObjectFrameSelector<'_>) -> bool {
    selector.instance_id == snapshot.instance_id()
        && selector.document_id == snapshot.document_id()
        && selector.content_revision == snapshot.content_revision()
        && selector.document_snapshot_id == snapshot.snapshot_id()
}
fn batch_limit() -> usize {
    #[cfg(feature = "export_job")]
    {
        crate::export_job::MAX_OBJECT_EXPORT_FRAMES
    }
    #[cfg(not(feature = "export_job"))]
    {
        1
    }
}

fn add_disable(
    targets: &mut Vec<(PropertyHolderTarget, String)>,
    target: &PropertyHolderTarget,
    property: &str,
) {
    if !targets
        .iter()
        .any(|entry| &entry.0 == target && entry.1 == property)
    {
        targets.push((target.clone(), property.to_owned()));
    }
}
fn disable_targets(
    records: &[RecordedFrame],
) -> Result<Vec<(PropertyHolderTarget, String)>, String> {
    let mut targets = Vec::new();
    for record in records {
        for effect in &record.effects {
            let dependencies = match effect {
                MotionEffect::Evaluated {
                    target,
                    property,
                    result,
                } => {
                    if expression_code(&record.origin, target, property).is_none() {
                        return Err(
                            "Motion effect has no exact original expression target/code".into()
                        );
                    }
                    if let Some(disable) = &result.disable {
                        if disable.content_revision != record.origin.content_revision()
                            || &disable.target != target
                            || &disable.property != property
                            || result.diagnostic != Some(Diagnostic::Timeout)
                        {
                            return Err(
                                "Motion timeout receipt target/revision is inconsistent".into()
                            );
                        }
                        add_disable(&mut targets, target, property);
                    }
                    &result.dependencies
                }
                MotionEffect::Refused { dependencies, .. } => dependencies,
            };
            for dependency in dependencies {
                if expression_code(&record.origin, &dependency.target, &dependency.property)
                    .is_none()
                {
                    return Err(
                        "Motion dependency has no exact original expression target/code".into(),
                    );
                }
                if dependency.disable {
                    if dependency.diagnostic != Some(Diagnostic::Timeout) {
                        return Err("Motion dependency disable has no original timeout".into());
                    }
                    add_disable(&mut targets, &dependency.target, &dependency.property);
                }
            }
        }
    }
    Ok(targets)
}

fn validate_frames(
    owner: &NativeObjectHistory,
    current: &ObjectSnapshot,
    expected: &ObjectFrameSelector<'_>,
    frames: &[u32],
    records: &[RecordedFrame],
) -> Result<(), String> {
    for record in records {
        verify(owner, &record.origin)
            .map_err(|error| format!("Motion finalization origin: {error:?}"))?;
        if !same_source(&record.source, current)
            || record.context != expected.context_id
            || record.scope != expected.scope_kind
            || !frames.contains(&record.frame)
        {
            return Err(
                "Motion finalization did not visit this exact request frame/context".into(),
            );
        }
    }
    if frames
        .iter()
        .any(|frame| !records.iter().any(|record| record.frame == *frame))
    {
        return Err("Motion finalization cannot bless an unvisited request frame".into());
    }
    Ok(())
}

fn replay_records(
    owner: &NativeObjectHistory,
    fresh: &ObjectSnapshot,
    records: &mut [RecordedFrame],
) {
    for record in records {
        if record.candidate {
            let selector = ObjectFrameSelector {
                instance_id: fresh.instance_id(),
                document_id: fresh.document_id(),
                content_revision: fresh.content_revision(),
                document_snapshot_id: fresh.snapshot_id(),
                context_id: &record.context,
                scope_kind: record.scope.clone(),
                frame: record.frame,
            };
            if let Err(error) = replay::replay(owner, fresh, &selector, record) {
                record.candidate = false;
                record.reason = error;
            }
        }
    }
}

/// The trusted callback runs only after the read/revision guards have returned.
/// The host preflights subscriber/pending state before calling this mutation.
pub fn finalize_object_motion_frame_effects_for_owner(
    owner: &mut NativeObjectHistory,
    expected: &ObjectFrameSelector<'_>,
    frames: &[u32],
    control: &mut ObjectMotionFrameControl,
) -> Result<ObjectMotionEffectsCommit, String> {
    let current = owner
        .acquire_snapshot(owner.content_revision())
        .ok_or("Motion owner snapshot unavailable")?;
    if !selector_matches(&current, expected)
        || frames.is_empty()
        || frames.len() > batch_limit()
        || frames[0] != expected.frame
        || frames.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(
            "Motion finalization owner/selector/request frames are stale or malformed".into(),
        );
    }
    if let Some(committed) = &control.committed_without_source {
        if committed.content_revision != current.content_revision()
            || control.pending.iter().any(|record| {
                record.origin.instance_id() != current.instance_id()
                    || record.origin.document_id() != current.document_id()
            })
        {
            return Err("Motion committed receipt belongs to a different owner".into());
        }
        return Ok(ObjectMotionEffectsCommit {
            before_revision: committed.content_revision,
            content_revision: committed.content_revision,
            document_snapshot_id: committed.document_snapshot_id.clone(),
            frame_ready: false,
        });
    }
    if control.pending.is_empty() {
        return Ok(ObjectMotionEffectsCommit {
            before_revision: current.content_revision(),
            content_revision: current.content_revision(),
            document_snapshot_id: current.snapshot_id().to_owned(),
            frame_ready: false,
        });
    }
    validate_frames(owner, &current, expected, frames, &control.pending)?;
    let targets = disable_targets(&control.pending)?;
    let (candidate, expected_document) =
        ObjectFillOwner::prepare_expression_disables(current.clone(), &targets)
            .map_err(|error| format!("Motion atomic disable preparation: {error:?}"))?;
    let before_revision = current.content_revision();
    let content_revision = owner
        .commit_property_edit(candidate)
        .map_err(|error| format!("Motion atomic disable commit: {error:?}"))?;
    let fresh = owner.acquire_snapshot(content_revision);
    let mut committed = ObjectMotionEffectsCommit {
        before_revision,
        content_revision,
        document_snapshot_id: fresh
            .as_ref()
            .map_or_else(String::new, |snapshot| snapshot.snapshot_id().to_owned()),
        frame_ready: false,
    };
    let Some(fresh) = fresh else {
        // A publication must reach the event/ACK barrier even if replay is lost.
        control.committed_without_source = Some(committed.clone());
        return Ok(committed);
    };
    let source_matches = match (current.project(), fresh.project()) {
        (None, None) => fresh.document() == &expected_document,
        (Some(original), Some(updated)) => updated == &original.with_drawing(expected_document),
        _ => false,
    };
    if fresh.instance_id() != current.instance_id()
        || fresh.document_id() != current.document_id()
        || !source_matches
        || (!committed.changed() && !same_source(&current, &fresh))
    {
        // Keep every old receipt; never relabel values onto an unknown source.
        control.committed_without_source = Some(committed.clone());
        return Ok(committed);
    }
    let mut ready = std::mem::take(&mut control.pending);
    if committed.changed() {
        // An enabled-flag change invalidates successful results from other
        // frames and outer expressions that read the disabled property. The
        // flattened effects do not prove their retained numbers valid on R2.
        publish_committed_timeouts(&mut control.diagnostics, &ready, &fresh);
        control.ready.clear();
        // Do not cache refused old frames either: the next actual owner read
        // must be free to evaluate the current source. No guest runs here.
        return Ok(committed);
    }
    replay_records(owner, &fresh, &mut ready);
    publish_records(&mut control.diagnostics, &ready);
    committed.frame_ready = frames.iter().all(|frame| {
        ready
            .iter()
            .any(|record| record.frame == *frame && record.candidate)
    });
    control.ready = ready;
    Ok(committed)
}
