//! The rig pose of a snapshot for the presentation reads (#2255): the stored rig, or a CANDIDATE of it, evaluated with the snapshot's own mesh provider.
//!
//! - [`RigVertexWrite`] is a held gesture as plain vertex writes: a `moveSegment` ([`segment_writes`], the handle mirror unless Alt breaks the
//!   tangent) or an Alt-drag of a chain end ([`ik_writes`], `findIKChain` + `rigSetIK` + one `rigDragIKEnd` solve through `rig_ik_solve::plan_drag`).
//!   [`candidate_rig`] lays them over the stored record; nothing is written, kept or published.
//! - The mesh provider is the document's image-mesh store plus the display rect of the raster record that carries the mesh on the layer's exposed
//!   frame (the scene join reads the same record through its frame packet): no store, or a mesh whose raster is absent or placed differently by
//!   several records, leaves the bind [`BindOutcome::Unavailable`]; nothing is guessed.
//! - [`evaluate_for_view`] runs the one evaluation of the layer (`evaluate_rig_pose_over`) and adds the posed world vertices of each mesh.
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{world_verts, MeshRect, MeshView};
use crate::evaluation::object_image_mesh_kernels::rig_ik_solve::{plan_drag, IkBone};
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose_over, source_frame_of, BindTarget, MeshPoseSource, MeshSnapshot, RigPoseEvaluation, RigPosePin, RigPoseRefusal,
};
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_rig_record::{KeyedTable, RigPair, RigRecord};
use crate::object_document::{FrameScopeKind, Presence};
use crate::revision::ObjectSnapshot;
use std::collections::BTreeMap;

/// One vertex of one bone with the members a held gesture rewrites (handles are vectors relative to the anchor).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RigVertexWrite {
    pub(crate) bone_id: String,
    pub(crate) vi: usize,
    pub(crate) point: Option<[f64; 2]>,
    pub(crate) handle_in: Option<[f64; 2]>,
    pub(crate) handle_out: Option<[f64; 2]>,
}

fn pair(value: [f64; 2]) -> Option<RigPair> {
    Some([serde_json::Number::from_f64(value[0])?, serde_json::Number::from_f64(value[1])?])
}

fn stored<'a>(snapshot: &'a ObjectSnapshot, layer_uid: &str) -> Option<&'a RigRecord> {
    snapshot.document().layers().iter().find(|layer| layer.layer_uid() == layer_uid)?.rig()
}

/// The stored rig with `writes` laid over it; `None` when a write names no bone or vertex.
pub(crate) fn candidate_rig(rig: &RigRecord, writes: &[RigVertexWrite]) -> Option<RigRecord> {
    let mut entries = rig.bones.entries().to_vec();
    for write in writes {
        let segment = entries.iter_mut().find(|(id, _)| id == &write.bone_id)?.1.segments.get_mut(write.vi)?;
        if let Some(point) = write.point {
            segment.point = pair(point)?;
        }
        if let Some(handle) = write.handle_in {
            segment.handle_in = Presence::Present(Some(pair(handle)?));
        }
        if let Some(handle) = write.handle_out {
            segment.handle_out = Presence::Present(Some(pair(handle)?));
        }
    }
    let mut next = rig.clone();
    next.bones = KeyedTable::new(entries);
    Some(next)
}

/// A held `moveSegment`: an anchor, or a handle (`hVec = pointer - anchor`) with its mirror unless the tangent is broken.
pub(crate) fn segment_writes(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    bone_id: &str,
    vi: usize,
    part: &str,
    to: [f64; 2],
    break_tangent: bool,
) -> Option<Vec<RigVertexWrite>> {
    let segment = stored(snapshot, layer_uid)?.bones.get(bone_id)?.segments.get(vi)?;
    let anchor = [segment.point[0].as_f64()?, segment.point[1].as_f64()?];
    let vector = [to[0] - anchor[0], to[1] - anchor[1]];
    let mirror = (!break_tangent).then_some([-vector[0], -vector[1]]);
    let write = |point, handle_in, handle_out| vec![RigVertexWrite { bone_id: bone_id.to_owned(), vi, point, handle_in, handle_out }];
    match part {
        "point" => Some(write(Some(to), None, None)),
        "handleIn" => Some(write(None, Some(vector), mirror)),
        "handleOut" => Some(write(None, mirror, Some(vector))),
        _ => None,
    }
}

/// A held Alt-drag of the chain end `bone_id:vi` toward `to`: the joint, the end and the coincident near vertex.
pub(crate) fn ik_writes(snapshot: &ObjectSnapshot, layer_uid: &str, bone_id: &str, vi: usize, to: [f64; 2]) -> Option<Vec<RigVertexWrite>> {
    let point = |s: &crate::object_document::object_rig_record::RigSegment| [s.point[0].as_f64().unwrap_or(f64::NAN), s.point[1].as_f64().unwrap_or(f64::NAN)];
    let bones: Vec<IkBone> = stored(snapshot, layer_uid)?
        .bones
        .entries()
        .iter()
        .map(|(id, bone)| IkBone {
            id: id.clone(),
            closed: bone.closed,
            segments: bone.segments.iter().map(point).collect(),
            rest_segments: bone.rest_segments.iter().map(point).collect(),
        })
        .collect();
    let plan = plan_drag(&bones, bone_id, vi, to).ok()?;
    Some(plan.writes.iter().map(|w| RigVertexWrite { bone_id: w.target.bone_id.clone(), vi: w.target.vi, point: Some(w.point), handle_in: None, handle_out: None }).collect())
}

struct SnapshotMeshes {
    by_id: BTreeMap<String, MeshSnapshot>,
}

impl MeshPoseSource for SnapshotMeshes {
    fn mesh(&self, mesh_id: &str) -> Option<&MeshSnapshot> {
        self.by_id.get(mesh_id)
    }
}

/// The rect of the single raster record of the layer's exposed frame that carries `mesh_id`; `None` for none or a conflict.
fn raster_rect(snapshot: &ObjectSnapshot, layer_uid: &str, frame: u32, mesh_id: &str) -> Option<MeshRect> {
    let mut found: Option<MeshRect> = None;
    for entry in snapshot.document().item_records()?.raster_entries() {
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
fn snapshot_meshes(snapshot: &ObjectSnapshot, layer_uid: &str, frame: u32, rig: &RigRecord) -> Option<SnapshotMeshes> {
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

/// The posed offsets of one image mesh with its posed vertices in world space (the first `outline` are the contour).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewMesh {
    pub(crate) mesh_id: String,
    pub(crate) offsets: Vec<[f64; 2]>,
    pub(crate) world: Vec<[f64; 2]>,
    pub(crate) outline: usize,
}

pub(crate) struct ViewEvaluation {
    pub(crate) evaluation: RigPoseEvaluation,
    pub(crate) meshes: Vec<ViewMesh>,
}

/// The one evaluation of the layer's rig at `frame` (the stored rig, or the candidate `writes` make of it) with the snapshot's mesh provider.
pub(crate) fn evaluate_for_view(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    writes: &[RigVertexWrite],
) -> Result<ViewEvaluation, RigPoseRefusal> {
    let base = stored(snapshot, layer_uid);
    let candidate = if writes.is_empty() {
        None
    } else {
        Some(candidate_rig(base.ok_or(RigPoseRefusal::NoRig)?, writes).ok_or(RigPoseRefusal::NoRig)?)
    };
    let meshes = candidate.as_ref().or(base).and_then(|rig| snapshot_meshes(snapshot, layer_uid, frame, rig));
    let port = meshes.as_ref().map(|m| m as &dyn MeshPoseSource);
    let evaluation = evaluate_rig_pose_over(snapshot, &RigPosePin::of(snapshot), layer_uid, frame, port, candidate.as_ref())?;
    let mut posed_meshes = Vec::new();
    for posed in &evaluation.meshes {
        let Some(source) = port.and_then(|p| p.mesh(&posed.mesh_id)) else { continue };
        let Some(rect) = source.rect else { continue };
        let view = MeshView { outline: &source.outline, verts: &source.verts, offsets: &posed.offsets, tris: &source.tris };
        posed_meshes.push(ViewMesh {
            mesh_id: posed.mesh_id.clone(),
            offsets: posed.offsets.clone(),
            world: world_verts(&view, &rect, None),
            outline: source.outline.len(),
        });
    }
    Ok(ViewEvaluation { evaluation, meshes: posed_meshes })
}
