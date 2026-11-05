//! Drawing layers, keyframes and the composition-order plan of one context (the scene or one component).
use super::{classify, dup, holders, kinds, object, stroke, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The eight layer colours New and Add cycle through; used when a baseline colour is missing or unreadable.
const PALETTE: [&str; 8] = [
    "#5FA875", "#B49BE6", "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4",
];

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !classify::is_default(v))
}

/// Everything one context's layers convert to.
#[derive(Default)]
pub(super) struct Set {
    pub(super) records: Vec<Value>,
    pub(super) objects: Vec<Value>,
    pub(super) order_layers: Vec<Value>,
    pub(super) scopes: BTreeMap<u32, Vec<Value>>,
    pub(super) motion: holders::Built,
    pub(super) dup: dup::Tables,
    /// Component frame picks (`componentFrame`, `blankOverride`) per instance layer uid.
    pub(super) frame_keys: HashMap<String, Vec<Value>>,
    /// Frames in manual tween mode: the stroke ids flagged `tweenOn`, per (layer uid, frame).
    pub(super) manual: BTreeMap<(String, u32), Vec<String>>,
    /// Keyframe numbers per layer uid.
    pub(super) keys: HashMap<String, Vec<u32>>,
}

/// Document facts the layers of one context are converted against.
pub(super) struct Scope<'a> {
    pub(super) context: &'a str,
    pub(super) root: &'a str,
    pub(super) total: u32,
    pub(super) folders: &'a Map<String, Value>,
    /// Symbol ids a layer may instance (empty inside a component: nesting is refused by the baseline).
    pub(super) symbols: &'a HashSet<String>,
}

pub(super) fn convert(
    layers: &[Value],
    scope: &Scope,
    ctx: &mut Ctx,
) -> Result<Set, LegacyImportError> {
    let uids: Vec<&str> = layers
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap_or(""))
        .collect();
    let mut seen = HashSet::new();
    for (index, uid) in uids.iter().enumerate() {
        if !seen.insert(*uid) {
            return Err(LegacyImportError::invalid(
                format!("{}[{index}].layerUid", scope.root),
                "two layers share one layerUid",
            ));
        }
    }
    let parents = parents(scope.root, layers, &uids, ctx)?;
    let env = kinds::Env {
        uids: &uids,
        folders: scope.folders,
        total: scope.total,
    };
    let mut set = Set::default();
    for (index, layer) in layers.iter().enumerate() {
        let path = format!("{}[{index}]", scope.root);
        let map = object(layer, &path)?;
        for (key, value) in map {
            classify::layer_member(&path, key, value, scope.total, ctx)?;
        }
        let uid = uids[index];
        let instance = map
            .get("symbolId")
            .and_then(Value::as_str)
            .is_some_and(|s| scope.symbols.contains(s));
        let drawn = frames(&path, map, uid, instance, scope, ctx)?;
        let mut strokes: HashMap<String, usize> = HashMap::new();
        for (frame, elements, records) in drawn.cells {
            for element in &elements {
                *strokes
                    .entry(element["strokeId"].as_str().unwrap_or("").to_owned())
                    .or_default() += 1;
            }
            set.scopes.entry(frame).or_default().extend(elements);
            set.objects.extend(records);
        }
        let mut record = layer_record(
            scope.context,
            index,
            map,
            uid,
            parents[index],
            &drawn.keys,
            &drawn.samples,
            ctx,
        );
        kinds::apply(&path, map, uid, &env, &mut record, ctx)?;
        set.records.push(record);
        set.order_layers
            .push(json!({"layerUid": uid, "naturalPosition": index, "order": {}}));
        holders::convert_layer(
            scope.context,
            &path,
            map,
            uid,
            &strokes,
            &mut set.motion,
            ctx,
        )?;
        dup::convert_layer(scope.context, &path, map, uid, &mut set.dup)?;
        for (frame, ids) in drawn.manual {
            set.manual.insert((uid.to_owned(), frame), ids);
        }
        if !drawn.frame_keys.is_empty() {
            set.frame_keys.insert(uid.to_owned(), drawn.frame_keys);
        }
        set.keys.insert(uid.to_owned(), drawn.keys);
    }
    Ok(set)
}

/// The scene's order-plan context, with the motion `order` properties folded into their entries.
pub(super) fn order_context(set: &mut Set) -> Value {
    for entry in &mut set.order_layers {
        let uid = entry["layerUid"].as_str().unwrap_or("").to_owned();
        if let Some(order) = set.motion.layer_order.get(&uid) {
            entry["order"] = order.clone();
        }
    }
    let scopes: Vec<Value> = if set.scopes.is_empty() {
        vec![json!({"frameScope": {"kind": "authored", "frame": 0}, "elements": []})]
    } else {
        std::mem::take(&mut set.scopes)
            .into_iter()
            .map(|(frame, mut elements)| {
                for element in &mut elements {
                    let key = (
                        element["layerUid"].as_str().unwrap_or("").to_owned(),
                        element["strokeId"].as_str().unwrap_or("").to_owned(),
                    );
                    if let Some(order) = set.motion.element_order.get(&key) {
                        element["order"] = order.clone();
                    }
                }
                json!({"frameScope": {"kind": "authored", "frame": frame}, "elements": elements})
            })
            .collect()
    };
    json!({"contextId": "scene-root", "layers": set.order_layers, "scopes": scopes})
}

/// Parent uid per layer. A dangling or self parent degrades to none (baseline); a cycle refuses.
fn parents<'a>(
    root: &str,
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
                    format!("{root}[{index}].parentLayerUid"),
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
                    format!("{root}[{start}].parentLayerUid"),
                    "layers are parented in a cycle",
                ));
            }
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn layer_record(
    context: &str,
    index: usize,
    map: &Map<String, Value>,
    uid: &str,
    parent: Option<&str>,
    keys: &[u32],
    samples: &[Value],
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
    let timing = json!({"format": "nemo.native-layer-timing", "formatVersion": 1, "family": "drawing", "contextId": context,
        "keys": keys.iter().map(|f| json!({"frame": f})).collect::<Vec<_>>(), "samples": samples});
    let mut record = json!({"layerUid": uid, "name": name, "visible": map.get("visible") != Some(&Value::Bool(false)),
        "locked": truthy(map.get("locked")), "color": color, "parentLayerUid": parent});
    // Only the scene context carries sparse timing; a component layer's exposure comes from its drawing frames.
    if context == "scene-root" {
        record["timing"] = timing;
    }
    if truthy(map.get("shy")) {
        record["shy"] = Value::Bool(true);
    }
    record
}

type Cells = Vec<(u32, Vec<Value>, Vec<Value>)>;

#[derive(Default)]
struct Drawn {
    keys: Vec<u32>,
    samples: Vec<Value>,
    cells: Cells,
    frame_keys: Vec<Value>,
    manual: Vec<(u32, Vec<String>)>,
}

/// Keyframe numbers, generated tween frames, and per drawn frame its order elements and object records.
fn frames(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    instance: bool,
    scope: &Scope,
    ctx: &mut Ctx,
) -> Result<Drawn, LegacyImportError> {
    let mut out = Drawn::default();
    let mut had_content = false;
    for (frame_index, frame) in map["frames"].as_array().into_iter().flatten().enumerate() {
        let frame_path = format!("{path}.frames[{frame_index}]");
        let members = object(frame, &frame_path)?;
        for (key, value) in members {
            classify::frame_member(&frame_path, key, value, ctx)?;
        }
        let strokes = members["strokes"].as_array().map_or(&[][..], Vec::as_slice);
        let number = frame_index as u32;
        let keyframe = truthy(members.get("isKeyframe"));
        let generated = !keyframe && truthy(members.get("isInterpolated"));
        if frame_index as u64 >= u64::from(scope.total) {
            if keyframe || generated || !strokes.is_empty() {
                ctx.warn(
                    &frame_path,
                    "beyond_total_frames",
                    "the frame lies after the last frame of the project, so it is not carried over",
                );
            }
            continue;
        }
        if !keyframe && !generated {
            if !strokes.is_empty() {
                ctx.warn(&frame_path, "hidden_strokes", "strokes stored on a held frame are never displayed, so they are not carried over");
            }
            continue;
        }
        let scene = scope.context == "scene-root";
        if !scene && keyframe && strokes.is_empty() && had_content {
            ctx.warn(&frame_path, "blank_keyframe_in_component", "a blank keyframe inside a component keeps showing the previous drawing, because a component layer stores no blank frames");
        }
        had_content |= !strokes.is_empty();
        if generated && !scene {
            // A generated frame inside a component is stored as an ordinary drawing.
        } else if generated {
            let mut sample = json!({"frame": number});
            if truthy(members.get("isManualEdit")) {
                sample["manualEdit"] = json!(true);
            }
            out.samples.push(sample);
        } else {
            ctx.keyframes += 1;
            out.keys.push(number);
            if instance {
                if let Some(pick) = frame_key(number, members) {
                    out.frame_keys.push(pick);
                }
            }
            if truthy(members.get("tweenManualMode")) {
                let flagged = strokes
                    .iter()
                    .filter(|s| truthy(s.get("tweenOn")))
                    .filter_map(|s| s.get("strokeId").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect();
                out.manual.push((number, flagged));
            }
        }
        if instance {
            if !strokes.is_empty() {
                ctx.warn(&frame_path, "hidden_strokes", "strokes on a component instance layer are never displayed, so they are not carried over");
            }
            continue;
        }
        let (mut elements, mut records, mut used) = (Vec::new(), Vec::new(), HashSet::new());
        for (position, stroke_value) in strokes.iter().enumerate() {
            let stroke_path = format!("{frame_path}.strokes[{position}]");
            let (mut record, stroke_id) = stroke::convert(
                &stroke_path,
                stroke_value,
                (uid, number, position),
                &mut used,
                ctx,
            )?;
            record["target"]["contextId"] = json!(scope.context);
            elements.push(json!({"layerUid": uid, "strokeId": stroke_id, "naturalPosition": position, "order": {}}));
            records.push(record);
            ctx.strokes += 1;
        }
        out.cells.push((number, elements, records));
    }
    Ok(out)
}

/// `componentFrame` / `blankOverride` held on one outer keyframe of an instance layer.
fn frame_key(frame: u32, members: &Map<String, Value>) -> Option<Value> {
    let mut pick = json!({"frame": frame});
    let mut any = false;
    if let Some(value) = members.get("componentFrame").filter(|v| v.is_number()) {
        pick["componentFrame"] = value.clone();
        any = true;
    }
    if truthy(members.get("blankOverride")) {
        pick["blankOverride"] = json!(true);
        any = true;
    }
    any.then_some(pick)
}
