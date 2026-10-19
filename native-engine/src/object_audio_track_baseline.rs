//! Lossless baseline-field normalization to the EXISTING project source/catalog owner.
//! This constructs wire candidates only; callers must use managed project admission.
//! dataB64 is represented by ONE retained asset, offsetFrames by ONE placement.
use super::AudioTrackRecord;
use crate::object_document::{AssetId, AssetInventory, AssetPlacement, AssetTarget, Presence};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Number, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioBaselineError {
    Invalid,
    Unavailable,
    Limit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Baseline {
    data_b64: String,
    offset_frames: Number,
    #[serde(default)]
    name: Presence<String>,
    #[serde(default)]
    audio_id: Presence<String>,
    #[serde(default)]
    volume: Presence<Number>,
    #[serde(default)]
    muted: Presence<bool>,
    #[serde(default)]
    trim_start: Presence<Number>,
    #[serde(default)]
    trim_end: Presence<Option<Number>>,
    #[serde(default, deserialize_with = "super::unique_optional_map")]
    motion:
        Presence<Option<std::collections::BTreeMap<String, crate::object_document::OrderTrack>>>,
    #[serde(default, deserialize_with = "super::unique_optional_map")]
    motion_static: Presence<Option<std::collections::BTreeMap<String, Vec<Number>>>>,
}
pub(crate) struct NormalizedAudioTrack {
    pub(crate) record: AudioTrackRecord,
    pub(crate) placement: AssetPlacement,
    pub(crate) source_wire: Value,
}
/// asset/placement IDs are supplied by the existing project producer, never guessed here.
/// Missing baseline AudioID remains unavailable, rather than silently minting an owner.
pub(crate) fn normalize_baseline_track(
    bytes: &[u8],
    asset_id: &str,
    placement_id: &str,
    max_input_bytes: usize,
) -> Result<NormalizedAudioTrack, AudioBaselineError> {
    if max_input_bytes == 0 || bytes.len() > max_input_bytes {
        return Err(AudioBaselineError::Limit);
    }
    // Derived struct admission also accepts positional arrays. Require the
    // declared object on original bytes; retain serde's duplicate/type checks.
    if bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{') {
        return Err(AudioBaselineError::Invalid);
    }
    let input: Baseline = serde_json::from_slice(bytes).map_err(|_| AudioBaselineError::Invalid)?;
    let uid = input
        .audio_id
        .value()
        .filter(|id| super::valid_audio_id(id))
        .ok_or(AudioBaselineError::Unavailable)?;
    let data = input
        .data_b64
        .strip_prefix("data:audio/wav;base64,")
        .ok_or(AudioBaselineError::Unavailable)?;
    if data.is_empty() {
        return Err(AudioBaselineError::Invalid);
    }
    let id = AssetId::new(asset_id.into()).map_err(|_| AudioBaselineError::Invalid)?;
    let placement = AssetPlacement::new_number(
        placement_id.into(),
        id,
        AssetTarget::AudioTrack {
            track_uid: uid.clone(),
        },
        input.offset_frames,
    )
    .map_err(|_| AudioBaselineError::Invalid)?;
    let title = input
        .name
        .value()
        .filter(|s| !s.trim().is_empty())
        .map(String::as_str)
        .unwrap_or(asset_id);
    let source_wire = json!({"id":asset_id,"name":title,"kind":"audio",
        "source":{"kind":"embedded","mediaType":"audio/wav","encoding":"base64","data":data}});
    let record = AudioTrackRecord {
        asset_id: asset_id.into(),
        audio_id: input.audio_id,
        name: input.name,
        volume: input.volume,
        muted: input.muted,
        trim_start: input.trim_start,
        trim_end: input.trim_end,
        motion: input.motion,
        motion_static: input.motion_static,
    };
    Ok(NormalizedAudioTrack {
        record,
        placement,
        source_wire,
    })
}
/// Read the actual stored catalog/placement. No default source, offset or fake decoded buffer.
pub(crate) fn baseline_track_wire(
    track: &AudioTrackRecord,
    inventory: &AssetInventory,
    max_output_bytes: usize,
) -> Result<Value, AudioBaselineError> {
    let uid = track
        .audio_id
        .value()
        .ok_or(AudioBaselineError::Unavailable)?;
    let asset = inventory
        .records()
        .iter()
        .find(|r| r.id().as_str() == track.asset_id)
        .ok_or(AudioBaselineError::Unavailable)?;
    if !asset.source().has_audio_admission() {
        return Err(AudioBaselineError::Unavailable);
    }
    let allocation = asset
        .source()
        .encoded_allocation()
        .ok_or(AudioBaselineError::Unavailable)?;
    let encoded_len =
        base64::encoded_len(allocation.len(), true).ok_or(AudioBaselineError::Limit)?;
    if max_output_bytes == 0
        || encoded_len
            .checked_add(22)
            .is_none_or(|n| n > max_output_bytes)
    {
        return Err(AudioBaselineError::Limit);
    }
    let placement = inventory
        .placements()
        .iter()
        .find(|p| {
            p.asset_id().as_str() == track.asset_id
                && matches!(p.target(), AssetTarget::AudioTrack { track_uid } if track_uid == uid)
        })
        .ok_or(AudioBaselineError::Unavailable)?;
    let mut out = serde_json::to_value(track).map_err(|_| AudioBaselineError::Invalid)?;
    let object = out.as_object_mut().ok_or(AudioBaselineError::Invalid)?;
    object.remove("assetId");
    object.insert(
        "dataB64".into(),
        json!(format!(
            "data:audio/wav;base64,{}",
            STANDARD.encode(allocation.as_slice())
        )),
    );
    object.insert("offsetFrames".into(), json!(placement.offset_frames()));
    if serde_json::to_vec(&out)
        .map_err(|_| AudioBaselineError::Invalid)?
        .len()
        > max_output_bytes
    {
        return Err(AudioBaselineError::Limit);
    }
    Ok(out)
}
#[cfg(test)]
#[path = "../tests/object_audio_track_baseline.rs"]
mod tests;
