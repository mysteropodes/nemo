//! The key-retime transaction uses the existing history and receipt owner: one history entry per changed
//! operation, a semantic no-op adds nothing and keeps redo, and the request fingerprint / replay store that
//! `object_command_dispatch.rs` already runs for every object command covers this operation too.
use crate::commands::PreparedExposureRetime;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_KEY_MOVE: &str = "command.document.object.layer.key.move";

/// True for the internal key-retime operation; the dispatcher routes exactly this here.
pub(crate) fn is_exposure_retime_operation(operation: &str) -> bool {
    operation == OP_KEY_MOVE
}

impl NativeObjectHistory {
    pub(crate) fn prepare_exposure_retime_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedExposureRetime, DispatchErrorCode> {
        self.owner.prepare_exposure_retime_json(bytes)
    }

    pub(crate) fn commit_exposure_retime(
        &mut self,
        candidate: PreparedExposureRetime,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_exposure_retime(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }

    /// Prepare and commit the operation the dispatcher named. The reply is the compact command result the other
    /// object commands use. Replay is the caller's receipt store.
    pub(crate) fn dispatch_exposure_retime(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Key retime payload is invalid.",
            )
        })?;
        let candidate = self.prepare_exposure_retime_json(&bytes).map_err(|code| {
            (
                code,
                "Key retime payload is invalid or its target is unavailable.",
            )
        })?;
        let before = self.content_revision();
        self.commit_exposure_retime(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Key retime could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_exposure_edit_retime_history.rs"]
mod tests;
