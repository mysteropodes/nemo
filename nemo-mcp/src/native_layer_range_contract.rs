//! Range-only view of the existing snapshot-pinned layer timing query.
use super::{exact, frame, revision};
use serde_json::Value;
pub(super) fn request(value: &Value) -> bool {
    exact(
        value,
        &[
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "layerUid",
            "frame",
            "view",
        ],
        &[],
    )
    .is_some_and(|p| {
        p["view"] == "range"
            && revision(p.get("atRevision")).is_some()
            && frame(p.get("frame")).is_some()
            && ["documentSnapshotId", "contextId", "layerUid"]
                .iter()
                .all(|k| p[*k].as_str().is_some_and(|s| !s.is_empty()))
    })
}
pub(super) fn result(document: &str, payload: &Value, value: &Value) -> bool {
    request(payload)
        && exact(
            value,
            &[
                "kind",
                "instanceId",
                "documentId",
                "atRevision",
                "documentSnapshotId",
                "contextId",
                "layerUid",
                "frame",
                "range",
            ],
            &[],
        )
        .is_some_and(|p| {
            p["kind"] == "layerRange"
                && p["documentId"].as_str() == Some(document)
                && p["instanceId"]
                    .as_str()
                    .is_some_and(crate::contract::bounded_identifier)
                && p["contextId"] == "scene-root"
                && [
                    "atRevision",
                    "documentSnapshotId",
                    "contextId",
                    "layerUid",
                    "frame",
                ]
                .iter()
                .all(|k| p.get(*k) == payload.get(*k))
                && exact(
                    &p["range"],
                    &[
                        "rawInPoint",
                        "rawOutPoint",
                        "inPoint",
                        "outPoint",
                        "hasCustomRange",
                        "hasTimeLink",
                    ],
                    &[],
                )
                .is_some_and(|r| {
                    ["rawInPoint", "rawOutPoint", "inPoint", "outPoint"]
                        .iter()
                        .all(|k| r[*k].as_f64().is_some_and(f64::is_finite))
                        && r["hasCustomRange"].is_boolean()
                        && r["hasTimeLink"].is_boolean()
                })
        })
}
#[cfg(test)]
#[path = "native_layer_range_contract_tests.rs"]
mod tests;
