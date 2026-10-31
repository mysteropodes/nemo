//! Closed stored-camera read contract (G44, #2357). Native owns the persistent camera keys and layer flag;
//! the read is pinned and paged, and cameraView stays session state that is never stored or returned.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.camera.read";
const SAFE: u64 = 9_007_199_254_740_991;

fn integer(v: Option<&Value>, min: u64, max: u64) -> bool {
    v.and_then(Value::as_u64)
        .is_some_and(|n| n >= min && n <= max)
}
fn number(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn numbers(v: Option<&Value>, len: usize) -> bool {
    v.and_then(Value::as_array)
        .is_some_and(|a| a.len() == len && a.iter().all(|n| number(Some(n))))
}
fn selector(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("settings") => exact(v, &["kind"], &[]).is_some(),
        Some("keyPage") => {
            exact(v, &["kind", "cursor", "limit"], &[]).is_some()
                && integer(v.get("cursor"), 0, SAFE)
                && integer(v.get("limit"), 1, 16)
        }
        _ => false,
    }
}
pub(super) fn request(op: &str, v: &Value) -> bool {
    op == READ
        && exact(
            v,
            &["atRevision", "documentSnapshotId", "contextId", "selector"],
            &[],
        )
        .is_some()
        && revision(v.get("atRevision")).is_some()
        && v.get("documentSnapshotId")
            .and_then(Value::as_str)
            .is_some_and(bounded_identifier)
        && v.get("contextId")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        && selector(&v["selector"])
}
fn presence(v: &Value) -> bool {
    integer(v.get("keyCount"), 0, SAFE)
        && v["keysPresent"]
            .as_bool()
            .is_some_and(|p| p || v["keyCount"] == 0)
}
fn settings(v: &Value) -> bool {
    exact(v, &["keysPresent", "keyCount"], &["layerOn"]).is_some()
        && presence(v)
        && v.get("layerOn").is_none_or(Value::is_boolean)
}
fn item(v: &Value) -> bool {
    exact(
        v,
        &["ordinal", "frame", "x", "y", "w"],
        &["rot", "hOut", "hIn", "ease"],
    )
    .is_some()
        && integer(v.get("ordinal"), 1, SAFE)
        && ["frame", "x", "y", "w"].iter().all(|k| number(v.get(*k)))
        && v.get("rot").is_none_or(|n| number(Some(n)))
        && v.get("hOut").is_none_or(|n| numbers(Some(n), 2))
        && v.get("hIn").is_none_or(|n| numbers(Some(n), 2))
        && v.get("ease").is_none_or(|n| numbers(Some(n), 4))
}
fn page(v: &Value, selector: &Value) -> bool {
    if exact(
        v,
        &[
            "kind",
            "keysPresent",
            "keyCount",
            "cursor",
            "items",
            "nextCursor",
        ],
        &[],
    )
    .is_none()
        || v["kind"] != "cameraKeyPage"
        || !presence(v)
        || v["cursor"] != selector["cursor"]
    {
        return false;
    }
    let (Some(count), Some(cursor), Some(items)) = (
        v["keyCount"].as_u64(),
        v["cursor"].as_u64(),
        v["items"].as_array(),
    ) else {
        return false;
    };
    if cursor > count
        || items.len() > 16
        || items.len() as u64 > selector["limit"].as_u64().unwrap_or(0)
    {
        return false;
    }
    let Some(end) = cursor.checked_add(items.len() as u64) else {
        return false;
    };
    if end > count
        || !items
            .iter()
            .enumerate()
            .all(|(i, it)| item(it) && it["ordinal"].as_u64() == Some(cursor + i as u64 + 1))
    {
        return false;
    }
    if end < count {
        !items.is_empty() && v["nextCursor"].as_u64() == Some(end)
    } else {
        v["nextCursor"].is_null()
    }
}
pub(super) fn result(op: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if op != READ
        || !request(op, payload)
        || exact(
            value,
            &[
                "instanceId",
                "documentId",
                "atRevision",
                "documentSnapshotId",
                "contextId",
                "result",
            ],
            &[],
        )
        .is_none()
        || !value
            .get("instanceId")
            .and_then(Value::as_str)
            .is_some_and(bounded_identifier)
        || value["documentId"].as_str() != Some(document)
        || ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .any(|k| value.get(*k) != payload.get(*k))
        || value["contextId"] != "scene-root"
    {
        return false;
    }
    match payload["selector"]["kind"].as_str() {
        Some("settings") => settings(&value["result"]),
        Some("keyPage") => page(&value["result"], &payload["selector"]),
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_camera_contract_tests.rs"]
mod tests;
