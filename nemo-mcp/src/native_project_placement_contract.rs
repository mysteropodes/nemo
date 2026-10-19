//! Closed placement DTO and receipt validation; the native placement/history owner is authoritative.
use super::exact;
use serde_json::Value;

pub(crate) const OP: &str = "command.document.object.project.placement.edit";
const MAX_OFFSET: u64 = 9_007_199_254_740_991;
fn source_id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 255 && !s.contains('\0'))
}
fn offset(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| v.is_finite() && v.abs() <= MAX_OFFSET as f64)
}
fn target(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("drawing") => exact(value, &["kind", "contextId", "layerUid"], &["strokeId"])
            .is_some_and(|v| {
                source_id(v.get("contextId"))
                    && source_id(v.get("layerUid"))
                    && (!v.contains_key("strokeId") || source_id(v.get("strokeId")))
            }),
        // This shape is declared so the actual owner can return typed unavailable.
        Some("audioTrack") => {
            exact(value, &["kind", "trackUid"], &[]).is_some_and(|v| source_id(v.get("trackUid")))
        }
        _ => false,
    }
}
pub(super) fn request(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("place") => exact(value, &["kind", "placement"], &[]).is_some_and(|v| {
            exact(
                &v["placement"],
                &["id", "assetId", "target", "offsetFrames"],
                &[],
            )
            .is_some_and(|p| {
                source_id(p.get("id"))
                    && source_id(p.get("assetId"))
                    && target(&p["target"])
                    && offset(p.get("offsetFrames"))
                    && (p["target"]["kind"] == "audioTrack" || p["offsetFrames"].as_i64().is_some())
            })
        }),
        Some("setOffset") => exact(value, &["kind", "placementId", "offsetFrames"], &[])
            .is_some_and(|v| source_id(v.get("placementId")) && offset(v.get("offsetFrames"))),
        Some("shiftOffset") => exact(value, &["kind", "placementId", "deltaFrames"], &[])
            .is_some_and(|v| source_id(v.get("placementId")) && offset(v.get("deltaFrames"))),
        Some("remove") => exact(value, &["kind", "placementId"], &[])
            .is_some_and(|v| source_id(v.get("placementId"))),
        _ => false,
    }
}
pub(super) fn result(value: &Value) -> bool {
    super::object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}

#[cfg(test)]
#[path = "native_project_placement_contract_tests.rs"]
mod tests;
