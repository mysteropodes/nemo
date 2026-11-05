//! Motion: `motion`, `motionStatic`, `expressions`, `exprControls` and `elementMotion` of baseline layers become the
//! frame-independent property holders of the native document; the `order` property belongs to the composition-order
//! plan. A holder is the baseline `ld` (layer holder) or `ld.elementMotion[strokeId]` (element holder).
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Members of a motion key the native key record owns.
const KEY_MEMBERS: &[&str] = &["frame", "v", "hold", "lockTo", "curvePoints", "hIn", "hOut"];
/// The longest expression body the native record admits.
const MAX_CODE_BYTES: usize = 4096;

pub(crate) const MAPPED: &[&str] = &[
    "motion",
    "motionStatic",
    "elementMotion",
    "expressions",
    "exprControls",
];

#[derive(Default)]
pub(crate) struct Built {
    pub(crate) holders: Vec<Value>,
    /// `order` per layer uid, for the order plan layer entries.
    pub(crate) layer_order: BTreeMap<String, Value>,
    /// `order` per (layer uid, stroke id), for the order plan element entries.
    pub(crate) element_order: BTreeMap<(String, String), Value>,
}

/// One layer's motion data: `strokes` is how many authored drawings each stroke id appears in.
pub(crate) fn convert_layer(
    context: &str,
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    strokes: &HashMap<String, usize>,
    out: &mut Built,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    let controls = controls(path, map)?;
    let (props, order) = properties(path, map, context == "scene-root", ctx)?;
    if !props.is_empty() || !controls.is_empty() {
        let mut holder =
            json!({"target": {"contextId": context, "layerUid": uid}, "properties": props});
        if !controls.is_empty() {
            holder["controls"] = Value::Array(controls);
        }
        out.holders.push(holder);
    }
    if let Some(order) = order {
        out.layer_order.insert(uid.to_owned(), order);
    }
    let Some(elements) = map.get("elementMotion").filter(|v| !is_default(v)) else {
        return Ok(());
    };
    let elements = super::object(elements, &format!("{path}.elementMotion"))?;
    for (stroke_id, value) in elements {
        let element_path = format!("{path}.elementMotion.{stroke_id}");
        let members = super::object(value, &element_path)?;
        let Some(&drawings) = strokes.get(stroke_id) else {
            if holder_has_content(members) {
                ctx.warn(
                    element_path,
                    "dangling_element_motion",
                    "the shape does not exist in any drawing, so its motion is not carried over",
                );
            }
            continue;
        };
        let (props, order) = properties(&element_path, members, context == "scene-root", ctx)?;
        if !props.is_empty() {
            out.holders.push(json!({"target": {"contextId": context, "layerUid": uid, "strokeId": stroke_id}, "properties": props}));
        }
        if let Some(order) = order {
            if drawings != 1 {
                return Err(LegacyImportError::unsupported(
                    format!("{element_path}.motion.order"),
                    "a shape order that spans several drawings has no native owner in this release",
                ));
            }
            out.element_order
                .insert((uid.to_owned(), stroke_id.clone()), order);
        }
    }
    Ok(())
}

fn holder_has_content(map: &Map<String, Value>) -> bool {
    ["motion", "motionStatic", "expressions"]
        .iter()
        .any(|key| map.get(*key).is_some_and(|v| !is_default(v)))
}

fn controls(path: &str, map: &Map<String, Value>) -> Result<Vec<Value>, LegacyImportError> {
    let Some(value) = map.get("exprControls").filter(|v| !is_default(v)) else {
        return Ok(Vec::new());
    };
    let list = value.as_array().ok_or_else(|| {
        LegacyImportError::unsupported(
            format!("{path}.exprControls"),
            "a list of controls is required",
        )
    })?;
    let mut out = Vec::new();
    for (position, control) in list.iter().enumerate() {
        let control_path = format!("{path}.exprControls[{position}]");
        let members = super::object(control, &control_path)?;
        let mut item = Map::new();
        for key in ["key", "name", "type", "default"] {
            if let Some(value) = members.get(key).filter(|v| !v.is_null()) {
                item.insert(key.into(), value.clone());
            }
        }
        if !item.contains_key("key") || !item.contains_key("name") || !item.contains_key("type") {
            return Err(LegacyImportError::unsupported(
                control_path,
                "a control needs a key, a name and a type",
            ));
        }
        out.push(Value::Object(item));
    }
    Ok(out)
}

fn number_list(path: &str, value: &Value) -> Result<Vec<Value>, LegacyImportError> {
    let items = match value {
        Value::Array(items) => items.clone(),
        Value::Number(_) => vec![value.clone()],
        _ => {
            return Err(LegacyImportError::unsupported(
                path,
                "a list of numbers is required",
            ))
        }
    };
    if items.iter().any(|n| n.as_f64().is_none()) {
        return Err(LegacyImportError::unsupported(
            path,
            "a list of numbers is required",
        ));
    }
    Ok(items)
}

fn key(path: &str, value: &Value, ctx: &mut Ctx) -> Result<(f64, Value), LegacyImportError> {
    let members = super::object(value, path)?;
    let frame = members
        .get("frame")
        .and_then(Value::as_f64)
        .ok_or_else(|| LegacyImportError::unsupported(path, "a key needs a frame number"))?;
    let mut out = Map::new();
    out.insert("frame".into(), members["frame"].clone());
    out.insert(
        "v".into(),
        Value::Array(number_list(
            &format!("{path}.v"),
            members.get("v").unwrap_or(&Value::Null),
        )?),
    );
    for (name, value) in members {
        if name == "frame" || name == "v" {
            continue;
        }
        if KEY_MEMBERS.contains(&name.as_str()) {
            if !value.is_null() {
                out.insert(name.clone(), value.clone());
            }
        } else if !is_default(value) {
            ctx.warn(
                format!("{path}.{name}"),
                "unknown_field",
                "unknown member, not carried over",
            );
        }
    }
    Ok((frame, Value::Object(out)))
}

pub(crate) fn track(
    path: &str,
    value: &Value,
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    let members = super::object(value, path)?;
    let Some(list) = members.get("keys").filter(|v| !is_default(v)) else {
        return Ok(None);
    };
    let list = list
        .as_array()
        .ok_or_else(|| LegacyImportError::unsupported(path, "a list of keys is required"))?;
    let mut keys: Vec<(f64, Value)> = Vec::new();
    for (position, item) in list.iter().enumerate() {
        keys.push(key(&format!("{path}.keys[{position}]"), item, ctx)?);
    }
    // `sortKeys` orders by frame; two keys on one frame keep the later one.
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut kept: Vec<(f64, Value)> = Vec::new();
    for entry in keys {
        if kept.last().is_some_and(|last| last.0 == entry.0) {
            ctx.warn(
                path,
                "duplicate_key_frame",
                "two keys share one frame, so the later one is kept",
            );
            kept.pop();
        }
        kept.push(entry);
    }
    Ok(Some(
        json!({"keys": kept.into_iter().map(|k| k.1).collect::<Vec<_>>()}),
    ))
}

fn expression(path: &str, value: &Value) -> Result<Option<Value>, LegacyImportError> {
    let members = super::object(value, path)?;
    let code = members.get("code").and_then(Value::as_str).unwrap_or("");
    let enabled = members.get("enabled").is_some_and(|v| !is_default(v));
    if code.is_empty() && !enabled {
        return Ok(None);
    }
    if code.len() > MAX_CODE_BYTES {
        return Err(LegacyImportError::unsupported(
            format!("{path}.code"),
            "an expression longer than 4096 bytes has no native owner",
        ));
    }
    Ok(Some(json!({"enabled": enabled, "code": code})))
}

/// The property records of one holder, plus its `order` property apart (it lives in the composition-order plan).
fn properties(
    path: &str,
    map: &Map<String, Value>,
    scene: bool,
    ctx: &mut Ctx,
) -> Result<(Vec<Value>, Option<Value>), LegacyImportError> {
    let section = |name: &str| -> Result<Option<&Map<String, Value>>, LegacyImportError> {
        match map.get(name).filter(|v| !is_default(v)) {
            None => Ok(None),
            Some(value) => super::object(value, &format!("{path}.{name}")).map(Some),
        }
    };
    let (motion, statics, expressions) = (
        section("motion")?,
        section("motionStatic")?,
        section("expressions")?,
    );
    let mut names: Vec<&String> = Vec::new();
    let mut seen = HashSet::new();
    for source in [motion, statics, expressions].into_iter().flatten() {
        for name in source.keys() {
            if seen.insert(name) {
                names.push(name);
            }
        }
    }
    let (mut out, mut order) = (Vec::new(), None);
    for name in names {
        let mut record = Map::new();
        if let Some(value) = statics.and_then(|s| s.get(name)).filter(|v| !is_default(v)) {
            let list = number_list(&format!("{path}.motionStatic.{name}"), value)?;
            record.insert("static".into(), Value::Array(list));
        }
        if let Some(value) = motion.and_then(|m| m.get(name)).filter(|v| !is_default(v)) {
            if let Some(track) = track(&format!("{path}.motion.{name}"), value, ctx)? {
                record.insert("track".into(), track);
            }
        }
        if let Some(value) = expressions
            .and_then(|e| e.get(name))
            .filter(|v| !is_default(v))
        {
            if let Some(expr) = expression(&format!("{path}.expressions.{name}"), value)? {
                record.insert("expression".into(), expr);
            }
        }
        if record.is_empty() {
            continue;
        }
        if name == "order" {
            if !scene {
                return Err(LegacyImportError::unsupported(
                    format!("{path}.motion.order"),
                    "a component layer order has no native owner in this release",
                ));
            }
            order = Some(Value::Object(record));
        } else {
            record.insert("property".into(), json!(name));
            out.push(Value::Object(record));
        }
    }
    Ok((out, order))
}
