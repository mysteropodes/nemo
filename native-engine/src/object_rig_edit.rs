//! Sealed rig edit transaction (I12) through the existing revision owner: the bone, IK-chain, bind and
//! per-vertex weight edits of the Rig tool (`src/js/rig-bridge.js`, `src/js/app.js` at fbe4b8a3).
//! Nothing here is a second writer, a rig store or a weight implementation.
//!
//! One typed preparation reads the pinned current snapshot, validates the complete request and the
//! target layer, builds the proposed rig on a detached copy, re-admits the whole document (the rig
//! admission of L03-N runs inside `validate_drawing`) and seals the result to its origin (instance,
//! document, snapshot, content revision). The commit is the existing sealed owner publish, atomic with
//! history, request replay and cancellation in the shared dispatcher. A refusal, a stale or foreign
//! pin, a malformed request or a cancelled one publishes nothing; an edit that changes nothing keeps
//! presence, bytes, history and redo.
//!
//! Actions (`command.document.object.rig.edit`, payload `{command, layerUid, action}`):
//! - `drawBone` (`finalizeRigBone`): a fresh `boneN` from the id counter, or, with `boneId`, the same
//!   bone redrawn (its radius kept); `segments` and `restSegments` are the same list;
//! - `moveSegment` (the pose write of `onMove`): an anchor, or a handle with the symmetric mirror
//!   unless `breakTangent`; only the live pose `segments` move, never `restSegments`;
//! - `setBoneRadius` (the influence-circle drag): `Math.max(1, radius)`;
//! - `bindStroke` (`rigBindStroke`) and `autoAssign` (`rigAutoAssignLayer`): the control-point weights of
//!   the L03-D kernel, the radius growth of the L03-I kernel; a re-bind REPLACES (the old bind is
//!   removed and the new one appended); a CompoundPath-like record is skipped and counted;
//! - `setWeight`, `removeWeight`, `addWeight`, `resetWeights` (the weight popover): the percent input,
//!   the remove button, the add-bone row (`{boneId, offset, w: 0.5}`), and "reset (auto)"
//!   (`rigWeighOnePoint`, the offset model); only that one vertex's list is written;
//! - `setIk` (`rigSetIK`): `l1`/`l2` measured on the live pose, the chain filed under `end.boneId:end.vi`;
//! - `removeIk`, `removeBone`, `removeBind`: there is no removal in the baseline UI. They are the
//!   reference-checked deletions the CRUD acceptance asks for: a bone still named by an IK chain or a
//!   weight entry is refused, never cascaded;
//! - `bindMesh`: a typed `unavailable` (the raster that carries a mesh is not a native record yet).
//!
//! Deliberate differences from the baseline, each pinned by a test: every number is finite and at most
//! 1e9 in magnitude; ids and counts are bounded; a bone needs two vertices (the baseline silently
//! stores nothing); an unknown bone in `boneIds` or a vertex past the bind is refused (the baseline
//! skips it silently); an absent rig is created only by `drawBone` (the baseline creates an empty one
//! in every call); the legacy window switch `SM_RIG_CONTROL_POINT_WEIGHTS` is not modelled; a locked
//! layer refuses EVERY action (`Unavailable`, as each sibling Drawing writer does; the baseline gates only
//! the auto-assign button) and `autoAssign` also refuses an active path effect (`canEditActiveLayer`); a
//! pressure ribbon cannot be bound (`Unavailable`: the pose commit would leave its centreline at rest).
//!
//! Not provided: pose evaluation, IK dragging, reset/commit pose, mesh binds, selection, the locked-layer
//! gesture escape (F06), the scene/export join and the JS adapter.
use crate::commands::object_fill_owner::path_effects_edit::layer_has_active_path_fx;
use crate::commands::ObjectFillOwner;
use crate::object_document::object_rig_record::{BindTarget, RigRecord, RigSegment};
use crate::object_document::{ObjectDocument, ObjectLayer, Presence};
use crate::object_snapshot::object_rig_edit_read::KernelRef;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_rig_edit_parse.rs"]
mod parse;
#[path = "object_rig_edit_plan.rs"]
mod plan;
#[path = "object_rig_edit_rules.rs"]
mod rules;
use plan::PlanRefusal;

/// Which part of a bone vertex a `moveSegment` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentPart {
    Point,
    HandleIn,
    HandleOut,
}

/// Which bind a weight edit addresses: the stroke id or the mesh id the bind drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RigTarget {
    Stroke(String),
    Mesh(String),
}

impl RigTarget {
    pub(crate) fn bind(&self) -> BindTarget<'_> {
        match self {
            Self::Stroke(id) => BindTarget::Stroke(id),
            Self::Mesh(id) => BindTarget::Mesh(id),
        }
    }
}

/// `{boneId, vi}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RigRef {
    pub(crate) bone_id: String,
    pub(crate) vi: u32,
}

impl RigRef {
    fn kernel(&self) -> KernelRef {
        KernelRef {
            bone_id: self.bone_id.clone(),
            vi: self.vi,
        }
    }
}

/// A validated action: nothing in it can fail again except the target lookup and the kernels.
#[derive(Debug)]
pub(crate) enum RigAction {
    DrawBone {
        bone_id: Option<String>,
        segments: Vec<RigSegment>,
        closed: bool,
    },
    MoveSegment {
        bone_id: String,
        vi: usize,
        part: SegmentPart,
        to: [f64; 2],
        break_tangent: bool,
    },
    SetBoneRadius {
        bone_id: String,
        radius: f64,
    },
    BindStroke {
        stroke_id: String,
        frame: u32,
        bone_ids: Option<Vec<String>>,
        radius: Option<f64>,
        rotate: bool,
        softness: Option<f64>,
    },
    BindMesh,
    AutoAssign {
        frame: u32,
        default_radius: Option<f64>,
        rotate: bool,
        softness: Option<f64>,
        paths: bool,
        meshes: bool,
    },
    SetWeight {
        target: RigTarget,
        vertex: usize,
        entry: usize,
        percent: f64,
    },
    AddWeight {
        target: RigTarget,
        vertex: usize,
        bone_id: String,
    },
    RemoveWeight {
        target: RigTarget,
        vertex: usize,
        entry: usize,
    },
    ResetWeights {
        target: RigTarget,
        vertex: usize,
        radius: Option<f64>,
        softness: f64,
    },
    SetIk {
        root: RigRef,
        joint: RigRef,
        end: RigRef,
        flip: bool,
    },
    RemoveIk {
        end_key: String,
    },
    RemoveBone {
        bone_id: String,
    },
    RemoveBind {
        target: RigTarget,
    },
}

/// What an edit reports besides the document: the binds a pass made and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RigEditSummary {
    /// Binds made by `bindStroke` / `autoAssign` (the baseline's `n`).
    pub(crate) bound: u32,
    /// Records of an unsupported family `autoAssign` skipped (the baseline's `skippedUnsupported`).
    pub(crate) skipped_unsupported: u32,
    /// `autoAssign` was asked for meshes (the default) but no native raster record exists to bind.
    pub(crate) mesh_binds_unavailable: bool,
}

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedRigEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
    summary: RigEditSummary,
}

impl PreparedRigEdit {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn summary(&self) -> &RigEditSummary {
        &self.summary
    }
}

/// The Drawing guard of every sibling writer (`check_drawing_layer`): a rig edit writes the layer's
/// drawing data, so a locked layer refuses all of them (F06), and `autoAssign` also refuses an active
/// path effect (`rigAutoAssignLayer` runs behind `canEditActiveLayer` -> `editRefusalReason`: what is on
/// screen is the deformed geometry, so weights computed from the stored points would belong to a shape
/// nobody sees). A refusal is the typed `Unavailable`; nothing is published.
fn check_layer(
    document: &ObjectDocument,
    layer: &ObjectLayer,
    action: &RigAction,
) -> Result<(), DispatchErrorCode> {
    if document.layer_is_locked(layer.layer_uid()) {
        return Err(DispatchErrorCode::Unavailable);
    }
    if matches!(action, RigAction::AutoAssign { .. }) && layer_has_active_path_fx(layer) {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(())
}

/// Builds the proposed rig of `layer_uid` (and the summary) without touching the document.
fn apply(
    document: &ObjectDocument,
    uid: &str,
    rig: Option<&RigRecord>,
    action: &RigAction,
) -> Result<(RigRecord, RigEditSummary), DispatchErrorCode> {
    let need = || rig.ok_or(DispatchErrorCode::NotFound);
    let refused = PlanRefusal::code;
    let mut summary = RigEditSummary::default();
    let next = match action {
        RigAction::DrawBone {
            bone_id,
            segments,
            closed,
        } => {
            let base = rig.cloned().unwrap_or_else(RigRecord::empty);
            rules::draw_bone(&base, bone_id.as_deref(), segments.clone(), *closed)?
        }
        RigAction::MoveSegment {
            bone_id,
            vi,
            part,
            to,
            break_tangent,
        } => rules::move_segment(need()?, bone_id, *vi, *part, *to, *break_tangent)?,
        RigAction::SetBoneRadius { bone_id, radius } => {
            rules::set_radius(need()?, bone_id, *radius)?
        }
        RigAction::BindStroke {
            stroke_id,
            frame,
            bone_ids,
            radius,
            rotate,
            softness,
        } => {
            let planned = plan::plan_stroke_bind(
                document,
                uid,
                stroke_id,
                *frame,
                bone_ids.as_deref(),
                *radius,
                *softness,
            )
            .map_err(refused)?;
            summary.bound = 1;
            rules::apply_binds(need()?, &[planned], *rotate)?
        }
        RigAction::BindMesh => return Err(DispatchErrorCode::Unavailable),
        RigAction::AutoAssign {
            frame,
            default_radius,
            rotate,
            softness,
            paths,
            meshes,
        } => {
            let planned =
                plan::plan_auto_assign(document, uid, *frame, *default_radius, *softness, *paths)
                    .map_err(refused)?;
            summary.bound = planned.binds.len() as u32;
            summary.skipped_unsupported = planned.skipped_unsupported;
            summary.mesh_binds_unavailable = *meshes && planned.radius.is_some();
            rules::apply_binds(need()?, &planned.binds, *rotate)?
        }
        RigAction::SetWeight {
            target,
            vertex,
            entry,
            percent,
        } => rules::set_weight(need()?, target, *vertex, *entry, *percent)?,
        RigAction::AddWeight {
            target,
            vertex,
            bone_id,
        } => {
            let offset = plan::plan_add_weight(document, uid, target.bind(), *vertex, bone_id)
                .map_err(refused)?;
            rules::add_weight(need()?, target, *vertex, bone_id, offset)?
        }
        RigAction::RemoveWeight {
            target,
            vertex,
            entry,
        } => rules::remove_weight(need()?, target, *vertex, *entry)?,
        RigAction::ResetWeights {
            target,
            vertex,
            radius,
            softness,
        } => {
            let entries =
                plan::plan_reset_weights(document, uid, target.bind(), *vertex, *radius, *softness)
                    .map_err(refused)?;
            rules::replace_weights(need()?, target, *vertex, &entries)?
        }
        RigAction::SetIk {
            root,
            joint,
            end,
            flip,
        } => {
            let lengths = plan::plan_ik(
                document,
                uid,
                &root.kernel(),
                &joint.kernel(),
                &end.kernel(),
            )
            .map_err(refused)?;
            rules::set_ik(need()?, [root, joint, end], *flip, lengths)?
        }
        RigAction::RemoveIk { end_key } => rules::remove_ik(need()?, end_key)?,
        RigAction::RemoveBone { bone_id } => rules::remove_bone(need()?, bone_id)?,
        RigAction::RemoveBind { target } => rules::remove_bind(need()?, target)?,
    };
    Ok((next, summary))
}

impl ObjectFillOwner {
    /// Strict `object.rig.edit` preparation from this owner's current immutable snapshot.
    pub(crate) fn prepare_rig_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedRigEdit, DispatchErrorCode> {
        let (layer_uid, action) = parse::parse_edit(bytes)?;
        let origin = self.current_snapshot();
        let document = origin.document();
        let layer = document
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        check_layer(document, layer, &action)?;
        let (rig, summary) = apply(document, &layer_uid, layer.rig(), &action)?;
        let mut candidate = document.clone();
        if layer.rig() != Some(&rig) {
            let slot = candidate
                .layers
                .iter_mut()
                .find(|layer| layer.layer_uid() == layer_uid)
                .ok_or(DispatchErrorCode::Internal)?;
            slot.rig = Presence::Present(rig);
            ObjectSnapshot::validate_drawing(&candidate)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        let changed = &candidate != document;
        Ok(PreparedRigEdit {
            origin,
            document: candidate,
            changed,
            summary,
        })
    }

    /// Publishes the candidate only on the owner, document, snapshot and revision it was prepared from.
    pub(crate) fn commit_rig_edit(
        &mut self,
        candidate: PreparedRigEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}

#[cfg(test)]
#[path = "../tests/object_rig_edit_controls.rs"]
mod controls_tests;
