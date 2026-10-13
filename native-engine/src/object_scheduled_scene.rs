//! Live scheduling admission above the scheduler/evaluation boundary.
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::{
    prepare_object_composition, ObjectCompositionWitness, ObjectPaintPlan, ObjectSceneError,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::{FrameScheduler, ScheduledFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduledObjectSceneError {
    Inactive,
    Pins(ObjectSceneError),
    RetainedSourceMismatch,
    UnsupportedOutput,
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
pub(crate) struct LiveObjectComposition<'a> {
    scheduler: &'a FrameScheduler,
    prepared: ScheduledObjectComposition,
}
impl LiveObjectComposition<'_> {
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

pub(crate) fn prepare_scheduled_object_composition<'a>(
    scheduler: &'a FrameScheduler,
    scheduled: &ScheduledFrame,
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
) -> Result<LiveObjectComposition<'a>, ScheduledObjectSceneError> {
    if !scheduler.is_current(scheduled) {
        return Err(ScheduledObjectSceneError::Inactive);
    }
    let snapshot = scheduler
        .pinned_object_snapshot(scheduled.work_id())
        .ok_or(ScheduledObjectSceneError::RetainedSourceMismatch)?;
    let composition =
        prepare_object_composition(plan, expected).map_err(ScheduledObjectSceneError::Pins)?;
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
        || !key.declared_resource_versions().is_empty()
    {
        return Err(ScheduledObjectSceneError::UnsupportedOutput);
    }
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
