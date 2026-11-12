//! `state.storyboard` (StoryBoard node space): modules, chains and the viewport are stored and preserved; the baseline
//! froze its interface, so nothing is evaluated here. Version-1 montages (inline `items` chips) become real instance
//! modules, exactly as `migrateLegacy` of the baseline does at load.
use super::classify::is_default;
use super::{object, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

fn number(value: Option<&Value>, fallback: f64) -> Value {
    match value {
        Some(v) if v.is_number() => v.clone(),
        _ => json!(fallback),
    }
}

pub(crate) fn convert(
    value: &Value,
    symbols: &HashSet<String>,
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    if is_default(value) {
        return Ok(None);
    }
    let board = object(value, "storyboard")?;
    let mut modules: Vec<Value> = board
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut next_id = board.get("nextId").and_then(Value::as_u64).unwrap_or(1) as u32;
    if !board.get("_v2").is_some_and(|v| !is_default(v)) {
        migrate_v1(&mut modules, &mut next_id, ctx);
    }
    let mut out = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let path = format!("storyboard.modules[{index}]");
        let members = object(module, &path)?;
        if let Some(item) = module_record(&path, members, symbols, ctx)? {
            out.push(item);
        }
    }
    let highest = out
        .iter()
        .filter_map(|m| m["id"].as_str()?.strip_prefix("sbm")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let mut record = json!({"format": "nemo.native-storyboard", "formatVersion": 1,
        "nextId": next_id.max(highest + 1), "modules": out});
    if let Some(pan) = board.get("pan").and_then(Value::as_object) {
        record["pan"] = json!({"x": number(pan.get("x"), 0.0), "y": number(pan.get("y"), 0.0)});
    }
    if let Some(zoom) = board.get("zoom").filter(|v| v.is_number()) {
        record["zoom"] = zoom.clone();
    }
    match board.get("activeMontageId") {
        Some(Value::String(id)) => record["activeMontageId"] = json!(id),
        Some(Value::Null) => record["activeMontageId"] = Value::Null,
        _ => {}
    }
    for (key, member) in board {
        let known = ["modules", "nextId", "_v2", "pan", "zoom", "activeMontageId"];
        if !known.contains(&key.as_str()) && !is_default(member) {
            ctx.warn(
                format!("storyboard.{key}"),
                "unknown_field",
                "unknown member, not carried over",
            );
        }
    }
    Ok(Some(record))
}

/// `migrateLegacy`: montage `items` chips become instance modules chained in order; inline audio becomes sound modules.
fn migrate_v1(modules: &mut Vec<Value>, next_id: &mut u32, ctx: &mut Ctx) {
    let mut added = Vec::new();
    for module in modules.iter_mut() {
        if module.get("type").and_then(Value::as_str) != Some("montage") {
            continue;
        }
        let (x, y) = (module["x"].clone(), module["y"].clone());
        let mut chain: Vec<Value> = module["chain"].as_array().cloned().unwrap_or_default();
        for item in module["items"].as_array().cloned().unwrap_or_default() {
            let id = format!("sbm{}", *next_id);
            *next_id += 1;
            let mut instance =
                json!({"id": id, "type": "instance", "symbolId": item["symbolId"], "x": x, "y": y});
            for key in ["trimIn", "trimOut", "duration"] {
                if item.get(key).is_some_and(|v| !v.is_null()) {
                    instance[key] = item[key].clone();
                }
            }
            chain.push(json!(id));
            added.push(instance);
        }
        module["chain"] = Value::Array(chain);
        if let Some(map) = module.as_object_mut() {
            map.remove("items");
        }
        if module["audio"]
            .as_array()
            .and_then(|a| a.first())
            .is_some_and(|a| a.get("dataB64").is_some_and(|d| !is_default(d)))
        {
            let mut converted = Vec::new();
            for audio in module["audio"].as_array().cloned().unwrap_or_default() {
                let id = format!("sbm{}", *next_id);
                *next_id += 1;
                added.push(json!({"id": id, "type": "sound", "name": audio["name"], "dataB64": audio["dataB64"], "x": x,
                    "y": y.as_f64().unwrap_or(0.0) + 140.0, "inSec": audio["inSec"], "outSec": audio["outSec"], "cursorSec": null}));
                converted.push(
                    json!({"moduleId": id, "offsetFrames": number(audio.get("offsetFrames"), 0.0)}),
                );
            }
            module["audio"] = Value::Array(converted);
        }
    }
    if !added.is_empty() {
        ctx.migrations.push(format!("storyboardV1:{}", added.len()));
    }
    modules.extend(added);
}

fn module_record(
    path: &str,
    members: &Map<String, Value>,
    symbols: &HashSet<String>,
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    let id = members
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LegacyImportError::invalid(format!("{path}.id"), "a module needs an id"))?;
    let kind = members.get("type").and_then(Value::as_str).unwrap_or("");
    let mut record = json!({"id": id, "type": kind, "x": number(members.get("x"), 0.0), "y": number(members.get("y"), 0.0)});
    let copy = |record: &mut Value, keys: &[&str]| {
        for key in keys {
            if let Some(value) = members.get(*key).filter(|v| !v.is_null()) {
                record[*key] = value.clone();
            }
        }
    };
    match kind {
        "instance" => {
            let symbol = members
                .get("symbolId")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !symbols.contains(symbol) {
                ctx.warn(
                    path,
                    "dangling_symbol",
                    "the module's component does not exist, so the module is not carried over",
                );
                return Ok(None);
            }
            copy(&mut record, &["symbolId", "trimIn", "trimOut", "duration"]);
        }
        "montage" => {
            record["playhead"] = number(members.get("playhead"), 0.0);
            record["chain"] = members
                .get("chain")
                .cloned()
                .filter(Value::is_array)
                .unwrap_or(json!([]));
            record["audio"] = members
                .get("audio")
                .cloned()
                .filter(Value::is_array)
                .unwrap_or(json!([]));
            copy(&mut record, &["name"]);
        }
        "sound" => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.dataB64"),
                "a StoryBoard sound needs a project audio asset, which a later import slice owns",
            ))
        }
        _ => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.type"),
                "an unknown StoryBoard module type has no native owner",
            ))
        }
    }
    Ok(Some(record))
}
