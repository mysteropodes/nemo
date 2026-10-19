//! Actual AudioTrack metadata edits in the existing persistent record table.
//! Source bytes, the sole placement offset, and Motion fields are never rewritten.
use crate::object_document::audio_tracks::{valid_audio_id, AudioTracks};
use crate::object_document::Presence;
use serde::Deserialize;
use serde_json::Number;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioTrackEditError {
    Invalid,
    NotFound,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum AudioTrackEdit {
    #[serde(rename = "setMuted")]
    SetMuted {
        #[serde(rename = "audioId")]
        audio_id: String,
        muted: bool,
    },
    #[serde(rename = "rename")]
    Rename {
        #[serde(rename = "audioId")]
        audio_id: String,
        name: String,
    },
    #[serde(rename = "setTrim")]
    SetTrim {
        #[serde(rename = "audioId")]
        audio_id: String,
        #[serde(default, rename = "trimStart")]
        trim_start: Presence<Number>,
        #[serde(default, rename = "trimEnd")]
        trim_end: Presence<Option<Number>>,
    },
    #[serde(rename = "unset")]
    Unset {
        #[serde(rename = "audioId")]
        audio_id: String,
        field: AudioTrackField,
    },
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AudioTrackField {
    Name,
    Muted,
    TrimStart,
    TrimEnd,
}
impl AudioTrackEdit {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, AudioTrackEditError> {
        if bytes.len() > 4096
            || bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
        {
            return Err(AudioTrackEditError::Invalid);
        }
        let edit: Self = serde_json::from_slice(bytes).map_err(|_| AudioTrackEditError::Invalid)?;
        let id = match &edit {
            Self::SetMuted { audio_id, .. }
            | Self::Rename { audio_id, .. }
            | Self::SetTrim { audio_id, .. }
            | Self::Unset { audio_id, .. } => audio_id,
        };
        if !valid_audio_id(id) {
            return Err(AudioTrackEditError::Invalid);
        }
        if let Self::Rename { name, .. } = &edit {
            if name.len() > 255 || name.contains('\0') {
                return Err(AudioTrackEditError::Invalid);
            }
        }
        if let Self::SetTrim {
            trim_start,
            trim_end,
            ..
        } = &edit
        {
            if trim_start.missing() && trim_end.missing() {
                return Err(AudioTrackEditError::Invalid);
            }
        }
        Ok(edit)
    }
}
// Compare a floating token with the exact integer domain only after proving
// its cast is in range. Converting BOTH inputs to f64 would collapse distinct
// integers above 2^53; Rust's saturating float casts also need strict upper bounds.
fn float_matches_integer(value: f64, integer: &Number) -> bool {
    if !value.is_finite() || value.fract() != 0. {
        return false;
    }
    if let Some(integer) = integer.as_i64() {
        value >= i64::MIN as f64 && value < -(i64::MIN as f64) && value as i64 == integer
    } else if let Some(integer) = integer.as_u64() {
        value >= 0. && value < 18_446_744_073_709_551_616. && value as u64 == integer
    } else {
        false
    }
}
fn number_equal(a: &Number, b: &Number) -> bool {
    if a == b {
        return true;
    }
    match (a.is_f64(), b.is_f64()) {
        (true, false) => a
            .as_f64()
            .is_some_and(|value| float_matches_integer(value, b)),
        (false, true) => b
            .as_f64()
            .is_some_and(|value| float_matches_integer(value, a)),
        _ => false,
    }
}
/// None means the exact existing table remains untouched, including field presence.
pub(crate) fn apply(
    before: &AudioTracks,
    edit: AudioTrackEdit,
) -> Result<Option<AudioTracks>, AudioTrackEditError> {
    let mut next = before.clone();
    let id = match &edit {
        AudioTrackEdit::SetMuted { audio_id, .. }
        | AudioTrackEdit::Rename { audio_id, .. }
        | AudioTrackEdit::SetTrim { audio_id, .. }
        | AudioTrackEdit::Unset { audio_id, .. } => audio_id,
    };
    let track = next
        .0
        .iter_mut()
        .find(|track| track.audio_id.value() == Some(id))
        .ok_or(AudioTrackEditError::NotFound)?;
    match edit {
        AudioTrackEdit::SetMuted { muted, .. } => {
            // Absent has the original false default; do not author false for a no-op.
            if track.muted.value().copied().unwrap_or(false) != muted {
                track.muted = Presence::Present(muted);
            }
        }
        AudioTrackEdit::Rename { name, .. } => track.name = Presence::Present(name),
        AudioTrackEdit::SetTrim {
            trim_start,
            trim_end,
            ..
        } => {
            if let Presence::Present(value) = trim_start {
                if !track
                    .trim_start
                    .value()
                    .is_some_and(|old| number_equal(old, &value))
                {
                    track.trim_start = Presence::Present(value);
                }
            }
            if let Presence::Present(value) = trim_end {
                let same = track
                    .trim_end
                    .value()
                    .is_some_and(|old| match (old, &value) {
                        (None, None) => true,
                        (Some(old), Some(value)) => number_equal(old, value),
                        _ => false,
                    });
                if !same {
                    track.trim_end = Presence::Present(value);
                }
            }
        }
        AudioTrackEdit::Unset { field, .. } => match field {
            AudioTrackField::Name => track.name = Presence::Missing,
            AudioTrackField::Muted => track.muted = Presence::Missing,
            AudioTrackField::TrimStart => track.trim_start = Presence::Missing,
            AudioTrackField::TrimEnd => track.trim_end = Presence::Missing,
        },
    }
    Ok((next != *before).then_some(next))
}
#[cfg(test)]
#[path = "../tests/object_audio_track_edit.rs"]
mod tests;
