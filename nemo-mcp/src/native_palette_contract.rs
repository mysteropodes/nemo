//! Closed field-specific palette contracts. All document writes stay in native history.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.palette.read";
pub(crate) const EDIT: &str = "command.document.object.project.palette.edit";
const SAFE: u64 = 9_007_199_254_740_991;

fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn integer(v: Option<&Value>, min: u64, max: u64) -> bool {
    v.and_then(Value::as_u64).is_some_and(|n| n >= min && n <= max)
}
fn finite(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn action(v: &Value) -> bool {
    let Some(kind) = v.get("kind").and_then(Value::as_str) else { return false; };
    let shape = match kind {
        "create" => exact(v, &["kind", "id", "name"], &["color"]).is_some(),
        "rename" => exact(v, &["kind", "ordinal", "name"], &[]).is_some(),
        "duplicate" => exact(v, &["kind", "ordinal", "id", "name"], &[]).is_some(),
        "remove" | "select" => exact(v, &["kind", "ordinal"], &[]).is_some(),
        "appendColor" => exact(v, &["kind", "ordinal", "color"], &[]).is_some(),
        "removeColor" => exact(v, &["kind", "ordinal", "index"], &[]).is_some(),
        "moveColor" => exact(v, &["kind", "ordinal", "from", "to"], &[]).is_some(),
        _ => false,
    };
    shape
        && ["id", "name", "color"].iter().all(|k| v.get(*k).is_none_or(text))
        && ["ordinal", "index", "from", "to"].iter()
            .all(|k| v.get(*k).is_none_or(|n| integer(Some(n), 1, SAFE)))
}
fn selector(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("settings") => exact(v, &["kind"], &[]).is_some(),
        Some("palettePage") => exact(v, &["kind", "cursor", "limit"], &[]).is_some()
            && integer(v.get("cursor"), 0, SAFE) && integer(v.get("limit"), 1, 16),
        Some("colorPage") => exact(v, &["kind", "ordinal", "cursor", "limit"], &[]).is_some()
            && integer(v.get("ordinal"), 1, SAFE)
            && integer(v.get("cursor"), 0, SAFE) && integer(v.get("limit"), 1, 16),
        _ => false,
    }
}
pub(super) fn request(op: &str, v: &Value) -> bool {
    if op == EDIT {
        return exact(v, &["command", "action"], &[]).is_some()
            && v["command"] == "object.project.palette.edit" && action(&v["action"]);
    }
    op == READ
        && exact(v, &["atRevision", "documentSnapshotId", "contextId", "selector"], &[]).is_some()
        && revision(v.get("atRevision")).is_some()
        && v.get("documentSnapshotId").and_then(Value::as_str).is_some_and(bounded_identifier)
        && v.get("contextId").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
        && selector(&v["selector"])
}
fn palette_count(v: &Value) -> Option<u64> {
    let count = v.get("paletteCount")?.as_u64().filter(|n| *n <= SAFE)?;
    let present = v.get("palettesPresent")?.as_bool()?;
    (present || count == 0).then_some(count)
}
fn settings(v: &Value) -> bool {
    if exact(v, &["palettesPresent", "paletteCount", "effectiveActivePaletteIdx"], &["activePaletteIdx"]).is_none()
        || !v.get("effectiveActivePaletteIdx").is_some_and(finite)
        || !v.get("activePaletteIdx").is_none_or(finite) {
        return false;
    }
    let Some(count) = palette_count(v) else { return false; };
    let effective_count = if v["palettesPresent"] == true { count } else { 1 };
    let expected = v.get("activePaletteIdx").and_then(Value::as_f64)
        .filter(|n| *n < effective_count as f64).unwrap_or(0.);
    v["effectiveActivePaletteIdx"].as_f64() == Some(expected)
}
fn page(v: &Value, selector: &Value, colors: bool) -> bool {
    let fields: &[&str] = if colors {
        &["kind", "paletteOrdinal", "colorCount", "cursor", "items", "nextCursor"]
    } else {
        &["kind", "palettesPresent", "paletteCount", "cursor", "items", "nextCursor"]
    };
    if exact(v, fields, &[]).is_none() || v["cursor"] != selector["cursor"] {
        return false;
    }
    let count = if colors {
        if v["kind"] != "paletteColorPage" || v["paletteOrdinal"] != selector["ordinal"] {
            return false;
        }
        v.get("colorCount").and_then(Value::as_u64).filter(|n| *n <= SAFE)
    } else {
        if v["kind"] != "palettePage" { return false; }
        palette_count(v)
    };
    let Some(count) = count else { return false; };
    let Some(cursor) = v["cursor"].as_u64() else { return false; };
    let Some(items) = v["items"].as_array() else { return false; };
    let limit = selector["limit"].as_u64().unwrap_or(0);
    if cursor > count || items.len() as u64 > limit || items.len() > 16 {
        return false;
    }
    let end = cursor + items.len() as u64;
    if end > count { return false; }
    if !items.iter().enumerate().all(|(i, item)| {
        let ordinal = cursor + i as u64 + 1;
        if colors {
            exact(item, &["index", "color"], &[]).is_some()
                && item["index"].as_u64() == Some(ordinal)
                && item.get("color").is_some_and(text)
        } else {
            exact(item, &["ordinal", "id", "name", "colorCount"], &[]).is_some()
                && item["ordinal"].as_u64() == Some(ordinal)
                && item.get("id").is_some_and(text) && item.get("name").is_some_and(text)
                && integer(item.get("colorCount"), 0, SAFE)
        }
    }) { return false; }
    if end < count {
        !items.is_empty() && v["nextCursor"].as_u64() == Some(end)
    } else {
        v["nextCursor"].is_null()
    }
}
pub(super) fn result(op: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if op == EDIT { return super::settings::result(value); }
    if op != READ || !request(op, payload)
        || exact(value, &["instanceId", "documentId", "atRevision", "documentSnapshotId", "contextId", "result"], &[]).is_none()
        || !value.get("instanceId").and_then(Value::as_str).is_some_and(bounded_identifier)
        || value["documentId"].as_str() != Some(document)
        || ["atRevision", "documentSnapshotId", "contextId"].iter().any(|k| value.get(*k) != payload.get(*k))
        || value["contextId"] != "scene-root" {
        return false;
    }
    let selector = &payload["selector"];
    match selector["kind"].as_str() {
        Some("settings") => settings(&value["result"]),
        Some("palettePage") => page(&value["result"], selector, false),
        Some("colorPage") => page(&value["result"], selector, true),
        _ => false,
    }
}

#[cfg(test)]
#[path = "native_palette_contract_tests.rs"]
mod tests;
