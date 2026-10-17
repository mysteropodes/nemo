//! L01-G (G22/G13): parameter edits and path-effect stack edits use the existing history and receipt owner.
//! One real change is one history entry and one monotonic revision; an edit that changes nothing leaves
//! presence, bytes, undo and redo untouched. The envelope itself (api version, identifiers, owner pins,
//! cancellation, `expectedRevision`, request fingerprint and retained replay) is NOT repeated here: it ran in
//! `object_command_dispatch.rs` before this body is reached, and the receipt is the compact command result.
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_PARAMETRIC_EDIT: &str = "command.document.object.parametric.edit";
const OP_PATH_EFFECTS_EDIT: &str = "command.document.object.path.effects.edit";

impl NativeObjectHistory {
    /// Operations the staged dispatcher routes here.
    pub(crate) fn vector_param_operation(operation: &str) -> bool {
        matches!(operation, OP_PARAMETRIC_EDIT | OP_PATH_EFFECTS_EDIT)
    }

    /// Prepare and commit one parameter or path-effect edit for the dispatcher.
    pub(crate) fn dispatch_vector_param(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Vector parameter payload is invalid.",
            )
        })?;
        let before = self.content_revision();
        let after = if operation == OP_PARAMETRIC_EDIT {
            let candidate = self
                .owner
                .prepare_parametric_edit_json(&bytes)
                .map_err(|code| {
                    (
                        code,
                        "Parametric edit payload or target is invalid or unavailable.",
                    )
                })?;
            self.owner
                .commit_parametric_edit(candidate)
                .map_err(|code| (code, "Parametric edit could not commit."))?
        } else {
            let candidate = self
                .owner
                .prepare_path_effects_json(&bytes)
                .map_err(|code| {
                    (
                        code,
                        "Path effect stack payload is invalid or the layer is absent.",
                    )
                })?;
            self.owner
                .commit_path_effects_edit(candidate)
                .map_err(|code| (code, "Path effect stack could not commit."))?
        };
        if after != before {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}

// The tests of BOTH operations: `tests/object_path_effects_edit.rs` is a nested child of the first file (it shares
// its support modules), so the whole focused run is the filter `object_parametric_edit_history`.
#[cfg(test)]
#[path = "../tests/object_parametric_edit.rs"]
mod parametric_edit_tests;
