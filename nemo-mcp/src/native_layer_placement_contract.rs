//! T12 placement-only view of the existing snapshot-pinned layer timing query.
use super::{exact, frame, revision};
use serde_json::Value;
const SELECTOR: [&str; 5] = [
    "atRevision",
    "documentSnapshotId",
    "contextId",
    "layerUid",
    "frame",
];
pub(super) fn request(value: &Value) -> bool {
    let mut fields = SELECTOR.to_vec();
    fields.push("view");
    exact(value, &fields, &[]).is_some_and(|p| {
        p["view"] == "placement"
            && revision(p.get("atRevision")).is_some()
            && frame(p.get("frame")).is_some()
            && ["documentSnapshotId", "contextId", "layerUid"]
                .iter()
                .all(|k| p[*k].as_str().is_some_and(|s| !s.is_empty()))
    })
}
fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|s| !s.is_empty())
}
fn placement(value: &Value) -> bool {
    value.is_null()
        || exact(
            value,
            &[
                "placementId",
                "assetId",
                "assetKind",
                "offsetFrames",
                "timeRemap",
                "interpMode",
                "frameBlend",
                "sourceFrameCount",
            ],
            &[],
        )
        .is_some_and(|p| {
            nonempty(&p["placementId"])
                && nonempty(&p["assetId"])
                && p["assetKind"] == "video"
                && p["offsetFrames"].as_f64().is_some_and(f64::is_finite)
                && (p["interpMode"].is_null() || nonempty(&p["interpMode"]))
                && (p["frameBlend"].is_null() || p["frameBlend"].is_boolean())
                && (p["sourceFrameCount"].is_null()
                    || p["sourceFrameCount"]
                        .as_u64()
                        .is_some_and(|n| (1..=u64::from(u32::MAX)).contains(&n)))
                && (p["timeRemap"].is_null() || remap(&p["timeRemap"]))
        })
}
fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
// The counted facts, the eased source frame (null without keys) and the key list,
// null when it does not fit the read; a listed key is one finite frame and value.
fn remap(value: &Value) -> bool {
    exact(value, &["keyCount", "keyAtFrame", "value", "keys"], &[]).is_some_and(|r| {
        let count = r["keyCount"].as_u64();
        count.is_some()
            && r["keyAtFrame"].is_boolean()
            && (r["value"].is_null() || finite(&r["value"]))
            && (r["keys"].is_null()
                || r["keys"].as_array().is_some_and(|keys| {
                    Some(keys.len() as u64) == count
                        && keys.iter().all(|k| {
                            exact(k, &["frame", "value"], &[])
                                .is_some_and(|k| finite(&k["frame"]) && finite(&k["value"]))
                        })
                }))
    })
}
pub(super) fn result(document: &str, payload: &Value, value: &Value) -> bool {
    let mut fields = vec!["kind", "instanceId", "documentId", "placement"];
    fields.extend(SELECTOR);
    request(payload)
        && exact(value, &fields, &[]).is_some_and(|p| {
            p["kind"] == "layerPlacement"
                && p["documentId"].as_str() == Some(document)
                && p["instanceId"]
                    .as_str()
                    .is_some_and(crate::contract::bounded_identifier)
                && p["contextId"] == "scene-root"
                && SELECTOR.iter().all(|k| p.get(*k) == payload.get(*k))
                && placement(&p["placement"])
        })
}
#[cfg(test)]
#[path = "native_layer_placement_contract_tests.rs"]
mod tests;
