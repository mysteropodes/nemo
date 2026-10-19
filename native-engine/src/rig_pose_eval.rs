//! Rig pose evaluation join (I12): one pinned `ObjectSnapshot` and frame in, detached posed geometry out.
//!
//! The baseline poses a rig inside the live Paper scene: `loadFrame` rebuilds the children,
//! `relinkRigBinds` FILTERS the binds down to the ones whose target still matches, then
//! `applyRigDeform` writes the deformed segments into the live paths and the deformed offsets into the
//! mesh store (`src/js/app.js` at fbe4b8a3). This module is the native join of that chain, built from
//! parts that already exist and own nothing new:
//! - the rig comes from the stored layer record (`ObjectLayer::rig`, L03-N), the stroke geometry from the
//!   authored object record of the same layer at the exposed source frame, never from a live object;
//! - which binds survive is `bind_link` (L03-N), reported per bind as data instead of a silent drop;
//! - the deformation is `apply_rig_deform` (L03-E over the L03-D weights), called ONCE per evaluation with
//!   every surviving bind in record order;
//! - the mesh store and the display rect of the image come through the [`MeshPoseSource`] port (the store
//!   is L03-J, the raster that carries the mesh is L03-R / L06): no provider wired means a typed
//!   [`PoseUnavailable`], never a defaulted rect, a guessed mesh or a fake pose.
//!
//! Semantics kept from the baseline, each pinned by the executed oracle:
//! - a bind poses the stored geometry: `point = rest + displacement`; a handle is written only where the
//!   kernel writes it (non-null rest handle), every other handle keeps the value the stored record holds;
//! - binds are applied in record order onto one geometry per stroke, so a later bind of the same stroke
//!   overwrites the fields it writes (the baseline loops `rig.binds` over one live path);
//! - a mesh bind writes `min(verts, rest)` offsets over the stored ones (equal to `verts` once the bind is
//!   linked) and the result is the whole offsets vector that feeds `scenePayloadFor`;
//! - the frame selects the authored source frame through the layer timing exactly like the frame packet
//!   (`exposure(frame).1`); a layer without timing exposes `frame` itself, an unexposed frame is refused.
//!
//! Deliberate differences, all typed: a baseline `TypeError` midway through the bind loop (after earlier
//! binds already mutated their targets) is [`RigPoseRefusal::Deform`] with the RECORD bind index and NO
//! geometry; the evaluation never mutates the snapshot, the rig or the mesh port (every output is an owned
//! copy); a bind the baseline would drop is [`BindOutcome::Unlinked`] with the reason.
//!
//! Not part of this join: linked-fill / `centerSegments` mirrors of a vector brush (L01 G14 records; the
//! runtime-only `centerRest` is not in the record), component contexts and reference scopes (the link
//! predicate is the one of `bind_link`), IK solving (a stored pose is already in `segments`; L03-Q).

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform::{
    apply_rig_deform, BindInput, BindPose, MeshBind, MeshTarget, PathBind, PathPose,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::DeformError;
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::Point2;
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_rig_record::{
    bind_link, BindLink, BindTarget, MeshVertexLookup,
};
use crate::object_document::{FrameScopeKind, ObjectLayer, ObjectRecord};
use crate::revision::ObjectSnapshot;

#[path = "rig_pose_eval_deliver.rs"]
mod deliver;
#[path = "rig_pose_eval_inputs.rs"]
mod inputs;
#[allow(unused_imports)]
pub(crate) use deliver::{evaluate_and_deliver, PosedGeometrySink};
use inputs::{deform_bones, owned_bind, OwnedBind};

/// The four identities a pinned read must present, copied from the snapshot it was prepared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RigPosePin<'a> {
    pub(crate) instance_id: &'a str,
    pub(crate) document_id: &'a str,
    pub(crate) content_revision: u64,
    pub(crate) snapshot_id: &'a str,
}

impl<'a> RigPosePin<'a> {
    /// The pin that matches `snapshot` exactly.
    pub(crate) fn of(snapshot: &'a ObjectSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            snapshot_id: snapshot.snapshot_id(),
        }
    }
}

/// Why no evaluation was produced at all (nothing partial is ever returned).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RigPoseRefusal {
    WrongInstance,
    WrongDocument,
    WrongRevision,
    WrongSnapshot,
    FrameOutOfRange,
    LayerAbsent,
    /// The layer exists but never had a rig (an empty rig is present and evaluates to nothing).
    NoRig,
    /// The layer timing exposes no drawing at this frame.
    NotExposed,
    /// The deformation could not be computed; `bind_index` indexes the rig record's `binds`.
    Deform {
        bind_index: usize,
        error: DeformError,
    },
}

/// A mesh as the pose reads it: the stored topology and rest data of the image-mesh store (L03-J) and
/// the display rect of the raster that carries it (L03-R). All normalized coordinates, as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshSnapshot {
    pub(crate) outline: Vec<Point2>,
    pub(crate) verts: Vec<Point2>,
    /// The stored offsets (the rest sculpt); shorter than `verts` reads as zero for the missing tail.
    pub(crate) offsets: Vec<Point2>,
    pub(crate) tris: Vec<u32>,
    /// `None` while no raster rect provider is wired.
    pub(crate) rect: Option<MeshRect>,
}

/// The mesh store port. A returned borrow must stay valid for the whole evaluation.
pub(crate) trait MeshPoseSource {
    /// `None`: the id is not in the store.
    fn mesh(&self, mesh_id: &str) -> Option<&MeshSnapshot>;
}

/// A provider the evaluation needs and did not get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseUnavailable {
    /// No image-mesh store is wired.
    MeshStore,
    /// The mesh exists but no display rect for its raster is wired.
    RasterRect,
}

/// What happened to one bind of the rig record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindOutcome {
    /// Deformed and merged into the output.
    Posed,
    /// The kernel skipped it (no live target); nothing was written.
    Skipped,
    /// The baseline `relinkRigBinds` would have dropped it; the reason is the link verdict.
    Unlinked(BindLink),
    /// A provider is missing; nothing was written and nothing was guessed.
    Unavailable(PoseUnavailable),
}

/// One posed vertex: point and both handles (relative vectors), as the stored record spells them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PosedSegment {
    pub(crate) point: Point2,
    pub(crate) handle_in: Point2,
    pub(crate) handle_out: Point2,
}

/// The posed geometry of one stroke after every bind that drives it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PosedStroke {
    pub(crate) stroke_id: String,
    pub(crate) closed: bool,
    pub(crate) segments: Vec<PosedSegment>,
}

/// The posed offsets of one mesh: the whole vector that feeds `scene_payload` / `scenePayloadFor`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PosedMesh {
    pub(crate) mesh_id: String,
    pub(crate) offsets: Vec<Point2>,
}

/// The detached result of one evaluation: owned, pinned to one snapshot and frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RigPoseEvaluation {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) snapshot_id: String,
    pub(crate) layer_uid: String,
    pub(crate) frame: u32,
    /// The authored frame the strokes were read from (the held key for a held frame).
    pub(crate) source_frame: u32,
    /// One outcome per bind of the rig record, in record order.
    pub(crate) binds: Vec<BindOutcome>,
    /// Posed strokes in first-bind order; a stroke no surviving bind drives is not listed.
    pub(crate) strokes: Vec<PosedStroke>,
    pub(crate) meshes: Vec<PosedMesh>,
}

enum Target {
    Path {
        base: PosedStroke,
    },
    Mesh {
        mesh_id: String,
        source: MeshSnapshot,
        rect: MeshRect,
    },
}

struct Entry {
    index: usize,
    owned: OwnedBind,
    target: Target,
}

fn check_pin(snapshot: &ObjectSnapshot, pin: &RigPosePin<'_>) -> Result<(), RigPoseRefusal> {
    if pin.instance_id != snapshot.instance_id() {
        return Err(RigPoseRefusal::WrongInstance);
    }
    if pin.document_id != snapshot.document_id() {
        return Err(RigPoseRefusal::WrongDocument);
    }
    if pin.content_revision != snapshot.content_revision() {
        return Err(RigPoseRefusal::WrongRevision);
    }
    if pin.snapshot_id != snapshot.snapshot_id() {
        return Err(RigPoseRefusal::WrongSnapshot);
    }
    Ok(())
}

/// The authored frame the layer shows at `frame`: the frame packet's `exposure(frame).1`, or `frame`
/// itself for a layer without timing.
fn source_frame_of(layer: &ObjectLayer, frame: u32) -> Option<u32> {
    match layer.timing.value() {
        Some(timing) => timing.exposure(frame).1,
        None => Some(frame),
    }
}

/// The same predicate `bind_link` uses to find the stroke a bind names (one layer, one authored frame).
fn stroke_record<'a>(
    objects: &'a [ObjectRecord],
    layer_uid: &str,
    stroke_id: &str,
    frame: u32,
) -> Option<&'a ObjectRecord> {
    objects.iter().find(|record| {
        let target = record.target();
        target.context_id() == SCENE_CONTEXT
            && target.layer_uid() == layer_uid
            && target.stroke_id() == stroke_id
            && target.frame() == frame
            && *target.scope_kind() == FrameScopeKind::Authored
    })
}

fn number(value: &serde_json::Number) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

fn base_stroke(record: &ObjectRecord, stroke_id: &str) -> PosedStroke {
    let geometry = &record.geometry;
    PosedStroke {
        stroke_id: stroke_id.to_owned(),
        closed: geometry.closed,
        segments: geometry
            .segments
            .iter()
            .map(|s| PosedSegment {
                point: [number(&s.point.x), number(&s.point.y)],
                handle_in: [number(&s.handle_in.x), number(&s.handle_in.y)],
                handle_out: [number(&s.handle_out.x), number(&s.handle_out.y)],
            })
            .collect(),
    }
}

fn overlay_stroke(strokes: &mut Vec<PosedStroke>, base: &PosedStroke, pose: &PathPose) {
    let at = match strokes.iter().position(|s| s.stroke_id == base.stroke_id) {
        Some(at) => at,
        None => {
            strokes.push(base.clone());
            strokes.len() - 1
        }
    };
    for (segment, written) in strokes[at].segments.iter_mut().zip(pose.segments.iter()) {
        segment.point = written.point;
        if let Some(handle) = written.handle_in {
            segment.handle_in = handle;
        }
        if let Some(handle) = written.handle_out {
            segment.handle_out = handle;
        }
    }
}

fn stored_offsets(source: &MeshSnapshot) -> Vec<Point2> {
    let mut offsets = source.offsets.clone();
    offsets.resize(source.verts.len(), [0.0, 0.0]);
    offsets
}

fn overlay_mesh(
    meshes: &mut Vec<PosedMesh>,
    mesh_id: &str,
    source: &MeshSnapshot,
    written: &[Point2],
) {
    let at = match meshes.iter().position(|m| m.mesh_id == mesh_id) {
        Some(at) => at,
        None => {
            meshes.push(PosedMesh {
                mesh_id: mesh_id.to_owned(),
                offsets: stored_offsets(source),
            });
            meshes.len() - 1
        }
    };
    for (slot, offset) in meshes[at].offsets.iter_mut().zip(written.iter()) {
        *slot = *offset;
    }
}

/// Evaluate a ROOT-layer rig at `frame` against one pinned snapshot.
/// The provider is document.layers()/objects(), never a component's inner tables;
/// stored target resolution is constrained to SCENE_CONTEXT and Authored scope.
///
/// `meshes` is the image-mesh provider; `None` means none is wired and every mesh bind reports
/// [`PoseUnavailable::MeshStore`]. Path binds never need it.
pub(crate) fn evaluate_rig_pose(
    snapshot: &ObjectSnapshot,
    pin: &RigPosePin<'_>,
    layer_uid: &str,
    frame: u32,
    meshes: Option<&dyn MeshPoseSource>,
) -> Result<RigPoseEvaluation, RigPoseRefusal> {
    check_pin(snapshot, pin)?;
    let document = snapshot.document();
    if frame >= document.total_frames() {
        return Err(RigPoseRefusal::FrameOutOfRange);
    }
    let layer = document
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == layer_uid)
        .ok_or(RigPoseRefusal::LayerAbsent)?;
    let rig = layer.rig().ok_or(RigPoseRefusal::NoRig)?;
    let source_frame = source_frame_of(layer, frame).ok_or(RigPoseRefusal::NotExposed)?;

    let vertex_count = |id: &str| -> Option<usize> {
        meshes
            .and_then(|source| source.mesh(id))
            .map(|mesh| mesh.verts.len())
    };
    let port: MeshVertexLookup<'_> = if meshes.is_some() {
        Some(&vertex_count)
    } else {
        None
    };

    let mut outcomes: Vec<BindOutcome> = Vec::with_capacity(rig.binds.len());
    let mut entries: Vec<Entry> = Vec::new();
    for (index, bind) in rig.binds.iter().enumerate() {
        let link = bind_link(bind, layer_uid, source_frame, document.objects(), port);
        let target = match (link, bind.target()) {
            (BindLink::Linked { .. }, Some(BindTarget::Stroke(stroke_id))) => {
                match stroke_record(document.objects(), layer_uid, stroke_id, source_frame) {
                    Some(record) => Target::Path {
                        base: base_stroke(record, stroke_id),
                    },
                    None => {
                        outcomes.push(BindOutcome::Unlinked(BindLink::StrokeMissing));
                        continue;
                    }
                }
            }
            (BindLink::Linked { .. }, Some(BindTarget::Mesh(mesh_id))) => {
                let found = meshes.and_then(|source| source.mesh(mesh_id));
                match found {
                    Some(mesh) => match mesh.rect {
                        Some(rect) => Target::Mesh {
                            mesh_id: mesh_id.to_owned(),
                            source: mesh.clone(),
                            rect,
                        },
                        None => {
                            outcomes.push(BindOutcome::Unavailable(PoseUnavailable::RasterRect));
                            continue;
                        }
                    },
                    None => {
                        outcomes.push(BindOutcome::Unlinked(BindLink::MeshMissing));
                        continue;
                    }
                }
            }
            (BindLink::MeshStoreUnavailable, _) => {
                outcomes.push(BindOutcome::Unavailable(PoseUnavailable::MeshStore));
                continue;
            }
            (other, _) => {
                outcomes.push(BindOutcome::Unlinked(other));
                continue;
            }
        };
        outcomes.push(BindOutcome::Skipped);
        entries.push(Entry {
            index,
            owned: owned_bind(bind),
            target,
        });
    }

    let bones = deform_bones(rig);
    let inputs: Vec<BindInput<'_>> = entries
        .iter()
        .map(|entry| match &entry.target {
            Target::Path { base } => BindInput::Path(PathBind {
                rest: &entry.owned.rest,
                rest_handles: entry.owned.rest_handles.as_deref(),
                weights: &entry.owned.weights,
                rotate: entry.owned.rotate,
                live_len: Some(base.segments.len()),
                center: None,
            }),
            Target::Mesh { source, rect, .. } => BindInput::Mesh(MeshBind {
                rest: &entry.owned.rest,
                weights: &entry.owned.weights,
                rotate: entry.owned.rotate,
                target: Some(MeshTarget {
                    verts: &source.verts,
                    rect: *rect,
                }),
            }),
        })
        .collect();
    let poses = apply_rig_deform(&bones, &inputs).map_err(|failure| RigPoseRefusal::Deform {
        bind_index: entries[failure.bind_index].index,
        error: failure.error,
    })?;

    let mut strokes: Vec<PosedStroke> = Vec::new();
    let mut posed_meshes: Vec<PosedMesh> = Vec::new();
    for (entry, pose) in entries.iter().zip(poses.iter()) {
        match (&entry.target, pose) {
            (Target::Path { base }, BindPose::Path(path)) => {
                overlay_stroke(&mut strokes, base, path);
                outcomes[entry.index] = BindOutcome::Posed;
            }
            (
                Target::Mesh {
                    mesh_id, source, ..
                },
                BindPose::Mesh(offsets),
            ) => {
                overlay_mesh(&mut posed_meshes, mesh_id, source, offsets);
                outcomes[entry.index] = BindOutcome::Posed;
            }
            _ => {}
        }
    }

    Ok(RigPoseEvaluation {
        instance_id: snapshot.instance_id().to_owned(),
        document_id: snapshot.document_id().to_owned(),
        content_revision: snapshot.content_revision(),
        snapshot_id: snapshot.snapshot_id().to_owned(),
        layer_uid: layer_uid.to_owned(),
        frame,
        source_frame,
        binds: outcomes,
        strokes,
        meshes: posed_meshes,
    })
}

#[cfg(test)]
#[path = "../tests/rig_pose_eval_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_eval_guards.rs"]
mod guard_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_eval_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/rig_pose_eval.rs"]
mod tests;
