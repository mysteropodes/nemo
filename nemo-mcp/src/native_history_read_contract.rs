//! Closed live-current history read, without mutation or historical stack semantics: the stack
//! depths (the cursor is `undoDepth`) and one bounded page of labelled entries.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;
pub(crate) const OP: &str = "query.document.object.history";
const MAX_PAGE: usize = 32;
const MAX_LABEL: usize = 32;
pub(super) fn request(value: &Value) -> bool {
    exact(
        value,
        &["atRevision", "documentSnapshotId"],
        &["offset", "limit"],
    )
    .is_some_and(|p| {
        revision(p.get("atRevision")).is_some()
            && p.get("documentSnapshotId")
                .and_then(Value::as_str)
                .is_some_and(bounded_identifier)
            && p.get("offset").is_none_or(|o| revision(Some(o)).is_some())
            && p.get("limit").is_none_or(|l| {
                l.as_u64()
                    .is_some_and(|l| (1..=MAX_PAGE as u64).contains(&l))
            })
    })
}
/// Entries are contiguous from the page `offset`, bounded, labelled and inside the stacks.
fn entries(payload: &Value, r: &serde_json::Map<String, Value>) -> bool {
    let (Some(offset), Some(list)) = (
        revision(r.get("offset")),
        r.get("entries").and_then(Value::as_array),
    ) else {
        return false;
    };
    let total = revision(r.get("undoDepth")).zip(revision(r.get("redoDepth")));
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(MAX_PAGE, |l| l as usize);
    payload
        .get("offset")
        .is_none_or(|o| Some(o) == r.get("offset"))
        && list.len() <= limit
        && total.is_some_and(|(u, d)| offset.saturating_add(list.len() as u64) <= u + d)
        && list.iter().enumerate().all(|(i, e)| {
            exact(e, &["index", "label"], &[]).is_some_and(|e| {
                revision(e.get("index")) == Some(offset + i as u64)
                    && e.get("label")
                        .and_then(Value::as_str)
                        .is_some_and(|l| !l.is_empty() && l.chars().count() <= MAX_LABEL)
            })
        })
}
pub(super) fn result(payload: &Value, value: &Value) -> bool {
    request(payload)
        && exact(
            value,
            &[
                "atRevision",
                "documentSnapshotId",
                "undoDepth",
                "redoDepth",
                "offset",
                "entries",
            ],
            &[],
        )
        .is_some_and(|r| {
            r.get("atRevision") == payload.get("atRevision")
                && r.get("documentSnapshotId") == payload.get("documentSnapshotId")
                && revision(r.get("undoDepth")).is_some()
                && revision(r.get("redoDepth")).is_some()
                && entries(payload, r)
        })
}
#[cfg(test)]
#[path = "native_history_read_contract_tests.rs"]
mod tests;
