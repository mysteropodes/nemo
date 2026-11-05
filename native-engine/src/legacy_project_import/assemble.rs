//! The native drawing document of a baseline project: scene layers with their order plan, then the families that hang
//! off them (Motion holders, folders, components, duplicators, StoryBoard, tween records).
use super::layers::{self, Scope};
use super::{components, storyboard, tweens, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

fn folder_table(doc: &Map<String, Value>) -> Result<Map<String, Value>, LegacyImportError> {
    let mut out = Map::new();
    let Some(value) = doc
        .get("layerFolders")
        .filter(|v| !super::classify::is_default(v))
    else {
        return Ok(out);
    };
    for (id, entry) in super::object(value, "layerFolders")? {
        let path = format!("layerFolders.{id}");
        let members = super::object(entry, &path)?;
        let mut folder = Map::new();
        if let Some(Value::String(name)) = members.get("name") {
            folder.insert("name".into(), json!(name));
        }
        if let Some(Value::Bool(collapsed)) = members.get("collapsed") {
            folder.insert("collapsed".into(), json!(collapsed));
        }
        out.insert(id.clone(), Value::Object(folder));
    }
    Ok(out)
}

/// `state.motionArcs`: keys are `layerUid:from-to-index`; the oldest `from-to-index` form belongs to the one layer that has
/// drawn keyframes at both ends (an ambiguous entry is dropped, as the baseline does).
fn motion_arcs(
    doc: &Map<String, Value>,
    scene: &[Value],
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    let Some(value) = doc
        .get("motionArcs")
        .filter(|v| !super::classify::is_default(v))
    else {
        return Ok(None);
    };
    let uids: HashSet<&str> = scene
        .iter()
        .filter_map(|l| l["layerUid"].as_str())
        .collect();
    let drawn = |layer: &Value, frame: usize| {
        layer["frames"].get(frame).is_some_and(|f| {
            super::classify::is_default(&f["isKeyframe"]) == false
                && f["strokes"].as_array().is_some_and(|s| !s.is_empty())
        })
    };
    let mut out = Map::new();
    for (key, arc) in super::object(value, "motionArcs")? {
        let parts: Vec<&str> = key.split('-').collect();
        let old = parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        let target = if old {
            let (a, b) = (
                parts[0].parse::<usize>().unwrap_or(0),
                parts[1].parse::<usize>().unwrap_or(0),
            );
            let owners: Vec<&Value> = scene
                .iter()
                .filter(|l| drawn(l, a) && drawn(l, b))
                .collect();
            match owners.as_slice() {
                [only] => Some(format!("{}:{key}", only["layerUid"].as_str().unwrap_or(""))),
                _ => None,
            }
        } else {
            key.split_once(':')
                .filter(|(uid, _)| uids.contains(uid))
                .map(|_| key.clone())
        };
        match target {
            Some(name) => {
                let record = match arc {
                    Value::Object(members) => {
                        let mut kept = Map::new();
                        for member in ["out", "in", "cx", "cy"] {
                            if let Some(v) = members.get(member) {
                                kept.insert(member.into(), v.clone());
                            }
                        }
                        Value::Object(kept)
                    }
                    other => other.clone(),
                };
                out.insert(name, record);
            }
            None => ctx.warn(
                format!("motionArcs.{key}"),
                "dangling_motion_arc",
                "the motion arc names no single existing layer, so it is not carried over",
            ),
        }
    }
    Ok((!out.is_empty()).then_some(Value::Object(out)))
}

pub(super) fn drawing(
    doc: &Map<String, Value>,
    scene: &[Value],
    total: u32,
    ctx: &mut Ctx,
) -> Result<Value, LegacyImportError> {
    let folders = folder_table(doc)?;
    let symbol_map = doc
        .get("symbols")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let ids = components::symbol_ids(doc);
    let scope = Scope {
        context: "scene-root",
        root: "layers",
        total,
        folders: &folders,
        symbols: &ids,
    };
    let mut set = layers::convert(scene, &scope, ctx)?;
    for (index, layer) in scene.iter().enumerate() {
        let instance = layer
            .get("symbolId")
            .and_then(Value::as_str)
            .is_some_and(|s| ids.contains(s));
        if !instance
            && layer
                .get("timeRemap")
                .is_some_and(|v| !super::classify::is_default(v))
        {
            return Err(LegacyImportError::unsupported(
                format!("layers[{index}].timeRemap"),
                "Time Remap belongs to a component instance layer",
            ));
        }
    }
    let mut parts = components::convert(&symbol_map, scene, ctx)?;
    let mut holder_list = std::mem::take(&mut set.motion.holders);
    let mut dups = std::mem::take(&mut set.dup);
    let mut tables = Map::new();
    if let Some(parts) = parts.as_mut() {
        holder_list.append(&mut parts.holders);
        dups.duplicators.append(&mut parts.dup.duplicators);
        dups.effector_layers.append(&mut parts.dup.effector_layers);
        components::attach_frame_keys(&mut parts.set, &set.frame_keys);
        tables.insert("components".into(), parts.set.clone());
    }
    if let Some(board) = doc.get("storyboard") {
        if let Some(record) = storyboard::convert(board, &ids, ctx)? {
            tables.insert("storyboard".into(), record);
        }
    }
    if !dups.is_empty() {
        tables.insert("duplicators".into(), dups.to_json());
    }
    let uids: HashSet<String> = scene
        .iter()
        .filter_map(|l| l["layerUid"].as_str().map(str::to_owned))
        .collect();
    let keys: HashMap<String, Vec<u32>> = std::mem::take(&mut set.keys);
    let manual = std::mem::take(&mut set.manual);
    let tween_store = tweens::convert(
        &tweens::Input {
            doc,
            uids: &uids,
            keys: &keys,
            manual: &manual,
        },
        ctx,
    )?;
    let arcs = motion_arcs(doc, scene, ctx)?;
    let context = layers::order_context(&mut set);
    let mut drawing = json!({"format": "nemo.native-object-document", "formatVersion": 2, "totalFrames": total,
        "layers": set.records, "objects": set.objects,
        "compositionOrder": {"format": "nemo.native-object-order-plan", "formatVersion": 1, "documentFormatVersion": 2,
            "contexts": [context], "migration": "explicit-plan-required", "referencePolicy": "unresolved"}});
    if !holder_list.is_empty() {
        drawing["propertyHolders"] = Value::Array(holder_list);
    }
    if !folders.is_empty() {
        drawing["layerFolders"] = Value::Object(folders);
    }
    if !tables.is_empty() {
        drawing["componentTables"] = Value::Object(tables);
    }
    if let Some(arcs) = arcs {
        drawing["motionArcs"] = arcs;
    }
    if let Some(store) = tween_store {
        drawing["tweens"] = store;
    }
    Ok(drawing)
}
