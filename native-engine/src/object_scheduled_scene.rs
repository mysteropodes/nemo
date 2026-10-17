//! Live scheduling admission above the scheduler/evaluation boundary.
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::{
    prepare_object_composition, ObjectCompositionWitness, ObjectPaintPlan, ObjectSceneError,
    SceneResourceMismatchKind,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::{FrameScheduler, ScheduledFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledObjectSceneError {
    Inactive,
    Pins(ObjectSceneError),
    RetainedSourceMismatch,
    UnsupportedOutput,
    ResourceDeclaration(SceneResourceMismatchKind),
    ForeignResult,
}

/// Retained identity and plan only; this value alone grants no consumption.
#[derive(Debug, Clone)]
pub(crate) struct ScheduledObjectComposition {
    work: ScheduledFrame,
    identity: ScheduledFrameIdentity,
    composition: ObjectCompositionWitness,
}
impl ScheduledObjectComposition {
    pub(crate) fn identity(&self) -> &ScheduledFrameIdentity {
        &self.identity
    }
    pub(crate) fn composition(&self) -> &ObjectCompositionWitness {
        &self.composition
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.work.key().output_spec().dimensions()
    }
}

/// Borrow the real scheduler throughout each compose/readback/byte-access call.
/// An owned result may outlive this permit, but consuming it needs a fresh one.
pub struct LiveObjectComposition<'a> {
    scheduler: &'a FrameScheduler,
    prepared: ScheduledObjectComposition,
}
impl LiveObjectComposition<'_> {
    /// Diagnostic identity only; it cannot replace this live scheduler borrow.
    pub fn identity(&self) -> &ScheduledFrameIdentity {
        self.prepared.identity()
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.prepared.dimensions()
    }
    pub(crate) fn prepared(&self) -> &ScheduledObjectComposition {
        &self.prepared
    }
    pub(crate) fn validate_result(
        &self,
        result: &ScheduledObjectComposition,
    ) -> Result<(), ScheduledObjectSceneError> {
        if !self.scheduler.is_current(&self.prepared.work) {
            return Err(ScheduledObjectSceneError::Inactive);
        }
        if !self.prepared.work.same_work_as(&result.work)
            || !self
                .prepared
                .composition
                .same_identity_as(&result.composition)
        {
            return Err(ScheduledObjectSceneError::ForeignResult);
        }
        Ok(())
    }
}

pub fn prepare_scheduled_object_composition<'a>(
    scheduler: &'a FrameScheduler,
    scheduled: &ScheduledFrame,
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
) -> Result<LiveObjectComposition<'a>, ScheduledObjectSceneError> {
    prepare_with(scheduler, scheduled, || {
        prepare_object_composition(plan, expected).map_err(ScheduledObjectSceneError::Pins)
    })
}

/// Consume only a render-scene-created witness through the original live work.
/// The witness retains its validated source; identity strings cannot create it.
pub fn prepare_scheduled_object_composition_from_witness<'a>(
    scheduler: &'a FrameScheduler,
    scheduled: &ScheduledFrame,
    witness: &ObjectCompositionWitness,
) -> Result<LiveObjectComposition<'a>, ScheduledObjectSceneError> {
    prepare_with(scheduler, scheduled, || Ok(witness.clone()))
}

fn prepare_with<'a>(
    scheduler: &'a FrameScheduler,
    scheduled: &ScheduledFrame,
    witness: impl FnOnce() -> Result<ObjectCompositionWitness, ScheduledObjectSceneError>,
) -> Result<LiveObjectComposition<'a>, ScheduledObjectSceneError> {
    if !scheduler.is_current(scheduled) {
        return Err(ScheduledObjectSceneError::Inactive);
    }
    let snapshot = scheduler
        .pinned_object_snapshot(scheduled.work_id())
        .ok_or(ScheduledObjectSceneError::RetainedSourceMismatch)?;
    let composition = witness()?;
    let key = scheduled.key();
    // A constructible paint plan already requires Authored scene-root numeric
    // order, as does object scheduling. No lower-layer scope import is needed.
    if composition.instance_id() != snapshot.instance_id()
        || composition.document_id() != snapshot.document_id()
        || composition.content_revision() != snapshot.content_revision()
        || composition.document_snapshot_id() != snapshot.snapshot_id()
        || composition.document_snapshot_id() != key.document_snapshot_id()
        || composition.context_id() != key.context_id()
        || composition.frame() != key.frame()
    {
        return Err(ScheduledObjectSceneError::RetainedSourceMismatch);
    }
    let output = key.output_spec();
    if key.quality() != "final"
        || output.kind() != "frame"
        || output.format() != "rgba8"
        || output.color_interpretation() != "srgb"
        || output.alpha_mode() != "straight"
    {
        return Err(ScheduledObjectSceneError::UnsupportedOutput);
    }
    // The witness's declared set must equal the key's declaration exactly (an
    // absent set is empty); nothing is acquired or released here.
    composition
        .verify_scene_resources(key.declared_resource_versions())
        .map_err(|mismatch| ScheduledObjectSceneError::ResourceDeclaration(mismatch.kind))?;
    // D2 alone checks device/target/padded-readback budgets before allocation.
    Ok(LiveObjectComposition {
        scheduler,
        prepared: ScheduledObjectComposition {
            work: scheduled.clone(),
            identity: ScheduledFrameIdentity::from_scheduled(scheduled),
            composition,
        },
    })
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scheduled_scene_witness.rs"]
mod witness_tests;
