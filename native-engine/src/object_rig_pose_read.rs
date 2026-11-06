//! Read-queries facade for the rig pose evaluation join (I13); no state, no publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the commit-pose transaction reaches the one pose evaluation
//! of L03-P (`evaluate_rig_pose`: stored rig + one pinned snapshot + one frame -> detached posed
//! geometry) through the single function below, exactly like the rig edit facade reaches its kernels.
//!
//! The answer is plain numbers and plain classes: the posed vertices of every stroke a surviving bind
//! drives, the posed offsets of every mesh a surviving bind drives, and one [`PoseBind`] class per bind of
//! the stored rig. Nothing here owns a rig, a mesh store or a deformation: an evaluation that cannot run is
//! a typed [`PoseRefusal`], never a defaulted pose. Without the `evaluation` feature there is no kernel and
//! every call reports [`PoseRefusal::Unavailable`].
//!
//! The mesh provider (#2255) is the snapshot's own image-mesh store plus the display rect of the raster record
//! that carries the mesh on the layer's exposed frame (the scene join reads the same record through its frame
//! packet): a store absent is [`PoseBind::Unavailable`], a mesh whose raster is absent or placed differently by
//! several records has no rect and is [`PoseBind::Unavailable`] too; nothing is guessed. A CANDIDATE rig
//! ([`evaluate_layer_pose_over`]) replaces the stored one for one detached evaluation (the live-pose preview).
use crate::object_document::object_rig_record::RigRecord;
use crate::revision::ObjectSnapshot;

/// One posed vertex: the anchor and both relative handle vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PoseVertex {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// The posed geometry of one stroke after every bind that drives it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PoseStroke {
    pub(crate) stroke_id: String,
    pub(crate) closed: bool,
    pub(crate) vertices: Vec<PoseVertex>,
}

/// The posed offsets of one image mesh, with its posed vertices in world space (the first `outline` are the contour).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PoseMesh {
    pub(crate) mesh_id: String,
    pub(crate) offsets: Vec<[f64; 2]>,
    pub(crate) world: Vec<[f64; 2]>,
    pub(crate) outline: usize,
}

/// What the evaluation did with one bind of the stored rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseBind {
    /// Deformed and merged into the output.
    Posed,
    /// The kernel had no live target for it; nothing was written.
    Skipped,
    /// The baseline `relinkRigBinds` would have dropped it (reason kept by the evaluation, not here).
    Unlinked,
    /// A provider the bind needs is not wired (the mesh store or the raster rect); nothing was guessed.
    Unavailable,
}

/// The detached result of one evaluation, pinned to the snapshot it was asked of.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerPose {
    /// The authored frame the strokes were read from (the held key for a held frame).
    pub(crate) source_frame: u32,
    /// One class per bind of the rig record, in record order.
    pub(crate) binds: Vec<PoseBind>,
    /// Posed strokes in first-bind order; a stroke no surviving bind drives is not listed.
    pub(crate) strokes: Vec<PoseStroke>,
    /// Posed meshes in first-bind order.
    pub(crate) meshes: Vec<PoseMesh>,
}

/// Why no evaluation was produced at all (nothing partial is ever returned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseRefusal {
    /// The evaluation layer is not part of this build.
    Unavailable,
    FrameOutOfRange,
    LayerAbsent,
    /// The layer exists but never had a rig.
    NoRig,
    /// The layer timing exposes no drawing at this frame.
    NotExposed,
    /// The deformation could not be computed (the baseline `applyRigDeform` threw).
    Deform,
    /// The pin of the snapshot did not match itself (an engine fault, never a caller error).
    Pin,
}

/// Evaluate the rig of `layer_uid` at `frame` against one pinned snapshot.
pub(crate) fn evaluate_layer_pose(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
) -> Result<LayerPose, PoseRefusal> {
    evaluate_layer_pose_over(snapshot, layer_uid, frame, None)
}

/// [`evaluate_layer_pose`] over a candidate rig (`None`: the stored one); nothing is written or kept.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn evaluate_layer_pose_over(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    candidate: Option<&RigRecord>,
) -> Result<LayerPose, PoseRefusal> {
    #[cfg(feature = "evaluation")]
    {
        evaluate(snapshot, layer_uid, frame, candidate)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(PoseRefusal::Unavailable)
    }
}

#[cfg(feature = "evaluation")]
mod provider {
    //! The image-mesh provider of the snapshot: the store plus the display rect of the raster record that carries the mesh.
    use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
    use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
        source_frame_of, BindTarget, MeshPoseSource, MeshSnapshot,
    };
    use crate::object_document::object_rig_record::RigRecord;
    use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
    use crate::object_document::FrameScopeKind;
    use crate::revision::ObjectSnapshot;
    use std::collections::BTreeMap;

    pub(super) struct SnapshotMeshes {
        by_id: BTreeMap<String, MeshSnapshot>,
    }

    impl MeshPoseSource for SnapshotMeshes {
        fn mesh(&self, mesh_id: &str) -> Option<&MeshSnapshot> {
            self.by_id.get(mesh_id)
        }
    }

    /// The rect of the single raster record of the layer's exposed frame that carries `mesh_id`; `None` for none or a conflict.
    fn raster_rect(snapshot: &ObjectSnapshot, layer_uid: &str, frame: u32, mesh_id: &str) -> Option<MeshRect> {
        let entries = snapshot.document().item_records()?.raster_entries();
        let mut found: Option<MeshRect> = None;
        for entry in entries {
            let target = &entry.target;
            if target.context_id() != SCENE_CONTEXT
                || target.scope_kind() != &FrameScopeKind::Authored
                || target.layer_uid() != layer_uid
                || target.frame() != frame
                || entry.item.mesh_id() != Some(mesh_id)
            {
                continue;
            }
            let r = entry.item.display_rect();
            let rect = MeshRect { x: r.x, y: r.y, width: r.width, height: r.height, rotation: r.rotation };
            match found {
                None => found = Some(rect),
                Some(previous) if previous == rect => {}
                Some(_) => return None,
            }
        }
        found
    }

    /// `None` when the document holds no mesh store (every mesh bind then reports the store unavailable).
    pub(super) fn snapshot_meshes(snapshot: &ObjectSnapshot, layer_uid: &str, frame: u32, rig: &RigRecord) -> Option<SnapshotMeshes> {
        let store = snapshot.document().image_meshes()?;
        let layer = snapshot.document().layers().iter().find(|l| l.layer_uid() == layer_uid)?;
        let source = source_frame_of(layer, frame);
        let mut by_id = BTreeMap::new();
        for bind in &rig.binds {
            let Some(BindTarget::Mesh(mesh_id)) = bind.target() else { continue };
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
                        rect: source.and_then(|at| raster_rect(snapshot, layer_uid, at, mesh_id)),
                    },
                );
            }
        }
        Some(SnapshotMeshes { by_id })
    }
}

#[cfg(feature = "evaluation")]
fn evaluate(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    candidate: Option<&RigRecord>,
) -> Result<LayerPose, PoseRefusal> {
    use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{world_verts, MeshView};
    use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
        evaluate_rig_pose_over, BindOutcome, MeshPoseSource, RigPosePin, RigPoseRefusal,
    };
    let pin = RigPosePin::of(snapshot);
    let stored = snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == layer_uid)
        .and_then(|layer| layer.rig());
    let meshes = candidate.or(stored).and_then(|rig| provider::snapshot_meshes(snapshot, layer_uid, frame, rig));
    let port = meshes.as_ref().map(|m| m as &dyn MeshPoseSource);
    let evaluation = evaluate_rig_pose_over(snapshot, &pin, layer_uid, frame, port, candidate).map_err(
        |refusal| match refusal {
            RigPoseRefusal::FrameOutOfRange => PoseRefusal::FrameOutOfRange,
            RigPoseRefusal::LayerAbsent => PoseRefusal::LayerAbsent,
            RigPoseRefusal::NoRig => PoseRefusal::NoRig,
            RigPoseRefusal::NotExposed => PoseRefusal::NotExposed,
            RigPoseRefusal::Deform { .. } => PoseRefusal::Deform,
            RigPoseRefusal::WrongInstance
            | RigPoseRefusal::WrongDocument
            | RigPoseRefusal::WrongRevision
            | RigPoseRefusal::WrongSnapshot => PoseRefusal::Pin,
        },
    )?;
    let binds = evaluation
        .binds
        .iter()
        .map(|outcome| match outcome {
            BindOutcome::Posed => PoseBind::Posed,
            BindOutcome::Skipped => PoseBind::Skipped,
            BindOutcome::Unlinked(_) => PoseBind::Unlinked,
            BindOutcome::Unavailable(_) => PoseBind::Unavailable,
        })
        .collect();
    let strokes = evaluation
        .strokes
        .iter()
        .map(|stroke| PoseStroke {
            stroke_id: stroke.stroke_id.clone(),
            closed: stroke.closed,
            vertices: stroke
                .segments
                .iter()
                .map(|segment| PoseVertex {
                    point: segment.point,
                    handle_in: segment.handle_in,
                    handle_out: segment.handle_out,
                })
                .collect(),
        })
        .collect();
    let mut posed_meshes = Vec::new();
    for posed in &evaluation.meshes {
        let Some(source) = port.and_then(|p| p.mesh(&posed.mesh_id)) else { continue };
        let Some(rect) = source.rect else { continue };
        let view = MeshView { outline: &source.outline, verts: &source.verts, offsets: &posed.offsets, tris: &source.tris };
        posed_meshes.push(PoseMesh {
            mesh_id: posed.mesh_id.clone(),
            offsets: posed.offsets.clone(),
            world: world_verts(&view, &rect, None),
            outline: source.outline.len(),
        });
    }
    Ok(LayerPose {
        source_frame: evaluation.source_frame,
        binds,
        strokes,
        meshes: posed_meshes,
    })
}
