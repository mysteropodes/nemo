//! Closed bounded text-read DTOs; the native snapshot owns targets and content.
use super::{exact, item, revision};
use serde_json::{json, Value};
pub(crate) const OP: &str = "query.document.object.text";
fn uint(v: &Value, max: u64) -> Option<u64> {
    v.as_u64().filter(|n| *n <= max)
}
fn opaque(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && !s.contains('\0') && s.len() <= 255)
}
fn scope(v: &Value) -> bool {
    exact(v, &["contextId", "layerUid", "frameScope"], &[]).is_some()
        && v["contextId"] == "scene-root"
        && opaque(&v["layerUid"])
        && exact(&v["frameScope"], &["kind", "frame"], &[]).is_some()
        && matches!(
            v["frameScope"]["kind"].as_str(),
            Some("authored" | "reference")
        )
        && uint(&v["frameScope"]["frame"], u32::MAX as u64).is_some()
}
fn target(v: &Value) -> bool {
    exact(v, &["contextId", "layerUid", "frameScope", "strokeId"], &[]).is_some()
        && opaque(&v["strokeId"])
        && scope(
            &json!({"contextId":v["contextId"],"layerUid":v["layerUid"],"frameScope":v["frameScope"]}),
        )
}
pub(super) fn request(v: &Value) -> bool {
    if exact(v, &["atRevision", "documentSnapshotId", "view"], &[]).is_none()
        || revision(v.get("atRevision")).is_none()
        || !opaque(&v["documentSnapshotId"])
    {
        return false;
    }
    let x = &v["view"];
    match x["kind"].as_str() {
        Some("targets") => {
            exact(x, &["kind", "scope", "offset", "limit"], &[]).is_some()
                && scope(&x["scope"])
                && uint(&x["offset"], u32::MAX as u64).is_some()
                && uint(&x["limit"], 8).is_some_and(|n| n >= 1)
        }
        Some("inspect") => exact(x, &["kind", "target"], &[]).is_some() && target(&x["target"]),
        Some("text") => {
            exact(x, &["kind", "target", "offset", "limit"], &[]).is_some()
                && target(&x["target"])
                && uint(&x["offset"], 196608).is_some()
                && uint(&x["limit"], 1024).is_some_and(|n| n >= 1)
        }
        _ => false,
    }
}
fn positive(v: &Value) -> bool {
    v.as_f64().is_some_and(|n| n.is_finite() && n > 0.0)
}
/// None preserves the unchanged legacy wire owner predicate for every other operation.
pub(crate) fn wrong_instance_refusal(
    q: &crate::contract::NativeApplicationRequest,
    r: &crate::contract::NativeApplicationResponse,
) -> bool {
    q.operation == OP && !r.ok && r.error.as_ref().is_some_and(|e| e.code == "wrong_instance")
}
pub(crate) fn owner_matches(
    q: &crate::contract::NativeApplicationRequest,
    r: &crate::contract::NativeApplicationResponse,
) -> Option<bool> {
    if q.operation != OP {
        return None;
    }
    if !r.ok {
        if let Some(error) = &r.error {
            if error.code == "wrong_instance" {
                return Some(true);
            }
            if error.code == "wrong_document" {
                return Some(
                    r.instance_id == q.instance_id
                        && error
                            .details
                            .as_ref()
                            .and_then(|d| d.get("requestedDocumentId"))
                            .and_then(Value::as_str)
                            == Some(q.document_id.as_str()),
                );
            }
        }
    }
    Some(r.instance_id == q.instance_id && r.document_id == q.document_id)
}
pub(super) fn result(document: &str, p: &Value, r: &Value) -> bool {
    if !request(p)
        || r["documentId"] != document
        || !opaque(&r["instanceId"])
        || r["atRevision"] != p["atRevision"]
        || r["documentSnapshotId"] != p["documentSnapshotId"]
        || r["kind"] != p["view"]["kind"]
        || serde_json::to_vec(r).map_or(true, |b| b.len() > 3072)
    {
        return false;
    }
    let v = &p["view"];
    match r["kind"].as_str() {
        Some("targets") => {
            if exact(
                r,
                &[
                    "instanceId",
                    "documentId",
                    "atRevision",
                    "documentSnapshotId",
                    "kind",
                    "scope",
                    "offset",
                    "total",
                    "items",
                    "nextOffset",
                ],
                &[],
            )
            .is_none()
                || r["scope"] != v["scope"]
                || r["offset"] != v["offset"]
            {
                return false;
            }
            let (Some(items), Some(offset), Some(total), Some(limit)) = (
                r["items"].as_array(),
                uint(&r["offset"], u32::MAX as u64),
                uint(&r["total"], u32::MAX as u64),
                v["limit"].as_u64(),
            ) else {
                return false;
            };
            if items.len() as u64 > limit || offset + items.len() as u64 > total {
                return false;
            }
            let mut seen = std::collections::HashSet::new();
            if !items.iter().all(|row| {
                exact(row, &["target"], &[]).is_some()
                    && target(&row["target"])
                    && ["contextId", "layerUid", "frameScope"]
                        .iter()
                        .all(|k| row["target"][*k] == v["scope"][*k])
                    && seen.insert(row["target"]["strokeId"].as_str().unwrap())
            }) {
                return false;
            }
            let end = offset + items.len() as u64;
            if r["nextOffset"].is_null() {
                end == total
            } else {
                !items.is_empty() && r["nextOffset"] == end && end < total
            }
        }
        Some("inspect") => {
            if exact(
                r,
                &[
                    "instanceId",
                    "documentId",
                    "atRevision",
                    "documentSnapshotId",
                    "kind",
                    "target",
                    "textBytes",
                    "block",
                ],
                &[],
            )
            .is_none()
                || r["target"] != v["target"]
                || !uint(&r["textBytes"], 196608).is_some_and(|n| n > 0)
                || r["block"].get("text").is_some()
            {
                return false;
            }
            if !positive(&r["block"]["size"])
                || ["fixedWidth", "fixedHeight"].iter().any(|k| {
                    r["block"]
                        .get(*k)
                        .is_some_and(|v| !v.is_null() && !positive(v))
                })
                || r["block"]
                    .get("lineHeightMult")
                    .is_some_and(|v| !positive(v))
            {
                return false;
            }
            let Some(mut block) = r["block"].as_object().cloned() else {
                return false;
            };
            block.insert("text".into(), json!("validation"));
            let mut created = v["target"].clone();
            created["strokeId"] = json!("");
            item::request(
                item::ITEM,
                &json!({"command":"object.text.create","target":created,"block":block}),
            )
        }
        Some("text") => {
            if exact(
                r,
                &[
                    "instanceId",
                    "documentId",
                    "atRevision",
                    "documentSnapshotId",
                    "kind",
                    "target",
                    "textBytes",
                    "offset",
                    "text",
                    "nextOffset",
                ],
                &[],
            )
            .is_none()
                || r["target"] != v["target"]
                || r["offset"] != v["offset"]
            {
                return false;
            }
            let (Some(text), Some(total), Some(offset), Some(limit)) = (
                r["text"].as_str(),
                uint(&r["textBytes"], 196608),
                uint(&r["offset"], 196608),
                v["limit"].as_u64(),
            ) else {
                return false;
            };
            let end = offset + text.len() as u64;
            if text.contains('\0') || text.len() as u64 > limit || end > total {
                return false;
            }
            if r["nextOffset"].is_null() {
                end == total
            } else {
                !text.is_empty() && r["nextOffset"] == end && end < total
            }
        }
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_text_read_contract_tests.rs"]
mod tests;
