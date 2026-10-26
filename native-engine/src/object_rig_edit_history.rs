//! Rig edits use the existing history and receipt owner (I12): the typed preparation of
//! `object_rig_edit.rs`, one history entry per real change, and the dispatch body the shared envelope of
//! `object_command_dispatch.rs` routes to. The envelope itself (api version, identifiers, owner pins,
//! cancellation, `expectedRevision`, request fingerprint and retained replay) is NOT repeated here: it
//! ran before this body is reached, so a replay never executes the edit twice.
use crate::commands::PreparedRigEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_RIG_EDIT: &str = "command.document.object.rig.edit";

/// True for the rig edit operation; the dispatcher routes exactly this here.
pub(crate) fn is_rig_edit_operation(operation: &str) -> bool {
    operation == OP_RIG_EDIT
}

impl NativeObjectHistory {
    pub(crate) fn prepare_rig_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedRigEdit, DispatchErrorCode> {
        self.owner.prepare_rig_edit_json(bytes)
    }

    /// A history entry only when the revision changed: an exact no-op keeps both stacks.
    pub(crate) fn commit_rig_edit(
        &mut self,
        candidate: PreparedRigEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_rig_edit(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }

    /// `command.document.object.rig.edit`: prepare, then commit through the sealed owner. The reply is
    /// the compact command result the other object commands use.
    pub(crate) fn dispatch_rig_edit(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Rig edit payload is invalid.",
            )
        })?;
        let candidate = self.prepare_rig_edit_json(&bytes).map_err(|code| {
            (
                code,
                "Rig edit payload is invalid or its target is unavailable.",
            )
        })?;
        let before = self.content_revision();
        self.commit_rig_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Rig edit could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_rig_edit_oracle.rs"]
mod object_rig_edit_oracle;

#[cfg(test)]
#[path = "../tests/object_rig_edit_rows.rs"]
mod object_rig_edit_rows;

#[cfg(test)]
#[path = "../tests/object_rig_edit.rs"]
mod object_rig_edit_tests;

#[cfg(test)]
#[path = "../tests/object_rig_edit_guards.rs"]
mod object_rig_edit_guards_tests;

#[cfg(test)]
#[path = "../tests/object_rig_edit_payloads.rs"]
mod object_rig_edit_payloads_tests;

#[cfg(test)]
#[path = "../tests/object_rig_edit_wire.rs"]
mod object_rig_edit_wire_tests;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_rig_edit_kernels.rs"]
mod object_rig_edit_kernel_tests;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_rig_edit_mesh_rows.rs"]
mod object_rig_edit_mesh_rows;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_rig_edit_meshes.rs"]
mod object_rig_edit_mesh_tests;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_rig_edit_mesh_controls.rs"]
mod object_rig_edit_mesh_control_tests;
