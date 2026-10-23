//! Strict, bounded wire payloads of the five group operations (G24, L02-G).
//! Object-only decoding (duplicate, unknown and positional members refused by
//! `object_deserialize!`), a byte bound on the whole payload and the text bounds
//! every identity and name must meet before any snapshot is read.
use crate::object_document::object_deserialize;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

/// A selection of a few hundred strokes fits; the bounds are this operation's own.
const MAX_PAYLOAD_BYTES: usize = 65_536;
const MAX_SELECTION: usize = 1024;
pub(super) const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_NAME_BYTES: usize = 256;

#[derive(Debug)]
pub(super) struct GroupPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) group_id: String,
    pub(super) name: String,
    pub(super) stroke_ids: Vec<String>,
}
object_deserialize!(GroupPayload {
    command: String,
    layer_uid: String,
    context_id: String,
    group_id: String,
    name: String,
    stroke_ids: Vec<String>
});

#[derive(Debug)]
pub(super) struct UngroupPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) stroke_ids: Vec<String>,
}
object_deserialize!(UngroupPayload {
    command: String,
    layer_uid: String,
    context_id: String,
    stroke_ids: Vec<String>
});

#[derive(Debug)]
pub(super) struct CombinePayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) group_id: String,
    pub(super) mode: String,
    pub(super) stroke_ids: Vec<String>,
}
object_deserialize!(CombinePayload {
    command: String,
    layer_uid: String,
    context_id: String,
    group_id: String,
    mode: String,
    stroke_ids: Vec<String>
});

#[derive(Debug)]
pub(super) struct ModePayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) group_id: String,
    pub(super) mode: String,
}
object_deserialize!(ModePayload {
    command: String,
    layer_uid: String,
    context_id: String,
    group_id: String,
    mode: String
});

#[derive(Debug)]
pub(super) struct RenamePayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) group_id: String,
    pub(super) name: String,
}
object_deserialize!(RenamePayload {
    command: String,
    layer_uid: String,
    context_id: String,
    group_id: String,
    name: String
});

#[derive(Debug)]
pub(super) struct ExitPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) context_id: String,
    pub(super) stroke_id: String,
}
object_deserialize!(ExitPayload {
    command: String,
    layer_uid: String,
    context_id: String,
    stroke_id: String
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

pub(super) fn target_ok(layer_uid: &str, context_id: &str) -> bool {
    text_ok(layer_uid, MAX_ID_BYTES) && text_ok(context_id, MAX_ID_BYTES)
}

pub(super) fn strokes_ok(strokes: &[String]) -> bool {
    !strokes.is_empty()
        && strokes.len() <= MAX_SELECTION
        && strokes.iter().all(|id| text_ok(id, MAX_ID_BYTES))
}
