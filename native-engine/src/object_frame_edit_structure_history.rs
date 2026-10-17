//! Frame-structure transactions use the existing history and receipt owner: one history entry per changed
//! operation, a semantic no-op adds nothing and keeps redo, and the request fingerprint / replay store that
//! `object_command_dispatch.rs` already runs for every object command covers these four operations too.
use crate::commands::PreparedFrameStructure;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_FRAMES_INSERT: &str = "command.document.object.frames.insert";
const OP_FRAMES_EXTEND: &str = "command.document.object.frames.extend";
const OP_FRAMES_DUPLICATE: &str = "command.document.object.frames.duplicate";
const OP_FRAMES_REMOVE: &str = "command.document.object.frames.remove";

/// True for the four internal frame-structure operations; the dispatcher routes exactly these here.
pub(crate) fn is_frame_structure_operation(operation: &str) -> bool {
    matches!(
        operation,
        OP_FRAMES_INSERT | OP_FRAMES_EXTEND | OP_FRAMES_DUPLICATE | OP_FRAMES_REMOVE
    )
}

fn command_of(operation: &str) -> Option<&'static str> {
    match operation {
        OP_FRAMES_INSERT => Some("object.frames.insert"),
        OP_FRAMES_EXTEND => Some("object.frames.extend"),
        OP_FRAMES_DUPLICATE => Some("object.frames.duplicate"),
        OP_FRAMES_REMOVE => Some("object.frames.remove"),
        _ => None,
    }
}

impl NativeObjectHistory {
    pub(crate) fn prepare_frame_structure_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameStructure, DispatchErrorCode> {
        self.owner.prepare_frame_structure_json(bytes)
    }

    pub(crate) fn commit_frame_structure(
        &mut self,
        candidate: PreparedFrameStructure,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_frame_structure(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }

    /// Prepare and commit one operation named by the dispatcher; the payload command must be that operation's.
    /// The reply is the compact command result the other object commands use. Replay is the caller's receipt store.
    pub(crate) fn dispatch_frame_structure(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message: &'static str| (DispatchErrorCode::InvalidRequest, message);
        let expected = command_of(operation)
            .ok_or_else(|| invalid("Frame structure operation is unknown."))?;
        let bytes = serde_json::to_vec(payload)
            .map_err(|_| invalid("Frame structure payload is invalid."))?;
        let candidate = self.prepare_frame_structure_json(&bytes).map_err(|code| {
            (
                code,
                "Frame structure payload is invalid or its target is unavailable.",
            )
        })?;
        if candidate.command() != expected {
            return Err(invalid(
                "Frame structure command does not match its operation.",
            ));
        }
        let before = self.content_revision();
        self.commit_frame_structure(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Frame structure could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_structure_history.rs"]
mod tests;
// L05-G (#2111): the layer range records through dispatch, undo, redo, replay and reopen.
#[cfg(test)]
#[path = "../tests/object_frame_edit_range_records_history.rs"]
mod object_frame_edit_range_records_history;
