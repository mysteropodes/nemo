//! Closed Draw symmetry metadata contract. Native owns persistent settings and history.
use crate::native_contract::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(crate) const READ: &str = "query.document.object.project.symmetry.read";
pub(crate) const EDIT: &str = "command.document.object.project.symmetry.edit";

fn text(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() <= 1024 && !s.contains('\0'))
}
fn numbers(v: &Value, keys: &[&str]) -> bool {
    keys.iter().all(|k| v[*k].is_f64() || v[*k].is_i64() || v[*k].is_u64())
}
fn axis(v: &Value) -> bool {
    exact(v, &["x1", "y1", "x2", "y2"], &[]).is_some() && numbers(v, &["x1", "y1", "x2", "y2"])
}
fn point(v: &Value) -> bool {
    exact(v, &["x", "y"], &[]).is_some() && numbers(v, &["x", "y"])
}
pub(crate) fn request(op: &str, v: &Value) -> bool {
    if op == EDIT {
        // The edit shape is owned by the declared transport schema alone.
        return crate::native_contract::request_schema::payload(op, v) == Some(true);
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
        && exact(&v["selector"], &["kind"], &[]).is_some()
        && v["selector"]["kind"] == "settings"
}
fn settings(v: &Value) -> bool {
    let e = &v["effective"];
    exact(
        v,
        &["effective"],
        &[
            "symmetryEnabled",
            "symmetryMode",
            "symmetryAxis",
            "symmetryRadialCenter",
            "symmetryRadialSectors",
            "symmetryExtend",
        ],
    )
    .is_some()
        && exact(
            e,
            &["enabled", "mode", "axis", "radialCenter", "radialSectors", "extend"],
            &[],
        )
        .is_some()
        && e["enabled"].is_boolean()
        && text(&e["mode"])
        && e["mode"] != ""
        && axis(&e["axis"])
        && point(&e["radialCenter"])
        && numbers(e, &["radialSectors"])
        && e["extend"].is_boolean()
        && v.get("symmetryEnabled").is_none_or(Value::is_boolean)
        && v.get("symmetryMode").is_none_or(text)
        && v.get("symmetryAxis").is_none_or(|a| a.is_null() || axis(a))
        && v.get("symmetryRadialCenter").is_none_or(|c| c.is_null() || point(c))
        && v.get("symmetryRadialSectors").is_none_or(|_| numbers(v, &["symmetryRadialSectors"]))
        && v.get("symmetryExtend").is_none_or(Value::is_boolean)
}
pub(crate) fn result(op: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if op == EDIT {
        return super::result(value);
    }
    op == READ
        && request(op, payload)
        && exact(
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
        .is_some()
        && value
            .get("instanceId")
            .and_then(Value::as_str)
            .is_some_and(bounded_identifier)
        && value["documentId"].as_str() == Some(document)
        && ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .all(|k| value.get(*k) == payload.get(*k))
        && value["contextId"] == "scene-root"
        && settings(&value["result"])
}
#[cfg(test)]
#[path = "native_symmetry_contract_tests.rs"]
mod tests;
