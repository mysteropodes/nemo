//! Clipboard transactions share the existing history and receipt owner. Bounded Cut preflights
//! its complete retained response against the existing wire limit before publishing a document.
use crate::commands::{
    cut_clipboard_payload, CellRef, FrameClipboardOperation, FrameClipboardOutcome,
    FrameClipboardPacket, PreparedFrameClipboard,
};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::transaction::{CompactResult, RetainedDisposition};

const OP_CLIPBOARD_COPY: &str = "command.document.object.frames.clipboard.copy";
const OP_CLIPBOARD_CUT: &str = "command.document.object.frames.clipboard.cut";
const OP_CLIPBOARD_PASTE: &str = "command.document.object.frames.clipboard.paste";
const OP_CLIPBOARD_DUPLICATE: &str = "command.document.object.frames.clipboard.duplicate";

/// Closed clipboard operations; public catalog admission is a separate boundary.
pub(crate) fn is_frame_clipboard_operation(operation: &str) -> bool {
    matches!(
        operation,
        OP_CLIPBOARD_COPY | OP_CLIPBOARD_PASTE | OP_CLIPBOARD_DUPLICATE | OP_CLIPBOARD_CUT
    )
}

fn command_of(operation: &str) -> Option<&'static str> {
    match operation {
        OP_CLIPBOARD_COPY => Some("object.frames.clipboard.copy"),
        OP_CLIPBOARD_CUT => Some("object.frames.clipboard.cut"),
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
        request: &OpacityRequest,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message: &'static str| (DispatchErrorCode::InvalidRequest, message);
        let expected = command_of(&request.operation)
            .ok_or_else(|| invalid("Frame clipboard operation is unknown."))?;
        let bytes = serde_json::to_vec(&request.payload)
            .map_err(|_| invalid("Frame clipboard payload is invalid."))?;
        if request.operation == OP_CLIPBOARD_COPY {
            // Reuse the existing detached three-field result, never prepare or commit Cut.
            let packet = self
                .owner
                .copy_frame_clipboard_json(&bytes)
                .map_err(|code| {
                    (
                        code,
                        "Frame clipboard copy payload or selection is invalid or unavailable.",
                    )
                })?;
            let result = CompactResult::FrameClipboardCut {
                applied: false,
                history_entries_added: 0,
                clipboard: cut_clipboard_payload(packet.as_ref()),
            };
            let prospective = RetainedDisposition::Exact {
                content_revision: self.content_revision(),
                outcome: Ok(result.clone()),
            };
            // Admit the complete correlated response before the caller retains any result.
            self.object_reply(request, &prospective)
                .map_err(|_| invalid("Frame clipboard result exceeds the bounded response."))?;
            return Ok(result);
        }
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
        if request.operation == OP_CLIPBOARD_CUT {
            let applied = candidate.changes_document();
            let result = CompactResult::FrameClipboardCut {
                applied,
                history_entries_added: u64::from(applied),
                clipboard: cut_clipboard_payload(candidate.packet()),
            };
            // Include all actual correlation/header fields, not just clipboard
            // bytes: a legal packet alone may overflow the 4096-byte response.
            let revision = before.checked_add(u64::from(applied)).ok_or((
                DispatchErrorCode::Unavailable,
                "Frame clipboard revision is exhausted.",
            ))?;
            let prospective = RetainedDisposition::Exact {
                content_revision: revision,
                outcome: Ok(result.clone()),
            };
            self.object_reply(request, &prospective)
                .map_err(|_| invalid("Frame clipboard result exceeds the bounded response."))?;
            // No serialization or reply-size failure is deferred until commit.
            self.commit_frame_clipboard(candidate)
                .map_err(|code| (code, "Frame clipboard could not commit."))?;
            return Ok(result);
        }
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
