//! Closed Shadow metadata contract. Native owns persistent palette, selection and history.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.shadow.read";
pub(crate) const EDIT: &str = "command.document.object.project.shadow.edit";
const SAFE: u64 = 9_007_199_254_740_991;

fn integer(v: Option<&Value>, min: u64, max: u64) -> bool {
    v.and_then(Value::as_u64)
        .is_some_and(|n| n >= min && n <= max)
}
fn text(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn action(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("add") => {
            exact(v, &["kind", "id", "color"], &[]).is_some() && text(&v["id"]) && text(&v["color"])
        }
        Some("select") => {
            exact(v, &["kind", "ordinal"], &[]).is_some() && integer(v.get("ordinal"), 1, SAFE)
        }
        _ => false,
    }
}
fn selector(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("settings") => exact(v, &["kind"], &[]).is_some(),
        Some("palettePage") => {
            exact(v, &["kind", "cursor", "limit"], &[]).is_some()
                && integer(v.get("cursor"), 0, SAFE)
                && integer(v.get("limit"), 1, 16)
        }
        _ => false,
    }
}
pub(super) fn request(op: &str, v: &Value) -> bool {
    if op == EDIT {
        return exact(v, &["command", "action"], &[]).is_some()
            && v["command"] == "object.project.shadow.edit"
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
fn presence(v: &Value) -> bool {
    integer(v.get("shadowColorCount"), 0, SAFE)
        && v["shadowPalettePresent"]
            .as_bool()
            .is_some_and(|p| p || v["shadowColorCount"] == 0)
}
fn settings(v: &Value) -> bool {
    exact(
        v,
        &["shadowPalettePresent", "shadowColorCount"],
        &["shadowActiveId"],
    )
    .is_some()
        && presence(v)
        && v.get("shadowActiveId").is_none_or(text)
}
fn page(v: &Value, selector: &Value) -> bool {
    if exact(
        v,
        &[
            "kind",
            "shadowPalettePresent",
            "shadowColorCount",
            "cursor",
            "items",
            "nextCursor",
        ],
        &[],
    )
    .is_none()
        || v["kind"] != "shadowPalettePage"
        || !presence(v)
        || v["cursor"] != selector["cursor"]
    {
        return false;
    }
    let Some(count) = v["shadowColorCount"].as_u64() else {
        return false;
    };
    let Some(cursor) = v["cursor"].as_u64() else {
        return false;
    };
    let Some(items) = v["items"].as_array() else {
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
        || !items.iter().enumerate().all(|(i, item)| {
            exact(item, &["ordinal", "id", "color"], &[]).is_some()
                && item["ordinal"].as_u64() == Some(cursor + i as u64 + 1)
                && text(&item["id"])
                && text(&item["color"])
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
        Some("palettePage") => page(&value["result"], &payload["selector"]),
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_shadow_contract_tests.rs"]
mod tests;
