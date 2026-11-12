//! D12 closed transport shapes; native owner alone resolves selected stable keys.
use crate::contract::MAX_SAFE_REVISION;
use crate::native_contract::{exact, object};
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.property.track.edit";
fn nonempty(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}
fn finite(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
/// One inclusive run of frames: `[from, to]`, finite and ordered.
fn run(value: &Value) -> bool {
    value.as_array().is_some_and(|pair| {
        pair.len() == 2
            && finite(pair.first())
            && finite(pair.get(1))
            && pair[0].as_f64() <= pair[1].as_f64()
    })
}
/// An entry names one key (`frame`) or the keys of one track inside runs of frames (`ranges`), never both.
fn selection(value: &Value) -> bool {
    exact(value, &["target", "property"], &["frame", "ranges"]).is_some_and(|s| {
        nonempty(s.get("property"))
            && (s.contains_key("frame") != s.contains_key("ranges"))
            && (!s.contains_key("frame") || finite(s.get("frame")))
            && (!s.contains_key("ranges")
                || s["ranges"]
                    .as_array()
                    .is_some_and(|r| (1..=512).contains(&r.len()) && r.iter().all(run)))
            && exact(&s["target"], &["contextId", "layerUid"], &["strokeId"]).is_some_and(|t| {
                nonempty(t.get("contextId"))
                    && nonempty(t.get("layerUid"))
                    && (!t.contains_key("strokeId") || nonempty(t.get("strokeId")))
            })
    })
}
fn in_range(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| v.abs() <= 1_000_000.0)
}
/// One key action (shared with the Time Remap key edit of the placement contract).
pub(crate) fn action(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("preset") => exact(value, &["kind", "preset"], &[]).is_some_and(|a| {
            matches!(
                a["preset"].as_str(),
                Some("linear" | "ease" | "easeIn" | "easeOut" | "hold")
            )
        }),
        Some("influence") => exact(value, &["kind", "side", "percent"], &[]).is_some_and(|a| {
            matches!(a["side"].as_str(), Some("in" | "out")) && finite(a.get("percent"))
        }),
        Some("subdivide") => exact(value, &["kind"], &[]).is_some(),
        // D37: per-key standing lock; mode is required, null clears.
        Some("lockTo") => exact(value, &["kind", "mode"], &[]).is_some_and(|a| {
            a["mode"].is_null() || matches!(a["mode"].as_str(), Some("in" | "out" | "layer"))
        }),
        // #2524: whole-selection key batches; `delta` is a whole number of frames.
        Some("shift") => exact(value, &["kind", "delta"], &[]).is_some_and(|a| {
            a["delta"]
                .as_i64()
                .is_some_and(|d| (-1_000_000..=1_000_000).contains(&d))
        }),
        Some("delete" | "flip" | "distribute") => exact(value, &["kind"], &[]).is_some(),
        // #2377: the selection box. `grab` goes with liquify and with nothing else.
        Some("warp") => exact(value, &["kind", "mode", "total"], &["grab"]).is_some_and(|a| {
            let liquify = a["mode"].as_str() == Some("liquify");
            matches!(
                a["mode"].as_str(),
                Some("top" | "bottom" | "left" | "right" | "liquify")
            ) && in_range(a.get("total"))
                && (a.contains_key("grab") == liquify)
                && (!liquify || in_range(a.get("grab")))
        }),
        // #2377: the key tint; `color` is required, null removes it.
        Some("color") => exact(value, &["kind", "color"], &[]).is_some_and(|a| {
            a["color"].is_null()
                || a["color"]
                    .as_str()
                    .is_some_and(super::property_read::key_color)
        }),
        _ => false,
    }
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "selections", "action"], &[]).is_some_and(|p| {
        p["command"].as_str() == Some("object.property.track.edit")
            && p["selections"].as_array().is_some_and(|selections| {
                (1..=128).contains(&selections.len())
                    && selections.iter().filter(|s| s.get("frame").is_some()).count() <= 32
                    && selections.iter().all(selection)
            })
            && action(&p["action"])
    })
}
pub(super) fn result(value: &Value) -> bool {
    object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && revision <= MAX_SAFE_REVISION
        && expected
            .filter(|v| *v <= MAX_SAFE_REVISION)
            .and_then(|v| v.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
#[path = "native_property_track_contract_tests.rs"]
mod tests;
