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
}
object_deserialize!(EditPayload {
    command: String,
    layer_uid: String,
    edit: String,
    #[serde(default)] mode: Presence<String>,
    #[serde(default)] source: Presence<String>,
    #[serde(default)] index: Presence<u32>,
    #[serde(default)] collapsed: Presence<bool>,
    #[serde(default)] effects: Presence<Vec<LayerEffect>>
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
    MatteMode(String),
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

/// Which of `mode, source, index, collapsed, effects` an edit takes.
fn rules(edit: &str) -> Option<[u8; 5]> {
    Some(match edit {
        "blend" | "matteMode" => [REQUIRED, FORBIDDEN, FORBIDDEN, FORBIDDEN, FORBIDDEN],
        "matteSource" | "effectsFromSet" => [FORBIDDEN, REQUIRED, FORBIDDEN, FORBIDDEN, FORBIDDEN],
        "matteClear" | "effectsFromClear" => [FORBIDDEN; 5],
        "matteMoreAdd" => [OPTIONAL, REQUIRED, FORBIDDEN, FORBIDDEN, FORBIDDEN],
        "matteMoreSet" => [OPTIONAL, OPTIONAL, REQUIRED, FORBIDDEN, FORBIDDEN],
        "matteMoreRemove" => [FORBIDDEN, FORBIDDEN, REQUIRED, FORBIDDEN, FORBIDDEN],
        "effects" => [FORBIDDEN, FORBIDDEN, FORBIDDEN, FORBIDDEN, REQUIRED],
        "folderCollapsed" => [FORBIDDEN, FORBIDDEN, FORBIDDEN, REQUIRED, FORBIDDEN],
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
            Some(mode) if matte_mode_ok(&mode) => Edit::MatteMode(mode),
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
        "effects" => Edit::Effects(payload.effects.value().cloned().unwrap_or_default()),
        "effectsFromSet" => Edit::EffectsFromSet(source.ok_or(DispatchErrorCode::InvalidRequest)?),
        "effectsFromClear" => Edit::EffectsFromClear,
        "folderCollapsed" => {
            Edit::FolderCollapsed(payload.collapsed.value().copied().unwrap_or(false))
        }
        _ => return bad(),
    })
}
