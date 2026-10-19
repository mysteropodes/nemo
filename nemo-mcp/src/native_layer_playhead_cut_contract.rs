//! F11 cut DTO and compact disposition; the existing native owner is authoritative.
use super::exact;
use crate::contract::bounded_identifier;
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.layer.cut";
const MAX_REVISION: u64 = 9_007_199_254_740_991;
fn authored_id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty() && id.len() <= 128)
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "contextId", "layerUid", "frame"], &[]).is_some_and(|p| {
        p.get("command").and_then(Value::as_str) == Some("object.layer.cut")
            && authored_id(p.get("contextId"))
            && authored_id(p.get("layerUid"))
            && p.get("frame")
                .and_then(Value::as_u64)
                .is_some_and(|frame| frame <= u64::from(u32::MAX))
    })
}
pub(super) fn result(value: &Value) -> bool {
    match value.get("applied").and_then(Value::as_bool) {
        Some(true) => exact(value, &["applied", "historyEntriesAdded", "layerUid"], &[])
            .is_some_and(|v| {
                v.get("historyEntriesAdded").and_then(Value::as_u64) == Some(1)
                    && v.get("layerUid")
                        .and_then(Value::as_str)
                        .is_some_and(bounded_identifier)
            }),
        Some(false) => exact(value, &["applied", "historyEntriesAdded"], &[])
            .is_some_and(|v| v.get("historyEntriesAdded").and_then(Value::as_u64) == Some(0)),
        None => false,
    }
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    revision <= MAX_REVISION
        && result(value)
        && expected
            .filter(|before| *before <= MAX_REVISION)
            .and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
#[path = "native_layer_playhead_cut_contract_tests.rs"]
mod tests;
