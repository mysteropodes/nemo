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
use std::collections::{HashMap, HashSet};

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    validate_clone_groups, IdSource, DUPLICATE_OFFSET_PX,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;
use crate::object_document::object_group_kernels::object_group_record::GroupTable;
use crate::object_document::object_group_kernels::object_selection_compound::resolve;
use crate::object_document::object_group_kernels::object_selection_compound_clones::apply_compound_clones;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    effective_frame, EditMode, HeldFramePromotion,
};
use crate::object_document::object_group_kernels::object_selection_edit_groups::{
    entries_of, tag_of,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    check_ids, check_target, layer_index, open_cell, SelectionError, SelectionPlan,
};
use crate::object_document::{
    object_deserialize, FrameScopeKind, ObjectDocument, ObjectRecord, Presence, RasterEntry,
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
    /// Existing brush companions, with no encoded media or external locator.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rasters: Presence<Vec<RasterEntry>>,
    /// Exact retained origin for packets that reference immutable asset bytes.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) source: Presence<ClipSource>,
}
object_deserialize!(ClipPayload {
    context_id: String,
    layer_uid: String,
    frame: u32,
    records: Vec<ObjectRecord>,
    #[serde(default)] groups: Presence<GroupTable>,
    #[serde(default)] rasters: Presence<Vec<RasterEntry>>,
    #[serde(default)] source: Presence<ClipSource>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ClipSource {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) stroke_ids: Vec<String>,
}
object_deserialize!(ClipSource {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    stroke_ids: Vec<String>
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
    let selected = resolve(document, context_id, layer_uid, shown, stroke_ids)?;
    let groups = match document.layers[index].groups.value() {
        Some(table) if table.context_id == context_id => Presence::Present(table.clone()),
        _ => Presence::Missing,
    };
    Ok(Some(ClipPayload {
        context_id: context_id.to_string(),
        layer_uid: layer_uid.to_string(),
        frame,
        records: selected.records,
        groups,
        rasters: if selected.rasters.is_empty() {
            Presence::Missing
        } else {
            Presence::Present(selected.rasters)
        },
        source: Presence::Missing,
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
            || clip
                .records
                .first()
                .is_some_and(|first| first.target.frame_scope.frame != target.frame_scope.frame)
        {
            return Err(SelectionError::ClipMismatch);
        }
        ids.push(target.stroke_id.clone());
    }
    check_ids(&ids)?;
    if let Some(table) = clip.groups.value() {
        validate_clip_groups(table, &clip.context_id, &ids)?;
    }
    Ok(())
}

/// A detached packet carries the WHOLE source table but only selected records.
/// Unselected stroke leaves intentionally drop during remap; group relationships
/// are complete in the packet and must resolve before any identities are minted.
fn validate_clip_groups(
    table: &GroupTable,
    context: &str,
    selected: &[String],
) -> Result<(), SelectionError> {
    let invalid = SelectionError::ClipTableUnreadable;
    if table.context_id != context {
        return Err(invalid);
    }
    let entries = entries_of(table).ok_or(SelectionError::ClipTableUnreadable)?;
    let tags: Vec<_> = entries
        .iter()
        .map(|entry| Some(entry.gid.as_str()))
        .collect();
    validate_clone_groups(&entries, &tags).map_err(SelectionError::Remap)?;
    let groups: HashMap<_, _> = table
        .entries
        .iter()
        .map(|entry| (entry.gid.as_str(), entry))
        .collect();
    let mut listed_by = HashMap::new();
    for entry in &table.entries {
        if entry.order.len() < 2 || selected.contains(&entry.gid) {
            return Err(invalid);
        }
        for id in &entry.order {
            if id.is_empty() {
                return Err(invalid);
            }
            if groups.contains_key(id.as_str())
                && listed_by.insert(id.as_str(), entry.gid.as_str()).is_some()
            {
                return Err(invalid);
            }
        }
    }
    for entry in &table.entries {
        if entry.parent.value().map(String::as_str) != listed_by.get(entry.gid.as_str()).copied() {
            return Err(invalid);
        }
        let mut parent = entry.parent.value();
        let mut depth = 0;
        while let Some(id) = parent {
            depth += 1;
            if depth > 16 {
                return Err(invalid);
            }
            parent = groups
                .get(id.as_str())
                .ok_or(SelectionError::ClipTableUnreadable)?
                .parent
                .value();
        }
    }
    let mut tagged = HashSet::new();
    for member in &table.members {
        if member.stroke_id.is_empty()
            || groups.contains_key(member.stroke_id.as_str())
            || !tagged.insert(member.stroke_id.as_str())
            || !groups
                .get(member.group_id.as_str())
                .is_some_and(|group| group.order.contains(&member.stroke_id))
        {
            return Err(invalid);
        }
    }
    Ok(())
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
    let clones = apply_compound_clones(
        &mut work,
        index,
        request.context_id,
        frame,
        &clip.records,
        clip.rasters.value().map_or(&[], Vec::as_slice),
        &tags,
        entries.as_deref(),
        offset,
        ids,
    )?;
    Ok(SelectionPlan::changed(
        document,
        work,
        frame,
        promoted,
        Vec::new(),
        clones.created,
        clones.created_groups,
        Vec::new(),
        Vec::new(),
    ))
}
