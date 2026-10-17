//! Geometry half of the image-mesh scene join (F17, F18; leaf L03-S): the stored mesh, the final
//! display rect and the optional pose become ONE world payload and ONE planned draw.
//!
//! Baseline: `buildSceneJson` (`src/js/engine-bridge.js` at fbe4b8a3, 1203-1215 and 1473-1485)
//! attaches `SMImageMesh.scenePayloadFor(meshId, rb, poseAt)` to an image item, where `rb` is the
//! FINAL display rect (every Motion, parent, 3D and duplicator step already folded in) and the mesh
//! lives in normalized rect space, so it rides that chain without a second copy of the transform
//! math. The engine then draws it with `draw_image_mesh` (modelled by `render_image_mesh_plan`).
//!
//! This file takes plain values only (the final rect, the decoded image size, the item opacity as the
//! engine reads it, the view) and the document's mesh store, so it sits in the evaluation layer and
//! imports no scene type. The render-scene half (`render_image_mesh_scene`) feeds it the rect, size
//! and opacity of the prepared raster image; nothing here multiplies a Motion matrix.
//!
//! Offsets. The stored `offsets` are the static sculpt. A rig pose (`rig_pose_eval`) evaluates to the
//! WHOLE offset vector that replaces them (the baseline deform writes the live store); it enters as
//! `MeshSceneSource::offsets`. The animated per-vertex Motion pose (`poseAt`) is added on top, per
//! vertex, exactly as `scenePayloadFor` does.
//!
//! Outcomes, each pinned against the executed baseline or a literal:
//! - `Ok(Some(geometry))`: the mesh draws. The [`MeshDraw`] is the plan, or `Nothing(skip)` where the
//!   baseline renderer pushes no layer at all (`PlanSkip`), never a flat image;
//! - `Ok(None)`: `scenePayloadFor` returned `null` (the mesh has no triangle), so the baseline draws
//!   the ordinary raster image and the raster producer keeps the item;
//! - `Err`: a tagged raster whose mesh cannot be found, a missing store, a malformed offsets vector or
//!   non-finite motion. The baseline silently drew a flat unmasked image for a missing mesh; the
//!   native join reports a typed, atomic refusal instead (the dangling tag is a document defect).
//!
//! Not provided: the scene item, the producer registry call and the GPU replay
//! (`render_image_mesh_scene`, `render_image_mesh_scene_replay`), the export and widget policy
//! (`render_image_mesh_scene_policy`), the mesh store itself (L03-J/L03-R) and any Motion or 3D math.

use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshRect, MeshView, Pose, ScenePayload,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    plan_mesh_draw, AffineCoeffs, MeshDrawPlan, MeshPlanInput, PlanSkip,
};
use crate::object_document::ImageMeshStore;

/// The document's mesh store as the join receives it (`None`: the document carries none).
pub(crate) type MeshStoreRef<'a> = Option<&'a ImageMeshStore>;

/// What the mesh draw needs besides the final rect, image size and opacity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshSceneSource<'a> {
    /// `data.meshId` of the raster (non-empty; an untagged raster never reaches this join).
    pub(crate) mesh_id: &'a str,
    /// The item's view transform `[a, b, c, d, tx, ty]` (it carries zoom; the plan reads half a device pixel from it).
    pub(crate) view: AffineCoeffs,
    /// Rig-posed offsets replacing the stored sculpt, one entry per vertex; `None` keeps the stored offsets.
    pub(crate) offsets: Option<&'a [[f64; 2]]>,
    /// Animated per-vertex offsets (`poseAt`), added per vertex; `None` entries add nothing.
    pub(crate) pose: Pose<'a>,
}

/// What the baseline renderer does with the planned mesh.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MeshDraw {
    /// One outer clip layer carrying the opacity once, then one filled triangle per entry.
    Plan(MeshDrawPlan),
    /// The renderer returns before pushing any layer; nothing is drawn (and nothing falls back to a flat image).
    Nothing(PlanSkip),
}

/// The world payload of one meshed image and what the renderer does with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MeshGeometry {
    /// `scenePayloadFor`: world vertices (R2), rest uvs (R5), triangles and the outline prefix.
    pub(crate) payload: ScenePayload,
    pub(crate) draw: MeshDraw,
}

impl MeshGeometry {
    pub(crate) fn plan(&self) -> Option<&MeshDrawPlan> {
        match &self.draw {
            MeshDraw::Plan(plan) => Some(plan),
            MeshDraw::Nothing(_) => None,
        }
    }

    /// Triangles that draw (0 when the renderer draws nothing).
    pub(crate) fn triangle_count(&self) -> usize {
        self.plan().map_or(0, |plan| plan.triangles.len())
    }
}

/// Typed refusals; every one leaves the caller with no geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MeshSceneError {
    EmptyMeshId,
    /// The document carries no mesh store although a raster names `mesh_id`.
    StoreUnavailable {
        mesh_id: String,
    },
    /// The store has no entry for the tag (a dangling tag).
    MeshMissing {
        mesh_id: String,
    },
    /// The rig-posed offsets do not have one entry per stored vertex.
    OffsetsLengthMismatch {
        mesh_id: String,
        expected: usize,
        got: usize,
    },
    /// A rig offset or an animated pose entry is NaN or infinite.
    NonFiniteMotion {
        mesh_id: String,
    },
}

impl MeshSceneError {
    /// Stable machine code.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            MeshSceneError::EmptyMeshId => "mesh-scene-empty-mesh-id",
            MeshSceneError::StoreUnavailable { .. } => "mesh-scene-store-unavailable",
            MeshSceneError::MeshMissing { .. } => "mesh-scene-mesh-missing",
            MeshSceneError::OffsetsLengthMismatch { .. } => "mesh-scene-offsets-length-mismatch",
            MeshSceneError::NonFiniteMotion { .. } => "mesh-scene-non-finite-motion",
        }
    }

    pub(crate) fn reason(&self) -> &'static str {
        match self {
            MeshSceneError::EmptyMeshId => "mesh scene tag must be a non-empty mesh id",
            MeshSceneError::StoreUnavailable { .. } => {
                "mesh store is unavailable for a meshed raster"
            }
            MeshSceneError::MeshMissing { .. } => {
                "meshed raster names a mesh the store does not hold"
            }
            MeshSceneError::OffsetsLengthMismatch { .. } => {
                "mesh rig offsets do not match the stored vertex count"
            }
            MeshSceneError::NonFiniteMotion { .. } => "mesh rig offsets and pose must be finite",
        }
    }
}

/// Plans the draw of `payload` for an image of `pixel_size`, at the engine's f32 `opacity`, for `view`.
pub(crate) fn plan_draw(
    payload: &ScenePayload,
    pixel_size: (u32, u32),
    opacity: f32,
    view: &AffineCoeffs,
) -> MeshDraw {
    match plan_mesh_draw(
        &MeshPlanInput::from_payload(payload),
        pixel_size.0,
        pixel_size.1,
        opacity,
        view,
    ) {
        Ok(plan) => MeshDraw::Plan(plan),
        Err(skip) => MeshDraw::Nothing(skip),
    }
}

fn finite_pair(point: &[f64; 2]) -> bool {
    point[0].is_finite() && point[1].is_finite()
}

/// Builds the mesh geometry of one image drawn into the final `rect`.
///
/// `opacity` is the item opacity as the engine reads it (the f32 of the sequential stage product).
/// Returns `Ok(None)` when the baseline would draw the plain image (a mesh without triangles).
pub(crate) fn prepare_mesh_geometry(
    rect: &ImageRect,
    pixel_size: (u32, u32),
    opacity: f32,
    source: &MeshSceneSource<'_>,
    store: MeshStoreRef<'_>,
) -> Result<Option<MeshGeometry>, MeshSceneError> {
    let mesh_id = source.mesh_id;
    if mesh_id.is_empty() {
        return Err(MeshSceneError::EmptyMeshId);
    }
    let store = store.ok_or_else(|| MeshSceneError::StoreUnavailable {
        mesh_id: mesh_id.to_owned(),
    })?;
    let mesh = store
        .get(mesh_id)
        .ok_or_else(|| MeshSceneError::MeshMissing {
            mesh_id: mesh_id.to_owned(),
        })?;
    let outline = mesh.outline_points();
    let rest = mesh.rest_points();
    let stored = mesh.effective_offsets();
    let offsets: &[[f64; 2]] = match source.offsets {
        Some(posed) => {
            if posed.len() != rest.len() {
                return Err(MeshSceneError::OffsetsLengthMismatch {
                    mesh_id: mesh_id.to_owned(),
                    expected: rest.len(),
                    got: posed.len(),
                });
            }
            posed
        }
        None => &stored,
    };
    let pose_finite = source
        .pose
        .map_or(true, |pose| pose.iter().flatten().all(finite_pair));
    if !offsets.iter().all(finite_pair) || !pose_finite {
        return Err(MeshSceneError::NonFiniteMotion {
            mesh_id: mesh_id.to_owned(),
        });
    }
    let view = MeshView {
        outline: &outline,
        verts: &rest,
        offsets,
        tris: mesh.triangles(),
    };
    let mesh_rect = MeshRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        rotation: rect.rotation,
    };
    let Some(payload) = scene_payload(&view, &mesh_rect, source.pose) else {
        return Ok(None);
    };
    let draw = plan_draw(&payload, pixel_size, opacity, &source.view);
    Ok(Some(MeshGeometry { payload, draw }))
}

#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_rows.rs"]
pub(crate) mod oracle_rows;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_pixels.rs"]
mod pixel_tests;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_raster.rs"]
pub(crate) mod raster;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_source.rs"]
mod tests;
