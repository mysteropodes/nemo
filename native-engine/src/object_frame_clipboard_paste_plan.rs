//! Sealed frame-clipboard planners (F12): paste, cut and duplicate selection, prepared against one retained snapshot
//! and published through the existing revision owner (src/js/timeline.js copyFrames, cutFrames, pasteFrames and
//! src/js/app.js duplicateSelectedFrames at fbe4b8a3). The copy half is L05-H's packet kernel, the clearing half is
//! L05-B's `clear_cells`, the cell write is `object_frame_clipboard_paste.rs`; this file adds the bounds and lock
//! policy the baseline applies per operation, the selection-end anchor of duplicate and the sealed prepare/commit pair.
//!
//! Cut keeps locked layers in the packet but never blanks them; duplicate is copy plus paste with its first frame at
//! `maxF + 1` of the SELECTION BOUNDS and its first layer on the ACTIVE layer, and never grows the timeline.
use crate::commands::object_fill_owner::object_frame_clipboard_packet::{
    copy_frame_cells, FrameClipboardPacket, PacketLimits, PacketPins, SelectedCell,
};
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    edit_refusal, layer_position, packet_refusal, paste_cells, CellRef, FrameClipboardOperation,
    PasteCell, PasteRequest,
};
use crate::commands::object_fill_owner::object_frame_clipboard_paste_wire::parse_clipboard_payload;
use crate::commands::object_fill_owner::object_frame_edit_document::{clear_cells, DocumentEdit};
use crate::commands::ObjectFillOwner;
use crate::object_document::{ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// The cells of a packet in the form a paste writes: a key, a blank key and a copied in-between land as full keys,
/// a held or unexposed cell lands plain with no strokes (L05-H `paste_is_keyframe` / `paste_records`).
pub(crate) fn packet_paste_cells(packet: &FrameClipboardPacket) -> Vec<PasteCell> {
    packet
        .cells()
        .iter()
        .map(|cell| PasteCell {
            rel_layer: cell.rel_layer() as u32,
            rel_frame: cell.rel_frame(),
            keyframe: cell.paste_is_keyframe(),
            records: cell.paste_records().to_vec(),
            order_elements: match cell.order_elements() {
                Presence::Present(elements) => Presence::Present(if cell.paste_is_keyframe() {
                    elements.clone()
                } else {
                    Vec::new()
                }),
                Presence::Missing => Presence::Missing,
            },
        })
        .collect()
}

fn copy_selection(
    snapshot: &ObjectSnapshot,
    selection: &[CellRef],
) -> Result<FrameClipboardPacket, DispatchErrorCode> {
    let pins = PacketPins {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
    };
    let cells: Vec<SelectedCell<'_>> = selection
        .iter()
        .map(|cell| SelectedCell {
            layer_uid: cell.layer_uid.as_str(),
            frame: cell.frame,
        })
        .collect();
    copy_frame_cells(snapshot, &pins, &cells, &PacketLimits::DEFAULT).map_err(packet_refusal)
}

fn paste(
    snapshot: &ObjectSnapshot,
    request: &PasteRequest,
) -> Result<DocumentEdit, DispatchErrorCode> {
    if request.instance_id != snapshot.instance_id() {
        return Err(DispatchErrorCode::WrongInstance);
    }
    if request.document_id != snapshot.document_id() {
        return Err(DispatchErrorCode::WrongDocument);
    }
    let document = snapshot.document();
    let anchor =
        layer_position(document, &request.anchor_layer).ok_or(DispatchErrorCode::NotFound)?;
    if request.anchor_frame >= document.total_frames {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    paste_cells(document, anchor, request.anchor_frame, &request.cells)
}

/// cutFrames: the packet is copied from the pre-edit document, then every selected cell of an UNLOCKED layer becomes
/// a plain hold (L05-B `clear_cells`). A cut that blanks nothing still returns its packet and publishes nothing.
fn cut(
    snapshot: &ObjectSnapshot,
    selection: &[CellRef],
) -> Result<(Option<FrameClipboardPacket>, DocumentEdit), DispatchErrorCode> {
    if selection.is_empty() {
        return Ok((None, DocumentEdit::NoChange));
    }
    let packet = copy_selection(snapshot, selection)?;
    let document = snapshot.document();
    let mut layers: Vec<&str> = Vec::new();
    for cell in selection {
        if !layers.contains(&cell.layer_uid.as_str()) {
            layers.push(cell.layer_uid.as_str());
        }
    }
    let mut current = document.clone();
    let mut changed = false;
    for uid in layers {
        let index = layer_position(document, uid).ok_or(DispatchErrorCode::NotFound)?;
        if document.layers[index].locked() {
            continue;
        }
        let frames: Vec<u32> = selection
            .iter()
            .filter(|cell| cell.layer_uid == uid)
            .map(|cell| cell.frame)
            .collect();
        if let DocumentEdit::Changed(next) =
            clear_cells(&current, uid, &frames).map_err(edit_refusal)?
        {
            current = next;
            changed = true;
        }
    }
    let edit = if changed {
        DocumentEdit::Changed(current)
    } else {
        DocumentEdit::NoChange
    };
    Ok((Some(packet), edit))
}

/// duplicateSelectedFrames: copy the selection, then paste it with its first frame at `maxF + 1` of the selection
/// bounds and its first layer on the active layer. The timeline never grows, so cells past the end are dropped.
fn duplicate(
    snapshot: &ObjectSnapshot,
    selection: &[CellRef],
    anchor_layer: &str,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let document = snapshot.document();
    let anchor = layer_position(document, anchor_layer).ok_or(DispatchErrorCode::NotFound)?;
    if selection.is_empty() {
        return Ok(DocumentEdit::NoChange);
    }
    let packet = copy_selection(snapshot, selection)?;
    let (_, min_frame) = packet.origin();
    let (_, span) = packet.extent();
    let base = min_frame
        .checked_add(span)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    paste_cells(document, anchor, base, &packet_paste_cells(&packet))
}

/// Detached, non-Clone candidate sealed to the snapshot it was prepared from; only this file can build one.
/// `document` is `None` for a semantic no-op, which publishes nothing and keeps redo. `packet` is the cut clipboard.
#[derive(Debug)]
pub(crate) struct PreparedFrameClipboard {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    packet: Option<FrameClipboardPacket>,
    command: &'static str,
}
impl PreparedFrameClipboard {
    /// The command the operation names, so a dispatcher can refuse an operation/command mismatch.
    pub(crate) fn command(&self) -> &'static str {
        self.command
    }
    /// Whether the sealed edit would publish a document; preparing still mutates nothing.
    pub(crate) fn changes_document(&self) -> bool {
        self.document.is_some()
    }
    /// The clipboard a cut produced (also when the cut blanked nothing); none for paste and duplicate.
    pub(crate) fn packet(&self) -> Option<&FrameClipboardPacket> {
        self.packet.as_ref()
    }
}

/// What a commit returns: the content revision after it and the cut clipboard, if any.
#[derive(Debug)]
pub(crate) struct FrameClipboardOutcome {
    pub(crate) revision: u64,
    pub(crate) packet: Option<FrameClipboardPacket>,
}

impl ObjectFillOwner {
    pub(crate) fn prepare_frame_clipboard(
        &self,
        operation: &FrameClipboardOperation,
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let (edit, packet, command) = match operation {
            FrameClipboardOperation::Paste(request) => (
                paste(&origin, request)?,
                None,
                "object.frames.clipboard.paste",
            ),
            FrameClipboardOperation::Cut { selection } => {
                let (packet, edit) = cut(&origin, selection)?;
                (edit, packet, "object.frames.clipboard.cut")
            }
            FrameClipboardOperation::Duplicate {
                selection,
                anchor_layer,
            } => (
                duplicate(&origin, selection, anchor_layer)?,
                None,
                "object.frames.clipboard.duplicate",
            ),
        };
        let document = match edit {
            DocumentEdit::Changed(next) => Some(next),
            DocumentEdit::NoChange => None,
        };
        Ok(PreparedFrameClipboard {
            origin,
            document,
            packet,
            command,
        })
    }

    /// copyFrames: the detached packet of the selection, read from the current snapshot; nothing is published.
    pub(crate) fn copy_frame_clipboard(
        &self,
        selection: &[CellRef],
    ) -> Result<FrameClipboardPacket, DispatchErrorCode> {
        copy_selection(&self.current_snapshot(), selection)
    }

    pub(crate) fn prepare_frame_clipboard_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        self.prepare_frame_clipboard(&parse_clipboard_payload(bytes)?)
    }

    pub(crate) fn commit_frame_clipboard(
        &mut self,
        candidate: PreparedFrameClipboard,
    ) -> Result<FrameClipboardOutcome, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let revision = match candidate.document {
            None => self.content_revision(),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable)?,
        };
        Ok(FrameClipboardOutcome {
            revision,
            packet: candidate.packet,
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_paste_plan.rs"]
mod tests;
