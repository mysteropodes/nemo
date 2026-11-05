//! Baseline load-time migrations as pure JSON transforms: frames-only files, the legacy step easing curve, time-link
//! offsets, effector channels, matte source uids and index-keyed tween spans (`importJSON`, motion.js, app.js, timeline.js).
use super::{object, Ctx, LegacyImportError};
use serde_json::{json, Map, Value};

/// JavaScript truthiness of one member.
fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !super::classify::is_default(v))
}

pub(crate) fn run(doc: &mut Map<String, Value>, ctx: &mut Ctx) -> Result<(), LegacyImportError> {
    frames_only(doc, ctx)?;
    validate_shape(doc)?;
    let layers = doc
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .expect("validated layers");
    layer_set(layers, ctx);
    let uids: Vec<String> = layers
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap_or("").to_owned())
        .collect();
    let mut keyed = 0;
    for map in ["tweenOverrides", "tweenEasing"] {
        if let Some(Value::Object(spans)) = doc.get_mut(map) {
            keyed += tween_span_keys(spans, &uids);
        }
    }
    note(ctx, "tweenSpanKeys", keyed);
    Ok(())
}

/// The per-layer load migrations, shared by the scene layers and the layers of every component.
pub(crate) fn layer_set(layers: &mut [Value], ctx: &mut Ctx) {
    let generated = assign_layer_uids(layers);
    note(ctx, "layerUid", generated);
    note(ctx, "legacyCurve", legacy_curves(layers));
    note(
        ctx,
        "timeLinkOffsets",
        layers.iter_mut().map(time_link_offsets).sum(),
    );
    note(
        ctx,
        "effectorChannels",
        layers.iter_mut().map(effector_channels).sum(),
    );
    note(ctx, "matteSourceUid", matte_source_uids(layers));
}

fn note(ctx: &mut Ctx, name: &str, count: usize) {
    if count > 0 {
        ctx.migrations.push(format!("{name}:{count}"));
    }
}

/// `parse` of project-document.js: a file with only `frames` becomes one layer named `Layer 1`.
fn frames_only(doc: &mut Map<String, Value>, ctx: &mut Ctx) -> Result<(), LegacyImportError> {
    if doc
        .get("layers")
        .is_some_and(|v| !super::classify::is_default(v))
    {
        return Ok(());
    }
    let Some(frames) = doc
        .get("frames")
        .filter(|v| !super::classify::is_default(v))
        .cloned()
    else {
        return Err(LegacyImportError::invalid(
            "layers",
            "a project holds layers or frames",
        ));
    };
    doc.insert(
        "layers".into(),
        json!([{"name": "Layer 1", "visible": true, "locked": false, "frames": frames}]),
    );
    ctx.migrations.push("framesOnly:1".into());
    Ok(())
}

fn validate_shape(doc: &Map<String, Value>) -> Result<(), LegacyImportError> {
    let layers = doc
        .get("layers")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| LegacyImportError::invalid("layers", "layers must be a non-empty array"))?;
    for (li, layer) in layers.iter().enumerate() {
        let path = format!("layers[{li}]");
        let frames = object(layer, &path)?
            .get("frames")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                LegacyImportError::invalid(format!("{path}.frames"), "frames must be an array")
            })?;
        for (fi, frame) in frames.iter().enumerate() {
            let fpath = format!("{path}.frames[{fi}]");
            if !object(frame, &fpath)?
                .get("strokes")
                .is_some_and(Value::is_array)
            {
                return Err(LegacyImportError::invalid(
                    format!("{fpath}.strokes"),
                    "strokes must be an array",
                ));
            }
        }
    }
    Ok(())
}

/// Every layer needs a stable identity before parents, mattes and tween spans resolve against it.
fn assign_layer_uids(layers: &mut [Value]) -> usize {
    let mut taken: Vec<String> = layers
        .iter()
        .filter_map(|l| l.get("layerUid").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let mut generated = 0;
    for (index, layer) in layers.iter_mut().enumerate() {
        if layer
            .get("layerUid")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            continue;
        }
        let mut candidate = format!("ly_legacy_{index}");
        while taken.contains(&candidate) {
            candidate.push('_');
        }
        taken.push(candidate.clone());
        layer["layerUid"] = Value::String(candidate);
        generated += 1;
    }
    generated
}

fn curve_is(points: &Value, expected: &[(f64, f64)]) -> bool {
    let Some(points) = points.as_array().filter(|p| p.len() == expected.len()) else {
        return false;
    };
    points.iter().zip(expected).all(|(p, (x, y))| {
        !p.get("tx").is_some_and(Value::is_number)
            && p.get("x")
                .and_then(Value::as_f64)
                .is_some_and(|v| (v - x).abs() <= 1e-9)
            && p.get("y")
                .and_then(Value::as_f64)
                .is_some_and(|v| (v - y).abs() <= 1e-9)
    })
}

/// `migrateLegacyCurves`: only a key still carrying the exact old step default gets the intended default curve.
fn legacy_curves(layers: &mut [Value]) -> usize {
    const LEGACY: [(f64, f64); 4] = [(0.0, 0.0), (0.42, 0.0), (0.58, 1.0), (1.0, 1.0)];
    let default = json!([{"x": 0, "y": 0}, {"x": 0.25, "y": 0.156}, {"x": 0.5, "y": 0.5}, {"x": 0.75, "y": 0.844}, {"x": 1, "y": 1}]);
    let mut count = 0;
    for layer in layers {
        let Some(Value::Object(motion)) = layer.get_mut("motion") else {
            continue;
        };
        for track in motion.values_mut() {
            let Some(keys) = track.get_mut("keys").and_then(Value::as_array_mut) else {
                continue;
            };
            for key in keys {
                if key.get("curvePoints").is_some_and(|p| curve_is(p, &LEGACY)) {
                    key["curvePoints"] = default.clone();
                    count += 1;
                }
            }
        }
    }
    count
}

/// `migrateTimeLinkOffsets`: legacy plain offsets on `timeLink` become static Motion values, once and non-destructively.
fn time_link_offsets(layer: &mut Value) -> usize {
    let Some(link) = layer.get("timeLink").filter(|l| l.is_object()).cloned() else {
        return 0;
    };
    let mut moved = 0;
    for (offset, prop) in [
        ("inOffset", "timeLinkInOffset"),
        ("outOffset", "timeLinkOutOffset"),
    ] {
        let value = link.get(offset);
        let keyed = truthy(layer.get("motion").and_then(|m| m.get(prop)))
            || truthy(layer.get("motionStatic").and_then(|m| m.get(prop)));
        if truthy(value) && !keyed {
            if !layer.get("motionStatic").is_some_and(Value::is_object) {
                layer["motionStatic"] = json!({});
            }
            layer["motionStatic"][prop] = json!([value]);
            moved += 1;
        }
    }
    moved
}

/// `effectorChannels`: an effector without `channels` folds its non-zero legacy offsets into channel entries, once.
fn effector_channels(layer: &mut Value) -> usize {
    let Some(effectors) = layer
        .get_mut("duplicator")
        .and_then(|d| d.get_mut("effectors"))
        .and_then(Value::as_array_mut)
    else {
        return 0;
    };
    let mut migrated = 0;
    for effector in effectors {
        if !effector.is_object() || effector.get("channels").is_some_and(|c| !c.is_null()) {
            continue;
        }
        let pair = |key: &str| {
            effector
                .get(key)
                .and_then(Value::as_array)
                .filter(|p| {
                    p.first().and_then(Value::as_f64).unwrap_or(0.0) != 0.0
                        || p.get(1).and_then(Value::as_f64).unwrap_or(0.0) != 0.0
                })
                .map(|p| {
                    json!([
                        super::color::number(p.first().and_then(Value::as_f64).unwrap_or(0.0)),
                        super::color::number(p.get(1).and_then(Value::as_f64).unwrap_or(0.0))
                    ])
                })
        };
        let single = |key: &str| effector.get(key).filter(|v| truthy(Some(v))).cloned();
        let mut channels = Vec::new();
        if let Some(value) = pair("offsetPos") {
            channels.push(json!({"prop": "position", "value": value}));
        }
        if let Some(value) = single("offsetRot") {
            channels.push(json!({"prop": "rotation", "value": [value]}));
        }
        if let Some(value) = pair("offsetScale") {
            channels.push(json!({"prop": "scale", "value": value}));
        }
        if let Some(value) = single("offsetOpacity") {
            channels.push(json!({"prop": "opacity", "value": [value]}));
        }
        effector["channels"] = Value::Array(channels);
        migrated += 1;
    }
    migrated
}

/// A matte saved before mattes were uid-referenced used the layer directly above as its source.
fn matte_source_uids(layers: &mut [Value]) -> usize {
    let mut count = 0;
    for index in 0..layers.len().saturating_sub(1) {
        let mode = layers[index]
            .get("matteMode")
            .and_then(Value::as_str)
            .unwrap_or("");
        if mode.is_empty() || mode == "none" || truthy(layers[index].get("matteSourceLayerUid")) {
            continue;
        }
        let above = layers[index + 1]["layerUid"].clone();
        layers[index]["matteSourceLayerUid"] = above;
        count += 1;
    }
    count
}

/// Tween span keys used to start with the layer index (`5:0-10`); they now start with its uid.
fn tween_span_keys(spans: &mut Map<String, Value>, uids: &[String]) -> usize {
    let mut moved = 0;
    let old: Vec<String> = spans.keys().cloned().collect();
    let mut out = Map::new();
    for key in old {
        let value = spans.remove(&key).unwrap_or(Value::Null);
        let rewritten = key
            .split_once(':')
            .filter(|(index, _)| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|(index, rest)| {
                index
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| uids.get(i))
                    .map(|uid| format!("{uid}:{rest}"))
            });
        moved += usize::from(rewritten.is_some());
        out.insert(rewritten.unwrap_or(key), value);
    }
    *spans = out;
    moved
}
