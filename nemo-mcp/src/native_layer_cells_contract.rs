//! Whole-row cells view of the existing snapshot-pinned layer timing query (U31, #2374).
use super::{exact, frame, revision};
use serde_json::Value;
const KINDS: [&str; 4] = ["key", "held", "interpolated", "unexposed"];
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
        p["view"] == "cells"
            && revision(p.get("atRevision")).is_some()
            && frame(p.get("frame")).is_some()
            && ["documentSnapshotId", "contextId", "layerUid"]
                .iter()
                .all(|k| p[*k].as_str().is_some_and(|s| !s.is_empty()))
    })
}
/// Runs tile the page in order from the requested frame; nextFrame, when present, follows the last run.
fn runs(start: u64, cells: &Value) -> bool {
    let Some(c) = exact(cells, &["runs", "nextFrame"], &[]) else {
        return false;
    };
    let Some(runs) = c["runs"].as_array().filter(|r| !r.is_empty()) else {
        return false;
    };
    let mut at = start;
    for run in runs {
        let Some(r) = run.as_array().filter(|r| r.len() == 6) else {
            return false;
        };
        let source = r[3].is_null() || frame(Some(&r[3])).is_some();
        if frame(Some(&r[0])) != Some(at)
            || !r[1].as_u64().is_some_and(|n| n >= 1)
            || !r[2].as_str().is_some_and(|k| KINDS.contains(&k))
            || !source
            || !r[4].is_boolean()
            || !r[5].is_boolean()
        {
            return false;
        }
        at += r[1].as_u64().unwrap_or(0);
    }
    c["nextFrame"].is_null() || frame(c.get("nextFrame")) == Some(at)
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
                "cells",
            ],
            &[],
        )
        .is_some_and(|p| {
            p["kind"] == "layerCells"
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
                && p["frame"]
                    .as_u64()
                    .is_some_and(|start| runs(start, &p["cells"]))
        })
}
#[cfg(test)]
#[path = "native_layer_cells_contract_tests.rs"]
mod tests;
