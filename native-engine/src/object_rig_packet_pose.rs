//! The ONE call site that evaluates a rig pose for a scene packet (L03-P, #2252; L07-L, #2230).
//!
//! The scene (viewport) and the export read the same retained `ObjectRenderScene`, and the posed geometry
//! of every consumer comes from [`layer_pose`], which holds the only `evaluate_rig_pose` call of the scene
//! join: the contour producer takes the posed strokes from it, the raster join takes the posed mesh offsets
//! from it. Nothing else in the scene path evaluates a rig, so the two cannot disagree.
//!
//! Providers, all read from the packet's own retained snapshot (no caller data):
//! * the rig, the bones and the binds come from the stored layer record;
//! * the mesh store is the document's `image_meshes`; `None` there is the typed
//!   `MeshStoreUnavailable`, never an empty store;
//! * the display rect of a mesh is the stored rect of the selected raster record that carries `meshId`
//!   in that layer (baseline `rasterImageRect(b._liveRaster)`: the raster's own rect, before the Motion,
//!   parent and 3D chain that the scene draw applies afterwards). A mesh whose raster is not selected, or
//!   which several rasters of the layer carry at different rects, has no rect: the evaluator reports the
//!   typed `RasterRect` unavailability and nothing is guessed.
//!
//! Only SCENE_CONTEXT / Authored packets over a top-level layer are admitted (component contexts are not a
//! provider here); anything else is the typed reason below, never an implied pose.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose, BindOutcome, BindTarget, MeshPoseSource, MeshSnapshot, RigPoseEvaluation, RigPosePin,
};
use crate::object_frame_packet::text::{FrameScopeKind, SCENE_CONTEXT};
use crate::object_frame_packet::{ObjectFramePacket, ObjectLayer};
use std::collections::BTreeMap;

pub(crate) const REASON_SCOPE: &str =
    "rig pose provider scope differs from the selected packet layer";
pub(crate) const REASON_REFUSED: &str = "stored rig pose evaluation refused the pinned frame";
pub(crate) const REASON_EXPOSURE: &str = "stored rig pose and packet exposure differ";
pub(crate) const REASON_MESH_UNAVAILABLE: &str = "native mesh rig pose is unavailable";

/// The mesh store and raster rects of ONE layer, owned for the evaluation.
struct PacketMeshes {
    by_id: BTreeMap<String, MeshSnapshot>,
}

impl MeshPoseSource for PacketMeshes {
    fn mesh(&self, mesh_id: &str) -> Option<&MeshSnapshot> {
        self.by_id.get(mesh_id)
    }
}

/// The rect of the single raster of `layer_index` that carries `mesh_id`; `None` for none or a conflict.
fn raster_rect(packet: &ObjectFramePacket, layer_index: usize, mesh_id: &str) -> Option<MeshRect> {
    let mut found: Option<MeshRect> = None;
    for record in packet.raster_records() {
        let item = &record.entry().item;
        if record.layer_index() != layer_index || item.mesh_id() != Some(mesh_id) {
            continue;
        }
        let r = item.display_rect();
        let rect = MeshRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            rotation: r.rotation,
        };
        match found {
            None => found = Some(rect),
            Some(previous) if previous == rect => {}
            Some(_) => return None,
        }
    }
    found
}

fn packet_meshes(packet: &ObjectFramePacket, layer_index: usize) -> Option<PacketMeshes> {
    let store = packet.snapshot().document().image_meshes()?;
    let rig = packet.source_layers()[layer_index].rig()?;
    let mut by_id = BTreeMap::new();
    for bind in &rig.binds {
        let Some(BindTarget::Mesh(mesh_id)) = bind.target() else {
            continue;
        };
        if by_id.contains_key(mesh_id) {
            continue;
        }
        if let Some(mesh) = store.get(mesh_id) {
            by_id.insert(
                mesh_id.to_owned(),
                MeshSnapshot {
                    outline: mesh.outline_points(),
                    verts: mesh.rest_points(),
                    offsets: mesh.effective_offsets(),
                    tris: mesh.triangles().to_vec(),
                    rect: raster_rect(packet, layer_index, mesh_id),
                },
            );
        }
    }
    Some(PacketMeshes { by_id })
}

/// Evaluate the rig of source layer `index` ONCE against the packet's retained snapshot and frame.
/// The only `evaluate_rig_pose` call of the scene join.
pub(crate) fn layer_pose(
    packet: &ObjectFramePacket,
    index: usize,
) -> Result<RigPoseEvaluation, &'static str> {
    let layer = &packet.source_layers()[index];
    let snapshot = packet.snapshot();
    if packet.context_id() != SCENE_CONTEXT
        || packet.scope_kind() != &FrameScopeKind::Authored
        || snapshot
            .document()
            .layers()
            .iter()
            .position(|source| source.layer_uid() == layer.layer_uid())
            != Some(index)
    {
        return Err(REASON_SCOPE);
    }
    let meshes = packet_meshes(packet, index);
    let evaluation = evaluate_rig_pose(
        snapshot,
        &RigPosePin::of(snapshot),
        layer.layer_uid(),
        packet.frame(),
        meshes.as_ref().map(|m| m as &dyn MeshPoseSource),
    )
    .map_err(|_| REASON_REFUSED)?;
    if Some(evaluation.source_frame) != packet.source_frame_for_layer(index) {
        return Err(REASON_EXPOSURE);
    }
    Ok(evaluation)
}

/// Every selected rigged layer, evaluated once. Per-bind unavailability does not invalidate independent
/// paths; unlinked binds keep their typed evaluator outcome and leave the stored geometry intact, as the
/// baseline `relinkRigBinds` does.
pub(crate) fn packet_poses(
    packet: &ObjectFramePacket,
) -> BTreeMap<usize, Result<RigPoseEvaluation, &'static str>> {
    let mut poses = BTreeMap::new();
    for record in packet.records() {
        let index = record.source_layer_index();
        if packet.source_layers()[index].rig().is_none() {
            continue;
        }
        poses
            .entry(index)
            .or_insert_with(|| layer_pose(packet, index));
    }
    poses
}

/// The posed offsets vector of `mesh_id` for a meshed raster of a rigged layer.
/// `Ok(None)`: no linked bind drives this mesh (the stored sculpt draws, as the baseline after relink).
/// `Err`: a bind that targets the mesh could not be evaluated (a provider is missing); no pose is faked.
pub(crate) fn mesh_offsets(
    packet: &ObjectFramePacket,
    layer_index: usize,
    mesh_id: &str,
    evaluation: &Result<RigPoseEvaluation, &'static str>,
) -> Result<Option<Vec<[f64; 2]>>, &'static str> {
    let evaluation = evaluation.as_ref().map_err(|reason| *reason)?;
    let rig = packet.source_layers()[layer_index]
        .rig()
        .ok_or(REASON_MESH_UNAVAILABLE)?;
    for (bind, outcome) in rig.binds.iter().zip(evaluation.binds.iter()) {
        if bind.target() == Some(BindTarget::Mesh(mesh_id))
            && matches!(outcome, BindOutcome::Unavailable(_))
        {
            return Err(REASON_MESH_UNAVAILABLE);
        }
    }
    Ok(evaluation
        .meshes
        .iter()
        .find(|mesh| mesh.mesh_id == mesh_id)
        .map(|mesh| mesh.offsets.clone()))
}

/// A rig does not move a raster's own rect: the baseline pivot is the Paper bounds of the layer, and a
/// raster's bounds ignore its mesh offsets (the rig pose rides the mesh, not the rect). A rigged layer is
/// therefore chain-safe only when it holds nothing else that the pose would move: no vector record (its
/// pivot bounds would be the unposed stored ones) and no text record. Any other rigged layer keeps the
/// typed `LayerContext` refusal.
pub(crate) fn rig_leaves_pivot_alone(packet: &ObjectFramePacket, index: usize, layer: &ObjectLayer) -> bool {
    layer.rig.value().is_none()
        || (packet
            .snapshot()
            .document()
            .objects()
            .iter()
            .all(|record| record.target().layer_uid() != layer.layer_uid())
            && packet.text_records().all(|record| record.layer_index() != index))
}

#[cfg(all(test, feature = "export_job"))]
#[path = "../tests/object_rig_packet_pose.rs"]
mod tests;
#[cfg(all(
    test,
    feature = "export_job",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_rig_packet_pose_gpu.rs"]
mod gpu_tests;
