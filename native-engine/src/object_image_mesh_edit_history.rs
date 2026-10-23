//! Image-mesh edits use the existing history and receipt owner (F17/F18): the typed preparation of
//! `object_image_mesh_edit.rs`, one history entry per real change, and the dispatch body the shared
//! envelope of `object_command_dispatch.rs` delegates to. The envelope itself (api version,
//! identifiers, owner pins, cancellation, `expectedRevision`, request fingerprint and retained replay)
//! is NOT repeated here: it ran before this body is reached.
use crate::commands::PreparedImageMeshEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

impl NativeObjectHistory {
    pub(crate) fn prepare_image_mesh_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedImageMeshEdit, DispatchErrorCode> {
        self.owner.prepare_image_mesh_edit_json(bytes)
    }

    /// A history entry only when the revision changed: an exact no-op keeps both stacks.
    pub(crate) fn commit_image_mesh_edit(
        &mut self,
        candidate: PreparedImageMeshEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_image_mesh_edit(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }

    /// `command.document.object.image.mesh.edit`: prepare, then commit through the sealed owner.
    pub(crate) fn dispatch_image_mesh_edit(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        const REFUSED: &str = "Image mesh edit payload or target is invalid or unavailable.";
        let candidate = self
            .prepare_image_mesh_edit_json(payload)
            .map_err(|code| (code, REFUSED))?;
        let before = self.content_revision();
        self.commit_image_mesh_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Image mesh edit could not commit."))
    }
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_oracle.rs"]
mod object_image_mesh_edit_oracle;

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_rows.rs"]
mod object_image_mesh_edit_rows;

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit.rs"]
mod object_image_mesh_edit_tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_guards.rs"]
mod object_image_mesh_edit_guards_tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_edit_payloads.rs"]
mod object_image_mesh_edit_payloads_tests;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_image_mesh_edit_retopology.rs"]
mod object_image_mesh_edit_retopology_tests;
