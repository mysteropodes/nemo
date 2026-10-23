//! L06-L: raster-item and text-block edits use the existing history and receipt owner.
use crate::commands::PreparedItemEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    pub(crate) fn prepare_item_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedItemEdit, DispatchErrorCode> {
        self.owner.prepare_item_edit_json(bytes)
    }
    /// A history entry only when the revision changed: an exact no-op keeps both stacks.
    pub(crate) fn commit_item_edit(
        &mut self,
        candidate: PreparedItemEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_item_edit(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_commands.rs"]
mod object_raster_asset_commands_tests;
#[cfg(test)]
#[path = "../tests/object_text_layout_commands.rs"]
mod object_text_layout_commands_tests;
#[cfg(test)]
#[path = "../tests/object_raster_asset_commands_support.rs"]
pub(crate) mod support;
