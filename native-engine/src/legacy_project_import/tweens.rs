//! Tween records: the per-span re-pairing overrides (`tweenOverrides`), custom easing (`tweenEasing`) and the manual
//! mode (`tweenManualMode` with `tweenOn` strokes) become the native tween store; the project's resample, step and
//! default easing become the options of every layer that has a span or generated frames, and of every keyed layer when they
//! differ from the native defaults. Span keys are `layerUid:from-to`.
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The default tween curve and the step curve older files stored in its place.
const DEFAULT: [(f64, f64); 4] = [(0.0, 0.0), (0.3, 0.05), (0.7, 0.95), (1.0, 1.0)];

#[derive(Default)]
struct Span {
    overrides: Vec<Value>,
    easing: Option<Value>,
    manual: Option<Vec<String>>,
}

fn parse_key(key: &str) -> Option<(&str, u32, u32)> {
    let (uid, range) = key.rsplit_once(':')?;
    let (from, to) = range.split_once('-')?;
    Some((uid, from.parse().ok()?, to.parse().ok()?))
}

fn curve(path: &str, value: &Value) -> Result<Vec<Value>, LegacyImportError> {
    let points = value.get("points").unwrap_or(value);
    let list = points.as_array().ok_or_else(|| {
        LegacyImportError::unsupported(path, "a list of curve points is required")
    })?;
    let mut out = Vec::new();
    for point in list {
        let mut item = Map::new();
        for key in ["x", "y", "tx", "ty"] {
            if let Some(n) = point.get(key).filter(|v| v.is_number()) {
                item.insert(key.into(), n.clone());
            }
        }
        if !item.contains_key("x") || !item.contains_key("y") {
            return Err(LegacyImportError::unsupported(
                path,
                "a curve point needs x and y",
            ));
        }
        out.push(Value::Object(item));
    }
    Ok(out)
}

fn is_default_curve(points: &[Value]) -> bool {
    points.len() == DEFAULT.len()
        && points.iter().zip(DEFAULT).all(|(p, (x, y))| {
            p.get("tx").is_none()
                && p["x"].as_f64().is_some_and(|v| (v - x).abs() < 1e-9)
                && p["y"].as_f64().is_some_and(|v| (v - y).abs() < 1e-9)
        })
}

pub(crate) struct Input<'a> {
    pub(crate) doc: &'a Map<String, Value>,
    pub(crate) uids: &'a HashSet<String>,
    pub(crate) keys: &'a HashMap<String, Vec<u32>>,
    /// Keyframe numbers of every layer of the scene and of the components.
    pub(crate) drawn: &'a HashMap<String, Vec<u32>>,
    /// Layers (scene or component) that hold generated tween frames.
    pub(crate) sampled: &'a HashSet<String>,
    pub(crate) manual: &'a BTreeMap<(String, u32), Vec<String>>,
}

pub(crate) fn convert(input: &Input, ctx: &mut Ctx) -> Result<Option<Value>, LegacyImportError> {
    let mut spans: BTreeMap<(String, u32, u32), Span> = BTreeMap::new();
    let place = |key: &str, ctx: &mut Ctx, map: &str| -> Option<(String, u32, u32)> {
        let found = parse_key(key).filter(|(uid, from, to)| input.uids.contains(*uid) && from < to);
        if found.is_none() {
            ctx.warn(
                format!("{map}.{key}"),
                "dangling_tween_span",
                "the tween span names no existing layer or frame range, so it is not carried over",
            );
        }
        found.map(|(uid, from, to)| (uid.to_owned(), from, to))
    };
    if let Some(Value::Object(overrides)) = input.doc.get("tweenOverrides") {
        for (key, value) in overrides {
            let path = format!("tweenOverrides.{key}");
            let list: Vec<Value> = match value {
                Value::Array(list) => list.clone(),
                Value::Object(m)
                    if m.get("mode")
                        .and_then(Value::as_str)
                        .is_some_and(|m| m == "auto")
                        && m.len() == 1 =>
                {
                    Vec::new()
                }
                v if is_default(v) => Vec::new(),
                _ => {
                    return Err(LegacyImportError::unsupported(
                        path,
                        "a list of re-pairings is required",
                    ))
                }
            };
            let Some(at) = place(key, ctx, "tweenOverrides") else {
                continue;
            };
            let span = spans.entry(at).or_default();
            for (position, item) in list.iter().enumerate() {
                let item_path = format!("{path}[{position}]");
                let members = super::object(item, &item_path)?;
                let mut entry = Map::new();
                for name in ["aId", "aIds", "bId"] {
                    if let Some(v) = members.get(name).filter(|v| !v.is_null()) {
                        entry.insert(name.into(), v.clone());
                    }
                }
                span.overrides.push(Value::Object(entry));
            }
        }
    }
    if let Some(Value::Object(easing)) = input.doc.get("tweenEasing") {
        for (key, value) in easing {
            if is_default(value) {
                continue;
            }
            let points = curve(&format!("tweenEasing.{key}"), value)?;
            let Some(at) = place(key, ctx, "tweenEasing") else {
                continue;
            };
            spans.entry(at).or_default().easing = Some(Value::Array(points));
        }
    }
    for ((uid, frame), ids) in input.manual {
        let next = input
            .keys
            .get(uid)
            .and_then(|k| k.iter().copied().find(|k| k > frame));
        match next {
            Some(to) => {
                spans.entry((uid.clone(), *frame, to)).or_default().manual = Some(ids.clone())
            }
            None => ctx.warn(
                format!("layers[{uid}].frames[{frame}].tweenManualMode"),
                "dangling_tween_span",
                "the manual tween frame has no next keyframe, so the mode is not carried over",
            ),
        }
    }
    spans.retain(|_, s| !s.overrides.is_empty() || s.easing.is_some() || s.manual.is_some());
    let options = options(input.doc, ctx)?;
    // The baseline settings are project-wide, the native ones belong to a layer: carry them onto every layer that has a span
    // or generated frames, and, when they are not the native defaults, onto every layer that can be tweened.
    let custom = options != default_options();
    if spans.is_empty() && input.sampled.is_empty() && !custom {
        return Ok(None);
    }
    let mut layers: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let carriers = input.sampled.iter().cloned().chain(
        input
            .drawn
            .iter()
            .filter(|(_, keys)| custom && !keys.is_empty())
            .map(|(uid, _)| uid.clone()),
    );
    for uid in carriers {
        layers.entry(uid).or_default();
    }
    for ((uid, from, to), span) in spans {
        let mut entry = json!({"from": from, "to": to, "overrides": span.overrides});
        if let Some(easing) = span.easing {
            entry["easing"] = easing;
        }
        if let Some(manual) = span.manual {
            entry["manualStrokes"] = json!(manual);
        }
        layers.entry(uid).or_default().push(entry);
    }
    let layers: Vec<Value> = layers
        .into_iter()
        .map(|(uid, spans)| json!({"layerUid": uid, "options": options, "spans": spans}))
        .collect();
    Ok(Some(json!({"layers": layers})))
}

fn default_options() -> Value {
    json!({"resamplePoints": 50, "step": 1, "skipManual": true, "easing": []})
}

fn options(doc: &Map<String, Value>, ctx: &mut Ctx) -> Result<Value, LegacyImportError> {
    let whole = |key: &str, fallback: f64, low: f64, high: f64, ctx: &mut Ctx| {
        let value = doc
            .get(key)
            .and_then(Value::as_f64)
            .filter(|n| *n != 0.0)
            .unwrap_or(fallback);
        let clamped = value.round().clamp(low, high);
        if clamped != value {
            ctx.warn(
                key,
                "value_clamped",
                "the value is outside the native range, so the nearest allowed value is used",
            );
        }
        clamped as u32
    };
    let easing = match doc.get("easingCurve").filter(|v| !is_default(v)) {
        Some(value) => {
            let points = curve("easingCurve", value)?;
            let legacy_step = points.len() == 4 && points[1]["x"] == json!(0.42);
            if is_default_curve(&points) || legacy_step {
                Vec::new()
            } else {
                points
            }
        }
        None => Vec::new(),
    };
    Ok(
        json!({"resamplePoints": whole("resamplePts", 50.0, 2.0, 256.0, ctx), "step": whole("tweenStep", 1.0, 1.0, 999.0, ctx),
        "skipManual": true, "easing": easing}),
    )
}
