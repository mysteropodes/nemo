//! Closed live-current history read, without mutation or historical stack semantics.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;
pub(crate) const OP: &str = "query.document.object.history";
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["atRevision", "documentSnapshotId"], &[]).is_some_and(|p| {
        revision(p.get("atRevision")).is_some()
            && p.get("documentSnapshotId")
                .and_then(Value::as_str)
                .is_some_and(bounded_identifier)
    })
}
pub(super) fn result(payload: &Value, value: &Value) -> bool {
    request(payload)
        && exact(
            value,
            &["atRevision", "documentSnapshotId", "undoDepth", "redoDepth"],
            &[],
        )
        .is_some_and(|r| {
            r.get("atRevision") == payload.get("atRevision")
                && r.get("documentSnapshotId") == payload.get("documentSnapshotId")
                && revision(r.get("undoDepth")).is_some()
                && revision(r.get("redoDepth")).is_some()
        })
}
#[cfg(test)]
#[path = "native_history_read_contract_tests.rs"]
mod tests;
