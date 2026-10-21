//! Original Perspective metadata gestures. Native owns defaults, storage and history.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.perspective.read";
pub(crate) const EDIT: &str = "command.document.object.project.perspective.edit";
const SAFE: u64 = 9_007_199_254_740_991;
const COUNTS: &[&str] = &[
    "perspectiveVPsPresent",
    "perspectiveVPsNull",
    "perspectiveVPCount",
    "effectiveVPCount",
    "vpSource",
];
fn integer(value: Option<&Value>, min: u64, max: u64) -> bool {
    value
        .and_then(Value::as_u64)
        .is_some_and(|n| n >= min && n <= max)
}
fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn text(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn action(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("setEnabled") => {
            exact(value, &["kind", "enabled"], &[]).is_some() && value["enabled"].is_boolean()
        }
        Some("setMode") => {
            exact(value, &["kind", "mode"], &[]).is_some()
                && matches!(
                    value["mode"].as_str(),
                    Some("1pt" | "2pt" | "3pt" | "fisheye")
                )
        }
        Some("reset") => exact(value, &["kind"], &[]).is_some(),
        Some("lockAll") => {
            exact(value, &["kind", "locked"], &[]).is_some() && value["locked"].is_boolean()
        }
        Some("move") => {
            exact(value, &["kind", "ordinal", "x", "y"], &[]).is_some()
                && integer(value.get("ordinal"), 1, SAFE)
                && finite(&value["x"])
                && finite(&value["y"])
        }
        Some("translate") => {
            exact(value, &["kind", "dx", "dy"], &[]).is_some()
                && finite(&value["dx"])
                && finite(&value["dy"])
        }
        _ => false,
    }
}
fn selector(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("settings") => exact(value, &["kind"], &[]).is_some(),
        Some("vpPage") => {
            exact(value, &["kind", "cursor", "limit"], &[]).is_some()
                && integer(value.get("cursor"), 0, SAFE)
                && integer(value.get("limit"), 1, 16)
        }
        _ => false,
    }
}
pub(super) fn request(operation: &str, value: &Value) -> bool {
    if operation == EDIT {
        return exact(value, &["command", "action"], &[]).is_some()
            && value["command"] == "object.project.perspective.edit"
            && action(&value["action"]);
    }
    operation == READ
        && exact(
            value,
            &["atRevision", "documentSnapshotId", "contextId", "selector"],
            &[],
        )
        .is_some()
        && revision(value.get("atRevision")).is_some()
        && value["documentSnapshotId"]
            .as_str()
            .is_some_and(bounded_identifier)
        && value["contextId"].as_str().is_some_and(|s| !s.is_empty())
        && selector(&value["selector"])
}
fn counts(value: &Value) -> bool {
    let Some(present) = value["perspectiveVPsPresent"].as_bool() else {
        return false;
    };
    let Some(null) = value["perspectiveVPsNull"].as_bool() else {
        return false;
    };
    if !integer(value.get("perspectiveVPCount"), 0, SAFE)
        || !integer(value.get("effectiveVPCount"), 1, SAFE)
    {
        return false;
    }
    let raw = value["perspectiveVPCount"].as_u64().unwrap();
    let effective = value["effectiveVPCount"].as_u64().unwrap();
    if !present && (null || raw != 0) || null && raw != 0 {
        return false;
    }
    match value["vpSource"].as_str() {
        Some("stored") => raw > 0 && effective == raw,
        Some("default") => raw == 0 && effective <= 3,
        _ => false,
    }
}
fn settings(value: &Value) -> bool {
    if exact(
        value,
        COUNTS,
        &[
            "perspectiveEnabled",
            "perspectiveMode",
            "perspectiveDensity",
        ],
    )
    .is_none()
        || !counts(value)
        || value
            .get("perspectiveEnabled")
            .is_some_and(|v| !v.is_boolean())
        || value.get("perspectiveMode").is_some_and(|v| !text(v))
        || value.get("perspectiveDensity").is_some_and(|v| !finite(v))
    {
        return false;
    }
    let expected = match value["perspectiveMode"].as_str() {
        Some("1pt" | "fisheye") => 1,
        Some("3pt") => 3,
        _ => 2,
    };
    value["vpSource"] != "default" || value["effectiveVPCount"] == expected
}
fn page(value: &Value, selector: &Value) -> bool {
    let required = [COUNTS, &["kind", "cursor", "items", "nextCursor"]].concat();
    if exact(value, &required, &[]).is_none()
        || !counts(value)
        || value["kind"] != "perspectiveVPPage"
        || value["cursor"] != selector["cursor"]
    {
        return false;
    }
    let Some(cursor) = value["cursor"].as_u64() else {
        return false;
    };
    let count = value["effectiveVPCount"].as_u64().unwrap();
    let Some(items) = value["items"].as_array() else {
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
        || !items.iter().enumerate().all(|(index, item)| {
            exact(item, &["ordinal", "x", "y"], &["locked"]).is_some()
                && item["ordinal"].as_u64() == Some(cursor + index as u64 + 1)
                && finite(&item["x"])
                && finite(&item["y"])
                && item.get("locked").is_none_or(Value::is_boolean)
                && (value["vpSource"] != "default" || item["locked"] == false)
        })
    {
        return false;
    }
    if end < count {
        !items.is_empty() && value["nextCursor"].as_u64() == Some(end)
    } else {
        value["nextCursor"].is_null()
    }
}
pub(super) fn result(operation: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if operation == EDIT {
        return super::settings::result(value);
    }
    if operation != READ
        || !request(operation, payload)
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
        || !value["instanceId"].as_str().is_some_and(bounded_identifier)
        || value["documentId"].as_str() != Some(document)
        || ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .any(|key| value.get(*key) != payload.get(*key))
        || value["contextId"] != "scene-root"
    {
        return false;
    }
    match payload["selector"]["kind"].as_str() {
        Some("settings") => settings(&value["result"]),
        Some("vpPage") => page(&value["result"], &payload["selector"]),
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_perspective_contract_tests.rs"]
mod tests;
