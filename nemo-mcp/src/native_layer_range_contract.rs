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
/// The layer's own markers: at most 64 `{frame, name?}`, a name of 1..=32 characters.
fn markers(value: &Value) -> bool {
    value.as_array().is_some_and(|list| {
        list.len() <= 64
            && list.iter().all(|m| {
                exact(m, &["frame"], &["name"]).is_some_and(|m| {
                    m["frame"].as_u64().is_some()
                        && m.get("name").is_none_or(|n| {
                            n.as_str()
                                .is_some_and(|n| (1..=32).contains(&n.chars().count()))
                        })
                })
            })
    })
}
/// The frames of the effect stack's keys: at most 512 whole frames.
fn effect_keys(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|list| list.len() <= 512 && list.iter().all(|f| f.as_u64().is_some()))
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
                && crate::native_contract::vector::context(&p["contextId"])
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
                    &["markers", "effectKeys"],
                )
                .is_some_and(|r| {
                    ["rawInPoint", "rawOutPoint", "inPoint", "outPoint"]
                        .iter()
                        .all(|k| r[*k].as_f64().is_some_and(f64::is_finite))
                        && r["hasCustomRange"].is_boolean()
                        && r["hasTimeLink"].is_boolean()
                        && r.get("markers").is_none_or(markers)
                        && r.get("effectKeys").is_none_or(effect_keys)
                })
        })
}
#[cfg(test)]
#[path = "native_layer_range_contract_tests.rs"]
mod tests;
