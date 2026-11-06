//! Symbols (components) and the layers that instance them: the component table of the native document.
//! `state.symbols[id]` keeps its name, length, rate and own layers (converted exactly like scene layers, in the context
//! named by the symbol id); an instance layer keeps the `sym*` placement fields, `symMatrix`, Time Remap and the held
//! `componentFrame` / `blankOverride` picks of its outer keyframes.
use super::classify::is_default;
use super::layers::{self, Scope, Set};
use super::{dup, holders, migrate, object, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub(crate) const LAYER_MAPPED: &[&str] = &[
    "symbolId",
    "symPlayMode",
    "symSpeed",
    "symPlacedAt",
    "symSingleFrame",
    "symTrimIn",
    "symTrimOut",
    "symMatrix",
    "timeRemap",
];
/// Symbol members carried into the document-wide tween store and motion arcs (their keys name the layers inside the component).
const SYMBOL_TWEEN_TABLES: &[&str] = &["motionArcs", "tweenOverrides", "tweenEasing"];

pub(crate) struct Converted {
    pub(crate) set: Value,
    pub(crate) holders: Vec<Value>,
    pub(crate) dup: dup::Tables,
    /// Keyframe numbers and generated-frame layers of every component layer, for the tween options.
    pub(crate) keys: std::collections::HashMap<String, Vec<u32>>,
    pub(crate) sampled: HashSet<String>,
    /// The uid of every layer inside a component.
    pub(crate) layer_uids: HashSet<String>,
    /// `sym.tweenOverrides`, `sym.tweenEasing` and `sym.motionArcs`, keyed by the uid of the component layer they name.
    pub(crate) tween_overrides: Map<String, Value>,
    pub(crate) tween_easing: Map<String, Value>,
    pub(crate) motion_arcs: Map<String, Value>,
}

/// The symbol ids that exist, and so may be instanced.
pub(crate) fn symbol_ids(doc: &Map<String, Value>) -> HashSet<String> {
    doc.get("symbols")
        .and_then(Value::as_object)
        .map(|m| m.keys().filter(|k| !k.is_empty()).cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn convert(
    symbols: &Map<String, Value>,
    scene: &[Value],
    ctx: &mut Ctx,
) -> Result<Option<Converted>, LegacyImportError> {
    let ids = symbol_ids(&Map::from_iter([(
        "symbols".to_owned(),
        Value::Object(symbols.clone()),
    )]));
    let mut out = Converted {
        set: Value::Null,
        holders: Vec::new(),
        dup: dup::Tables::default(),
        keys: std::collections::HashMap::new(),
        sampled: HashSet::new(),
        layer_uids: HashSet::new(),
        tween_overrides: Map::new(),
        tween_easing: Map::new(),
        motion_arcs: Map::new(),
    };
    let mut components = Vec::new();
    for (id, symbol) in symbols {
        let path = format!("symbols.{id}");
        let members = object(symbol, &path)?;
        components.push(component(id, &path, members, &mut out, ctx)?);
    }
    let mut instances = Vec::new();
    for (index, layer) in scene.iter().enumerate() {
        let symbol = layer.get("symbolId").and_then(Value::as_str).unwrap_or("");
        if symbol.is_empty() {
            continue;
        }
        let path = format!("layers[{index}]");
        if !ids.contains(symbol) {
            ctx.warn(
                format!("{path}.symbolId"),
                "dangling_symbol",
                "the component does not exist, so the layer is a plain layer",
            );
            continue;
        }
        instances.push(instance(&path, layer, symbol, ctx)?);
    }
    if components.is_empty() && instances.is_empty() {
        return Ok(None);
    }
    out.set = json!({"format": "nemo.native-component-set", "formatVersion": 1, "components": components, "instances": instances});
    Ok(Some(out))
}

fn component(
    id: &str,
    path: &str,
    members: &Map<String, Value>,
    out: &mut Converted,
    ctx: &mut Ctx,
) -> Result<Value, LegacyImportError> {
    for (key, value) in members {
        let known = [
            "name",
            "totalFrames",
            "fps",
            "layers",
            "exposedProps",
            "markers",
            "cameraKeys",
        ];
        if !known.contains(&key.as_str())
            && !SYMBOL_TWEEN_TABLES.contains(&key.as_str())
            && !is_default(value)
        {
            ctx.warn(
                format!("{path}.{key}"),
                "unknown_field",
                "unknown member, not carried over",
            );
        }
    }
    let mut layers = members
        .get("layers")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .cloned()
        .ok_or_else(|| {
            LegacyImportError::invalid(
                format!("{path}.layers"),
                "a component holds at least one layer",
            )
        })?;
    migrate::layer_set(&mut layers, ctx);
    let layer_uids: Vec<String> = layers
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap_or("").to_owned())
        .collect();
    out.layer_uids
        .extend(layer_uids.iter().filter(|uid| !uid.is_empty()).cloned());
    for (member, table) in [
        ("tweenOverrides", &mut out.tween_overrides),
        ("tweenEasing", &mut out.tween_easing),
    ] {
        let Some(value) = members.get(member).filter(|v| !is_default(v)) else {
            continue;
        };
        let member_path = format!("{path}.{member}");
        let mut spans = object(value, &member_path)?.clone();
        migrate::tween_span_keys(&mut spans, &layer_uids);
        for (key, span) in spans {
            table.entry(key).or_insert(span);
        }
    }
    if let Some(value) = members.get("motionArcs").filter(|v| !is_default(v)) {
        let arcs = crate::history::legacy_project_import::assemble::normalize_arcs(
            value,
            &format!("{path}.motionArcs"),
            &layers,
            ctx,
        )?;
        for (key, arc) in arcs {
            out.motion_arcs.entry(key).or_insert(arc);
        }
    }
    let total = members
        .get("totalFrames")
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && *n >= 1.0 && *n <= 1_000_000.0)
        .map(|n| n as u32)
        .or_else(|| {
            layers[0]
                .get("frames")
                .and_then(Value::as_array)
                .map(|f| f.len() as u32)
                .filter(|n| *n > 0)
        })
        .ok_or_else(|| {
            LegacyImportError::unsupported(
                format!("{path}.totalFrames"),
                "a positive whole number is required",
            )
        })?;
    let root = format!("{path}.layers");
    let no_symbols = HashSet::new();
    // Organisational folders are a scene table; a component layer is never in one.
    let no_folders = Map::new();
    let scope = Scope {
        context: id,
        root: &root,
        total,
        folders: &no_folders,
        symbols: &no_symbols,
    };
    let set: Set = layers::convert(&layers, &scope, ctx)?;
    out.holders.extend(set.motion.holders);
    out.dup.duplicators.extend(set.dup.duplicators);
    out.dup.effector_layers.extend(set.dup.effector_layers);
    out.keys.extend(set.keys);
    out.sampled.extend(set.sampled);
    let mut record = json!({"symbolId": id, "totalFrames": total, "layers": set.records, "objects": set.objects});
    if let Some(Value::String(name)) = members.get("name") {
        record["name"] = json!(name);
    }
    if let Some(fps) = members
        .get("fps")
        .filter(|v| v.as_f64().is_some_and(|n| n > 0.0))
    {
        record["fps"] = fps.clone();
    }
    if let Some(Value::Array(props)) = members.get("exposedProps") {
        let mut list = Vec::new();
        for (position, prop) in props.iter().enumerate() {
            let prop_path = format!("{path}.exposedProps[{position}]");
            let item = object(prop, &prop_path)?;
            let mut entry = Map::new();
            for key in ["key", "label", "targetStrokeId", "targetField", "default"] {
                if let Some(value) = item.get(key).filter(|v| !v.is_null()) {
                    entry.insert(key.into(), value.clone());
                }
            }
            list.push(Value::Object(entry));
        }
        record["exposedProps"] = Value::Array(list);
    }
    // `sym.cameraKeys` and `sym.markers` are the component's own camera and composition markers (stored on the record,
    // admitted there); an empty table is what an entered symbol stores and carries nothing.
    for key in ["cameraKeys", "markers"] {
        if let Some(list) = members.get(key).filter(|v| !is_default(v)) {
            record[key] = list.clone();
        }
    }
    Ok(record)
}

fn instance(
    path: &str,
    layer: &Value,
    symbol: &str,
    ctx: &mut Ctx,
) -> Result<Value, LegacyImportError> {
    let uid = layer["layerUid"].as_str().unwrap_or("");
    let mut record = json!({"contextId": "scene-root", "layerUid": uid, "symbolId": symbol});
    for key in [
        "symPlayMode",
        "symSpeed",
        "symPlacedAt",
        "symSingleFrame",
        "symTrimIn",
        "symTrimOut",
        "symMatrix",
    ] {
        if let Some(value) = layer.get(key).filter(|v| !v.is_null()) {
            record[key] = value.clone();
        }
    }
    if let Some(remap) = layer.get("timeRemap").filter(|v| !is_default(v)) {
        if let Some(track) = holders::track(&format!("{path}.timeRemap"), remap, ctx)? {
            record["timeRemap"] = track;
        }
    }
    Ok(record)
}

/// Frame picks held by the instance layers, merged into their records once the layer pass has read them.
pub(crate) fn attach_frame_keys(
    set: &mut Value,
    picks: &std::collections::HashMap<String, Vec<Value>>,
) {
    let Some(instances) = set.get_mut("instances").and_then(Value::as_array_mut) else {
        return;
    };
    for instance in instances {
        let uid = instance["layerUid"].as_str().unwrap_or("").to_owned();
        if let Some(keys) = picks.get(&uid) {
            instance["frameKeys"] = Value::Array(keys.clone());
        }
    }
}
