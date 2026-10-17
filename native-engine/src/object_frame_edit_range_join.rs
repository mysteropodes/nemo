//! F11 (L05-G): the join of the layer range records into the frame-structure transactions (L05-C). The
//! dense editor moves `inPoint`, `outPoint`, layer markers and the composition markers with the frames
//! only in two operations, F5 and single-frame removal (src/js/app.js `insertFrame`, `removeFrame`;
//! `removeFrameSpan`, src/js/timeline.js `extendExposure` and `duplicateKeyframe` move none of them);
//! `object_frame_edit_ranges.rs` (L05-F) is that arithmetic and this file applies it to the stored
//! `ObjectLayer.range` of every layer of the already edited document.
//!
//! The edited document comes from the structure transaction; this file only rewrites `range` members and
//! hands the document back (the caller revalidates it, which admits the new range values). A layer with
//! no `range` is left without one: nothing is defaulted. `timeLink` and its Motion offsets never change
//! here, every marker keeps its name and color, and F5 shifts only the layers that really received the
//! slot (`targeted`, decided by the caller from selection and lock policy). Input is consumed whole and
//! returned whole, so a typed refusal cannot leave a partly shifted document.
//!
//! Decision recorded for the lead: the native remove operation names `from..=to`. One frame is the
//! baseline `removeFrame` (shifts ranges); two or more are `removeFrameSpan` (shifts none, baseline
//! quirk kept). The baseline also routes a one-cell timeline SELECTION through `removeFrameSpan`; that
//! single case is not reachable natively and deliberately follows the shifting single-frame removal.
//!
//! Not stored natively yet, so reported as typed `RangeUnavailable` and never faked: the composition
//! markers (`state.markers`, a pure port is `shift_marker_records`) and the work-area end (`waOut`, the
//! kernel's `work_area_out`).
use crate::object_document::{
    marker_bounds, markers_with_bounds, FrameEditKind, LayerRole, MarkerRecord, ObjectDocument,
    Presence, RangeEdit, RangeEditError, RangeRecordError,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// One structure edit, in the terms of the transaction that performed it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum StructureEdit<'a> {
    /// F5 at `cell`; `targeted` are the layers that really received the slot (selected and unlocked).
    Insert { cell: u32, targeted: &'a [&'a str] },
    /// Extend exposure: `count` slots after `cell` in every layer.
    Extend { cell: u32, count: u32 },
    /// Duplicate keyframe: one slot after `cell` in every layer.
    Duplicate { cell: u32 },
    /// Remove `from..=to` from every layer.
    Remove { from: u32, to: u32 },
}

/// A baseline side effect of a frame edit that has no native record yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeUnavailable {
    /// `state.markers` shifts with F5 and single-frame removal; no native store holds them.
    CompositionMarkers,
    /// `state.waOut` follows every one of these edits; the work area is not a document record.
    WorkAreaEnd,
}

/// Typed refusals; no partial document accompanies any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeJoinError {
    /// The kernel refused the edit (cursor, span, emptying the timeline, overflow).
    Edit(RangeEditError),
    /// A kernel result could not be written back onto a stored record.
    Record(RangeRecordError),
    /// The edited document does not have the frame count the edit implies.
    TotalMismatch,
}

/// The joined document and what was left to later owners.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RangeJoin {
    pub(crate) document: ObjectDocument,
    /// Layers whose `range` member changed.
    pub(crate) layers_changed: usize,
    pub(crate) pending: &'static [RangeUnavailable],
}

/// The kernel edit a structure edit performs on a `total`-frame timeline, or the kernel's refusal.
pub(crate) fn range_edit_for(
    edit: &StructureEdit<'_>,
    total: u32,
) -> Result<RangeEdit, RangeJoinError> {
    let (kind, cursor) = match edit {
        StructureEdit::Insert { cell, .. } => (FrameEditKind::InsertFrame, *cell),
        StructureEdit::Extend { cell, count } => {
            (FrameEditKind::ExtendExposure { count: *count }, *cell)
        }
        StructureEdit::Duplicate { cell } => (FrameEditKind::DuplicateKeyframe, *cell),
        StructureEdit::Remove { from, to } => {
            if to < from {
                return Err(RangeJoinError::Edit(RangeEditError::SpanOutsideTimeline));
            }
            if from == to {
                (FrameEditKind::RemoveFrame, *from)
            } else {
                let count = (to - from)
                    .checked_add(1)
                    .ok_or(RangeJoinError::Edit(RangeEditError::Overflow))?;
                (FrameEditKind::RemoveFrameSpan { count }, *from)
            }
        }
    };
    let range_edit = RangeEdit {
        kind,
        cursor,
        total,
    };
    range_edit.new_total().map_err(RangeJoinError::Edit)?;
    Ok(range_edit)
}

/// What a kernel edit leaves to other owners.
pub(crate) fn pending_for(edit: &RangeEdit) -> &'static [RangeUnavailable] {
    if edit.kind.shift_ranges() {
        &[
            RangeUnavailable::CompositionMarkers,
            RangeUnavailable::WorkAreaEnd,
        ]
    } else {
        &[RangeUnavailable::WorkAreaEnd]
    }
}

/// Apply `range_edit` to the `range` member of every layer of `after`, the document the structure
/// transaction produced. `targeted` names the layers that received the F5 slot (empty otherwise).
pub(crate) fn join_with_edit(
    mut after: ObjectDocument,
    range_edit: &RangeEdit,
    targeted: &[&str],
) -> Result<RangeJoin, RangeJoinError> {
    let new_total = range_edit.new_total().map_err(RangeJoinError::Edit)?;
    if after.total_frames != new_total {
        return Err(RangeJoinError::TotalMismatch);
    }
    let mut layers_changed = 0;
    for layer in &mut after.layers {
        let next = match &layer.range {
            Presence::Missing => None,
            Presence::Present(range) => {
                let role = if targeted.iter().any(|uid| *uid == layer.layer_uid.as_str()) {
                    LayerRole::Targeted
                } else {
                    LayerRole::Padded
                };
                let moved = range_edit
                    .layer_ranges(role, &range.to_ranges())
                    .map_err(RangeJoinError::Edit)?;
                let next = range.with_ranges(&moved).map_err(RangeJoinError::Record)?;
                if next == *range {
                    None
                } else {
                    Some(next)
                }
            }
        };
        if let Some(next) = next {
            layer.range = Presence::Present(next);
            layers_changed += 1;
        }
    }
    Ok(RangeJoin {
        document: after,
        layers_changed,
        pending: pending_for(range_edit),
    })
}

/// `join_with_edit` for the edit a structure transaction performed on a `total_before`-frame timeline.
pub(crate) fn join_structure_ranges(
    after: ObjectDocument,
    total_before: u32,
    edit: &StructureEdit<'_>,
) -> Result<RangeJoin, RangeJoinError> {
    let range_edit = range_edit_for(edit, total_before)?;
    let targeted: &[&str] = match edit {
        StructureEdit::Insert { targeted, .. } => *targeted,
        _ => &[],
    };
    join_with_edit(after, &range_edit, targeted)
}

/// The exit the structure transaction uses: join the ranges into the edited document, map a typed
/// refusal to the dispatcher's codes (a kernel refusal is an invalid request, a record or frame-count
/// disagreement an internal fault) and run the shared strict drawing admission again when a range
/// really moved, so a shifted value that leaves the admitted bounds is refused instead of published.
pub(crate) fn join_document_ranges(
    after: ObjectDocument,
    total_before: u32,
    edit: &StructureEdit<'_>,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let joined = join_structure_ranges(after, total_before, edit).map_err(|error| match error {
        RangeJoinError::Edit(_) => DispatchErrorCode::InvalidRequest,
        RangeJoinError::Record(_) | RangeJoinError::TotalMismatch => DispatchErrorCode::Internal,
    })?;
    if joined.layers_changed > 0 {
        ObjectSnapshot::validate_drawing(&joined.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
    }
    Ok(joined.document)
}

/// Port for the composition markers once a native store holds them: the same arithmetic as a targeted
/// layer, authored text kept.
pub(crate) fn shift_marker_records(
    edit: &RangeEdit,
    markers: &[MarkerRecord],
) -> Result<Vec<MarkerRecord>, RangeJoinError> {
    let shifted = edit
        .comp_markers(&Some(marker_bounds(markers)))
        .map_err(RangeJoinError::Edit)?
        .unwrap_or_default();
    markers_with_bounds(markers, &shifted).map_err(RangeJoinError::Record)
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_range_records_controls.rs"]
mod object_frame_edit_range_records_controls;
#[cfg(test)]
#[path = "../tests/object_frame_edit_range_records_join.rs"]
mod object_frame_edit_range_records_join;
#[cfg(test)]
#[path = "../tests/object_frame_edit_range_records_oracle.rs"]
mod object_frame_edit_range_records_oracle;
