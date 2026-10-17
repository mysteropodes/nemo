//! Rig-posed stroke and image-mesh scene producer (L07-L; I16a, G03a).
//!
//! The evaluated side of the baseline `applyRigDeform` (src/js/app.js at fbe4b8a3)
//! followed by `SMImageMesh.scenePayloadFor` (src/js/image-mesh.js): one pinned frame,
//! one detached value per source record, reported through the L07-C registry.
//! * A rig-driven STROKE target runs the L03-E deform kernel and reports the posed
//!   boundary (`PathPose`); a bind the baseline skips (no live stroke) is a typed
//!   unavailable entry, never a stroke that silently stays at rest.
//! * A MESH target (rig-driven or not) reports an [`EvaluatedMesh`]: the L03-A
//!   `ScenePayload` of the stored mesh with the rig offsets laid over the stored ones
//!   (the baseline writes `mesh.offsets[0 .. min(verts, rest))`, the tail keeps its
//!   stored offsets) plus the optional animated per-vertex pose, and the image size
//!   and opacity the draw needs. A mesh with no triangles has no payload in the
//!   baseline (the image is then a plain raster owned by the raster producer), so it
//!   is reported unavailable here instead of drawing nothing.
//! * The scene and the export consume the SAME `Arc<EvaluatedMesh>`; both call
//!   [`EvaluatedMesh::draw_plan`] (the L03-H draw plan) with their own view
//!   transform, so the evaluated geometry cannot differ between them.
//!
//! The deform runs once per call over the rig-driven targets in target order (the
//! baseline binds are independent); a deform failure or an unavailable rig makes
//! every rig-driven target unavailable with the reason, and static mesh targets are
//! unaffected. Nothing here reads or writes a document, a rig record or a mesh store:
//! the caller (the record join of L03-N/L03-R) hands over borrowed values already
//! resolved at the packet's pinned frame, and the output is detached from them.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshRect, MeshView, ScenePayload,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    plan_mesh_draw, AffineCoeffs, MeshDrawPlan, MeshPlanInput, PlanSkip,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform::{
    apply_rig_deform, BindInput, BindPose, MeshBind, MeshTarget, PathBind, PathPose,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::DeformBone;
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{Point2, WeightEntry};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::object_frame_packet::ObjectFramePacket;
use std::sync::Arc;

pub(crate) const PRODUCER: &str = "rig-pose";

pub(crate) const REASON_STROKE_UNPOSED: &str =
    "the rig bind has no live stroke segments, so the stroke is not posed";
pub(crate) const REASON_NO_TRIANGLES: &str =
    "the image mesh has no triangles, so the image is a plain raster for the raster producer";
pub(crate) const REASON_POSE_MISMATCH: &str =
    "the rig answered with a pose of another kind than its bind";

/// The mesh payload of an image plus what its draw needs, detached and immutable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EvaluatedMesh {
    payload: ScenePayload,
    image_width: u32,
    image_height: u32,
    opacity: f32,
}

impl EvaluatedMesh {
    pub(crate) fn payload(&self) -> &ScenePayload {
        &self.payload
    }

    pub(crate) fn image_size(&self) -> (u32, u32) {
        (self.image_width, self.image_height)
    }

    pub(crate) fn opacity(&self) -> f32 {
        self.opacity
    }

    /// The draw plan under one view transform. The scene and the export each pass
    /// their own view; the geometry in the plan is a pure function of this value.
    pub(crate) fn draw_plan(&self, view: &AffineCoeffs) -> Result<MeshDrawPlan, PlanSkip> {
        plan_mesh_draw(
            &MeshPlanInput::from_payload(&self.payload),
            self.image_width,
            self.image_height,
            self.opacity,
            view,
        )
    }
}

/// What one source record reports.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PoseItem {
    Stroke(Arc<PathPose>),
    Mesh(Arc<EvaluatedMesh>),
}

/// The stored mesh of one image with its final display rect, borrowed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshSource<'a> {
    pub(crate) mesh: MeshView<'a>,
    pub(crate) rect: MeshRect,
    pub(crate) image_width: u32,
    pub(crate) image_height: u32,
    pub(crate) opacity: f32,
    /// Animated per-vertex offsets (`poseAt`), added on top of the stored and rig offsets.
    pub(crate) pose: Option<&'a [Option<[f64; 2]>]>,
}

/// One source record to pose.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PoseTarget<'a> {
    RigStroke {
        source_object_index: usize,
        bind: PathBind<'a>,
    },
    RigMesh {
        source_object_index: usize,
        /// World positions the weights were made on.
        rest: &'a [Point2],
        weights: &'a [Vec<WeightEntry>],
        rotate: bool,
        source: MeshSource<'a>,
    },
    Mesh {
        source_object_index: usize,
        source: MeshSource<'a>,
    },
}

/// The rig at the pinned frame, or why there is none.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RigAvailability<'a> {
    Ready(&'a [DeformBone]),
    Unavailable(&'static str),
}

impl<'a> PoseTarget<'a> {
    fn source_object_index(&self) -> usize {
        match self {
            PoseTarget::RigStroke {
                source_object_index,
                ..
            }
            | PoseTarget::RigMesh {
                source_object_index,
                ..
            }
            | PoseTarget::Mesh {
                source_object_index,
                ..
            } => *source_object_index,
        }
    }

    /// The deform bind of a rig-driven target; a static mesh has none.
    fn bind(&self) -> Option<BindInput<'a>> {
        match self {
            PoseTarget::RigStroke { bind, .. } => Some(BindInput::Path(*bind)),
            PoseTarget::RigMesh {
                rest,
                weights,
                rotate,
                source,
                ..
            } => Some(BindInput::Mesh(MeshBind {
                rest: *rest,
                weights: *weights,
                rotate: *rotate,
                target: Some(MeshTarget {
                    verts: source.mesh.verts,
                    rect: source.rect,
                }),
            })),
            PoseTarget::Mesh { .. } => None,
        }
    }
}

/// Stored offsets with the rig offsets laid over the first `rig.len()` vertices.
fn effective_offsets(source: &MeshSource<'_>, rig: Option<&[Point2]>) -> Vec<[f64; 2]> {
    (0..source.mesh.verts.len())
        .map(|i| {
            rig.and_then(|offsets| offsets.get(i))
                .or_else(|| source.mesh.offsets.get(i))
                .copied()
                .unwrap_or([0.0, 0.0])
        })
        .collect()
}

fn evaluate_mesh(source: &MeshSource<'_>, rig: Option<&[Point2]>) -> Result<EvaluatedMesh, String> {
    let offsets = effective_offsets(source, rig);
    let view = MeshView {
        outline: source.mesh.outline,
        verts: source.mesh.verts,
        offsets: &offsets,
        tris: source.mesh.tris,
    };
    let payload = scene_payload(&view, &source.rect, source.pose)
        .ok_or_else(|| REASON_NO_TRIANGLES.to_owned())?;
    Ok(EvaluatedMesh {
        payload,
        image_width: source.image_width,
        image_height: source.image_height,
        opacity: source.opacity,
    })
}

fn pose_item(target: &PoseTarget<'_>, pose: Option<&BindPose>) -> Result<PoseItem, String> {
    match (target, pose) {
        (PoseTarget::RigStroke { .. }, Some(BindPose::Path(path))) => {
            Ok(PoseItem::Stroke(Arc::new(path.clone())))
        }
        (PoseTarget::RigStroke { .. }, Some(BindPose::Skipped)) => {
            Err(REASON_STROKE_UNPOSED.to_owned())
        }
        (PoseTarget::RigMesh { source, .. }, Some(BindPose::Mesh(offsets))) => {
            evaluate_mesh(source, Some(offsets.as_slice())).map(|m| PoseItem::Mesh(Arc::new(m)))
        }
        (PoseTarget::Mesh { source, .. }, None) => {
            evaluate_mesh(source, None).map(|m| PoseItem::Mesh(Arc::new(m)))
        }
        _ => Err(REASON_POSE_MISMATCH.to_owned()),
    }
}

/// One producer output for the packet's pinned frame: exactly one entry per target,
/// an item or a typed unavailable entry, in target order.
pub(crate) fn produce_pose_output(
    packet: &ObjectFramePacket,
    rig: RigAvailability<'_>,
    targets: &[PoseTarget<'_>],
) -> ProducerOutput<PoseItem> {
    let binds: Vec<BindInput<'_>> = targets.iter().filter_map(PoseTarget::bind).collect();
    let poses: Result<Vec<BindPose>, String> = if binds.is_empty() {
        Ok(Vec::new())
    } else {
        match rig {
            RigAvailability::Unavailable(reason) => Err(reason.to_owned()),
            RigAvailability::Ready(bones) => apply_rig_deform(bones, &binds).map_err(|failure| {
                format!(
                    "the rig deform failed at bind {}: {:?}",
                    failure.bind_index, failure.error
                )
            }),
        }
    };
    let mut output = ProducerOutput::new(PRODUCER, ScenePin::of_packet(packet));
    let mut cursor = 0;
    for target in targets {
        let index = target.source_object_index();
        let outcome = if target.bind().is_some() {
            let slot = cursor;
            cursor += 1;
            match &poses {
                Err(reason) => Err(reason.clone()),
                Ok(list) => pose_item(target, list.get(slot)),
            }
        } else {
            pose_item(target, None)
        };
        output = match outcome {
            Ok(item) => output.with_item(index, item),
            Err(reason) => output.with_unavailable(index, reason),
        };
    }
    output
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_rig_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_packet_rows.rs"]
pub(crate) mod packet_rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_rig_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_rig.rs"]
mod tests;
