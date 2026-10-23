//! Closed field-specific guide contracts. All document writes stay in native history.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.guide.read";
pub(crate) const EDIT: &str = "command.document.object.project.guide.edit";
const SAFE: u64 = 9_007_199_254_740_991;

fn integer(v: Option<&Value>, min: u64, max: u64) -> bool {
    v.and_then(Value::as_u64)
        .is_some_and(|n| n >= min && n <= max)
}
fn finite(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn axis(v: &Value) -> bool {
    matches!(v.as_str(), Some("h" | "v"))
}
fn action(v: &Value) -> bool {
    let shape = match v.get("kind").and_then(Value::as_str) {
        Some("add") => exact(v, &["kind", "axis", "position"], &[]).is_some(),
        Some("move") => exact(v, &["kind", "axis", "ordinal", "position"], &[]).is_some(),
        Some("remove") => exact(v, &["kind", "axis", "ordinal"], &[]).is_some(),
        _ => false,
    };
    shape
        && axis(&v["axis"])
        && v.get("ordinal").is_none_or(|n| integer(Some(n), 1, SAFE))
        && v.get("position").is_none_or(finite)
}
fn selector(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("settings") => exact(v, &["kind"], &[]).is_some(),
        Some("axisPage") => {
            exact(v, &["kind", "axis", "cursor", "limit"], &[]).is_some()
                && axis(&v["axis"])
                && integer(v.get("cursor"), 0, SAFE)
                && integer(v.get("limit"), 1, 16)
        }
        _ => false,
    }
}
pub(super) fn request(op: &str, v: &Value) -> bool {
    if op == EDIT {
        return exact(v, &["command", "action"], &[]).is_some()
            && v["command"] == "object.project.guide.edit"
            && action(&v["action"]);
    }
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
fn presence(present: &Value, count: &Value, guides: bool) -> bool {
    present.as_bool().is_some_and(|p| {
        (guides || !p) && integer(Some(count), 0, SAFE) && (p || count.as_u64() == Some(0))
    })
}
fn settings(v: &Value) -> bool {
    exact(
        v,
        &["guidesPresent", "hPresent", "vPresent", "hCount", "vCount"],
        &[],
    )
    .is_some()
        && v["guidesPresent"].as_bool().is_some_and(|present| {
            presence(&v["hPresent"], &v["hCount"], present)
                && presence(&v["vPresent"], &v["vCount"], present)
        })
}
fn page(v: &Value, selector: &Value) -> bool {
    if exact(
        v,
        &[
            "kind",
            "axis",
            "guidesPresent",
            "axisPresent",
            "guideCount",
            "cursor",
            "items",
            "nextCursor",
        ],
        &[],
    )
    .is_none()
        || v["kind"] != "guideAxisPage"
        || v["axis"] != selector["axis"]
        || v["cursor"] != selector["cursor"]
    {
        return false;
    }
    let Some(present) = v["guidesPresent"].as_bool() else {
        return false;
    };
    if !presence(&v["axisPresent"], &v["guideCount"], present) {
        return false;
    }
    let Some(count) = v["guideCount"].as_u64() else {
        return false;
    };
    let Some(cursor) = v["cursor"].as_u64() else {
        return false;
    };
    let Some(items) = v["items"].as_array() else {
        return false;
    };
    let limit = selector["limit"].as_u64().unwrap_or(0);
    if cursor > count || items.len() > 16 || items.len() as u64 > limit {
        return false;
    }
    let Some(end) = cursor.checked_add(items.len() as u64) else {
        return false;
    };
    if end > count
        || !items.iter().enumerate().all(|(i, item)| {
            exact(item, &["ordinal", "position"], &[]).is_some()
                && item["ordinal"].as_u64() == Some(cursor + i as u64 + 1)
                && item.get("position").is_some_and(finite)
        })
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
    if op == EDIT {
        return super::settings::result(value);
    }
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
        Some("axisPage") => page(&value["result"], &payload["selector"]),
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_guide_contract_tests.rs"]
mod tests;
