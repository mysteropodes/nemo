//! The single call site that hands one posed geometry to every consumer (I12, F19 "same geometry reaches
//! scene and export").
//!
//! The baseline poses the live Paper scene once and both the screen render (`buildSceneJson`) and the
//! export (`export.js`, through the same scene builder) read that one scene. Natively the scene producer
//! and the export producer are separate consumers (L07/L08 and L03-S); to keep them on one geometry the
//! pose is evaluated exactly once per call here and the SAME borrowed [`RigPoseEvaluation`] is offered to
//! each [`PosedGeometrySink`]. A consumer cannot ask for a second, possibly different evaluation through
//! this path, and it cannot mutate the value it is shown (it receives a shared borrow of an owned result).
//!
//! The mesh half of that geometry is `PosedMesh::scene_payload`: the posed offsets fed through the image
//! mesh kernel's `scene_payload` (the port of `scenePayloadFor`) with the stored topology, so the scene
//! item a consumer builds from a posed mesh is the one the baseline `scenePayloadFor` produces.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshView, ScenePayload,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose, MeshPoseSource, MeshSnapshot, PosedMesh, RigPoseEvaluation, RigPosePin,
    RigPoseRefusal,
};
use crate::revision::ObjectSnapshot;

/// A consumer of posed geometry (the scene producer, the export producer).
pub(crate) trait PosedGeometrySink {
    /// Called at most once per [`evaluate_and_deliver`]; the borrow ends with the call.
    fn accept(&mut self, evaluation: &RigPoseEvaluation);
}

/// Evaluate once, then show the same result to `scene` and to `export`, in that order. A refusal
/// reaches neither consumer.
pub(crate) fn evaluate_and_deliver(
    snapshot: &ObjectSnapshot,
    pin: &RigPosePin<'_>,
    layer_uid: &str,
    frame: u32,
    meshes: Option<&dyn MeshPoseSource>,
    scene: &mut dyn PosedGeometrySink,
    export: &mut dyn PosedGeometrySink,
) -> Result<RigPoseEvaluation, RigPoseRefusal> {
    let evaluation = evaluate_rig_pose(snapshot, pin, layer_uid, frame, meshes)?;
    scene.accept(&evaluation);
    export.accept(&evaluation);
    Ok(evaluation)
}

impl PosedMesh {
    /// The scene payload of this posed mesh over the stored topology of `source`; `None` when the mesh has
    /// no triangles or no display rect is known (nothing is defaulted).
    pub(crate) fn scene_payload(&self, source: &MeshSnapshot) -> Option<ScenePayload> {
        let rect = source.rect?;
        scene_payload(
            &MeshView {
                outline: &source.outline,
                verts: &source.verts,
                offsets: &self.offsets,
                tris: &source.tris,
            },
            &rect,
            None,
        )
    }
}
