//! Frame-clipboard cell-write kernel (F12): what the dense editor's pasteFrames does to the CONTENT of an
//! `ObjectDocument` (src/js/timeline.js pasteFrames at fbe4b8a3), plus the typed operation records shared by the
//! planners in `object_frame_clipboard_paste_plan.rs`. Strict payloads live in `object_frame_clipboard_paste_wire.rs`;
//! history depth, request replay and dispatch in `object_frame_clipboard_paste_history.rs`.
//!
//! Baseline behavior kept on purpose: a paste targets `anchor layer index + relLayer` and `anchor frame + relFrame`
//! and silently drops every cell outside the layers or frames; a locked target layer is skipped; each written cell
//! REPLACES what the target held (records, timing entry); a copied in-between lands as a full key, a copied plain
//! cell lands plain; stroke ids are the originals (a target is keyed by context, scope, layer and stroke id, so a copy
//! on another layer or frame stays unique). Ordered (v2) documents are refused typed: the packet carries no
//! composition-order elements to write.
use crate::commands::object_fill_owner::object_frame_clipboard_packet::{
    ClipboardPacketError, MAX_PACKET_CELLS,
};
use crate::commands::object_fill_owner::object_frame_edit_document::{
    drop_cells, DocumentEdit, FrameEditError,
};
use crate::object_document::{
    FrameScope, FrameScopeKind, LayerTimingError, ObjectDocument, ObjectRecord, Presence,
    TimingEdit,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use std::collections::HashSet;

const CONTEXT: &str = "scene-root";

/// One selected cell: a stable layer identity and an absolute frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CellRef {
    pub(crate) layer_uid: String,
    pub(crate) frame: u32,
}

/// One clipboard cell in the form a paste writes it (the baseline copy after pasteFrames normalised an in-between
/// to a full key): offsets from the selection bounds, whether the target becomes a key, and the strokes it holds.
/// A plain cell (`keyframe: false`) holds none.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PasteCell {
    pub(crate) rel_layer: u32,
    pub(crate) rel_frame: u32,
    pub(crate) keyframe: bool,
    pub(crate) records: Vec<ObjectRecord>,
}

/// A paste: the identity of the document the clipboard was copied from, the target anchor (baseline
/// `activeLayerIdx` and `currentFrame`) and the cells.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PasteRequest {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) anchor_layer: String,
    pub(crate) anchor_frame: u32,
    pub(crate) cells: Vec<PasteCell>,
}

/// One typed operation. Cut is typed-only: its result is the clipboard packet, which the compact command reply
/// cannot carry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FrameClipboardOperation {
    Paste(PasteRequest),
    Cut {
        selection: Vec<CellRef>,
    },
    Duplicate {
        selection: Vec<CellRef>,
        anchor_layer: String,
    },
}

pub(crate) fn packet_refusal(error: ClipboardPacketError) -> DispatchErrorCode {
    use ClipboardPacketError as E;
    match error {
        E::WrongInstance => DispatchErrorCode::WrongInstance,
        E::WrongDocument => DispatchErrorCode::WrongDocument,
        E::WrongRevision | E::WrongSnapshot => DispatchErrorCode::StaleRevision,
        E::UnknownLayer => DispatchErrorCode::NotFound,
        E::OrderedDocumentUnavailable
        | E::TimingUnavailable
        | E::ExposureUnavailable
        | E::RecordUnavailable => DispatchErrorCode::Unavailable,
        E::EmptySelection
        | E::FrameOutOfRange
        | E::DuplicateCell
        | E::TooManyCells
        | E::TooManyBytes => DispatchErrorCode::InvalidRequest,
    }
}
pub(crate) fn edit_refusal(error: FrameEditError) -> DispatchErrorCode {
    match error {
        FrameEditError::Invalid => DispatchErrorCode::InvalidRequest,
        FrameEditError::UnknownLayer => DispatchErrorCode::NotFound,
        FrameEditError::Unsupported
        | FrameEditError::MissingTiming
        | FrameEditError::PreservedOutOfRange => DispatchErrorCode::Unavailable,
    }
}
pub(crate) fn timing_refusal(error: LayerTimingError) -> DispatchErrorCode {
    edit_refusal(FrameEditError::from(error))
}

pub(crate) fn scene_authored(record: &ObjectRecord) -> bool {
    record.target.context_id == CONTEXT
        && record.target.frame_scope.kind == FrameScopeKind::Authored
}
pub(crate) fn layer_position(document: &ObjectDocument, uid: &str) -> Option<usize> {
    document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == uid)
}
/// The one exit for a result built here: the shared strict drawing admission runs on the complete document.
pub(crate) fn finish(document: ObjectDocument) -> Result<DocumentEdit, DispatchErrorCode> {
    ObjectSnapshot::validate_drawing(&document).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(DocumentEdit::Changed(document))
}

/// The record as a copy at another cell: only the layer and the frame change, never the stroke id.
fn retarget(record: &mut ObjectRecord, layer: &str, frame: u32) {
    record.target.layer_uid = layer.to_owned();
    record.target.frame_scope = FrameScope {
        kind: FrameScopeKind::Authored,
        frame,
    };
}

/// True when the target cell already is exactly what this paste would write there, so writing it changes nothing.
fn cell_matches(
    document: &ObjectDocument,
    layer_index: usize,
    frame: u32,
    cell: &PasteCell,
) -> bool {
    let layer = &document.layers[layer_index];
    let Some(timing) = layer.timing.value() else {
        return false;
    };
    let (kind, _, _) = timing.exposure(frame);
    let timing_same = if cell.keyframe {
        kind == "key"
    } else {
        kind != "key" && kind != "interpolated"
    };
    if !timing_same {
        return false;
    }
    let current: Vec<&ObjectRecord> = document
        .objects
        .iter()
        .filter(|record| {
            scene_authored(record)
                && record.target.layer_uid == layer.layer_uid
                && record.target.frame_scope.frame == frame
        })
        .collect();
    current.len() == cell.records.len()
        && current.iter().zip(&cell.records).all(|(have, want)| {
            let mut moved = want.clone();
            retarget(&mut moved, &layer.layer_uid, frame);
            **have == moved
        })
}

/// Write one pasted cell: the timing entry follows the clipboard cell (a key promotes, replacing an in-between; a
/// plain cell clears), whatever the target held is dropped, and the copies are appended in clipboard order.
fn write_cell(
    next: &mut ObjectDocument,
    layer_index: usize,
    frame: u32,
    cell: &PasteCell,
) -> Result<(), DispatchErrorCode> {
    let total = next.total_frames;
    let layer_uid = next.layers[layer_index].layer_uid.clone();
    let edit = {
        let timing = next.layers[layer_index]
            .timing
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        let result = if cell.keyframe {
            timing.promote_cell(frame, total)
        } else {
            timing.clear_cells(&[frame], total)
        };
        result.map_err(timing_refusal)?
    };
    if let TimingEdit::Changed(timing) = edit {
        next.layers[layer_index].timing = Presence::Present(timing);
    }
    drop_cells(next, &layer_uid, &|at| at == frame);
    for record in &cell.records {
        let mut copy = record.clone();
        retarget(&mut copy, &layer_uid, frame);
        next.objects.push(copy);
    }
    Ok(())
}

/// pasteFrames over an anchor that may lie anywhere: cells outside the layers or frames and cells on locked layers
/// are dropped, every other cell is written, and a paste that changes nothing is `NoChange`.
pub(crate) fn paste_cells(
    document: &ObjectDocument,
    anchor_layer: usize,
    anchor_frame: u32,
    cells: &[PasteCell],
) -> Result<DocumentEdit, DispatchErrorCode> {
    if document.format_version != 1 {
        return Err(DispatchErrorCode::Unavailable);
    }
    if cells.len() > MAX_PACKET_CELLS {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let mut seen = HashSet::new();
    for cell in cells {
        let well_formed = seen.insert((cell.rel_layer, cell.rel_frame))
            && cell.records.iter().all(scene_authored)
            && (cell.keyframe || cell.records.is_empty());
        if !well_formed {
            return Err(DispatchErrorCode::InvalidRequest);
        }
    }
    let total = document.total_frames;
    let mut next = document.clone();
    let mut changed = false;
    for cell in cells {
        let layer_index = anchor_layer.checked_add(cell.rel_layer as usize);
        let Some(layer_index) = layer_index.filter(|index| *index < document.layers.len()) else {
            continue;
        };
        let Some(frame) = anchor_frame
            .checked_add(cell.rel_frame)
            .filter(|frame| *frame < total)
        else {
            continue;
        };
        if document.layers[layer_index].locked() || cell_matches(&next, layer_index, frame, cell) {
            continue;
        }
        write_cell(&mut next, layer_index, frame, cell)?;
        changed = true;
    }
    if !changed {
        return Ok(DocumentEdit::NoChange);
    }
    finish(next)
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_paste.rs"]
mod tests;
