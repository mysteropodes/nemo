//! Strict payloads of the frame-clipboard commands (F12), `object.frames.clipboard.paste` and
//! `object.frames.clipboard.duplicate`, and the wire form of a packet. One field set per command; duplicate, unknown,
//! missing, null, wrongly typed, negative, fractional, > u32 and over-4096-byte inputs are `InvalidRequest`. Cut has no
//! payload: its result is the packet, which the compact command reply cannot carry (typed API only).
//!
//! The existing 4096-byte object message bound applies unchanged, so a pasted packet that does not fit refuses; the
//! ingress bound for large packets is an open Architect decision and nothing here changes it.
use crate::commands::object_fill_owner::object_frame_clipboard_packet::FrameClipboardPacket;
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    CellRef, FrameClipboardOperation, PasteCell, PasteRequest,
};
use crate::commands::object_fill_owner::object_frame_clipboard_paste_plan::packet_paste_cells;
use crate::object_document::{object_deserialize, ObjectRecord, OrderElement, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug)]
struct WireCell {
    rel_layer: u32,
    rel_frame: u32,
    keyframe: bool,
    records: Vec<ObjectRecord>,
    order_elements: Presence<Vec<OrderElement>>,
}
object_deserialize!(WireCell { rel_layer: u32, rel_frame: u32, keyframe: bool, records: Vec<ObjectRecord>,
    #[serde(default)] order_elements: Presence<Vec<OrderElement>> });

#[derive(Debug)]
struct WireRef {
    layer_uid: String,
    frame: u32,
}
object_deserialize!(WireRef {
    layer_uid: String,
    frame: u32
});

#[derive(Debug)]
struct ClipboardPayload {
    command: String,
    instance_id: Presence<String>,
    document_id: Presence<String>,
    layer_uid: Presence<String>,
    frame: Presence<u32>,
    cells: Presence<Vec<WireCell>>,
    selection: Presence<Vec<WireRef>>,
}
object_deserialize!(ClipboardPayload {
    command: String,
    #[serde(default)] instance_id: Presence<String>,
    #[serde(default)] document_id: Presence<String>,
    #[serde(default)] layer_uid: Presence<String>,
    #[serde(default)] frame: Presence<u32>,
    #[serde(default)] cells: Presence<Vec<WireCell>>,
    #[serde(default)] selection: Presence<Vec<WireRef>>
});

fn require<T>(field: Presence<T>) -> Result<T, DispatchErrorCode> {
    match field {
        Presence::Present(value) => Ok(value),
        Presence::Missing => Err(DispatchErrorCode::InvalidRequest),
    }
}
fn forbid<T>(field: &Presence<T>) -> Result<(), DispatchErrorCode> {
    if field.missing() {
        Ok(())
    } else {
        Err(DispatchErrorCode::InvalidRequest)
    }
}
fn nonempty(value: String) -> Result<String, DispatchErrorCode> {
    if value.is_empty() {
        Err(DispatchErrorCode::InvalidRequest)
    } else {
        Ok(value)
    }
}

/// Parse one clipboard command payload into its typed operation.
pub(crate) fn parse_clipboard_payload(
    bytes: &[u8],
) -> Result<FrameClipboardOperation, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let payload: ClipboardPayload =
        serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    match payload.command.as_str() {
        "object.frames.clipboard.paste" => {
            forbid(&payload.selection)?;
            let cells = require(payload.cells)?
                .into_iter()
                .map(|cell| PasteCell {
                    rel_layer: cell.rel_layer,
                    rel_frame: cell.rel_frame,
                    keyframe: cell.keyframe,
                    records: cell.records,
                    order_elements: cell.order_elements,
                })
                .collect();
            Ok(FrameClipboardOperation::Paste(PasteRequest {
                instance_id: nonempty(require(payload.instance_id)?)?,
                document_id: nonempty(require(payload.document_id)?)?,
                anchor_layer: nonempty(require(payload.layer_uid)?)?,
                anchor_frame: require(payload.frame)?,
                cells,
            }))
        }
        "object.frames.clipboard.duplicate" => {
            forbid(&payload.instance_id)?;
            forbid(&payload.document_id)?;
            forbid(&payload.frame)?;
            forbid(&payload.cells)?;
            let selection = require(payload.selection)?
                .into_iter()
                .map(|cell| {
                    nonempty(cell.layer_uid).map(|layer_uid| CellRef {
                        layer_uid,
                        frame: cell.frame,
                    })
                })
                .collect::<Result<Vec<CellRef>, DispatchErrorCode>>()?;
            Ok(FrameClipboardOperation::Duplicate {
                selection,
                anchor_layer: nonempty(require(payload.layer_uid)?)?,
            })
        }
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

/// The paste payload that carries `packet` to the cell `(anchor_layer, anchor_frame)`: the packet's source identity,
/// its cells in selection order in paste form, and the target anchor. This is the one definition of the wire form.
pub(crate) fn paste_payload(
    packet: &FrameClipboardPacket,
    anchor_layer: &str,
    anchor_frame: u32,
) -> Value {
    let cells: Vec<Value> = packet_paste_cells(packet)
        .iter()
        .map(|cell| {
            let mut value = json!({"relLayer":cell.rel_layer,"relFrame":cell.rel_frame,
                "keyframe":cell.keyframe,"records":cell.records});
            if let Presence::Present(elements) = &cell.order_elements {
                value["orderElements"] = json!(elements);
            }
            value
        })
        .collect();
    json!({"command":"object.frames.clipboard.paste",
        "instanceId":packet.instance_id(),"documentId":packet.document_id(),
        "layerUid":anchor_layer,"frame":anchor_frame,"cells":cells})
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_paste_wire.rs"]
mod tests;
