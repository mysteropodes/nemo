//! Drawing layers, keyframes and the composition-order plan of the converted drawing document.
use super::{classify, object, stroke, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};

/// The eight layer colours New and Add cycle through; used when a baseline colour is missing or unreadable.
const PALETTE: [&str; 8] = [
    "#5FA875", "#B49BE6", "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4",
];

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !classify::is_default(v))
}

pub(super) fn build(
    layers: &[Value],
    total: u32,
    ctx: &mut Ctx,
) -> Result<Value, LegacyImportError> {
    let uids: Vec<&str> = layers
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap_or(""))
        .collect();
    let mut seen = HashSet::new();
    for (index, uid) in uids.iter().enumerate() {
        if !seen.insert(*uid) {
            return Err(LegacyImportError::invalid(
                format!("layers[{index}].layerUid"),
                "two layers share one layerUid",
            ));
        }
    }
    let parents = parents(layers, &uids, ctx)?;
    let (mut records, mut objects, mut order_layers) = (Vec::new(), Vec::new(), Vec::new());
    let mut scopes: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    for (index, layer) in layers.iter().enumerate() {
        let path = format!("layers[{index}]");
        let map = object(layer, &path)?;
        for (key, value) in map {
            classify::layer_member(&path, key, value, total, ctx)?;
        }
        let (keys, drawn) = frames(&path, map, uids[index], total, ctx)?;
        for (frame, elements, mut records_of_frame) in drawn {
            scopes.entry(frame).or_default().extend(elements);
            objects.append(&mut records_of_frame);
        }
        records.push(layer_record(
            index,
            map,
            uids[index],
            parents[index],
            &keys,
            ctx,
        ));
        order_layers.push(json!({"layerUid": uids[index], "naturalPosition": index, "order": {}}));
    }
    if scopes.is_empty() {
        scopes.insert(0, Vec::new());
    }
    let scopes: Vec<Value> = scopes.into_iter().map(|(frame, elements)| json!({"frameScope": {"kind": "authored", "frame": frame}, "elements": elements})).collect();
    Ok(
        json!({"format": "nemo.native-object-document", "formatVersion": 2, "totalFrames": total, "layers": records, "objects": objects,
        "compositionOrder": {"format": "nemo.native-object-order-plan", "formatVersion": 1, "documentFormatVersion": 2,
            "contexts": [{"contextId": "scene-root", "layers": order_layers, "scopes": scopes}],
            "migration": "explicit-plan-required", "referencePolicy": "unresolved"}}),
    )
}

/// Parent uid per layer. A dangling or self parent degrades to none (baseline); a cycle refuses.
fn parents<'a>(
    layers: &'a [Value],
    uids: &[&str],
    ctx: &mut Ctx,
) -> Result<Vec<Option<&'a str>>, LegacyImportError> {
    let mut out: Vec<Option<&str>> = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        let parent = layer
            .get("parentLayerUid")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty());
        let resolved = match parent {
            Some(p) if p == uids[index] || !uids.contains(&p) => {
                ctx.warn(
                    format!("layers[{index}].parentLayerUid"),
                    "dangling_parent",
                    "the parent layer does not exist, so the layer has no parent",
                );
                None
            }
            other => other,
        };
        out.push(resolved);
    }
    for start in 0..out.len() {
        let (mut at, mut steps) = (start, 0);
        while let Some(parent) = out[at] {
            at = uids.iter().position(|u| *u == parent).unwrap_or(0);
            steps += 1;
            if steps > out.len() {
                return Err(LegacyImportError::unsupported(
                    format!("layers[{start}].parentLayerUid"),
                    "layers are parented in a cycle",
                ));
            }
        }
    }
    Ok(out)
}

fn layer_record(
    index: usize,
    map: &Map<String, Value>,
    uid: &str,
    parent: Option<&str>,
    keys: &[u32],
    ctx: &mut Ctx,
) -> Value {
    let name = match map.get("name") {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        _ => format!("Layer {}", index + 1),
    };
    let color = match map
        .get("color")
        .and_then(Value::as_str)
        .and_then(super::color::layer_hex)
    {
        Some(hex) => hex,
        None => {
            if truthy(map.get("color")) {
                ctx.warn(
                    format!("layers[{index}].color"),
                    "unreadable_color",
                    "the layer colour is not a hex colour, so a palette colour is used",
                );
            }
            PALETTE[index % PALETTE.len()].to_owned()
        }
    };
    let timing = json!({"format": "nemo.native-layer-timing", "formatVersion": 1, "family": "drawing", "contextId": "scene-root",
        "keys": keys.iter().map(|f| json!({"frame": f})).collect::<Vec<_>>(), "samples": []});
    let mut record = json!({"layerUid": uid, "name": name, "visible": map.get("visible") != Some(&Value::Bool(false)),
        "locked": truthy(map.get("locked")), "color": color, "parentLayerUid": parent, "timing": timing});
    if truthy(map.get("shy")) {
        record["shy"] = Value::Bool(true);
    }
    record
}

type Drawn = Vec<(u32, Vec<Value>, Vec<Value>)>;

/// Keyframe numbers plus, per keyframe, its order elements and its object records.
fn frames(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    total: u32,
    ctx: &mut Ctx,
) -> Result<(Vec<u32>, Drawn), LegacyImportError> {
    let (mut keys, mut drawn) = (Vec::new(), Vec::new());
    for (frame_index, frame) in map["frames"].as_array().into_iter().flatten().enumerate() {
        let frame_path = format!("{path}.frames[{frame_index}]");
        let members = object(frame, &frame_path)?;
        for (key, value) in members {
            classify::frame_member(&frame_path, key, value, ctx)?;
        }
        let strokes = members["strokes"].as_array().map_or(&[][..], Vec::as_slice);
        if truthy(members.get("isInterpolated")) {
            return Err(LegacyImportError::unsupported(
                format!("{frame_path}.isInterpolated"),
                "a generated tween frame has no native owner in this release",
            ));
        }
        let keyframe = truthy(members.get("isKeyframe"));
        let number = frame_index as u32;
        if frame_index as u64 >= u64::from(total) {
            if keyframe || !strokes.is_empty() {
                ctx.warn(
                    &frame_path,
                    "beyond_total_frames",
                    "the frame lies after the last frame of the project, so it is not carried over",
                );
            }
            continue;
        }
        if !keyframe {
            if !strokes.is_empty() {
                ctx.warn(&frame_path, "hidden_strokes", "strokes stored on a held frame are never displayed, so they are not carried over");
            }
            continue;
        }
        ctx.keyframes += 1;
        keys.push(number);
        let (mut elements, mut records, mut used) = (Vec::new(), Vec::new(), HashSet::new());
        for (position, stroke_value) in strokes.iter().enumerate() {
            let stroke_path = format!("{frame_path}.strokes[{position}]");
            let (record, stroke_id) = stroke::convert(
                &stroke_path,
                stroke_value,
                (uid, number, position),
                &mut used,
                ctx,
            )?;
            elements.push(json!({"layerUid": uid, "strokeId": stroke_id, "naturalPosition": position, "order": {}}));
            records.push(record);
            ctx.strokes += 1;
        }
        drawn.push((number, elements, records));
    }
    Ok((keys, drawn))
}
