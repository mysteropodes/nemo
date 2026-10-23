//! Encoded Audio joins the SAME managed asset decoder/allocation callbacks.
//! Byte admission is strict WAV container admission, not native playback/decode.
use crate::object_codec::project::asset_wire::payload;
use crate::object_codec::project::ProjectWireError;
use crate::object_document::{audio_wave, AssetSource};
use serde_json::value::RawValue;
use std::sync::Arc;

pub(super) fn prepare(
    raw: &RawValue,
    cap: Option<usize>,
) -> Result<(String, usize), ProjectWireError> {
    payload::prepare(raw, cap.ok_or(ProjectWireError::Unsupported)?)
}
pub(super) fn decode(
    data: &str,
    length: usize,
    cap: Option<usize>,
    allocate: &mut impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
) -> Result<(AssetSource, Arc<Vec<u8>>), ProjectWireError> {
    let cap = cap.ok_or(ProjectWireError::Unsupported)?;
    let allocation = Arc::new(payload::decode(data, length, allocate)?);
    let admitted =
        audio_wave::admit_wave(Arc::clone(&allocation), cap).map_err(|error| match error {
            audio_wave::WaveError::Invalid => ProjectWireError::Invalid,
            audio_wave::WaveError::Unsupported => ProjectWireError::Unsupported,
            audio_wave::WaveError::Limit => ProjectWireError::Budget,
        })?;
    Ok((AssetSource::admitted_audio(admitted), allocation))
}

/// Check record-to-catalog references BEFORE encoded allocation or owner identity.
pub(super) fn validate_tracks(
    document: &crate::object_document::ObjectDocument,
    kind: impl Fn(&str) -> Option<crate::object_document::AssetKind>,
) -> Result<(), ProjectWireError> {
    if let Some(tracks) = document.audio_tracks.value() {
        for track in tracks.records() {
            if kind(&track.asset_id) != Some(crate::object_document::AssetKind::Audio) {
                return Err(ProjectWireError::Invalid);
            }
        }
    }
    Ok(())
}
