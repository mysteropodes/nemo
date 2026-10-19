//! One AudioTrack metadata edit is one existing undo/history transaction.
use crate::commands::PreparedAudioTrackEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
impl NativeObjectHistory {
    pub(crate) fn prepare_audio_track_edit(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedAudioTrackEdit, DispatchErrorCode> {
        self.owner.prepare_audio_track_edit(bytes)
    }
    pub(crate) fn commit_audio_track_edit(
        &mut self,
        candidate: PreparedAudioTrackEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_audio_track_edit(candidate)?;
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
#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_audio_track_edit_history.rs"]
mod tests;
