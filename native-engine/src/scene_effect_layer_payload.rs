//! Strict, bounded wire payloads of the layer compositing transactions (G11, L08-J).
//! Object-only decoding (duplicate, unknown, null and positional members refused by
//! `object_deserialize!`), a byte bound on the whole payload, and per-edit member rules checked before any
//! snapshot is read. One flat map carries every edit; which members an edit needs is a table, not a guess.
use crate::object_document::{
    object_deserialize, LayerEffect, Presence, BLEND_MODE_NAMES, MATTE_MODE_NAMES,
};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

/// The dispatcher bounds every non-vector request at 4096 bytes; the payload keeps the same bound.
pub(super) const MAX_PAYLOAD_BYTES: usize = 4096;
pub(super) const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_NAME_BYTES: usize = 256;
/// The most keys one group edit names (the read model lists at most as many blend keys).
pub(super) const MAX_GROUP_KEYS: usize = 64;

#[derive(Debug)]
pub(super) struct EditPayload {
    pub(super) command: String,
    pub(super) layer_uid: String,
    pub(super) edit: String,
    pub(super) mode: Presence<String>,
    pub(super) source: Presence<String>,
    pub(super) index: Presence<u32>,
    pub(super) collapsed: Presence<bool>,
    pub(super) effects: Presence<Vec<LayerEffect>>,
    pub(super) frames: Presence<Vec<u32>>,
    pub(super) by: Presence<i32>,
}
object_deserialize!(EditPayload {
    command: String,
    layer_uid: String,
    edit: String,
    #[serde(default)] mode: Presence<String>,
    #[serde(default)] source: Presence<String>,
    #[serde(default)] index: Presence<u32>,
    #[serde(default)] collapsed: Presence<bool>,
    #[serde(default)] effects: Presence<Vec<LayerEffect>>,
    #[serde(default)] frames: Presence<Vec<u32>>,
    #[serde(default)] by: Presence<i32>
});

#[derive(Debug)]
pub(super) struct CreatePayload {
    pub(super) command: String,
    pub(super) kind: String,
    pub(super) name: String,
}
object_deserialize!(CreatePayload {
    command: String,
    kind: String,
    name: String
});

/// A validated edit; every text is already bounded and every mode is in its vocabulary.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Edit {
    Blend(String),
    MatteSource(String),
    /// The matte menu pick: the mode, and optionally the source it freezes in the same write.
    MatteMode {
        mode: String,
        source: Option<String>,
    },
    MatteClear,
    MatteMoreAdd {
        source: String,
        mode: String,
    },
    MatteMoreSet {
        index: usize,
        source: Option<String>,
        mode: Option<String>,
    },
    MatteMoreRemove(usize),
    /// Hold-only blend key: the mode from `frame` on (an existing key at the frame is replaced).
    BlendKeySet {
        frame: u32,
        mode: String,
    },
    BlendKeyRemove(u32),
    /// Several keys of one layer removed in one write (the Delete key over a selection of Blend diamonds). Strictly
    /// increasing frames, every one of them a key.
    BlendKeysRemove(Vec<u32>),
    /// The keys at `frames` moved by `by` frames in one write (a drag or a nudge of the selected Blend diamonds): the
    /// modes travel with the keys, the group lands whole or not at all (baseline `onDragMove` / `nudgeSelectedKeys`).
    BlendKeysMove {
        frames: Vec<u32>,
        by: i32,
    },
    /// The stopwatch off: drop every key; with a mode, that mode becomes the static blend in the same write.
    BlendKeysClear(Option<String>),
    Effects(Vec<LayerEffect>),
    EffectsFromSet(String),
    EffectsFromClear,
    FolderCollapsed(bool),
}

pub(super) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)
}

pub(super) fn id_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES
}

pub(super) fn name_ok(value: &str) -> bool {
    value.len() <= MAX_NAME_BYTES
}

const FORBIDDEN: u8 = 0;
const REQUIRED: u8 = 1;
const OPTIONAL: u8 = 2;

fn allowed(present: bool, rule: u8) -> bool {
    match rule {
        FORBIDDEN => !present,
        REQUIRED => present,
        _ => true,
    }
}

/// Which of `mode, source, index, collapsed, effects, frames, by` an edit takes. A blend key's frame travels as `index`.
fn rules(edit: &str) -> Option<[u8; 7]> {
    const N: u8 = FORBIDDEN;
    Some(match edit {
        "blend" => [REQUIRED, N, N, N, N, N, N],
        "matteMode" => [REQUIRED, OPTIONAL, N, N, N, N, N],
        "matteSource" | "effectsFromSet" => [N, REQUIRED, N, N, N, N, N],
        "matteClear" | "effectsFromClear" => [N; 7],
        "matteMoreAdd" => [OPTIONAL, REQUIRED, N, N, N, N, N],
        "matteMoreSet" => [OPTIONAL, OPTIONAL, REQUIRED, N, N, N, N],
        "matteMoreRemove" => [N, N, REQUIRED, N, N, N, N],
        "blendKeySet" => [REQUIRED, N, REQUIRED, N, N, N, N],
        "blendKeyRemove" => [N, N, REQUIRED, N, N, N, N],
        "blendKeysClear" => [OPTIONAL, N, N, N, N, N, N],
        "blendKeysRemove" => [N, N, N, N, N, REQUIRED, N],
        "blendKeysMove" => [N, N, N, N, N, REQUIRED, REQUIRED],
        "effects" => [N, N, N, N, REQUIRED, N, N],
        "folderCollapsed" => [N, N, N, REQUIRED, N, N, N],
        _ => return None,
    })
}

fn bad<T>() -> Result<T, DispatchErrorCode> {
    Err(DispatchErrorCode::InvalidRequest)
}

fn matte_mode_ok(mode: &str) -> bool {
    MATTE_MODE_NAMES.contains(&mode)
}

fn source_of(payload: &EditPayload) -> Result<Option<String>, DispatchErrorCode> {
    match payload.source.value() {
        Some(source) if id_ok(source) => Ok(Some(source.clone())),
        Some(_) => bad(),
        None => Ok(None),
    }
}

/// The named keys of a group edit: 1 to `MAX_GROUP_KEYS` frames, strictly increasing (no key twice).
fn key_frames(payload: &EditPayload) -> Result<Vec<u32>, DispatchErrorCode> {
    match payload.frames.value() {
        Some(frames)
            if !frames.is_empty()
                && frames.len() <= MAX_GROUP_KEYS
                && frames.windows(2).all(|pair| pair[0] < pair[1]) =>
        {
            Ok(frames.clone())
        }
        _ => bad(),
    }
}

/// Reduce a decoded payload to one validated edit. Existence of the layers it names is the owner's check.
pub(super) fn edit_of(payload: &EditPayload) -> Result<Edit, DispatchErrorCode> {
    let rule = match rules(&payload.edit) {
        Some(rule) => rule,
        None => return bad(),
    };
    let present = [
        !payload.mode.missing(),
        !payload.source.missing(),
        !payload.index.missing(),
        !payload.collapsed.missing(),
        !payload.effects.missing(),
        !payload.frames.missing(),
        !payload.by.missing(),
    ];
    if !present
        .iter()
        .zip(rule.iter())
        .all(|(present, rule)| allowed(*present, *rule))
    {
        return bad();
    }
    let mode = payload.mode.value().cloned();
    let index = payload.index.value().map(|index| *index as usize);
    let source = source_of(payload)?;
    Ok(match payload.edit.as_str() {
        "blend" => match mode {
            Some(mode) if BLEND_MODE_NAMES.contains(&mode.as_str()) => Edit::Blend(mode),
            _ => return bad(),
        },
        "matteMode" => match mode {
            Some(mode) if matte_mode_ok(&mode) => Edit::MatteMode { mode, source },
            _ => return bad(),
        },
        "matteSource" => Edit::MatteSource(source.ok_or(DispatchErrorCode::InvalidRequest)?),
        "matteClear" => Edit::MatteClear,
        "matteMoreAdd" => {
            let mode = mode.unwrap_or_else(|| "alpha".to_string());
            if !matte_mode_ok(&mode) {
                return bad();
            }
            Edit::MatteMoreAdd {
                source: source.ok_or(DispatchErrorCode::InvalidRequest)?,
                mode,
            }
        }
        "matteMoreSet" => {
            if (mode.is_none() && source.is_none())
                || mode.as_deref().is_some_and(|m| !matte_mode_ok(m))
            {
                return bad();
            }
            Edit::MatteMoreSet {
                index: index.ok_or(DispatchErrorCode::InvalidRequest)?,
                source,
                mode,
            }
        }
        "matteMoreRemove" => Edit::MatteMoreRemove(index.ok_or(DispatchErrorCode::InvalidRequest)?),
        "blendKeySet" => match (mode, payload.index.value()) {
            (Some(mode), Some(frame)) if BLEND_MODE_NAMES.contains(&mode.as_str()) => {
                Edit::BlendKeySet { frame: *frame, mode }
            }
            _ => return bad(),
        },
        "blendKeyRemove" => Edit::BlendKeyRemove(*payload.index.value().ok_or(DispatchErrorCode::InvalidRequest)?),
        "blendKeysRemove" => Edit::BlendKeysRemove(key_frames(payload)?),
        "blendKeysMove" => match payload.by.value() {
            Some(by) if *by != 0 => Edit::BlendKeysMove {
                frames: key_frames(payload)?,
                by: *by,
            },
            _ => return bad(),
        },
        "blendKeysClear" => match mode {
            Some(mode) if !BLEND_MODE_NAMES.contains(&mode.as_str()) => return bad(),
            mode => Edit::BlendKeysClear(mode),
        },
        "effects" => Edit::Effects(payload.effects.value().cloned().unwrap_or_default()),
        "effectsFromSet" => Edit::EffectsFromSet(source.ok_or(DispatchErrorCode::InvalidRequest)?),
        "effectsFromClear" => Edit::EffectsFromClear,
        "folderCollapsed" => {
            Edit::FolderCollapsed(payload.collapsed.value().copied().unwrap_or(false))
        }
        _ => return bad(),
    })
}
