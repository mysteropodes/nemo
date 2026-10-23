//! Reset pose and commit pose (I13): the two Rig-panel buttons of `src/js/timeline.js` as ONE sealed
//! transaction each through the existing revision owner (`rigResetPose` and `rigCommitFrame` of
//! `src/js/app.js` at fbe4b8a3). Nothing here is a second writer, a deformation or a frame store.
//!
//! Operation `command.document.object.rig.pose`, payload `{command, layerUid, action}`:
//! - `{kind:"reset"}` (`rigResetPose`): every bone's live pose `segments` becomes a deep copy of its
//!   `restSegments`; the mesh offsets of every linked mesh bind are recomputed to zero (the baseline's
//!   own `applyRigDeform` at the rest pose writes exact zeros). A layer without a rig, and a rig whose
//!   pose already is the rest pose, change nothing: no revision, no history entry, redo kept.
//! - `{kind:"commit", frame}` (`rigCommitFrame`): the live pose is baked into the stored strokes of the
//!   frame. The order is the baseline lesson (`app.js` 2540-2546), kept as three phases that publish
//!   together or not at all: (1) keyframe promotion of the frame (`promote_cells` of the L05-A/B
//!   document kernel; a held cell becomes a key holding copies of the records it showed), (2) the pose
//!   evaluation of L03-P against the PROMOTED document, (3) replacement of the stored segments of every
//!   posed stroke. Rest rig data (`restSegments`, `binds`, `ikChains`, `nextId`) is never written.
//!
//! One typed preparation reads the pinned current snapshot, runs the phases on a detached copy,
//! re-admits the whole document (`validate_drawing`, which runs the rig admission of L03-N) and seals the
//! result to its origin (instance, document, snapshot, content revision). The commit is the existing
//! owner publish, atomic with history, request replay and cancellation in the shared dispatcher. Every
//! refusal, a stale or foreign pin, a malformed request and a cancelled one publish nothing.
//!
//! Deliberate differences from the baseline, each pinned by a test:
//! - the baseline commit with binds that all fail to relink still promotes the keyframe and reports
//!   success; here no stroke would be posed, so it is the typed `no_rigged_stroke` refusal and nothing is
//!   published (a promotion with no pose is not what the button promises);
//! - a mesh bind needs the image-mesh store and the raster rect (L03-J/L06 providers): a commit that meets
//!   one is the typed `pose_unavailable`; a reset needs only the store (it writes zeros);
//! - the baseline reset re-deforms the live objects and may throw midway; the record edit is complete
//!   before any evaluation, so the native reset has no such failure;
//! - locked layers and layers with an active path effect are refused (`canEditActiveLayer` ->
//!   `editRefusalReason`: `ObjectDocument::layer_is_locked`, `layer_has_active_path_fx`), after the baseline's own
//!   "no rig / no binds" checks, for the reset as well as the commit;
//! - a commit that would pose a pressure ribbon (`ObjectRecord::is_pressure_ribbon`) is the typed
//!   `pressure_ribbon_unavailable` refusal: the baseline poses the centreline too (`rigCommitFrame`), which
//!   no kernel here does, and writing only the outline would leave the centreline and width profile at rest;
//! - a layer without a timing record has no held cells: the frame is its own key and nothing is promoted.
//!
//! Not provided: the pose drag and IK dragging (L03-P kernels), the scene/export join (L03-S), the
//! linked-fill / centerline mirrors of a vector brush (L01 G14), selection, undo-checkpoint UI state
//! (`_rigPoseUndoPushed` is a runtime flag, history is the owner's) and the JS adapter.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    promote_cells, DocumentEdit, FrameEditError,
};
use crate::commands::object_fill_owner::path_effects_edit::layer_has_active_path_fx;
use crate::commands::ObjectFillOwner;
use crate::object_document::{ObjectDocument, ObjectLayer, Presence};
use crate::object_snapshot::object_rig_pose_read::{
    evaluate_layer_pose, PoseBind, PoseRefusal as ReadRefusal,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_rig_pose_commit_meshes.rs"]
mod meshes;
#[path = "object_rig_pose_commit_parse.rs"]
mod parse;
#[path = "object_rig_pose_commit_rules.rs"]
mod rules;

/// A validated action: nothing in it can fail again except the target lookup and the phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseAction {
    Reset,
    Commit { frame: u32 },
}

/// Why a pose transaction published nothing. Stable `reason()` strings are the typed contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseRefusal {
    /// The payload is not a valid `object.rig.pose` request.
    Malformed,
    LayerAbsent,
    /// The layer is locked (`canEditActiveLayer`).
    Locked,
    /// The layer has an active path effect (`canEditActiveLayer` -> `editRefusalReason`): what is on
    /// screen is the deformed geometry, so a pose written to the stored points would land under it.
    PathEffects,
    /// A posed stroke is a pressure ribbon: the commit poses `geometry.segments` only, so its centreline
    /// and width profile would stay at rest while the outline moves (the baseline poses them too).
    PressureRibbon,
    /// No rig, no binds, or no bind that poses a stroke at this frame (`toastNoRiggedStroke`).
    NoRiggedStroke,
    FrameOutOfRange,
    /// Keyframe promotion refused (unsupported timing family, admission of the promoted document).
    KeyframeUnavailable,
    /// The evaluation kernels or a provider (mesh store, raster rect) are not available.
    PoseUnavailable,
    /// The layer timing exposes no drawing at the frame.
    NotExposed,
    /// The stored rig cannot be evaluated (the baseline `applyRigDeform` threw).
    DeformFailed,
    /// An invariant of the engine failed (the candidate document was not admitted).
    Internal,
}

impl PoseRefusal {
    pub(crate) fn code(self) -> DispatchErrorCode {
        match self {
            Self::Malformed | Self::FrameOutOfRange | Self::DeformFailed => {
                DispatchErrorCode::InvalidRequest
            }
            Self::LayerAbsent | Self::NoRiggedStroke | Self::NotExposed => {
                DispatchErrorCode::NotFound
            }
            Self::Locked
            | Self::PathEffects
            | Self::PressureRibbon
            | Self::KeyframeUnavailable
            | Self::PoseUnavailable => DispatchErrorCode::Unavailable,
            Self::Internal => DispatchErrorCode::Internal,
        }
    }

    /// The stable machine-readable reason; `message()` starts with it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Malformed => "malformed_request",
            Self::LayerAbsent => "layer_absent",
            Self::Locked => "layer_locked",
            Self::PathEffects => "path_effects_active",
            Self::PressureRibbon => "pressure_ribbon_unavailable",
            Self::NoRiggedStroke => "no_rigged_stroke",
            Self::FrameOutOfRange => "frame_out_of_range",
            Self::KeyframeUnavailable => "keyframe_unavailable",
            Self::PoseUnavailable => "pose_unavailable",
            Self::NotExposed => "frame_not_exposed",
            Self::DeformFailed => "deform_failed",
            Self::Internal => "internal",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Malformed => "malformed_request: not a valid object.rig.pose request.",
            Self::LayerAbsent => "layer_absent: the layer is not in the document.",
            Self::Locked => "layer_locked: the layer is locked.",
            Self::PathEffects => "path_effects_active: the layer has an active path effect.",
            Self::PressureRibbon => {
                "pressure_ribbon_unavailable: a bound stroke is a pressure ribbon."
            }
            Self::NoRiggedStroke => "no_rigged_stroke: no bound stroke to pose at this frame.",
            Self::FrameOutOfRange => "frame_out_of_range: the frame is outside the document.",
            Self::KeyframeUnavailable => "keyframe_unavailable: the frame cannot become a key.",
            Self::PoseUnavailable => "pose_unavailable: the pose evaluation is not available.",
            Self::NotExposed => "frame_not_exposed: the layer shows no drawing at this frame.",
            Self::DeformFailed => "deform_failed: the stored rig cannot be evaluated.",
            Self::Internal => "internal: the posed document was not admitted.",
        }
    }
}

/// What a prepared pose reports besides the document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PoseReport {
    /// A commit promoted a held cell to a key.
    pub(crate) promoted: bool,
    /// Strokes whose segments the commit replaced.
    pub(crate) posed_strokes: u32,
    /// Binds the baseline `relinkRigBinds` would have dropped (kept in the record, never rewritten).
    pub(crate) unlinked_binds: u32,
    /// Mesh offset arrays a reset recomputed to zero.
    pub(crate) meshes_zeroed: u32,
    /// Mesh binds a reset could not reach (store absent, mesh absent or another vertex count).
    pub(crate) mesh_binds_unlinked: u32,
}

/// The origin and proposed document cannot be forged by callers.
#[derive(Debug, Clone)]
pub(crate) struct PreparedRigPose {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
    report: PoseReport,
}

impl PreparedRigPose {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn report(&self) -> &PoseReport {
        &self.report
    }
}

fn layer_index(document: &ObjectDocument, uid: &str) -> Result<usize, PoseRefusal> {
    document
        .layers()
        .iter()
        .position(|layer| layer.layer_uid() == uid)
        .ok_or(PoseRefusal::LayerAbsent)
}

/// The one exit for a changed document: the shared strict drawing admission runs on the complete result.
fn admit(
    document: &ObjectDocument,
    candidate: ObjectDocument,
    report: PoseReport,
) -> Result<(ObjectDocument, PoseReport), PoseRefusal> {
    if &candidate != document {
        ObjectSnapshot::validate_drawing(&candidate).map_err(|_| PoseRefusal::Internal)?;
    }
    Ok((candidate, report))
}

/// `canEditActiveLayer()` as far as the document records it: the lock, then an active path effect.
fn check_editable(document: &ObjectDocument, layer: &ObjectLayer) -> Result<(), PoseRefusal> {
    if document.layer_is_locked(layer.layer_uid()) {
        return Err(PoseRefusal::Locked);
    }
    if layer_has_active_path_fx(layer) {
        return Err(PoseRefusal::PathEffects);
    }
    Ok(())
}

/// `rigResetPose` as the Reset button runs it: no rig is a silent return, then the edit guard.
fn plan_reset(
    document: &ObjectDocument,
    uid: &str,
) -> Result<(ObjectDocument, PoseReport), PoseRefusal> {
    let index = layer_index(document, uid)?;
    let layer = &document.layers()[index];
    let rig = match layer.rig() {
        Some(rig) => rig,
        None => return Ok((document.clone(), PoseReport::default())),
    };
    check_editable(document, layer)?;
    let mut candidate = document.clone();
    if let Some(next) = rules::rest_rig(rig) {
        candidate.layers[index].rig = Presence::Present(next);
    }
    let counts = meshes::zero_pose_offsets(&mut candidate, rig)?;
    let report = PoseReport {
        meshes_zeroed: counts.zeroed,
        mesh_binds_unlinked: counts.unlinked,
        ..PoseReport::default()
    };
    admit(document, candidate, report)
}

fn promotion_refusal(error: FrameEditError) -> PoseRefusal {
    match error {
        FrameEditError::UnknownLayer => PoseRefusal::LayerAbsent,
        FrameEditError::Invalid
        | FrameEditError::Unsupported
        | FrameEditError::MissingTiming
        | FrameEditError::PreservedOutOfRange => PoseRefusal::KeyframeUnavailable,
    }
}

fn read_refusal(refusal: ReadRefusal) -> PoseRefusal {
    match refusal {
        ReadRefusal::Unavailable => PoseRefusal::PoseUnavailable,
        ReadRefusal::FrameOutOfRange => PoseRefusal::FrameOutOfRange,
        ReadRefusal::LayerAbsent => PoseRefusal::LayerAbsent,
        ReadRefusal::NoRig => PoseRefusal::NoRiggedStroke,
        ReadRefusal::NotExposed => PoseRefusal::NotExposed,
        ReadRefusal::Deform => PoseRefusal::DeformFailed,
        ReadRefusal::Pin => PoseRefusal::Internal,
    }
}

/// `rigCommitFrame`: promotion, then deform, then replace; all three or nothing.
fn plan_commit(
    origin: &ObjectSnapshot,
    uid: &str,
    frame: u32,
) -> Result<(ObjectDocument, PoseReport), PoseRefusal> {
    let document = origin.document();
    let index = layer_index(document, uid)?;
    let layer = &document.layers()[index];
    // `!ld.rig || !ld.rig.binds.length` comes first, then `canEditActiveLayer()`.
    if layer.rig().map_or(true, |rig| rig.binds.is_empty()) {
        return Err(PoseRefusal::NoRiggedStroke);
    }
    check_editable(document, layer)?;
    if frame >= document.total_frames() {
        return Err(PoseRefusal::FrameOutOfRange);
    }
    // Phase 1: ensureKeyframe. A layer without timing has no held cells: the frame is its own key.
    let mut report = PoseReport::default();
    let promoted_document = if layer.timing.value().is_some() {
        match promote_cells(document, uid, &[frame]).map_err(promotion_refusal)? {
            DocumentEdit::Changed(next) => {
                report.promoted = true;
                next
            }
            DocumentEdit::NoChange => document.clone(),
        }
    } else {
        document.clone()
    };
    // Phase 2: applyRigDeform, on the document the promotion produced (a fresh pinned snapshot of it).
    let promoted_snapshot = ObjectSnapshot::new(origin.instance_id(), promoted_document.clone())
        .map_err(|_| PoseRefusal::Internal)?;
    let pose = evaluate_layer_pose(&promoted_snapshot, uid, frame).map_err(read_refusal)?;
    if pose.binds.contains(&PoseBind::Unavailable) {
        return Err(PoseRefusal::PoseUnavailable);
    }
    if pose.strokes.is_empty() {
        return Err(PoseRefusal::NoRiggedStroke);
    }
    report.unlinked_binds = pose
        .binds
        .iter()
        .filter(|bind| **bind == PoseBind::Unlinked)
        .count() as u32;
    report.posed_strokes = pose.strokes.len() as u32;
    // Phase 3: saveActiveLayerFrame -- the posed segments replace the stored ones of the promoted key.
    let mut candidate = promoted_document;
    rules::replace_segments(&mut candidate, uid, pose.source_frame, &pose.strokes)?;
    admit(document, candidate, report)
}

impl ObjectFillOwner {
    /// Strict `object.rig.pose` preparation from this owner's current immutable snapshot.
    pub(crate) fn prepare_rig_pose_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedRigPose, PoseRefusal> {
        let (layer_uid, action) = parse::parse_pose(bytes)?;
        let origin = self.current_snapshot();
        let (document, report) = match action {
            PoseAction::Reset => plan_reset(origin.document(), &layer_uid)?,
            PoseAction::Commit { frame } => plan_commit(&origin, &layer_uid, frame)?,
        };
        let changed = &document != origin.document();
        Ok(PreparedRigPose {
            origin,
            document,
            changed,
            report,
        })
    }

    /// Publishes the candidate only on the owner, document, snapshot and revision it was prepared from.
    pub(crate) fn commit_rig_pose(
        &mut self,
        candidate: PreparedRigPose,
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
#[path = "../tests/object_rig_pose_commit_controls.rs"]
mod controls_tests;
