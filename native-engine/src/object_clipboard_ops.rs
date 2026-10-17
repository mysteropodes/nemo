//! Sealed clipboard transactions (F15, L02-H): copy, cut and paste of selected
//! shapes through the existing revision owner. A private child of
//! `object_selection_edit_ops.rs`, which owns the sealed candidate type.
//!
//! Copy is a pure read of the owner's current snapshot into a detached
//! [`ClipPayload`]. Paste prepares a candidate from a clip and a destination;
//! cut prepares the clip and the delete candidate from ONE snapshot, so
//! committing the delete is the point of no return and a stale owner publishes
//! neither. Every refusal happens at preparation, before anything is published.
use crate::commands::object_fill_owner::selection_edit::wire::{
    clip_ok, edit_mode, parse, refusal_code, strokes_ok, target_ok, PastePayload, SelectionPayload,
};
use crate::commands::object_fill_owner::selection_edit::{
    PreparedSelectionEdit, SelectionEditKind,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_clipboard_plan::{
    copy_selection, plan_paste, ClipPayload, PasteRequest,
};
use crate::object_document::object_group_kernels::object_selection_edit_clones::RevisionIds;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    EditMode, UnavailablePromotion,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    plan_delete, SelectionRequest, MAX_REVISION,
};
use crate::request_receipts::DispatchErrorCode;

/// Where a clip lands, by stable identity (typed callers).
#[derive(Debug, Clone)]
pub(crate) struct PasteDestination {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) frame: u32,
    pub(crate) mode: EditMode,
}

impl ObjectFillOwner {
    /// `copySelection` from a strict payload; `None` for an empty selection.
    pub(crate) fn copy_selection_json(
        &self,
        bytes: &[u8],
    ) -> Result<Option<ClipPayload>, DispatchErrorCode> {
        let payload: SelectionPayload = parse(bytes)?;
        edit_mode(&payload.mode)?;
        if payload.command != "object.clipboard.copy"
            || !target_ok(&payload.context_id, &payload.layer_uid)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        copy_selection(
            origin.document(),
            &payload.context_id,
            &payload.layer_uid,
            payload.frame,
            &payload.stroke_ids,
        )
        .map_err(|error| refusal_code(&error))
    }

    /// `pasteSelection` of a typed clip.
    pub(crate) fn prepare_clipboard_paste(
        &self,
        clip: &ClipPayload,
        destination: &PasteDestination,
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        if !clip_ok(clip) || !target_ok(&destination.context_id, &destination.layer_uid) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let revision = origin
            .content_revision()
            .checked_add(1)
            .filter(|revision| *revision <= MAX_REVISION)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let mut ids = RevisionIds::new(revision, origin.document());
        let request = PasteRequest {
            context_id: &destination.context_id,
            layer_uid: &destination.layer_uid,
            frame: destination.frame,
            mode: destination.mode,
        };
        let plan = plan_paste(
            origin.document(),
            clip,
            &request,
            &UnavailablePromotion,
            &mut ids,
        );
        PreparedSelectionEdit::seal(origin, SelectionEditKind::Paste, plan)
    }

    /// `pasteSelection` from a strict payload carrying the clip inline.
    pub(crate) fn prepare_clipboard_paste_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        let payload: PastePayload = parse(bytes)?;
        let mode = edit_mode(&payload.mode)?;
        if payload.command != "object.clipboard.paste" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let destination = PasteDestination {
            context_id: payload.context_id,
            layer_uid: payload.layer_uid,
            frame: payload.frame,
            mode,
        };
        self.prepare_clipboard_paste(&payload.clip, &destination)
    }

    /// `cutSelection`: the clip and the delete candidate of the same snapshot.
    /// The clip is `None` (and the candidate a no-op) for an empty selection.
    pub(crate) fn prepare_clipboard_cut_json(
        &self,
        bytes: &[u8],
    ) -> Result<(Option<ClipPayload>, PreparedSelectionEdit), DispatchErrorCode> {
        let payload: SelectionPayload = parse(bytes)?;
        let mode = edit_mode(&payload.mode)?;
        if payload.command != "object.clipboard.cut"
            || !target_ok(&payload.context_id, &payload.layer_uid)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let clip = copy_selection(
            origin.document(),
            &payload.context_id,
            &payload.layer_uid,
            payload.frame,
            &payload.stroke_ids,
        )
        .map_err(|error| refusal_code(&error))?;
        let request = SelectionRequest {
            context_id: &payload.context_id,
            layer_uid: &payload.layer_uid,
            frame: payload.frame,
            mode,
            stroke_ids: &payload.stroke_ids,
        };
        let plan = plan_delete(origin.document(), &request, &UnavailablePromotion);
        let prepared = PreparedSelectionEdit::seal(origin, SelectionEditKind::Cut, plan)?;
        Ok((clip, prepared))
    }
}
