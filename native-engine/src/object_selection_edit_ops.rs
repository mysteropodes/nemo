//! Sealed selection-edit transactions (F08, L02-H): delete the selected
//! shapes, duplicate them, and split a layer into one layer per element, through
//! the existing revision owner. Declared as a private child of
//! `object_fill_owner.rs`, next to the layer lifecycle / group / property
//! preparations it mirrors; the clipboard operations (F15) are its child.
//!
//! Preparation parses a bounded, strict payload (object-only, duplicate and
//! unknown members refused), reads the owner's current immutable snapshot, asks
//! the document-layer plan for a detached candidate, runs the same storage
//! admission layer-create runs and seals the result to that snapshot. Commit
//! re-checks instance, document and revision exactly like `commit_layer_create`;
//! a plan that changes nothing publishes nothing. No history, receipt or clock
//! lives here: fresh identities are named after the revision the edit publishes
//! and the one history entry comes from `NativeObjectHistory`. The Animation 2D
//! hold promotion uses the existing L05 frame kernel inside the same detached
//! candidate, so promotion and the selected edit publish together.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    clone_cell, drop_cells,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_selection_edit_clones::RevisionIds;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    HeldFramePromotion, PromotionRefusal,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    plan_delete, plan_duplicate, SelectionError, SelectionPlan, SelectionRequest, MAX_REVISION,
};
use crate::object_document::object_group_kernels::object_selection_edit_split::{
    plan_split, SplitLabels, SplitRequest,
};
use crate::object_document::{FrameScopeKind, ItemRecords, ObjectDocument, Presence, TimingEdit};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_clipboard_ops.rs"]
mod clipboard;
pub(crate) use clipboard::PasteDestination;
#[path = "object_selection_edit_payload.rs"]
mod wire;
use wire::{
    edit_mode, labels_ok, parse, refusal_code, strokes_ok, target_ok, SelectionPayload,
    SplitPayload,
};

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_selection_companion_owner.rs"]
mod companion_owner_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionEditKind {
    Delete,
    Duplicate,
    Split,
    Paste,
    Cut,
}

/// The real frame provider; it prepares content and timing without publishing.
/// The selection plan and the existing revision/history owner commit both once.
struct RealHeldFramePromotion;

impl HeldFramePromotion for RealHeldFramePromotion {
    fn promote(
        &self,
        document: &ObjectDocument,
        layer_uid: &str,
        frame: u32,
    ) -> Result<ObjectDocument, PromotionRefusal> {
        let index = document.layers.iter().position(|layer| layer.layer_uid == layer_uid)
            .ok_or(PromotionRefusal::Invalid)?;
        let timing = document.layers[index].timing.value().ok_or(PromotionRefusal::Unavailable)?;
        let (kind, source, _) = timing.exposure(frame);
        let TimingEdit::Changed(promoted) = timing.promote_cell(frame, document.total_frames)
            .map_err(|_| PromotionRefusal::Invalid)? else { return Err(PromotionRefusal::Invalid); };
        let mut next = document.clone();
        next.layers[index].timing = Presence::Present(promoted);
        if kind != "interpolated" {
            // The existing L05 operations own path/timing/order behavior. Join the
            // same cell's item records before final admission, including unselected
            // items, so promoting a hold cannot split a compound or lose content.
            drop_cells(&mut next, layer_uid, &|at| at == frame);
            if let Some(from) = source { clone_cell(&mut next, layer_uid, from, frame); }
            if let Some(items) = document.items.value() {
                let in_cell = |target: &crate::object_document::ObjectTarget, at| {
                    target.context_id == "scene-root" && target.layer_uid == layer_uid
                        && target.frame_scope.kind == FrameScopeKind::Authored && target.frame_scope.frame == at
                };
                let mut rasters = items.raster_entries().iter().filter(|e| !in_cell(&e.target, frame)).cloned().collect::<Vec<_>>();
                let mut text = items.text_entries().iter().filter(|e| !in_cell(&e.target, frame)).cloned().collect::<Vec<_>>();
                if let Some(from) = source {
                    rasters.extend(items.raster_entries().iter().filter(|e| in_cell(&e.target, from)).cloned().map(|mut e| {
                        e.target.frame_scope.frame = frame; e
                    }));
                    text.extend(items.text_entries().iter().filter(|e| in_cell(&e.target, from)).cloned().map(|mut e| {
                        e.target.frame_scope.frame = frame; e
                    }));
                }
                next.items = ItemRecords::normalized(rasters, text);
            }
        }
        ObjectSnapshot::validate_drawing(&next).map_err(|_| PromotionRefusal::Invalid)?;
        Ok(next)
    }
}

/// A detached candidate sealed to the snapshot it was planned from. Only this
/// module (and its clipboard child) can create one or replace its origin or
/// after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedSelectionEdit {
    origin: ObjectSnapshot,
    plan: SelectionPlan,
    kind: SelectionEditKind,
}

// Read by the typed in-process callers (tests today, the L02-J client port
// next); the compact receipt carries only applied/historyEntriesAdded.
#[allow(dead_code)]
impl PreparedSelectionEdit {
    pub(crate) fn kind(&self) -> SelectionEditKind {
        self.kind
    }
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
    /// The authored frame that held the drawing.
    pub(crate) fn content_frame(&self) -> u32 {
        self.plan.content_frame
    }
    /// An Animation 2D hold was promoted to a key inside this edit.
    pub(crate) fn promoted(&self) -> bool {
        self.plan.promoted
    }
    /// Deleted shapes, in selection order.
    pub(crate) fn removed(&self) -> &[String] {
        &self.plan.removed
    }
    /// New shapes, in selection order: the selection after a duplicate or paste.
    pub(crate) fn created(&self) -> &[String] {
        &self.plan.created
    }
    pub(crate) fn created_groups(&self) -> &[String] {
        &self.plan.created_groups
    }
    pub(crate) fn dissolved_groups(&self) -> &[String] {
        &self.plan.dissolved_groups
    }
    /// A split's pieces, bottom to top; the last one keeps the source uid.
    pub(crate) fn created_layers(&self) -> &[String] {
        &self.plan.created_layers
    }
}

impl PreparedSelectionEdit {
    fn seal(
        origin: ObjectSnapshot,
        kind: SelectionEditKind,
        plan: Result<SelectionPlan, SelectionError>,
    ) -> Result<Self, DispatchErrorCode> {
        let plan = plan.map_err(|error| refusal_code(&error))?;
        if !plan.publication().matches(origin.document(), &plan.document) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if plan.changed {
            ObjectSnapshot::validate_drawing(&plan.document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(Self { origin, plan, kind })
    }
}

/// The revision the edit will publish, bounded like the layer identities.
fn next_revision(origin: &ObjectSnapshot) -> Result<u64, DispatchErrorCode> {
    origin
        .content_revision()
        .checked_add(1)
        .filter(|revision| *revision <= MAX_REVISION)
        .ok_or(DispatchErrorCode::Unavailable)
}

impl ObjectFillOwner {
    pub(crate) fn prepare_selection_delete_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        let payload: SelectionPayload = parse(bytes)?;
        let mode = edit_mode(&payload.mode)?;
        if payload.command != "object.selection.delete"
            || !target_ok(&payload.context_id, &payload.layer_uid)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let request = SelectionRequest {
            context_id: &payload.context_id,
            layer_uid: &payload.layer_uid,
            frame: payload.frame,
            mode,
            stroke_ids: &payload.stroke_ids,
        };
        let origin = self.current_snapshot();
        let plan = plan_delete(origin.document(), &request, &RealHeldFramePromotion);
        PreparedSelectionEdit::seal(origin, SelectionEditKind::Delete, plan)
    }

    pub(crate) fn prepare_selection_duplicate_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        let payload: SelectionPayload = parse(bytes)?;
        let mode = edit_mode(&payload.mode)?;
        if payload.command != "object.selection.duplicate"
            || !target_ok(&payload.context_id, &payload.layer_uid)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let request = SelectionRequest {
            context_id: &payload.context_id,
            layer_uid: &payload.layer_uid,
            frame: payload.frame,
            mode,
            stroke_ids: &payload.stroke_ids,
        };
        let origin = self.current_snapshot();
        let mut ids = RevisionIds::new(next_revision(&origin)?, origin.document());
        let plan = plan_duplicate(
            origin.document(),
            &request,
            &RealHeldFramePromotion,
            &mut ids,
        );
        PreparedSelectionEdit::seal(origin, SelectionEditKind::Duplicate, plan)
    }

    pub(crate) fn prepare_layer_split_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        let payload: SplitPayload = parse(bytes)?;
        if payload.command != "object.layer.split"
            || !target_ok(&payload.context_id, &payload.layer_uid)
            || !labels_ok(&payload.labels)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let request = SplitRequest {
            context_id: &payload.context_id,
            layer_uid: &payload.layer_uid,
            frame: payload.frame,
            labels: SplitLabels {
                shape: &payload.labels.shape,
                stroke: &payload.labels.stroke,
            },
            revision: next_revision(&origin)?,
        };
        let plan = plan_split(origin.document(), &request);
        PreparedSelectionEdit::seal(origin, SelectionEditKind::Split, plan)
    }

    pub(crate) fn commit_selection_edit(
        &mut self,
        candidate: PreparedSelectionEdit,
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
        if !candidate.plan.changed {
            return Ok(self.content_revision());
        }
        let proof = candidate.plan.publication();
        if !proof.matches(current.document(), &candidate.plan.document) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        self.revisions
            .publish_selection(&candidate.origin, proof)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
