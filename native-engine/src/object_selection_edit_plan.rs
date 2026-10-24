//! Selection-edit plans (F08/F15, L02-H): delete and duplicate the selected
//! shapes of one layer cell as detached candidate documents over the existing
//! `ObjectDocument`, its order plan, property holders and group table.
//!
//! Mirrors `SM.deleteSelStrokes` (`src/js/timeline.js`) and `duplicateSelection`
//! / `_materializeClones` (`src/js/tools.js`) at fbe4b8a3. Every function reads
//! an immutable document and returns a [`SelectionPlan`] holding a complete
//! candidate (the input is never touched) or one typed refusal; there is no
//! partial result. Identity comes from the L02-C remap kernel through an
//! injected [`IdSource`]; this file owns no history, clock or randomness.
//! Storage admission of the candidate belongs to the transaction that seals the
//! plan against an origin snapshot. Lock decisions beyond "a locked layer is not
//! edited" belong to Root F06.
use std::collections::HashSet;
use crate::object_document::object_group_kernels::{object_selection_compound as compound, object_selection_compound_clones::apply_compound_clones};

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    IdSource, RemapError, DUPLICATE_OFFSET_PX,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;
use crate::object_document::object_group_kernels::object_group_plan::{
    plan_group_edit, GroupEdit, GroupPlanError,
};
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    drop_stroke_holders, in_cell, in_layer, remove_elements, resolve_frame, EditMode,
    HeldFramePromotion, PromotionRefusal, Resolved,
};
use crate::object_document::object_group_kernels::object_selection_edit_groups::{
    entries_of, tag_of,
};
use crate::object_document::{ObjectDocument, ObjectRecord};

/// Largest content revision an identity may embed (2^53 - 1).
pub(crate) const MAX_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectionError {
    UnknownLayer(String),
    /// The layer is locked; the lock policy is Root F06's.
    Locked(String),
    EmptyStrokeId,
    DuplicateStroke(String),
    FrameOutOfRange(u32),
    /// Motion mode, held frame with no key at or before it: the baseline stores nothing.
    NoKeyframe,
    Promotion(PromotionRefusal),
    /// The shape is not an authored record of the resolved cell.
    UnknownStroke(String),
    /// The destination layer's group table belongs to another context.
    TableContext,
    TableUnreadable(String),
    /// A group edit the plan itself builds was refused (not reachable for valid input).
    GroupEdit,
    Remap(RemapError),
    NonFiniteCoordinate,
    NeedTwoElements,
    /// The layer also holds records of another context; a split is per layer.
    MultiContext(String),
    EmptyLabel,
    RevisionOutOfRange,
    IdUnavailable,
    OrderPlanIncomplete(String),
    /// A clip record does not belong to the clip's own layer and context.
    ClipMismatch,
    ClipTableUnreadable,
}

/// Sealed exact before/after from a named document-layer selection plan.
/// No command, adapter or revision caller can construct a publication grant.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectionPublication {
    before: ObjectDocument,
    after: ObjectDocument,
}
impl SelectionPublication {
    fn new(before: &ObjectDocument, after: &ObjectDocument) -> Self {
        Self { before: before.clone(), after: after.clone() }
    }
    pub(crate) fn matches(&self, before: &ObjectDocument, after: &ObjectDocument) -> bool {
        self.before == *before && self.after == *after
    }
    pub(crate) fn before(&self) -> &ObjectDocument { &self.before }
    pub(crate) fn after(&self) -> &ObjectDocument { &self.after }
    pub(crate) fn into_document(self) -> ObjectDocument { self.after }
}

/// What an edit changed, for the typed in-process callers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectionPlan {
    publication: SelectionPublication,
    pub(crate) document: ObjectDocument,
    /// `false`: the candidate equals the input (an empty selection).
    pub(crate) changed: bool,
    /// The authored frame the edit read and wrote.
    pub(crate) content_frame: u32,
    /// An Animation 2D hold was promoted to a key first.
    pub(crate) promoted: bool,
    /// Deleted strokes, in selection order.
    pub(crate) removed: Vec<String>,
    /// New strokes, in selection order (the selection after a duplicate or paste).
    pub(crate) created: Vec<String>,
    /// New combine groups that survived settling, in table order.
    pub(crate) created_groups: Vec<String>,
    /// Groups of the origin dissolved because they fell below two entries.
    pub(crate) dissolved_groups: Vec<String>,
    /// A split's pieces, bottom to top, consecutive from the source slot.
    pub(crate) created_layers: Vec<String>,
}

impl SelectionPlan {
    pub(crate) fn publication(&self) -> SelectionPublication { self.publication.clone() }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn changed(before: &ObjectDocument, document: ObjectDocument,
        content_frame: u32, promoted: bool, removed: Vec<String>, created: Vec<String>,
        created_groups: Vec<String>, dissolved_groups: Vec<String>, created_layers: Vec<String>) -> Self {
        Self { publication: SelectionPublication::new(before, &document), document,
            changed: true, content_frame, promoted, removed, created, created_groups,
            dissolved_groups, created_layers }
    }
    pub(crate) fn unchanged(document: &ObjectDocument, frame: u32) -> Self {
        Self {
            publication: SelectionPublication::new(document, document),
            document: document.clone(),
            changed: false,
            content_frame: frame,
            promoted: false,
            removed: Vec::new(),
            created: Vec::new(),
            created_groups: Vec::new(),
            dissolved_groups: Vec::new(),
            created_layers: Vec::new(),
        }
    }
}

/// One selection: shapes of one layer cell, by stable stroke identity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectionRequest<'a> {
    pub(crate) context_id: &'a str,
    pub(crate) layer_uid: &'a str,
    /// The playhead frame (not necessarily the authored frame that holds the drawing).
    pub(crate) frame: u32,
    pub(crate) mode: EditMode,
    pub(crate) stroke_ids: &'a [String],
}

pub(crate) fn layer_index(
    document: &ObjectDocument,
    layer_uid: &str,
) -> Result<usize, SelectionError> {
    document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == layer_uid)
        .ok_or_else(|| SelectionError::UnknownLayer(layer_uid.to_string()))
}

/// The common refusals every edit makes before it reads any content.
pub(crate) fn check_target(
    document: &ObjectDocument,
    layer_uid: &str,
    frame: u32,
) -> Result<usize, SelectionError> {
    let index = layer_index(document, layer_uid)?;
    if document.layer_is_locked(layer_uid) {
        return Err(SelectionError::Locked(layer_uid.to_string()));
    }
    if frame >= document.total_frames {
        return Err(SelectionError::FrameOutOfRange(frame));
    }
    Ok(index)
}

pub(crate) fn check_ids(stroke_ids: &[String]) -> Result<(), SelectionError> {
    let mut seen: HashSet<&str> = HashSet::new();
    for id in stroke_ids {
        if id.is_empty() {
            return Err(SelectionError::EmptyStrokeId);
        }
        if !seen.insert(id.as_str()) {
            return Err(SelectionError::DuplicateStroke(id.clone()));
        }
    }
    Ok(())
}

/// Resolves the cell the edit writes (promoting a 2D hold through the port).
/// Returns the working document, the authored frame and whether it promoted.
pub(crate) fn open_cell(
    document: &ObjectDocument,
    index: usize,
    context_id: &str,
    frame: u32,
    mode: EditMode,
    promotion: &dyn HeldFramePromotion,
) -> Result<(ObjectDocument, u32, bool), SelectionError> {
    let layer = &document.layers[index];
    match resolve_frame(layer, context_id, frame, mode) {
        Resolved::At(cell) => Ok((document.clone(), cell, false)),
        Resolved::Promote(cell) => {
            let promoted = promotion
                .promote(document, &layer.layer_uid, cell)
                .map_err(SelectionError::Promotion)?;
            Ok((promoted, cell, true))
        }
        Resolved::NoKeyframe => Err(SelectionError::NoKeyframe),
    }
}

fn group_edit_error(error: GroupPlanError) -> SelectionError {
    match error {
        GroupPlanError::UnknownLayer(uid) => SelectionError::UnknownLayer(uid),
        GroupPlanError::UnknownStroke(id) => SelectionError::UnknownStroke(id),
        GroupPlanError::Locked(uid) => SelectionError::Locked(uid),
        GroupPlanError::ContextMismatch { .. } => SelectionError::TableContext,
        GroupPlanError::TableUnreadable(uid) => SelectionError::TableUnreadable(uid),
        GroupPlanError::IdCollidesWithStroke(_)
        | GroupPlanError::TooDeep
        | GroupPlanError::Kernel(_) => SelectionError::GroupEdit,
    }
}

pub(crate) fn find_record<'d>(
    document: &'d ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
    stroke_id: &str,
) -> Result<&'d ObjectRecord, SelectionError> {
    document
        .objects
        .iter()
        .find(|record| {
            in_cell(record, context_id, layer_uid, frame) && record.target.stroke_id == stroke_id
        })
        .ok_or_else(|| SelectionError::UnknownStroke(stroke_id.to_string()))
}

/// `deleteSelStrokes`: the selected shapes of one cell leave the document. A
/// shape that no longer exists in ANY authored frame of the layer also leaves its
/// holders and the group table (a group left with fewer than two entries
/// dissolves in the same plan); a shape that survives in another frame keeps both.
pub(crate) fn plan_delete(
    document: &ObjectDocument,
    request: &SelectionRequest<'_>,
    promotion: &dyn HeldFramePromotion,
) -> Result<SelectionPlan, SelectionError> {
    let index = check_target(document, request.layer_uid, request.frame)?;
    if request.stroke_ids.is_empty() {
        return Ok(SelectionPlan::unchanged(document, request.frame));
    }
    check_ids(request.stroke_ids)?;
    let (mut work, frame, promoted) = open_cell(
        document,
        index,
        request.context_id,
        request.frame,
        request.mode,
        promotion,
    )?;
    let (context, layer) = (request.context_id, request.layer_uid);
    let selection = compound::resolve(&work, context, layer, frame, request.stroke_ids)?;
    let all_ids: Vec<String> = selection.records.iter().map(|r|r.target.stroke_id.clone())
        .chain(selection.rasters.iter().map(|r|r.target.stroke_id.clone())).collect();
    let gone: Vec<String> = all_ids.iter()
        .filter(|id| {
            !work.objects.iter().any(|record| {
                in_layer(record, context, layer)
                    && record.target.frame_scope.frame != frame
                    && record.target.stroke_id == **id
            }) && !work.items.value().is_some_and(|items|items.targets().any(|target|
                target.context_id==context && target.layer_uid==layer && target.frame_scope.frame!=frame && target.stroke_id==**id))
        })
        .cloned()
        .collect();
    let mut dissolved_groups: Vec<String> = Vec::new();
    let has_table = work.layers[index]
        .groups
        .value()
        .is_some_and(|table| table.context_id == context);
    let gone_paths:Vec<String>=gone.iter().filter(|id|selection.records.iter().any(|r|r.target.stroke_id==**id)).cloned().collect();
    if has_table && !gone_paths.is_empty() {
        let edit = GroupEdit::Detach { strokes: gone_paths };
        let group_plan = plan_group_edit(&work, layer, context, &edit).map_err(group_edit_error)?;
        dissolved_groups = group_plan.removed;
        work = group_plan.document;
    }
    let ids: HashSet<&str> = all_ids.iter().map(String::as_str).collect();
    work.objects.retain(|record| {
        !(in_cell(record, context, layer, frame) && ids.contains(record.target.stroke_id.as_str()))
    });
    compound::remove_rasters(&mut work, &selection.rasters.iter().map(|r|r.target.clone()).collect::<Vec<_>>());
    remove_elements(&mut work, context, layer, frame, &ids);
    drop_stroke_holders(&mut work, context, layer, &gone);
    Ok(SelectionPlan::changed(document, work, frame, promoted, request.stroke_ids.to_vec(),
        Vec::new(), Vec::new(), dissolved_groups, Vec::new()))
}

/// `duplicateSelection`: copies of the selected shapes land on top of the same
/// layer cell, nudged by [`DUPLICATE_OFFSET_PX`], with fresh identities, fresh
/// combine groups for the groups the copies belong to, and the originals untouched.
pub(crate) fn plan_duplicate(
    document: &ObjectDocument,
    request: &SelectionRequest<'_>,
    promotion: &dyn HeldFramePromotion,
    ids: &mut dyn IdSource,
) -> Result<SelectionPlan, SelectionError> {
    let index = check_target(document, request.layer_uid, request.frame)?;
    if request.stroke_ids.is_empty() {
        return Ok(SelectionPlan::unchanged(document, request.frame));
    }
    check_ids(request.stroke_ids)?;
    let (mut work, frame, promoted) = open_cell(
        document,
        index,
        request.context_id,
        request.frame,
        request.mode,
        promotion,
    )?;
    let (context, layer) = (request.context_id, request.layer_uid);
    let selection = compound::resolve(&work, context, layer, frame, request.stroke_ids)?;
    let records = selection.records;
    let table = work.layers[index]
        .groups
        .value()
        .filter(|table| table.context_id == context)
        .cloned();
    let (tags, entries): (Vec<Option<String>>, Option<Vec<GroupEntry>>) = match &table {
        Some(table) => (
            records.iter().map(|record| tag_of(table, &record.target.stroke_id))
                .collect(),
            Some(
                entries_of(table)
                    .ok_or_else(|| SelectionError::TableUnreadable(layer.to_string()))?,
            ),
        ),
        None => (vec![None; records.len()], None),
    };
    let clones = apply_compound_clones(
        &mut work,
        index,
        context,
        frame,
        &records,
        &selection.rasters,
        &tags,
        entries.as_deref(),
        DUPLICATE_OFFSET_PX,
        ids,
    )?;
    Ok(SelectionPlan::changed(document, work, frame, promoted, Vec::new(),
        clones.created, clones.created_groups, Vec::new(), Vec::new()))
}
