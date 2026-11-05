//! Conversion followed by the one strict native admission: the converted bytes must be a project the codec accepts.
use super::{convert_omitting, Converted, LegacyImportError, MediaTranscoder, NoTranscoder, OPTIONAL_GROUPS};
use std::collections::HashMap;
use std::sync::Mutex;
use crate::history::NativeObjectHistory;

fn admitted(bytes: &[u8]) -> bool {
    NativeObjectHistory::from_project_file("legacy-import-probe", bytes).is_ok()
}

/// Convert and prove the result admissible. When the codec refuses the file, optional metadata groups (palettes, guides,
/// comments, camera and similar editor data) are dropped one at a time, each with a warning, until it admits; drawing
/// content is never dropped, so a refusal that survives that names the whole project.
pub fn convert_admitted(bytes: &[u8]) -> Result<Converted, LegacyImportError> {
    convert_admitted_with(bytes, &NoTranscoder)
}

/// Converts each payload once however many times the optional groups are bisected.
struct Memo<'a> {
    inner: &'a dyn MediaTranscoder,
    done: Mutex<HashMap<(bool, String, Vec<u8>), Option<Vec<u8>>>>,
}
impl Memo<'_> {
    fn run(&self, png: bool, mime: &str, bytes: &[u8], make: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        let key = (png, mime.to_owned(), bytes.to_vec());
        if let Some(hit) = self.done.lock().ok().and_then(|done| done.get(&key).cloned()) {
            return hit;
        }
        let made = make();
        if let Ok(mut done) = self.done.lock() {
            done.insert(key, made.clone());
        }
        made
    }
}
impl MediaTranscoder for Memo<'_> {
    fn to_png(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        self.run(true, mime, bytes, || self.inner.to_png(mime, bytes))
    }
    fn to_wav(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        self.run(false, mime, bytes, || self.inner.to_wav(mime, bytes))
    }
}

pub fn convert_admitted_with(
    bytes: &[u8],
    host: &dyn MediaTranscoder,
) -> Result<Converted, LegacyImportError> {
    let memo = Memo {
        inner: host,
        done: Mutex::new(HashMap::new()),
    };
    let transcoder: &dyn MediaTranscoder = &memo;
    let first = convert_omitting(bytes, &[], transcoder)?;
    if admitted(&first.project_file) {
        return Ok(first);
    }
    for group in OPTIONAL_GROUPS {
        let alone = convert_omitting(bytes, &[*group], transcoder)?;
        if admitted(&alone.project_file) {
            return Ok(alone);
        }
    }
    let mut dropped = Vec::new();
    for group in OPTIONAL_GROUPS {
        dropped.push(*group);
        let next = convert_omitting(bytes, &dropped, transcoder)?;
        if admitted(&next.project_file) {
            return Ok(next);
        }
    }
    let culprit = serde_json::from_slice(&first.project_file)
        .ok()
        .and_then(|file| attribute(&file, &first.stroke_paths));
    Err(culprit.unwrap_or_else(|| {
        LegacyImportError::unsupported(
            "$",
            "the native engine refused the converted drawing content",
        )
    }))
}

/// Layer members of the converted record that a slice-2 family wrote, with the baseline member a refusal names.
const LAYER_FIELDS: &[(&str, &[&str])] = &[
    ("blendMode", &["blendMode"]),
    (
        "matteMode",
        &["matteMode", "matteSourceLayerUid", "mattesMore"],
    ),
    ("isNullLayer", &["isNullLayer", "nullPos"]),
    (
        "isGuideLayer",
        &["isGuideLayer", "guidePos", "guideOrientation"],
    ),
    ("widget", &["widget"]),
    ("isFolderLayer", &["isFolderLayer", "folderCollapsed"]),
    ("folderId", &["folderId"]),
    ("keyLock", &["keyLock"]),
    ("threeD", &["threeD"]),
    ("followPath", &["followPath"]),
    ("range", &["range"]),
    ("textAnimators", &["textAnimators"]),
    ("effects", &["effects"]),
    ("effectsFrom", &["effectsFrom"]),
    ("isEffectLayer", &["isEffectLayer"]),
    ("pathFx", &["pathFx"]),
    ("groups", &["groups"]),
    ("rig", &["rig"]),
];
const DRAWING_PARTS: &[(&str, &str)] = &[
    ("layerFolders", "layerFolders"),
    ("tweens", "tweenOverrides"),
];
const TABLES: &[(&str, &str)] = &[
    ("duplicators", "duplicator"),
    ("storyboard", "storyboard"),
    ("components", "symbols"),
];

pub(crate) type StrokePaths = std::collections::HashMap<(String, u32, String), String>;

/// Remove one stroke-level family from every record at once; the first family whose removal makes the file admissible is
/// the culprit, named at the first stroke that carries it.
fn stroke_members(
    file: &serde_json::Value,
    probe: &dyn Fn(&serde_json::Value) -> bool,
    paths: &StrokePaths,
) -> Option<LegacyImportError> {
    use serde_json::Value;
    let objects = file["drawing"]["objects"].as_array()?;
    let mut groups: Vec<(&str, Vec<&str>)> = super::stroke_ext::EXT_MEMBERS
        .iter()
        .map(|(ext, legacy)| (*ext, legacy.to_vec()))
        .collect();
    groups.push(("pressure", vec!["isVectorBrush"]));
    groups.push(("effects", vec!["effects"]));
    for (member, legacy) in groups {
        let mut trial = file.clone();
        let mut first = None;
        for (index, record) in trial["drawing"]["objects"].as_array_mut()?.iter_mut().enumerate() {
            let map = record.as_object_mut()?;
            let keys: &[&str] = match member {
                "isMask" => &["isMask", "maskMode", "maskFeather"],
                "linkedFillId" => &["linkedFillId", "isLinkedFillCompanion"],
                other => &[other],
            };
            let mut present = false;
            if member == "effects" {
                present |= map.remove("effects").is_some();
            } else if let Some(ext) = map.get_mut("vectorExt").and_then(Value::as_object_mut) {
                for key in keys {
                    present |= ext.remove(*key).is_some();
                }
                if ext.is_empty() {
                    map.remove("vectorExt");
                }
            }
            if present && first.is_none() {
                first = Some(index);
            }
        }
        let Some(index) = first.filter(|_| probe(&trial)) else {
            continue;
        };
        let target = &objects[index]["target"];
        let key = (
            target["layerUid"].as_str().unwrap_or("").to_owned(),
            target["frameScope"]["frame"].as_u64().unwrap_or(0) as u32,
            target["strokeId"].as_str().unwrap_or("").to_owned(),
        );
        let base = paths.get(&key).cloned().unwrap_or_else(|| "$".to_owned());
        let named = legacy
            .iter()
            .find(|name| {
                objects[index]
                    .get("vectorExt")
                    .is_some_and(|e| e.get(member).is_some())
                    || *name == &"effects"
            })
            .unwrap_or(&legacy[0]);
        return Some(LegacyImportError::unsupported(
            format!("{base}.{named}"),
            "the native engine refused this converted stroke content",
        ));
    }
    None
}

/// The codec refused the converted drawing: remove one converted family at a time and name the first whose removal makes
/// it admissible, so the refusal points at the baseline member instead of the whole project.
fn attribute(file: &serde_json::Value, ctx_paths: &StrokePaths) -> Option<LegacyImportError> {
    use serde_json::Value;
    let probe = |candidate: &Value| admitted(&serde_json::to_vec(candidate).unwrap_or_default());
    let uids: Vec<String> = file["drawing"]["layers"]
        .as_array()?
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap_or("").to_owned())
        .collect();
    let layer_path = |uid: &str, member: &str| {
        let index = uids.iter().position(|u| u == uid).unwrap_or(0);
        format!("layers[{index}].{member}")
    };
    let refuse = |path: String| {
        LegacyImportError::unsupported(path, "the native engine refused this converted content")
    };
    for (name, members) in LAYER_FIELDS {
        let mut trial = file.clone();
        let mut first = None;
        for (index, layer) in trial["drawing"]["layers"]
            .as_array_mut()?
            .iter_mut()
            .enumerate()
        {
            let map = layer.as_object_mut()?;
            let mut present = false;
            for member in *members {
                present |= map.remove(*member).is_some();
            }
            if present && first.is_none() {
                first = Some(index);
            }
        }
        if let Some(index) = first.filter(|_| probe(&trial)) {
            let range = file["drawing"]["layers"][index]["range"].as_object();
            let legacy = match (*name, range) {
                ("range", Some(r)) => ["inPoint", "outPoint", "markers", "timeLink"]
                    .into_iter()
                    .find(|k| r.contains_key(*k))
                    .unwrap_or("inPoint"),
                _ => name,
            };
            return Some(refuse(format!("layers[{index}].{legacy}")));
        }
    }
    let holders = file["drawing"]["propertyHolders"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for (index, holder) in holders.iter().enumerate() {
        let mut trial = file.clone();
        trial["drawing"]["propertyHolders"]
            .as_array_mut()?
            .remove(index);
        if probe(&trial) {
            let context = holder["target"]["contextId"].as_str().unwrap_or("");
            let uid = holder["target"]["layerUid"].as_str().unwrap_or("");
            let member = if holder["target"].get("strokeId").is_some() {
                "elementMotion"
            } else {
                "motion"
            };
            return Some(refuse(if context == "scene-root" {
                layer_path(uid, member)
            } else {
                format!("symbols.{context}.layers.{uid}.{member}")
            }));
        }
    }
    if let Some(error) = stroke_members(file, &probe, ctx_paths) {
        return Some(error);
    }
    for (part, legacy) in DRAWING_PARTS {
        let mut trial = file.clone();
        if trial["drawing"].as_object_mut()?.remove(*part).is_some() && probe(&trial) {
            return Some(refuse((*legacy).to_owned()));
        }
    }
    for (part, legacy) in TABLES {
        let mut trial = file.clone();
        let removed = trial["drawing"]["componentTables"]
            .as_object_mut()
            .and_then(|t| t.remove(*part));
        if removed.is_some() && probe(&trial) {
            return Some(refuse((*legacy).to_owned()));
        }
    }
    None
}
