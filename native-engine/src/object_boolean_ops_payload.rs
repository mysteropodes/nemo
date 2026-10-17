//! Strict, bounded wire payloads of the three boolean operations (G19 + G18, L02-I).
//! Object-only decoding (duplicate, unknown and positional members refused by
//! `object_deserialize!`), a byte bound on the whole payload and the text and
//! count bounds every identity and the eraser footprint must meet before any
//! snapshot is read. The public capability declaration of these operations is
//! closed (Root D10/D11/D12); this is the staged wire shape.
use crate::object_document::object_deserialize;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const MAX_PAYLOAD_BYTES: usize = 65_536;
const MAX_SELECTION: usize = 1024;
const MAX_FOOTPRINT_POINTS: usize = 1024;
pub(super) const MAX_ID_BYTES: usize = 128;

#[derive(Debug)]
pub(super) struct OpPayload {
    pub(super) command: String,
    pub(super) mode: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) frame: u32,
    pub(super) stroke_ids: Vec<String>,
}
object_deserialize!(OpPayload {
    command: String,
    mode: String,
    layer_uid: String,
    context_id: String,
    frame: u32,
    stroke_ids: Vec<String>
});

#[derive(Debug)]
pub(super) struct ErasePayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) frame: u32,
    pub(super) stroke_id: String,
    pub(super) footprint: Vec<[f64; 2]>,
}
object_deserialize!(ErasePayload {
    command: String,
    layer_uid: String,
    context_id: String,
    frame: u32,
    stroke_id: String,
    footprint: Vec<[f64; 2]>
});

#[derive(Debug)]
pub(super) struct FlattenPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) frame: u32,
    pub(super) group_id: String,
}
object_deserialize!(FlattenPayload {
    command: String,
    layer_uid: String,
    context_id: String,
    frame: u32,
    group_id: String
});

pub(super) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)
}

/// A non-empty, bounded text without control characters.
pub(super) fn text_ok(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_ID_BYTES && !text.chars().any(char::is_control)
}

pub(super) fn target_ok(layer_uid: &str, context_id: &str) -> bool {
    text_ok(layer_uid) && text_ok(context_id)
}

pub(super) fn strokes_ok(strokes: &[String]) -> bool {
    strokes.len() >= 2 && strokes.len() <= MAX_SELECTION && strokes.iter().all(|id| text_ok(id))
}

/// A footprint ring has at least three finite points and a bounded count.
pub(super) fn footprint_ok(points: &[[f64; 2]]) -> bool {
    points.len() >= 3
        && points.len() <= MAX_FOOTPRINT_POINTS
        && points.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}
