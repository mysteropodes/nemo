//! Strict, bounded wire payloads of the selection-edit and clipboard
//! transactions (F08/F15, L02-H) and the one refusal table that maps a typed
//! plan refusal to the request-receipt vocabulary.
//!
//! Object-only decoding (duplicate, unknown and positional members refused by
//! `object_deserialize!`), a byte bound on the whole payload and the text bounds
//! every identity must meet before any snapshot is read. The bounds are these
//! operations' own; over the staged dispatcher the 4096-byte envelope is the
//! effective limit.
use crate::object_document::object_deserialize;
use crate::object_document::object_group_kernels::object_clipboard_plan::ClipPayload;
use crate::object_document::object_group_kernels::object_clipboard_remap::RemapError;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    EditMode, PromotionRefusal,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::SelectionError;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const MAX_PAYLOAD_BYTES: usize = 65_536;
pub(super) const MAX_SELECTION: usize = 1024;
pub(super) const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_LABEL_BYTES: usize = 64;

/// delete / duplicate / copy / cut: shapes of one layer cell by stable identity.
#[derive(Debug)]
pub(super) struct SelectionPayload {
    pub(super) command: String,
    pub(super) context_id: String,
    pub(super) layer_uid: String,
    pub(super) frame: u32,
    pub(super) mode: String,
    pub(super) stroke_ids: Vec<String>,
}
object_deserialize!(SelectionPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    mode: String,
    stroke_ids: Vec<String>
});

#[derive(Debug)]
pub(super) struct PastePayload {
    pub(super) command: String,
    pub(super) context_id: String,
    pub(super) layer_uid: String,
    pub(super) frame: u32,
    pub(super) mode: String,
    pub(super) clip: ClipPayload,
}
object_deserialize!(PastePayload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    mode: String,
    clip: ClipPayload
});

#[derive(Debug)]
pub(super) struct LabelsPayload {
    pub(super) shape: String,
    pub(super) stroke: String,
}
object_deserialize!(LabelsPayload {
    shape: String,
    stroke: String
});

#[derive(Debug)]
pub(super) struct SplitPayload {
    pub(super) command: String,
    pub(super) context_id: String,
    pub(super) layer_uid: String,
    pub(super) frame: u32,
    pub(super) labels: LabelsPayload,
}
object_deserialize!(SplitPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    labels: LabelsPayload
});

pub(super) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)
}

/// A non-empty, bounded text without control characters.
pub(super) fn text_ok(text: &str, limit: usize) -> bool {
    !text.is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
}

pub(super) fn target_ok(context_id: &str, layer_uid: &str) -> bool {
    text_ok(context_id, MAX_ID_BYTES) && text_ok(layer_uid, MAX_ID_BYTES)
}

/// An empty selection is a valid no-op; every id must be a real identity.
pub(super) fn strokes_ok(strokes: &[String]) -> bool {
    strokes.len() <= MAX_SELECTION && strokes.iter().all(|id| text_ok(id, MAX_ID_BYTES))
}

pub(super) fn clip_ok(clip: &ClipPayload) -> bool {
    target_ok(&clip.context_id, &clip.layer_uid) && clip.records.len() <= MAX_SELECTION
}

pub(super) fn labels_ok(labels: &LabelsPayload) -> bool {
    text_ok(&labels.shape, MAX_LABEL_BYTES) && text_ok(&labels.stroke, MAX_LABEL_BYTES)
}

/// `state.appMode`: the two modes the baseline distinguishes.
pub(super) fn edit_mode(text: &str) -> Result<EditMode, DispatchErrorCode> {
    match text {
        "animation2d" => Ok(EditMode::Animation2d),
        "motion" => Ok(EditMode::Motion),
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

fn remap_code(error: &RemapError) -> DispatchErrorCode {
    match error {
        // A provider that cannot supply an identity is unavailable, not malformed input.
        RemapError::IdUnavailable(_) | RemapError::EmptyFreshId(_) | RemapError::IdCollision(_) => {
            DispatchErrorCode::Unavailable
        }
        RemapError::EmptySelection
        | RemapError::NonFiniteOffset
        | RemapError::DuplicateSourceStrokeId(_)
        | RemapError::DabsWithoutBrushGroup(_)
        | RemapError::EmptyGroupId
        | RemapError::DuplicateGroupId(_)
        | RemapError::UnknownGroup(_)
        | RemapError::GroupCycle(_)
        | RemapError::UnadmittedKind(_)
        | RemapError::UnknownReferenceKind(_) => DispatchErrorCode::InvalidRequest,
    }
}

/// How a typed plan refusal reaches the request-receipt vocabulary. Exhaustive:
/// a new refusal fails the build instead of defaulting.
pub(super) fn refusal_code(error: &SelectionError) -> DispatchErrorCode {
    match error {
        SelectionError::UnknownLayer(_) | SelectionError::UnknownStroke(_) => {
            DispatchErrorCode::NotFound
        }
        SelectionError::Locked(_)
        | SelectionError::TableContext
        | SelectionError::RevisionOutOfRange
        | SelectionError::IdUnavailable
        | SelectionError::Promotion(PromotionRefusal::Unavailable) => {
            DispatchErrorCode::Unavailable
        }
        SelectionError::EmptyStrokeId
        | SelectionError::DuplicateStroke(_)
        | SelectionError::FrameOutOfRange(_)
        | SelectionError::NoKeyframe
        | SelectionError::Promotion(PromotionRefusal::Invalid)
        | SelectionError::NonFiniteCoordinate
        | SelectionError::NeedTwoElements
        | SelectionError::MultiContext(_)
        | SelectionError::EmptyLabel
        | SelectionError::ClipMismatch
        | SelectionError::ClipTableUnreadable => DispatchErrorCode::InvalidRequest,
        SelectionError::TableUnreadable(_)
        | SelectionError::GroupEdit
        | SelectionError::OrderPlanIncomplete(_) => DispatchErrorCode::Internal,
        SelectionError::Remap(remap) => remap_code(remap),
    }
}
