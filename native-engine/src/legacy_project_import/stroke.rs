//! One stored baseline stroke (`serP`) as an `editable-vector-path` object record; every other member family refuses.
use super::{classify, color, stroke_brush, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const MAPPED: &[&str] = &[
    "segments",
    "closed",
    "strokeColor",
    "hasRealStroke",
    "strokeWidth",
    "strokeCap",
    "strokeJoin",
    "miterLimit",
    "fillColor",
    "opacity",
    "dashArray",
    "dashOffset",
    "paintOrder",
    "strokeId",
    "tweenOn",
];
/// Pressure-ribbon members, consumed by `stroke_brush` when `isVectorBrush` is set.
pub(super) const BRUSH: &[&str] = &[
    "isVectorBrush",
    "centerSegments",
    "widthProfile",
    "strokeProfile",
    "profileBase",
    "isFillShape",
];
/// Stroke members a later slice owns: fills, brushes, text, masks, groups, roles, review, rasters, duplicator copies.
const LATER: &[&str] = &[
    "isFillCloseLine",
    "fillSeed",
    "fillSeeds",
    "fillGapPx",
    "fillWalls",
    "brushGroupId",
    "isLinkedFillCompanion",
    "linkedFillId",
    "boxAngle",
    "xformAnchorKey",
    "xformAnchorCustom",
    "isBrushTextureCopy",
    "brushTexturePreset",
    "bitmapBrushSpec",
    "bitmapPressureProfile",
    "preTextureOpacity",
    "preTextureStroke",
    "channelTag",
    "shadowSwatchId",
    "ownerId",
    "ownerName",
    "ownerColor",
    "revisionParentId",
    "isRevisionGhost",
    "revisionAction",
    "preRevisionOpacity",
    "fillGradient",
    "groupId",
    "trackRoleId",
    "attachedToRoleId",
    "attachOffset",
    "elemHidden",
    "paramShape",
    "effects",
    "isVectorText",
    "vectorChar",
    "charIndex",
    "wordIndex",
    "lineIndex",
    "isText",
    "isTextRoot",
    "text",
    "vectorFont",
    "textSize",
    "textColor",
    "textAlign",
    "textFixedWidth",
    "anchorTopLeft",
    "bold",
    "italic",
    "underline",
    "strike",
    "letterSpacing",
    "wordSpacing",
    "lineHeightMult",
    "textCase",
    "isMask",
    "maskMode",
    "maskFeather",
    "strokeGradientAlongPath",
    "isRaster",
    "src",
    "meshId",
    "isDuplicatorCopy",
    "dupIndex",
    "dupCloneId",
    "dup3D",
];

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !classify::is_default(v))
}

pub(super) fn point(
    path: &str,
    value: Option<&Value>,
    fallback_zero: bool,
) -> Result<Value, LegacyImportError> {
    match value {
        None | Some(Value::Null) if fallback_zero => Ok(json!({"x": 0, "y": 0})),
        Some(Value::Array(pair))
            if pair.len() >= 2 && pair[0].is_number() && pair[1].is_number() =>
        {
            Ok(json!({"x": pair[0], "y": pair[1]}))
        }
        _ => Err(LegacyImportError::invalid(
            path,
            "an [x, y] number pair is required",
        )),
    }
}

fn paint(path: &str, text: &str) -> Result<Value, LegacyImportError> {
    color::solid(text)
        .ok_or_else(|| LegacyImportError::unsupported(path, "the colour is not a hex colour"))
}

fn choice(
    path: &str,
    value: Option<&Value>,
    fallback: &str,
    allowed: &[&str],
) -> Result<Value, LegacyImportError> {
    let text = value.and_then(Value::as_str).unwrap_or(fallback);
    if allowed.contains(&text) {
        Ok(json!(text))
    } else {
        Err(LegacyImportError::unsupported(
            path,
            "the value has no native equivalent",
        ))
    }
}

fn finite(path: &str, value: Option<&Value>) -> Result<Option<f64>, LegacyImportError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(_) => Err(LegacyImportError::invalid(path, "a number is required")),
    }
}

pub(super) fn convert(
    path: &str,
    value: &Value,
    at: (&str, u32, usize),
    used: &mut HashSet<String>,
    ctx: &mut Ctx,
) -> Result<(Value, String), LegacyImportError> {
    let map: &Map<String, Value> = super::object(value, path)?;
    let brush = truthy(map.get("isVectorBrush"));
    for (key, member) in map {
        let key = key.as_str();
        if MAPPED.contains(&key) || (brush && BRUSH.contains(&key)) {
            continue;
        }
        if LATER.contains(&key) || BRUSH.contains(&key) {
            if !classify::is_default(member) {
                return Err(LegacyImportError::unsupported(
                    format!("{path}.{key}"),
                    "this stroke content has no native owner in this release",
                ));
            }
        } else if !classify::is_default(member) {
            ctx.warn(
                format!("{path}.{key}"),
                "unknown_field",
                "unknown member, not carried over",
            );
        }
    }
    let segments = map
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            LegacyImportError::invalid(format!("{path}.segments"), "segments must be an array")
        })?;
    if !(2..=256).contains(&segments.len()) {
        return Err(LegacyImportError::unsupported(
            format!("{path}.segments"),
            "a path needs 2 to 256 anchors",
        ));
    }
    let mut anchors = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let spath = format!("{path}.segments[{index}]");
        let parts = super::object(segment, &spath)?;
        anchors.push(
            json!({"point": point(&format!("{spath}.point"), parts.get("point"), false)?,
            "handleIn": point(&format!("{spath}.handleIn"), parts.get("handleIn"), true)?,
            "handleOut": point(&format!("{spath}.handleOut"), parts.get("handleOut"), true)?}),
        );
    }
    let fill = match map
        .get("fillColor")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
    {
        Some(text) => paint(&format!("{path}.fillColor"), text)?,
        None => Value::Null,
    };
    // desP: hasRealStroke === false is authoritative; a ribbon never carries a stroke; otherwise `strokeColor || '#fff'`.
    let no_stroke = brush || map.get("hasRealStroke") == Some(&Value::Bool(false));
    let stroke = if no_stroke {
        Value::Null
    } else {
        paint(
            &format!("{path}.strokeColor"),
            map.get("strokeColor")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .unwrap_or("#fff"),
        )?
    };
    let width = finite(&format!("{path}.strokeWidth"), map.get("strokeWidth"))?
        .filter(|w| *w != 0.0)
        .unwrap_or(3.0);
    if width < 0.0 {
        return Err(LegacyImportError::unsupported(
            format!("{path}.strokeWidth"),
            "a stroke width cannot be negative",
        ));
    }
    let mut opacity = finite(&format!("{path}.opacity"), map.get("opacity"))?.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&opacity) {
        ctx.warn(
            format!("{path}.opacity"),
            "opacity_clamped",
            "the opacity lies outside 0 to 1 and was limited to that range",
        );
        opacity = opacity.clamp(0.0, 1.0);
    }
    let mut body = json!({"stroke": stroke, "hasRealStroke": !stroke.is_null(), "strokeWidth": color::number(width),
        "strokeCap": choice(&format!("{path}.strokeCap"), map.get("strokeCap"), "round", &["butt", "round", "square"])?,
        "strokeJoin": choice(&format!("{path}.strokeJoin"), map.get("strokeJoin"), "round", &["miter", "round", "bevel"])?,
        "opacity": color::number(opacity)});
    if let Some(limit) = finite(&format!("{path}.miterLimit"), map.get("miterLimit"))? {
        body["miterLimit"] = color::number(limit);
    }
    if let Some(dashes) = map
        .get("dashArray")
        .and_then(Value::as_array)
        .filter(|d| !d.is_empty())
    {
        body["dashArray"] = Value::Array(dashes.clone());
    }
    if let Some(offset) = finite(&format!("{path}.dashOffset"), map.get("dashOffset"))? {
        body["dashOffset"] = color::number(offset);
    }
    if truthy(map.get("paintOrder")) {
        body["paintOrder"] = choice(
            &format!("{path}.paintOrder"),
            map.get("paintOrder"),
            "",
            &["fillFirst", "strokeFirst"],
        )?;
    }
    let stroke_id = stroke_id(map, at, used, path, ctx);
    let mut record = json!({"schemaVersion": 1, "family": "editable-vector-path",
        "target": {"contextId": "scene-root", "frameScope": {"kind": "authored", "frame": at.1}, "layerUid": at.0, "strokeId": stroke_id},
        "geometry": {"kind": "cubic-path", "closed": truthy(map.get("closed")), "coordinateSpace": "document-world", "handleSpace": "relative", "segments": anchors},
        "fill": fill, "vector": body});
    if brush {
        record["vectorExt"] = stroke_brush::pressure(path, map)?;
    }
    Ok((record, stroke_id))
}

/// Keep the stored id; a missing or repeated id inside one frame gets a deterministic one so identity stays unique.
fn stroke_id(
    map: &Map<String, Value>,
    at: (&str, u32, usize),
    used: &mut HashSet<String>,
    path: &str,
    ctx: &mut Ctx,
) -> String {
    let stored = map
        .get("strokeId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let mut id = stored.map_or_else(
        || format!("legacy-stroke:{}:{}:{}", at.0, at.1, at.2),
        str::to_owned,
    );
    if stored.is_none() {
        ctx.warn(
            format!("{path}.strokeId"),
            "stroke_id_generated",
            "the stroke had no id, so a stable one was generated",
        );
    }
    while !used.insert(id.clone()) {
        id = format!("{id}~{}", at.2);
        if stored.is_some() {
            ctx.warn(
                format!("{path}.strokeId"),
                "stroke_id_repeated",
                "the id is used twice on this frame, so it was made unique",
            );
        }
    }
    id
}
