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
//! `isMatteSource`, `timeLinkUid` and `followPathUid` (#2372: the stored Link time source and Follow path target), `followPathAlign` (true only: the orient toggle of that path), `keyLock` (D37: the layer's standing key-lock mode `in`, `out` or `layer`; omitted when none), `hasDuplicator` (true only: the layer hosts a stored duplicator) with `dupSourceMode` (`shape` or `objects`, as that duplicator stores it), `opacity` (the static value), `opacityAnimated`, and (#2372) `effects` (the layer's stored effect stack, so the
//! effects panel can send a complete replacement) or `effectsOversize` when the stack is past the row or document bound, and the same for the path-effect stack (`pathFx`, `pathFxOversize`). Top level: `layerFolders` (the organisational folder
//! rows) and `layerModelUnavailable`. The colour label stays the existing `color` row member.
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::ObjectDocument;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// #2372: a layer's effect stack travels in its row up to this many encoded bytes. The bound is the dispatcher's own
/// 4096-byte request bound, so it is never below what an edit can write: a full-stack edit request carries the stack
/// plus at most ~215 bytes of envelope (measured with a 128-byte layer uid), hence any stack an accepted edit leaves
/// behind is strictly smaller than this and stays listed. Measured baseline entries (`{type,enabled,p1..p8}`, as the
/// panel adds them) are 105-127 bytes, path-effect entries 58-65, so the row holds ~32-39 effects or ~66 path effects.
/// The document-wide budget keeps the info answer inside the 4 MiB project-info bound.
const EFFECTS_ROW_BYTES: usize = 4096;
const EFFECTS_TOTAL_BYTES: usize = 1024 * 1024;

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
    // One uid index per info build: a per-matte linear scan made this O(layers x matted layers) on every revision sync.
    let mut by_uid: HashMap<&str, usize> = HashMap::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        by_uid.entry(layer.layer_uid()).or_insert(index);
    }
    let mut used = HashSet::new();
    for (index, layer) in layers.iter().enumerate().filter(|(_, l)| l.matte_on()) {
        let source = match layer.matte_source_uid().filter(|uid| !uid.is_empty()) {
            Some(uid) => by_uid.get(uid).copied().filter(|other| *other != index),
            None => Some(index + 1).filter(|below| *below < layers.len()),
        };
        used.extend(source);
    }
    used
}

/// #2372: a stored stack travels in its row up to the per-row and document-wide byte bounds; past either it is named
/// oversize, never truncated.
fn carry_stack<T: serde::Serialize>(
    row: &mut Map<String, Value>,
    stack: &[T],
    key: &str,
    oversize_key: &str,
    budget: &mut usize,
) {
    let encoded = serde_json::to_value(stack).ok();
    let size = encoded
        .as_ref()
        .map_or(usize::MAX, |value| value.to_string().len());
    match encoded {
        Some(value) if size <= EFFECTS_ROW_BYTES && size <= *budget => {
            *budget -= size;
            row.insert(key.into(), value);
        }
        _ => {
            row.insert(oversize_key.into(), json!(true));
        }
    }
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
    // #2256: the layers that host a duplicator and its source mode (the baseline `ld.duplicator` / `.sourceMode`, `shape` when unset).
    let hosts: HashMap<&str, &str> = document
        .duplicator_set()
        .map(|set| {
            set.duplicators
                .iter()
                .filter(|d| d.context_id == SCENE_CONTEXT)
                .map(|d| {
                    let mode = d.duplicator.source_mode.value().map_or("shape", String::as_str);
                    (d.layer_uid.as_str(), if mode == "objects" { "objects" } else { "shape" })
                })
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
    let mut effects_budget = EFFECTS_TOTAL_BYTES;
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
            // #2372: the stored Link time source and Follow path target, so the layer menus can mark the current pick.
            if let Some(link) = layer
                .range
                .value()
                .and_then(|range| range.time_link.value())
                .and_then(Option::as_ref)
            {
                row.insert("timeLinkUid".into(), json!(link.uid));
            }
            if let Some(follow) = layer.follow_path.value().and_then(Option::as_ref) {
                row.insert("followPathUid".into(), json!(follow.target_layer_uid()));
                if follow.align() {
                    row.insert("followPathAlign".into(), json!(true));
                }
            }
            // D37: the layer's standing key-lock mode (in/out/layer) so the Motion lock menu can tick it; absent or null reads as none.
            if let Some(mode) = layer.key_lock.value().copied().flatten() {
                row.insert("keyLock".into(), json!(mode));
            }
            if let Some(mode) = hosts.get(uid) {
                row.insert("hasDuplicator".into(), json!(true));
                row.insert("dupSourceMode".into(), json!(mode));
            }
            if let Some(stack) = layer.effects().filter(|stack| !stack.is_empty()) {
                carry_stack(&mut row, stack, "effects", "effectsOversize", &mut effects_budget);
            }
            // #2372: the path-effect stack (baseline `ld.pathFx`) rides the same way, so the path-effects panel can resend it whole.
            if let Some(stack) = layer.path_fx().filter(|stack| !stack.is_empty()) {
                carry_stack(&mut row, stack, "pathFx", "pathFxOversize", &mut effects_budget);
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
#[cfg(test)]
#[path = "../tests/object_layer_read_model_duplicator.rs"]
mod duplicator_tests;
#[cfg(test)]
#[path = "../tests/object_layer_read_model_key_lock.rs"]
mod key_lock_tests;
