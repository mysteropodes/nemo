//! Conversion followed by the one strict native admission: the converted bytes must be a project the codec accepts.
use super::{convert_omitting, Converted, LegacyImportError, OPTIONAL_GROUPS};
use crate::history::NativeObjectHistory;

fn admitted(bytes: &[u8]) -> bool {
    NativeObjectHistory::from_project_file("legacy-import-probe", bytes).is_ok()
}

/// Convert and prove the result admissible. When the codec refuses the file, optional metadata groups (palettes, guides,
/// comments, camera and similar editor data) are dropped one at a time, each with a warning, until it admits; drawing
/// content is never dropped, so a refusal that survives that names the whole project.
pub fn convert_admitted(bytes: &[u8]) -> Result<Converted, LegacyImportError> {
    let first = convert_omitting(bytes, &[])?;
    if admitted(&first.project_file) {
        return Ok(first);
    }
    for group in OPTIONAL_GROUPS {
        let alone = convert_omitting(bytes, &[*group])?;
        if admitted(&alone.project_file) {
            return Ok(alone);
        }
    }
    let mut dropped = Vec::new();
    for group in OPTIONAL_GROUPS {
        dropped.push(*group);
        let next = convert_omitting(bytes, &dropped)?;
        if admitted(&next.project_file) {
            return Ok(next);
        }
    }
    let culprit = serde_json::from_slice(&first.project_file)
        .ok()
        .and_then(|file| attribute(&file));
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

/// The codec refused the converted drawing: remove one converted family at a time and name the first whose removal makes
/// it admissible, so the refusal points at the baseline member instead of the whole project.
fn attribute(file: &serde_json::Value) -> Option<LegacyImportError> {
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
