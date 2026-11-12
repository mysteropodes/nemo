//! F06 one-field layer policy contract; storage and history stay in native owner.
use crate::native_contract::{exact, object};
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.layer.policy.edit";
pub(super) fn request(value: &Value) -> bool {
    super::request_schema::payload(OP, value) == Some(true)
}

pub(super) fn result(value: &Value) -> bool {
    object::visibility_result(value)
}
/// The 3D switch (#2520) is saved with a revision but is no undo step (baseline `toggleLayer3D` pushed none): applied with no history entry.
fn saved_without_history(value: &Value) -> bool {
    value["applied"] == true && value["historyEntriesAdded"] == 0
}
pub(super) fn result_for(payload: &Value, value: &Value) -> bool {
    result(value)
        || payload.get("flag").and_then(Value::as_str) == Some("threeD") && exact(value, &["applied", "historyEntriesAdded"], &[]).is_some() && saved_without_history(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    (result(value) || exact(value, &["applied", "historyEntriesAdded"], &[]).is_some() && saved_without_history(value))
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
#[path = "native_layer_policy_contract_tests.rs"]
mod tests;
