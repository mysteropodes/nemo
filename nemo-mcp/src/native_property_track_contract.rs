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
fn selection(value: &Value) -> bool {
    exact(value, &["target", "property", "frame"], &[]).is_some_and(|s| {
        nonempty(s.get("property"))
            && finite(s.get("frame"))
            && exact(&s["target"], &["contextId", "layerUid"], &["strokeId"]).is_some_and(|t| {
                nonempty(t.get("contextId"))
                    && nonempty(t.get("layerUid"))
                    && (!t.contains_key("strokeId") || nonempty(t.get("strokeId")))
            })
    })
}
fn action(value: &Value) -> bool {
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
        _ => false,
    }
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "selections", "action"], &[]).is_some_and(|p| {
        p["command"].as_str() == Some("object.property.track.edit")
            && p["selections"].as_array().is_some_and(|selections| {
                (1..=32).contains(&selections.len()) && selections.iter().all(selection)
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
