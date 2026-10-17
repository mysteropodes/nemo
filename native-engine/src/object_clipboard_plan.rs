//! Clipboard plans (F15, L02-H): copy of the selected shapes into a detached,
//! self-contained [`ClipPayload`] and paste of that payload into any layer cell,
//! as detached candidate documents. Cut is copy plus the delete plan of
//! `object_selection_edit_plan.rs`, sealed to one snapshot by the transaction.
//!
//! Mirrors `copySelection` / `pasteSelection` / `_snapshotForClone` /
//! `_materializeClones` / `_remapCombineGroups` (`src/js/tools.js`, fbe4b8a3).
//! The clip carries copies of the selected records (their original identities
//! are the sources of the remap) and the source layer's whole group table, like
//! the baseline's `_canvasClip.groups`, so a paste needs nothing from the source
//! layer, which may have changed or vanished. Every reference of a clip is
//! decided before the first identity is minted: the strict record decoding turns
//! a member this document has no record for (a mesh, rig or component reference)
//! into a refusal, the L02-C kernel refuses a malformed group table, and a paste
//! that cannot complete returns a typed refusal and no candidate.
use serde::{Deserialize, Serialize};

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    IdSource, DUPLICATE_OFFSET_PX,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;
use crate::object_document::object_group_kernels::object_group_record::GroupTable;
use crate::object_document::object_group_kernels::object_selection_edit_clones::apply_clones;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    effective_frame, EditMode, HeldFramePromotion,
};
use crate::object_document::object_group_kernels::object_selection_edit_groups::{
    entries_of, tag_of,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    check_ids, check_target, find_record, layer_index, open_cell, SelectionError, SelectionPlan,
};
use crate::object_document::{
    object_deserialize, FrameScopeKind, ObjectDocument, ObjectRecord, Presence,
};

/// A copied selection: where it came from and what it was.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ClipPayload {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    /// The playhead frame at copy time (`_canvasClip.frameIdx`); the records
    /// carry the authored frame that actually held the drawing.
    pub(crate) frame: u32,
    /// The selected records in selection order (the clone order).
    pub(crate) records: Vec<ObjectRecord>,
    /// The source layer's whole group table, when it had one in this context.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) groups: Presence<GroupTable>,
}
object_deserialize!(ClipPayload {
    context_id: String,
    layer_uid: String,
    frame: u32,
    records: Vec<ObjectRecord>,
    #[serde(default)] groups: Presence<GroupTable>
});

/// `copySelection`: `None` for an empty selection (nothing is copied). Reading
/// is not editing, so a locked layer can be copied from; a hold shows its key.
pub(crate) fn copy_selection(
    document: &ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
    stroke_ids: &[String],
) -> Result<Option<ClipPayload>, SelectionError> {
    let index = layer_index(document, layer_uid)?;
    if frame >= document.total_frames {
        return Err(SelectionError::FrameOutOfRange(frame));
    }
    if stroke_ids.is_empty() {
        return Ok(None);
    }
    check_ids(stroke_ids)?;
    let shown = effective_frame(&document.layers[index], context_id, frame)
        .ok_or(SelectionError::NoKeyframe)?;
    let mut records: Vec<ObjectRecord> = Vec::with_capacity(stroke_ids.len());
    for id in stroke_ids {
        records.push(find_record(document, context_id, layer_uid, shown, id)?.clone());
    }
    let groups = match document.layers[index].groups.value() {
        Some(table) if table.context_id == context_id => Presence::Present(table.clone()),
        _ => Presence::Missing,
    };
    Ok(Some(ClipPayload {
        context_id: context_id.to_string(),
        layer_uid: layer_uid.to_string(),
        frame,
        records,
        groups,
    }))
}

/// Where a clip lands.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PasteRequest<'a> {
    pub(crate) context_id: &'a str,
    pub(crate) layer_uid: &'a str,
    pub(crate) frame: u32,
    pub(crate) mode: EditMode,
}

fn validate_clip(clip: &ClipPayload) -> Result<(), SelectionError> {
    let mut ids: Vec<String> = Vec::with_capacity(clip.records.len());
    for record in &clip.records {
        let target = &record.target;
        if target.frame_scope.kind != FrameScopeKind::Authored
            || target.context_id != clip.context_id
            || target.layer_uid != clip.layer_uid
        {
            return Err(SelectionError::ClipMismatch);
        }
        ids.push(target.stroke_id.clone());
    }
    check_ids(&ids)
}

/// `pasteSelection`: copies of the clip land on top of the destination cell, 12 px
/// over the source only when destination and source are the same layer and
/// frame. A hold is handled by mode (2D promotes through the port, Motion writes
/// into the key it inherits from) exactly once. An empty clip changes nothing.
pub(crate) fn plan_paste(
    document: &ObjectDocument,
    clip: &ClipPayload,
    request: &PasteRequest<'_>,
    promotion: &dyn HeldFramePromotion,
    ids: &mut dyn IdSource,
) -> Result<SelectionPlan, SelectionError> {
    let index = check_target(document, request.layer_uid, request.frame)?;
    if clip.records.is_empty() {
        return Ok(SelectionPlan::unchanged(document, request.frame));
    }
    validate_clip(clip)?;
    let (tags, entries): (Vec<Option<String>>, Option<Vec<GroupEntry>>) = match clip.groups.value()
    {
        Some(table) => (
            clip.records
                .iter()
                .map(|record| tag_of(table, &record.target.stroke_id))
                .collect(),
            Some(entries_of(table).ok_or(SelectionError::ClipTableUnreadable)?),
        ),
        None => (vec![None; clip.records.len()], None),
    };
    let (mut work, frame, promoted) = open_cell(
        document,
        index,
        request.context_id,
        request.frame,
        request.mode,
        promotion,
    )?;
    let same_place = clip.layer_uid == request.layer_uid
        && clip.context_id == request.context_id
        && clip.frame == request.frame;
    let offset = if same_place { DUPLICATE_OFFSET_PX } else { 0.0 };
    let clones = apply_clones(
        &mut work,
        index,
        request.context_id,
        frame,
        &clip.records,
        &tags,
        entries.as_deref(),
        offset,
        ids,
    )?;
    Ok(SelectionPlan {
        document: work,
        changed: true,
        content_frame: frame,
        promoted,
        removed: Vec::new(),
        created: clones.created,
        created_groups: clones.created_groups,
        dissolved_groups: Vec::new(),
        created_layers: Vec::new(),
    })
}
