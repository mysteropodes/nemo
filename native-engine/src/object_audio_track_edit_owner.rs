//! Sealed persistent AudioTrack edits on the existing immutable project owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    audio_track_edit::{self, AudioTrackEdit, AudioTrackEditError},
    ObjectDocument, Presence,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

fn code(error: AudioTrackEditError) -> DispatchErrorCode {
    match error {
        AudioTrackEditError::Invalid => DispatchErrorCode::InvalidRequest,
        AudioTrackEditError::NotFound => DispatchErrorCode::NotFound,
    }
}
#[derive(Debug)]
pub(crate) struct PreparedAudioTrackEdit {
    origin: ObjectSnapshot,
    next: Option<ObjectDocument>,
}
impl ObjectFillOwner {
    pub(crate) fn prepare_audio_track_edit(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedAudioTrackEdit, DispatchErrorCode> {
        let edit = AudioTrackEdit::parse(bytes).map_err(code)?;
        let origin = self.current_snapshot();
        let project = origin.project().ok_or(DispatchErrorCode::Unavailable)?;
        let inventory = project
            .asset_inventory()
            .ok_or(DispatchErrorCode::Unavailable)?;
        // Resolve the real persistent source relationship before making any candidate.
        crate::object_document::media_timing::object_media_link_edit::check_inventory(
            project.drawing(),
            inventory,
        )
        .map_err(|refusal| {
            if refusal.unresolved() {
                DispatchErrorCode::NotFound
            } else {
                DispatchErrorCode::Unavailable
            }
        })?;
        let tracks = project
            .drawing()
            .audio_tracks
            .value()
            .ok_or(DispatchErrorCode::NotFound)?;
        let next = audio_track_edit::apply(tracks, edit)
            .map_err(code)?
            .map(|tracks| {
                let mut drawing = project.drawing().clone();
                drawing.audio_tracks = Presence::Present(tracks);
                drawing
            });
        if let Some(drawing) = &next {
            ObjectSnapshot::validate_drawing(drawing)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
            crate::object_document::media_timing::object_media_link_edit::check_inventory(
                drawing, inventory,
            )
            .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
        Ok(PreparedAudioTrackEdit { origin, next })
    }
    pub(crate) fn commit_audio_track_edit(
        &mut self,
        candidate: PreparedAudioTrackEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let Some(drawing) = candidate.next else {
            return Ok(current.content_revision());
        };
        // Existing publish retains the SAME ProjectEnvelope inventory and source Arcs.
        self.revisions
            .publish(drawing)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
