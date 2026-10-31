//! #2371: the per-layer facts a layer-list presenter needs, added to the existing project-info layer rows.
//!
//! Baseline `renderLayerList` (`timeline.js`, 1065e9d8) draws a kind icon, a colour dot, folder arrows, a component badge,
//! matte badges and blend/opacity indicators. Each one is read here from the native record that already stores it; nothing
//! is stored, minted or invented, and a fact with no native storage is named in `layerModelUnavailable` instead of being
//! guessed. A fact that is simply not set on a layer is omitted from its row (absent differs from a default).
//!
//! Row additions (all optional except `kind`): `kind` (the `layer-kind.js` `keyOf` taxonomy, or `unknown` when the stored
//! content contradicts itself), `folderCollapsed` (a folder layer's stored state), `folderId` (organisational membership),
//! `componentId` (the symbol an instance layer places), `blendMode`, `matteMode`, `matteSourceLayerUid`, `mattesMore`,
//! `isMatteSource`, `opacity` (the static value) and `opacityAnimated`. Top level: `layerFolders` (the organisational folder
//! rows) and `layerModelUnavailable`. The colour label stays the existing `color` row member.
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::ObjectDocument;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Facts that exist in the baseline layer row but have no native storage; a presenter reports them unavailable.
const UNAVAILABLE: [&str; 5] = [
    "linkGroup",
    "kind:camera",
    "kind:guide",
    "kind:montage",
    "kind:video",
];

#[derive(Default)]
struct Content<'a> {
    vector: bool,
    text: bool,
    raster: bool,
    first_asset: Option<&'a str>,
    several_assets: bool,
}

fn content_by_layer(document: &ObjectDocument) -> HashMap<&str, Content<'_>> {
    let mut map: HashMap<&str, Content<'_>> = HashMap::new();
    for record in document.objects() {
        if record.target.context_id == SCENE_CONTEXT {
            map.entry(record.target.layer_uid.as_str())
                .or_default()
                .vector = true;
        }
    }
    let Some(items) = document.items.value() else {
        return map;
    };
    for entry in items.raster_entries() {
        if entry.target.context_id != SCENE_CONTEXT {
            continue;
        }
        let content = map.entry(entry.target.layer_uid.as_str()).or_default();
        content.raster = true;
        let asset = entry.item.asset_id().as_str();
        match content.first_asset {
            None => content.first_asset = Some(asset),
            Some(first) if first != asset => content.several_assets = true,
            Some(_) => {}
        }
    }
    for entry in items.text_entries() {
        if entry.target.context_id == SCENE_CONTEXT {
            map.entry(entry.target.layer_uid.as_str()).or_default().text = true;
        }
    }
    map
}

/// Baseline `keyOf` order for the kinds that have native storage; content decides only after the structural flags.
fn content_kind(content: Option<&Content<'_>>) -> &'static str {
    match content {
        None => "draw",
        Some(c) if c.text && (c.raster || c.vector) => "unknown",
        Some(c) if c.text => "text",
        Some(c) if c.raster && !c.vector => {
            if c.several_assets {
                "sequence"
            } else {
                "image"
            }
        }
        Some(_) => "draw",
    }
}

/// Baseline `matteSourceIndicesInUse` (`timeline.js`, 1065e9d8): the layer indices consumed as a matte SOURCE.
/// An active matte (any mode but empty/`none`) names its source by `matteSourceLayerUid` (first other layer with that
/// uid; a dangling or self reference resolves to nothing, so that matte marks no source); a matte with no stored uid
/// falls back to the legacy implicit adjacency, the layer below (`i + 1`). Only the first matte counts: baseline never
/// consulted `mattesMore` here, so a layer used only as an additional matte source carries no badge.
fn matte_sources_in_use(document: &ObjectDocument) -> HashSet<usize> {
    let layers = document.layers();
    let mut used = HashSet::new();
    for (index, layer) in layers.iter().enumerate().filter(|(_, l)| l.matte_on()) {
        let source = match layer.matte_source_uid().filter(|uid| !uid.is_empty()) {
            Some(uid) => layers
                .iter()
                .enumerate()
                .find(|(other, l)| *other != index && l.layer_uid() == uid)
                .map(|(other, _)| other),
            None => Some(index + 1).filter(|below| *below < layers.len()),
        };
        used.extend(source);
    }
    used
}

/// One extra-member map per layer, index-aligned with `document.layers()`.
pub(crate) fn layer_members(document: &ObjectDocument) -> Vec<Map<String, Value>> {
    let content = content_by_layer(document);
    let effectors: HashSet<&str> = document
        .duplicator_set()
        .map(|set| {
            set.effector_layers
                .iter()
                .filter(|e| e.context_id == SCENE_CONTEXT)
                .map(|e| e.layer_uid.as_str())
                .collect()
        })
        .unwrap_or_default();
    let sources = matte_sources_in_use(document);
    let opacity: HashMap<&str, &crate::object_document::PropertyRecord> = document
        .property_holders()
        .unwrap_or_default()
        .iter()
        .filter(|h| h.target.context_id == SCENE_CONTEXT && h.target.stroke_id.value().is_none())
        .flat_map(|h| {
            h.properties
                .iter()
                .filter(|p| p.property == "opacity")
                .map(move |p| (h.target.layer_uid.as_str(), p))
        })
        .collect();
    document
        .layers()
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let uid = layer.layer_uid();
            let instance = document
                .component_set()
                .and_then(|set| set.instance(SCENE_CONTEXT, uid));
            let kind = if instance.is_some() {
                "component"
            } else if layer.is_effect_layer() {
                "effect"
            } else if layer.is_folder_layer() {
                "folder"
            } else if layer.is_null_layer() {
                "null"
            } else if layer.widget().is_some() {
                "widget"
            } else if effectors.contains(uid) {
                "effector"
            } else {
                content_kind(content.get(uid))
            };
            let mut row = Map::new();
            row.insert("kind".into(), json!(kind));
            if kind == "folder" {
                if let Some(collapsed) = layer.folder_collapsed.value() {
                    row.insert("folderCollapsed".into(), json!(collapsed));
                }
            }
            if let Some(folder) = layer.folder_id() {
                row.insert("folderId".into(), json!(folder));
            }
            if let Some(instance) = instance {
                row.insert("componentId".into(), json!(instance.symbol_id));
            }
            if let Some(blend) = layer.blend_mode() {
                row.insert("blendMode".into(), json!(blend));
            }
            if let Some(mode) = layer.matte_mode() {
                row.insert("matteMode".into(), json!(mode));
            }
            if let Some(source) = layer.matte_source_uid() {
                row.insert("matteSourceLayerUid".into(), json!(source));
            }
            if !layer.mattes_more().is_empty() {
                let more: Vec<Value> = layer
                    .mattes_more()
                    .iter()
                    .map(|m| json!({"uid":m.uid,"mode":m.mode}))
                    .collect();
                row.insert("mattesMore".into(), Value::Array(more));
            }
            if sources.contains(&index) {
                row.insert("isMatteSource".into(), json!(true));
            }
            if let Some(record) = opacity.get(uid) {
                if let Some([value]) = record.static_value.value().map(Vec::as_slice) {
                    row.insert("opacity".into(), Value::Number(value.clone()));
                }
                let keyed = record.track.value().is_some_and(|t| !t.keys.is_empty());
                if keyed || record.expression.value().is_some() {
                    row.insert("opacityAnimated".into(), json!(true));
                }
            }
            row
        })
        .collect()
}

/// Top-level members: the organisational folder rows (omitted when the document has none) and the typed gaps.
pub(crate) fn model_members(document: &ObjectDocument) -> Map<String, Value> {
    let mut members = Map::new();
    if let Some(folders) = document.layer_folders() {
        let rows: Vec<Value> = folders
            .iter()
            .map(|(id, folder)| {
                let mut row = json!({"folderId":id,"collapsed":folder.collapsed()});
                if let Some(name) = folder.name() {
                    row["name"] = json!(name);
                }
                row
            })
            .collect();
        if !rows.is_empty() {
            members.insert("layerFolders".into(), Value::Array(rows));
        }
    }
    members.insert("layerModelUnavailable".into(), json!(UNAVAILABLE));
    members
}

/// Adds the row members and the top-level members to a project-info value whose `layers` array is index-aligned
/// with `document.layers()`.
pub(crate) fn extend_layers(document: &ObjectDocument, info: &mut Value) {
    let rows = layer_members(document);
    if let Some(layers) = info["layers"].as_array_mut() {
        for (layer, extra) in layers.iter_mut().zip(rows) {
            if let Some(object) = layer.as_object_mut() {
                object.extend(extra);
            }
        }
    }
    if let Some(object) = info.as_object_mut() {
        object.extend(model_members(document));
    }
}

#[cfg(test)]
#[path = "../tests/object_layer_read_model.rs"]
mod tests;
