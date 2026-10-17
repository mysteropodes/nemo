//! Frame-clipboard transactions use the existing history and receipt owner: one history entry per changed
//! operation, a semantic no-op adds nothing and keeps redo, and the request fingerprint / replay store that
//! `object_command_dispatch.rs` already runs for every object command covers paste and duplicate too. Cut is the typed
//! `cut_frame_cells` below: its packet is the result and cannot ride a compact command reply, so it has no operation.
use crate::commands::{
    CellRef, FrameClipboardOperation, FrameClipboardOutcome, FrameClipboardPacket,
    PreparedFrameClipboard,
};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_CLIPBOARD_PASTE: &str = "command.document.object.frames.clipboard.paste";
const OP_CLIPBOARD_DUPLICATE: &str = "command.document.object.frames.clipboard.duplicate";

/// True for the two internal clipboard operations; the dispatcher routes exactly these here.
pub(crate) fn is_frame_clipboard_operation(operation: &str) -> bool {
    matches!(operation, OP_CLIPBOARD_PASTE | OP_CLIPBOARD_DUPLICATE)
}

fn command_of(operation: &str) -> Option<&'static str> {
    match operation {
        OP_CLIPBOARD_PASTE => Some("object.frames.clipboard.paste"),
        OP_CLIPBOARD_DUPLICATE => Some("object.frames.clipboard.duplicate"),
        _ => None,
    }
}

impl NativeObjectHistory {
    pub(crate) fn prepare_frame_clipboard(
        &self,
        operation: &FrameClipboardOperation,
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        self.owner.prepare_frame_clipboard(operation)
    }

    /// copyFrames: the clipboard packet of the selection, read from the current snapshot. Read-only: it publishes
    /// nothing, adds no history entry and does not touch redo.
    pub(crate) fn copy_frame_clipboard(
        &self,
        selection: &[CellRef],
    ) -> Result<FrameClipboardPacket, DispatchErrorCode> {
        self.owner.copy_frame_clipboard(selection)
    }

    pub(crate) fn prepare_frame_clipboard_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        self.owner.prepare_frame_clipboard_json(bytes)
    }

    /// Publish a prepared candidate with ONE history entry when it changed the document; a no-op publishes nothing,
    /// adds no entry and keeps redo.
    pub(crate) fn commit_frame_clipboard(
        &mut self,
        candidate: PreparedFrameClipboard,
    ) -> Result<FrameClipboardOutcome, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let outcome = self.owner.commit_frame_clipboard(candidate)?;
        if outcome.revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision: outcome.revision,
            });
            self.redo.clear();
        }
        Ok(outcome)
    }

    /// Cut the selection: blank the unlocked cells in one history entry and return the clipboard packet copied from
    /// the pre-edit document (none for an empty selection). A cut that blanks nothing still returns its packet.
    pub(crate) fn cut_frame_cells(
        &mut self,
        selection: &[CellRef],
    ) -> Result<Option<FrameClipboardPacket>, DispatchErrorCode> {
        let candidate = self.prepare_frame_clipboard(&FrameClipboardOperation::Cut {
            selection: selection.to_vec(),
        })?;
        Ok(self.commit_frame_clipboard(candidate)?.packet)
    }

    /// Prepare and commit one operation named by the dispatcher; the payload command must be that operation's.
    /// The reply is the compact command result the other object commands use. Replay is the caller's receipt store.
    pub(crate) fn dispatch_frame_clipboard(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message: &'static str| (DispatchErrorCode::InvalidRequest, message);
        let expected = command_of(operation)
            .ok_or_else(|| invalid("Frame clipboard operation is unknown."))?;
        let bytes = serde_json::to_vec(payload)
            .map_err(|_| invalid("Frame clipboard payload is invalid."))?;
        let candidate = self.prepare_frame_clipboard_json(&bytes).map_err(|code| {
            (
                code,
                "Frame clipboard payload is invalid or its target is unavailable.",
            )
        })?;
        if candidate.command() != expected {
            return Err(invalid(
                "Frame clipboard command does not match its operation.",
            ));
        }
        let before = self.content_revision();
        self.commit_frame_clipboard(candidate)
            .map(|outcome| CompactResult::Command {
                applied: outcome.revision != before,
                history_entries_added: u64::from(outcome.revision != before),
            })
            .map_err(|code| (code, "Frame clipboard could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_paste_history.rs"]
mod tests;
