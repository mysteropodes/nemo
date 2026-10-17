//! A placement edit is one transaction in the existing object history (G42).
use crate::commands::PreparedPlacementEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    pub(crate) fn prepare_placement_edit(
        &self,
        edit: PlacementEdit,
    ) -> Result<PreparedPlacementEdit, DispatchErrorCode> {
        self.owner.prepare_placement_edit(edit)
    }

    /// A changed edit pushes one undo entry and clears redo; a no-op does neither.
    pub(crate) fn commit_placement_edit(
        &mut self,
        candidate: PreparedPlacementEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_placement_edit(candidate)?;
        if before_revision != after_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_media_link_history.rs"]
mod tests;
