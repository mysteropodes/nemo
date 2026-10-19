//! Public/staged AudioTrack metadata route on the common receipt dispatcher.
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
pub(super) const OP_AUDIO_TRACK_EDIT: &str = "command.document.object.audio.track.edit";
impl NativeObjectHistory {
    /// Parent performs original-byte ingress, owner/replay/cancel/revision checks first.
    pub(super) fn dispatch_audio_track_edit(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        const REFUSAL: &str = "AudioTrack edit payload or persistent owner is unavailable.";
        let bytes = serde_json::to_vec(payload)
            .map_err(|_| (DispatchErrorCode::InvalidRequest, REFUSAL))?;
        let candidate = self
            .prepare_audio_track_edit(&bytes)
            .map_err(|code| (code, REFUSAL))?;
        let before = self.content_revision();
        self.commit_audio_track_edit(candidate)
            .map(|after| CompactResult::Command {
                applied: after != before,
                history_entries_added: u64::from(after != before),
            })
            .map_err(|code| (code, REFUSAL))
    }
}
