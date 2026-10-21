//! Closed D26 root-composition payloads/results. No state, transport or fallback.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;
pub(crate) const READ: &str = "query.document.object.project.composition.read";
pub(crate) const EDIT: &str = "command.document.object.project.composition.edit";
fn finite(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn text(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn uint(v: Option<&Value>, min: u64, max: u64) -> bool {
    v.and_then(Value::as_u64)
        .is_some_and(|n| n >= min && n <= max)
}
fn optional(v: &Value, keys: &[&str], check: fn(&Value) -> bool) -> bool {
    keys.iter().all(|k| v.get(*k).is_none_or(check))
}
fn number(v: &Value) -> bool {
    finite(Some(v))
}
fn boolean(v: &Value) -> bool {
    v.is_boolean()
}
fn action(v: &Value) -> bool {
    let Some(kind) = v.get("kind").and_then(Value::as_str) else {
        return false;
    };
    let shape = match kind {
        "addMarker" => {
            exact(v, &["kind", "frame"], &["name", "color"]).is_some() && finite(v.get("frame"))
        }
        "updateMarker" => {
            exact(v, &["kind", "ordinal"], &["frame", "name", "color"]).is_some()
                && uint(v.get("ordinal"), 1, 4096)
        }
        "removeMarker" => {
            exact(v, &["kind", "ordinal"], &[]).is_some() && uint(v.get("ordinal"), 1, 4096)
        }
        "clearMarkers" => exact(v, &["kind"], &[]).is_some(),
        "setBpm" => exact(v, &["kind"], &["bpm", "bpmOffset", "bpmShow"]).is_some(),
        _ => false,
    };
    shape
        && optional(v, &["frame", "bpm", "bpmOffset"], number)
        && optional(v, &["name", "color"], text)
        && optional(v, &["bpmShow"], boolean)
}
fn selector(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("settings") => exact(v, &["kind"], &[]).is_some(),
        Some("markerAt") => {
            exact(v, &["kind", "index"], &[]).is_some()
                && uint(v.get("index"), 1, 9_007_199_254_740_991)
        }
        Some("markerNamed") => {
            exact(v, &["kind", "name"], &[]).is_some() && v.get("name").is_some_and(text)
        }
        Some("markerNearest") => {
            exact(v, &["kind", "frame"], &[]).is_some() && finite(v.get("frame"))
        }
        Some("markerPage") => {
            exact(v, &["kind", "cursor", "limit"], &[]).is_some()
                && uint(v.get("cursor"), 0, 4096)
                && uint(v.get("limit"), 1, 16)
        }
        _ => false,
    }
}
pub(super) fn request(op: &str, v: &Value) -> bool {
    if op == EDIT {
        return exact(v, &["command", "action"], &[]).is_some()
            && v["command"] == "object.project.composition.edit"
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
fn marker_count(v: &Value) -> Option<usize> {
    let count = v.get("markerCount")?.as_u64().filter(|n| *n <= 4096)? as usize;
    let present = v.get("markersPresent")?.as_bool()?;
    (present || count == 0).then_some(count)
}
fn settings(v: &Value) -> bool {
    exact(
        v,
        &[
            "markersPresent",
            "markerCount",
            "effectiveBpm",
            "effectiveBpmOffset",
            "effectiveBpmShow",
        ],
        &["bpm", "bpmOffset", "bpmShow"],
    )
    .is_some()
        && marker_count(v).is_some()
        && optional(
            v,
            &["bpm", "bpmOffset", "effectiveBpm", "effectiveBpmOffset"],
            number,
        )
        && optional(v, &["bpmShow"], boolean)
        && v["effectiveBpmShow"].is_boolean()
        && v["effectiveBpm"].as_f64() == Some(v.get("bpm").and_then(Value::as_f64).unwrap_or(120.0))
        && v["effectiveBpmOffset"].as_f64()
            == Some(v.get("bpmOffset").and_then(Value::as_f64).unwrap_or(0.0))
        && v["effectiveBpmShow"].as_bool()
            == Some(v.get("bpmShow").and_then(Value::as_bool).unwrap_or(false))
}
fn page(v: &Value, s: &Value) -> bool {
    if exact(
        v,
        &[
            "kind",
            "markersPresent",
            "markerCount",
            "cursor",
            "items",
            "nextCursor",
        ],
        &[],
    )
    .is_none()
        || v["kind"] != "markerPage"
        || v["cursor"] != s["cursor"]
    {
        return false;
    }
    let Some(count) = marker_count(v) else {
        return false;
    };
    let Some(cursor) = v["cursor"].as_u64().map(|n| n as usize) else {
        return false;
    };
    let Some(items) = v["items"].as_array() else {
        return false;
    };
    let limit = s["limit"].as_u64().unwrap_or(0) as usize;
    if cursor > count || items.len() > limit || cursor + items.len() > count {
        return false;
    }
    if !items.iter().enumerate().all(|(i, item)| {
        exact(item, &["ordinal", "frame"], &["name", "color"]).is_some()
            && item["ordinal"].as_u64() == Some((cursor + i + 1) as u64)
            && finite(item.get("frame"))
            && optional(item, &["name", "color"], text)
    }) {
        return false;
    }
    let end = cursor + items.len();
    if end < count {
        !items.is_empty() && v["nextCursor"].as_u64() == Some(end as u64)
    } else {
        v["nextCursor"].is_null()
    }
}
pub(super) fn result(op: &str, document: &str, payload: &Value, v: &Value) -> bool {
    if op == EDIT {
        return super::settings::result(v);
    }
    if op != READ
        || !request(op, payload)
        || exact(
            v,
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
    {
        return false;
    }
    if !v
        .get("instanceId")
        .and_then(Value::as_str)
        .is_some_and(bounded_identifier)
        || v["documentId"].as_str() != Some(document)
        || ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .any(|k| v.get(*k) != payload.get(*k))
        || v["contextId"] != "scene-root"
    {
        return false;
    }
    let body = &v["result"];
    let s = &payload["selector"];
    match s["kind"].as_str() {
        Some("settings") => settings(body),
        Some("markerPage") => page(body, s),
        Some("markerAt" | "markerNamed" | "markerNearest") => {
            exact(body, &["index", "frame", "time", "name", "color"], &[]).is_some()
                && uint(body.get("index"), 1, 4096)
                && finite(body.get("frame"))
                && finite(body.get("time"))
                && body.get("name").is_some_and(text)
                && body.get("color").is_some_and(|c| c.is_null() || text(c))
                && (s["kind"] != "markerAt" || body["index"] == s["index"])
                && (s["kind"] != "markerNamed" || body["name"] == s["name"])
        }
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_composition_contract_tests.rs"]
mod tests;
