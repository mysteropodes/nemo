//! Strict payload of the Repeat Selection command (F13), `object.frames.repeat`, and the one definition of its wire
//! form. The command names the selection BOUNDS the dense editor reads from `selBounds()` (the cycle depends on
//! nothing else): the first and last layer by stable `layerUid` (every layer between them in document order is
//! cycled) and the first and last frame. `times` is the raw count the editor was given; the baseline rule
//! (`parseInt(times, 10) || 1`, clamped to 1..50) is applied by the transaction, not here.
//!
//! One field set; duplicate, unknown, missing, null, wrongly typed, fractional, negative-frame, > u32 and
//! over-4096-byte inputs are `InvalidRequest`. The existing 4096-byte object message bound is unchanged.
use crate::commands::object_fill_owner::object_frame_clipboard_repeat::RepeatRequest;
use crate::object_document::object_deserialize;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_PAYLOAD_BYTES: usize = 4096;
const COMMAND: &str = "object.frames.repeat";

#[derive(Debug)]
struct RepeatPayload {
    command: String,
    from_layer_uid: String,
    to_layer_uid: String,
    from_frame: u32,
    to_frame: u32,
    times: i64,
}
object_deserialize!(RepeatPayload {
    command: String,
    from_layer_uid: String,
    to_layer_uid: String,
    from_frame: u32,
    to_frame: u32,
    times: i64
});

/// Parse one repeat payload into its typed request.
pub(crate) fn parse_repeat_payload(bytes: &[u8]) -> Result<RepeatRequest, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let payload: RepeatPayload =
        serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    if payload.command != COMMAND
        || payload.from_layer_uid.is_empty()
        || payload.to_layer_uid.is_empty()
    {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(RepeatRequest {
        from_layer: payload.from_layer_uid,
        to_layer: payload.to_layer_uid,
        from_frame: payload.from_frame,
        to_frame: payload.to_frame,
        times: payload.times,
    })
}

/// The payload that carries `request`: the one definition of the wire form.
pub(crate) fn repeat_payload(request: &RepeatRequest) -> Value {
    json!({"command":COMMAND,"fromLayerUid":request.from_layer,"toLayerUid":request.to_layer,
        "fromFrame":request.from_frame,"toFrame":request.to_frame,"times":request.times})
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_repeat_wire.rs"]
mod tests;
