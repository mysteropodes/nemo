//! Key-exposure transactions use the existing history and receipt owner: one history entry per changed
//! operation, a semantic no-op adds nothing and keeps redo, and the request fingerprint / replay store that
//! `object_command_dispatch.rs` already runs for every object command covers these four operations too.
use crate::commands::PreparedExposureKey;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_KEY_PROMOTE: &str = "command.document.object.layer.key.promote";
const OP_KEY_CLEAR: &str = "command.document.object.layer.key.clear";
const OP_KEY_CLEAR_CELLS: &str = "command.document.object.layer.key.clear_cells";
const OP_TWEEN_REMOVE: &str = "command.document.object.layer.tween.remove";

/// True for the four internal key-exposure operations; the dispatcher routes exactly these here.
pub(crate) fn is_exposure_key_operation(operation: &str) -> bool {
    matches!(
        operation,
        OP_KEY_PROMOTE | OP_KEY_CLEAR | OP_KEY_CLEAR_CELLS | OP_TWEEN_REMOVE
    )
}

fn command_of(operation: &str) -> Option<&'static str> {
    match operation {
        OP_KEY_PROMOTE => Some("object.layer.key.promote"),
        OP_KEY_CLEAR => Some("object.layer.key.clear"),
        OP_KEY_CLEAR_CELLS => Some("object.layer.key.clear_cells"),
        OP_TWEEN_REMOVE => Some("object.layer.tween.remove"),
        _ => None,
    }
}

impl NativeObjectHistory {
    pub(crate) fn prepare_exposure_key_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedExposureKey, DispatchErrorCode> {
        self.owner.prepare_exposure_key_json(bytes)
    }

    pub(crate) fn commit_exposure_key(
        &mut self,
        candidate: PreparedExposureKey,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_exposure_key(candidate)?;
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
    pub(crate) fn dispatch_exposure_key(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message: &'static str| (DispatchErrorCode::InvalidRequest, message);
        let expected =
            command_of(operation).ok_or_else(|| invalid("Exposure key operation is unknown."))?;
        let bytes =
            serde_json::to_vec(payload).map_err(|_| invalid("Exposure key payload is invalid."))?;
        let candidate = self.prepare_exposure_key_json(&bytes).map_err(|code| {
            (
                code,
                "Exposure key payload is invalid or its target is unavailable.",
            )
        })?;
        if candidate.command() != expected {
            return Err(invalid(
                "Exposure key command does not match its operation.",
            ));
        }
        let before = self.content_revision();
        self.commit_exposure_key(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Exposure key could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_exposure_edit_key_history.rs"]
mod tests;
