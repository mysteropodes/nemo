//! Repeat Selection uses the existing history and receipt owner: one history entry per changed cycle, a semantic no-op
//! adds nothing and keeps redo, and the request fingerprint / replay store that `object_command_dispatch.rs` already
//! runs for every object command covers it too. The operation is internal; its public declaration is X03's.
use crate::commands::{PreparedFrameRepeat, RepeatRequest};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_FRAMES_REPEAT: &str = "command.document.object.frames.repeat";

/// True for the internal repeat operation; the dispatcher routes exactly this here.
pub(crate) fn is_frame_repeat_operation(operation: &str) -> bool {
    operation == OP_FRAMES_REPEAT
}

impl NativeObjectHistory {
    pub(crate) fn prepare_frame_repeat(
        &self,
        request: &RepeatRequest,
    ) -> Result<PreparedFrameRepeat, DispatchErrorCode> {
        self.owner.prepare_frame_repeat(request)
    }

    pub(crate) fn prepare_frame_repeat_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameRepeat, DispatchErrorCode> {
        self.owner.prepare_frame_repeat_json(bytes)
    }

    /// Publish a prepared cycle with ONE history entry when it changed the document; a no-op publishes nothing, adds no
    /// entry and keeps redo.
    pub(crate) fn commit_frame_repeat(
        &mut self,
        candidate: PreparedFrameRepeat,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let revision = self.owner.commit_frame_repeat(candidate)?;
        if revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision: revision,
            });
            self.redo.clear();
        }
        Ok(revision)
    }

    /// Prepare and commit one cycle; returns the content revision after it.
    pub(crate) fn repeat_frames(
        &mut self,
        request: &RepeatRequest,
    ) -> Result<u64, DispatchErrorCode> {
        let candidate = self.prepare_frame_repeat(request)?;
        self.commit_frame_repeat(candidate)
    }

    /// Prepare and commit the operation named by the dispatcher. The reply is the compact command result the other
    /// object commands use. Replay is the caller's receipt store.
    pub(crate) fn dispatch_frame_repeat(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message: &'static str| (DispatchErrorCode::InvalidRequest, message);
        if !is_frame_repeat_operation(operation) {
            return Err(invalid("Frame repeat operation is unknown."));
        }
        let bytes =
            serde_json::to_vec(payload).map_err(|_| invalid("Frame repeat payload is invalid."))?;
        let candidate = self.prepare_frame_repeat_json(&bytes).map_err(|code| {
            (
                code,
                "Frame repeat payload is invalid or its target is unavailable.",
            )
        })?;
        let before = self.content_revision();
        self.commit_frame_repeat(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Frame repeat could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_repeat_history.rs"]
mod tests;
