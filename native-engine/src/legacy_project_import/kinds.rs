//! Layer kind, compositing and organisation members of a baseline layer (slice 2): blend and mattes, Null, Guide, Widget
//! and Folder layers, the folder table, the stored in/out range with its time link and markers, key lock, 3D and
//! follow path. Each member keeps the baseline name and value, so the native layer record is the baseline member.
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};

/// Document facts every member is checked against.
pub(crate) struct Env<'a> {
    pub(crate) uids: &'a [&'a str],
    pub(crate) folders: &'a Map<String, Value>,
    pub(crate) total: u32,
}

/// The layer members this module converts (they leave the slice-1 refusal list).
pub(crate) const MAPPED: &[&str] = &[
    "blendMode",
    "blendKeys",
    "matteMode",
    "matteSourceLayerUid",
    "mattesMore",
    "isNullLayer",
    "nullPos",
    "nullShape",
    "isGuideLayer",
    "guidePos",
    "guideOrientation",
    "isWidgetLayer",
    "widget",
    "isFolderLayer",
    "folderCollapsed",
    "folderId",
    "keyLock",
    "threeD",
    "followPath",
    "inPoint",
    "outPoint",
    "markers",
    "timeLink",
];

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !is_default(v))
}

fn finite_pair(path: &str, value: &Value) -> Result<Value, LegacyImportError> {
    match value.as_array().map(Vec::as_slice) {
        Some([x, y]) if x.as_f64().is_some() && y.as_f64().is_some() => Ok(value.clone()),
        _ => Err(LegacyImportError::unsupported(
            path,
            "two finite numbers are required",
        )),
    }
}

fn frame_number(path: &str, value: &Value) -> Result<i64, LegacyImportError> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() < 1e9)
        .map(|n| n as i64)
        .ok_or_else(|| LegacyImportError::unsupported(path, "a whole frame number is required"))
}

pub(crate) fn apply(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    env: &Env,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    compositing(path, map, uid, env, record, ctx)?;
    roles(path, map, record, ctx)?;
    organisation(path, map, env, record, ctx)?;
    flags(path, map, uid, env, record, ctx)?;
    range(path, map, uid, env, record, ctx)
}

/// Baseline `importJSON`: `blendKeys` keeps the entries that are `{frame: number, mode: string}`. Native stores them
/// strictly increasing: a repeated frame keeps the LAST entry (what the baseline's linear scan would hold) and the order
/// is restored by frame; an entry the baseline would drop, or one that is not a whole non-negative frame, is dropped
/// with a warning.
fn blend_keys(path: &str, map: &Map<String, Value>, record: &mut Value, ctx: &mut Ctx) {
    let Some(Value::Array(entries)) = map.get("blendKeys") else {
        return;
    };
    let mut kept: Vec<(u32, String)> = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        let frame = entry
            .get("frame")
            .and_then(Value::as_f64)
            .filter(|frame| frame.fract() == 0. && (0. ..=f64::from(u32::MAX)).contains(frame))
            .map(|frame| frame as u32);
        let mode = entry.get("mode").and_then(Value::as_str).filter(|mode| !mode.is_empty());
        match (frame, mode) {
            (Some(frame), Some(mode)) => match kept.iter_mut().find(|(at, _)| *at == frame) {
                Some(slot) => slot.1 = mode.to_owned(),
                None => kept.push((frame, mode.to_owned())),
            },
            _ => ctx.warn(
                format!("{path}.blendKeys[{position}]"),
                "dropped_blend_key",
                "a blend key needs a whole frame and a mode name, so it is not carried over",
            ),
        }
    }
    kept.sort_by_key(|(frame, _)| *frame);
    if !kept.is_empty() {
        record["blendKeys"] = Value::Array(
            kept.into_iter()
                .map(|(frame, mode)| json!({"frame": frame, "mode": mode}))
                .collect(),
        );
    }
}

fn compositing(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    env: &Env,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    match map.get("blendMode") {
        Some(Value::String(name)) if !name.is_empty() && name != "normal" => {
            record["blendMode"] = json!(name);
        }
        Some(v) if is_default(v) || v == "normal" => {}
        Some(_) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.blendMode"),
                "a blend mode name is required",
            ))
        }
        None => {}
    }
    blend_keys(path, map, record, ctx);
    let mode = match map.get("matteMode") {
        Some(Value::String(name)) if !name.is_empty() && name != "none" => Some(name.as_str()),
        Some(v) if is_default(v) || v == "none" => None,
        Some(_) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.matteMode"),
                "a matte mode name is required",
            ))
        }
        None => None,
    };
    let Some(mode) = mode else {
        return Ok(());
    };
    record["matteMode"] = json!(mode);
    let known = |other: &str| other != uid && env.uids.contains(&other);
    match map.get("matteSourceLayerUid") {
        Some(Value::String(source)) if known(source) => {
            record["matteSourceLayerUid"] = json!(source);
        }
        Some(v) if !is_default(v) => ctx.warn(
            format!("{path}.matteSourceLayerUid"),
            "dangling_matte_source",
            "the matte source layer does not exist, so the matte has no effect",
        ),
        _ => {}
    }
    if let Some(Value::Array(extra)) = map.get("mattesMore") {
        let mut kept = Vec::new();
        for (position, entry) in extra.iter().enumerate() {
            let entry_path = format!("{path}.mattesMore[{position}]");
            let members = super::object(entry, &entry_path)?;
            let source = members.get("uid").and_then(Value::as_str).unwrap_or("");
            let name = members.get("mode").and_then(Value::as_str).unwrap_or("");
            if known(source) && !name.is_empty() {
                kept.push(json!({"uid": source, "mode": name}));
            } else {
                ctx.warn(
                    entry_path,
                    "dangling_matte_source",
                    "the extra matte source layer does not exist, so it is not carried over",
                );
            }
        }
        if !kept.is_empty() {
            record["mattesMore"] = Value::Array(kept);
        }
    } else if truthy(map.get("mattesMore")) {
        return Err(LegacyImportError::unsupported(
            format!("{path}.mattesMore"),
            "a list of mattes is required",
        ));
    }
    Ok(())
}

fn roles(
    path: &str,
    map: &Map<String, Value>,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    if truthy(map.get("isNullLayer")) {
        record["isNullLayer"] = json!(true);
        if let Some(pos) = map.get("nullPos").filter(|v| !v.is_null()) {
            record["nullPos"] = finite_pair(&format!("{path}.nullPos"), pos)?;
        }
    }
    if truthy(map.get("nullShape")) {
        ctx.warn(
            format!("{path}.nullShape"),
            "display_preference",
            "the Null marker shape is an editor display setting, so it is not carried over",
        );
    }
    if truthy(map.get("isGuideLayer")) {
        record["isGuideLayer"] = json!(true);
        if let Some(pos) = map.get("guidePos").filter(|v| !v.is_null()) {
            record["guidePos"] = finite_pair(&format!("{path}.guidePos"), pos)?;
        }
        if let Some(Value::String(orientation)) = map.get("guideOrientation") {
            record["guideOrientation"] = json!(orientation);
        }
    }
    match (truthy(map.get("isWidgetLayer")), map.get("widget")) {
        (true, Some(widget @ Value::Object(_))) => record["widget"] = widget.clone(),
        (false, Some(widget)) if !is_default(widget) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.widget"),
                "a widget record needs isWidgetLayer",
            ))
        }
        (true, _) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.widget"),
                "a widget layer needs its widget record",
            ))
        }
        _ => {}
    }
    Ok(())
}

fn organisation(
    path: &str,
    map: &Map<String, Value>,
    env: &Env,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    let folder = truthy(map.get("isFolderLayer"));
    if folder {
        record["isFolderLayer"] = json!(true);
        if truthy(map.get("folderCollapsed")) {
            record["folderCollapsed"] = json!(true);
        }
    }
    match map.get("folderId") {
        Some(Value::String(id)) if !id.is_empty() => {
            if env.folders.contains_key(id) {
                record["folderId"] = json!(id);
            } else {
                ctx.warn(
                    format!("{path}.folderId"),
                    "dangling_folder",
                    "the folder does not exist, so the layer is not in a folder",
                );
            }
        }
        Some(v) if is_default(v) => {}
        Some(_) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.folderId"),
                "a folder id is required",
            ))
        }
        None => {}
    }
    Ok(())
}

fn flags(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    env: &Env,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    match map.get("keyLock") {
        Some(Value::String(mode)) if ["in", "out", "layer"].contains(&mode.as_str()) => {
            record["keyLock"] = json!(mode);
        }
        Some(v) if is_default(v) => {}
        Some(_) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.keyLock"),
                "the key lock is in, out or layer",
            ))
        }
        None => {}
    }
    if truthy(map.get("threeD")) {
        record["threeD"] = json!(true);
    }
    match map.get("followPath") {
        Some(Value::Object(link)) => {
            let target = link.get("targetLayerUid").and_then(Value::as_str);
            match target {
                Some(t) if t != uid && env.uids.contains(&t) => {
                    let mut out = json!({"targetLayerUid": t});
                    if let Some(align) = link.get("align").and_then(Value::as_bool) {
                        out["align"] = json!(align);
                    }
                    record["followPath"] = out;
                }
                _ => ctx.warn(
                    format!("{path}.followPath"),
                    "dangling_follow_path",
                    "the path layer does not exist, so the layer follows no path",
                ),
            }
        }
        Some(v) if is_default(v) => {}
        Some(_) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.followPath"),
                "a follow path record is required",
            ))
        }
        None => {}
    }
    Ok(())
}

/// `inPoint`/`outPoint` at their defaults, no markers and no link store nothing; anything else is one `range` record.
fn range(
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    env: &Env,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    let mut range = Map::new();
    if let Some(value) = map.get("inPoint").filter(|v| !v.is_null()) {
        let frame = frame_number(&format!("{path}.inPoint"), value)?;
        if frame > 0 {
            range.insert("inPoint".into(), json!(frame));
        }
    }
    if let Some(value) = map.get("outPoint").filter(|v| !v.is_null()) {
        let frame = frame_number(&format!("{path}.outPoint"), value)?;
        if frame < i64::from(env.total) - 1 {
            range.insert("outPoint".into(), json!(frame));
        }
    }
    match map.get("markers") {
        Some(Value::Array(list)) if !list.is_empty() => {
            let mut out = Vec::new();
            for (position, marker) in list.iter().enumerate() {
                let marker_path = format!("{path}.markers[{position}]");
                let members = super::object(marker, &marker_path)?;
                let frame = frame_number(
                    &format!("{marker_path}.frame"),
                    members.get("frame").unwrap_or(&Value::Null),
                )?;
                let mut item = json!({"frame": frame});
                for key in ["name", "color"] {
                    if let Some(Value::String(text)) = members.get(key) {
                        item[key] = json!(text);
                    }
                }
                out.push(item);
            }
            range.insert("markers".into(), Value::Array(out));
        }
        Some(v) if !is_default(v) && !v.is_array() => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.markers"),
                "a list of markers is required",
            ))
        }
        _ => {}
    }
    if let Some(link) = map.get("timeLink").filter(|v| !is_default(v)) {
        let members = super::object(link, &format!("{path}.timeLink"))?;
        let source = members.get("uid").and_then(Value::as_str).unwrap_or("");
        if !source.is_empty() && source != uid && env.uids.contains(&source) {
            let mut out = json!({"uid": source});
            for (key, name) in [("mode", "mode"), ("srcAnchor", "srcAnchor")] {
                if let Some(Value::String(text)) = members.get(key) {
                    out[name] = json!(text);
                }
            }
            range.insert("timeLink".into(), out);
        } else {
            ctx.warn(
                format!("{path}.timeLink"),
                "dead_time_link",
                "the time link has no existing source layer, so the layer keeps its own range",
            );
        }
    }
    if !range.is_empty() {
        record["range"] = Value::Object(range);
    }
    Ok(())
}
