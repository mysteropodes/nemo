//! Resource binding of the L08-K join: the producer-owned set meets the render-scene witness (G48).
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    ObjectSceneResourceSet, SceneResourceMismatch,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::SceneEffectPlan;
use crate::object_render_scene::ObjectCompositionWitness;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindResourcesError {
    /// The witness was prepared from another pin than the plan.
    WrongWitness,
    Resources(SceneResourceMismatch),
}

/// Attach the producer-owned resource set to the render-scene witness once it equals the scene's declaration
/// (the same `(id, version)` map the scheduler key is built from). A mismatch returns the first differing id and
/// attaches nothing; the set (and its retention tokens) is dropped with the refusal.
pub(crate) fn bind_scene_resources(
    witness: ObjectCompositionWitness,
    plan: &SceneEffectPlan,
    set: Arc<ObjectSceneResourceSet>,
) -> Result<ObjectCompositionWitness, BindResourcesError> {
    let pin = plan.pin();
    if witness.instance_id() != pin.instance_id
        || witness.document_id() != pin.document_id
        || witness.content_revision() != pin.content_revision
        || witness.document_snapshot_id() != pin.document_snapshot_id
        || witness.context_id() != pin.context_id
        || witness.frame() != pin.frame
    {
        return Err(BindResourcesError::WrongWitness);
    }
    set.verify_declared(plan.declared_resource_versions())
        .map_err(BindResourcesError::Resources)?;
    Ok(witness.with_scene_resources(set))
}
