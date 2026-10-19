//! Strict public shell for one atomic native bar gesture; no ranges or document values supplied.
use super::{exact, frame, name, MAX_SAFE};
use serde_json::Value;
use std::collections::HashSet;
fn part(value: Option<&Value>) -> bool {
    matches!(value.and_then(Value::as_str), Some("in" | "out" | "both"))
}
fn scope(value: &Value) -> bool {
    exact(value, &["kind", "frame"], &[]).is_some_and(|p| {
        matches!(p["kind"].as_str(), Some("authored" | "reference"))
            && frame(p.get("frame")).is_some()
    })
}
fn target(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("property") => exact(value, &["kind", "target", "property"], &[]).is_some_and(|p| {
            name(p.get("property"))
                && exact(&p["target"], &["contextId", "layerUid"], &["strokeId"]).is_some_and(|t| {
                    name(t.get("contextId"))
                        && name(t.get("layerUid"))
                        && t.get("strokeId").is_none_or(|v| name(Some(v)))
                })
        }),
        Some("layerOrder" | "timeRemap") => exact(value, &["kind", "contextId", "layerUid"], &[])
            .is_some_and(|p| name(p.get("contextId")) && name(p.get("layerUid"))),
        Some("elementOrder") => exact(
            value,
            &["kind", "contextId", "layerUid", "frameScope", "strokeId"],
            &[],
        )
        .is_some_and(|p| {
            name(p.get("contextId"))
                && name(p.get("layerUid"))
                && name(p.get("strokeId"))
                && scope(&p["frameScope"])
        }),
        _ => false,
    }
}
fn selected(value: &Value) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    if items.len() > 4096 {
        return false;
    }
    let mut seen = HashSet::new();
    items.iter().all(|item| {
        exact(item, &["target", "frame"], &[]).is_some_and(|p| {
            let Some(n) = p["frame"].as_f64().filter(|n| n.is_finite()) else {
                return false;
            };
            let normalized = if n == 0.0 { 0.0 } else { n };
            target(&p["target"])
                && serde_json::to_string(&p["target"])
                    .ok()
                    .is_some_and(|t| seen.insert((t, normalized.to_bits())))
        })
    })
}
pub(super) fn request(value: &Value) -> bool {
    exact(
        value,
        &[
            "command",
            "contextId",
            "layerUid",
            "activeLayerUid",
            "part",
            "barSelection",
            "samples",
            "plainClick",
            "alt",
            "frame",
            "selectedKeys",
        ],
        &[],
    )
    .is_some_and(|p| {
        if p["command"] != "object.layer.range.drag"
            || !name(p.get("contextId"))
            || !name(p.get("layerUid"))
            || !name(p.get("activeLayerUid"))
            || !part(p.get("part"))
            || !p["plainClick"].is_boolean()
            || !p["alt"].is_boolean()
            || frame(p.get("frame")).is_none()
            || !selected(&p["selectedKeys"])
        {
            return false;
        }
        let Some(samples) = p["samples"].as_array() else {
            return false;
        };
        if samples.len() > 4096
            || !samples.iter().all(|v| {
                v.as_i64()
                    .is_some_and(|n| (-MAX_SAFE..=MAX_SAFE).contains(&n))
            })
        {
            return false;
        }
        let Some(bars) = p["barSelection"].as_array() else {
            return false;
        };
        if bars.len() > 4096 {
            return false;
        }
        let mut seen = HashSet::new();
        bars.iter().all(|v| {
            exact(v, &["layerUid", "part"], &[]).is_some_and(|b| {
                name(b.get("layerUid"))
                    && part(b.get("part"))
                    && seen.insert(b["layerUid"].as_str().unwrap())
            })
        })
    })
}
#[cfg(test)]
#[path = "native_layer_shift_contract_tests.rs"]
mod tests;
