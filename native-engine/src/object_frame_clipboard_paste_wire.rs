//! Strict frame-clipboard payloads (F12): copy, paste, cut and duplicate, plus the detached packet wire form.
//! One field set per command; duplicate, unknown,
//! missing, null, wrongly typed, negative, fractional, > u32 and over-budget inputs are `InvalidRequest`. Cut admits
//! selection only; its detached result uses the same normalized cells as paste. The dispatcher must check the complete
//! correlated response against the operation-specific full-message budget BEFORE commit.
//! Copy/Cut/Paste alone use the F12 transport budget; Duplicate retains4096.
//! Records plus complete order arrays retain the existing4MiB copied-data bound.
use crate::commands::object_fill_owner::object_frame_clipboard_packet::FrameClipboardPacket;
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    CellRef, FrameClipboardOperation, PasteCell, PasteRequest,
};
use crate::commands::object_fill_owner::object_frame_clipboard_paste_plan::packet_paste_cells;
use crate::object_document::{object_deserialize, ObjectRecord, OrderElement, Presence};
use crate::request_receipts::{frame_clipboard_transport as budget, DispatchErrorCode};
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

impl crate::commands::ObjectFillOwner {
    /// Strict read-only Copy ingress. The existing snapshot-pinned producer owns packet semantics.
    /// Empty selection returns no new packet; it does not clear a client's prior clipboard.
    pub(crate) fn copy_frame_clipboard_json(
        &self,
        bytes: &[u8],
    ) -> Result<Option<FrameClipboardPacket>, DispatchErrorCode> {
        if bytes.len() > budget::MESSAGE_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: ClipboardPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.frames.clipboard.copy" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        forbid(&payload.instance_id)?;
        forbid(&payload.document_id)?;
        forbid(&payload.layer_uid)?;
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
        if selection.is_empty() {
            Ok(None)
        } else {
            self.copy_frame_clipboard(&selection).map(Some)
        }
    }
}

/// Parse one clipboard command payload into its typed operation.
pub(crate) fn parse_clipboard_payload(
    bytes: &[u8],
) -> Result<FrameClipboardOperation, DispatchErrorCode> {
    if bytes.len() > budget::MESSAGE_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let payload: ClipboardPayload =
        serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    if !matches!(
        payload.command.as_str(),
        "object.frames.clipboard.cut" | "object.frames.clipboard.paste"
    ) && bytes.len() > MAX_PAYLOAD_BYTES
    {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    match payload.command.as_str() {
        "object.frames.clipboard.cut" => {
            forbid(&payload.instance_id)?;
            forbid(&payload.document_id)?;
            forbid(&payload.layer_uid)?;
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
            Ok(FrameClipboardOperation::Cut { selection })
        }
        "object.frames.clipboard.paste" => {
            forbid(&payload.selection)?;
            let wire_cells = require(payload.cells)?;
            let mut data_bytes = 0;
            for cell in &wire_cells {
                for record in &cell.records {
                    if !budget::charge(&mut data_bytes, record) {
                        return Err(DispatchErrorCode::InvalidRequest);
                    }
                }
                if let Presence::Present(order) = &cell.order_elements {
                    if !budget::charge(&mut data_bytes, order) {
                        return Err(DispatchErrorCode::InvalidRequest);
                    }
                }
            }
            let cells = wire_cells
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

/// One definition of normalized detached cells shared by public Copy/Cut and existing paste.
fn packet_cells_payload(packet: &FrameClipboardPacket) -> Vec<Value> {
    packet_paste_cells(packet)
        .iter()
        .map(|cell| {
            let mut value = json!({"relLayer":cell.rel_layer,"relFrame":cell.rel_frame,
                "keyframe":cell.keyframe,"records":cell.records});
            if let Presence::Present(elements) = &cell.order_elements {
                value["orderElements"] = json!(elements);
            }
            value
        })
        .collect()
}

/// The paste payload that carries `packet` to the cell `(anchor_layer, anchor_frame)`: the packet's source identity,
/// its cells in selection order in paste form, and the target anchor. Existing paste shape is unchanged.
pub(crate) fn paste_payload(
    packet: &FrameClipboardPacket,
    anchor_layer: &str,
    anchor_frame: u32,
) -> Value {
    json!({"command":"object.frames.clipboard.paste",
        "instanceId":packet.instance_id(),"documentId":packet.document_id(),
        "layerUid":anchor_layer,"frame":anchor_frame,"cells":packet_cells_payload(packet)})
}

/// Immutable source identity and existing paste cells, with no invented destination anchor.
/// Empty Cut has no packet. This is NOT the full response-size admission: the owner must
/// encode/check the eventual correlated envelope before committing, including its header.
pub(crate) fn cut_clipboard_payload(packet: Option<&FrameClipboardPacket>) -> Value {
    packet
        .map(|packet| {
            json!({"instanceId":packet.instance_id(),
        "documentId":packet.document_id(),"cells":packet_cells_payload(packet)})
        })
        .unwrap_or(Value::Null)
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_paste_wire.rs"]
mod tests;
