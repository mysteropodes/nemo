//! Closed subselect intents and immutable geometry pages on the existing object routes.
use crate::native_contract::{exact, revision};
use serde_json::Value;
pub(crate) const EDIT: &str = "command.document.object.subselect.edit";
pub(super) fn view_request(p: &Value) -> bool {
    let v = &p["view"];
    exact(p, &["atRevision", "documentSnapshotId", "view"], &[]).is_some()
        && revision(p.get("atRevision")).is_some()
        && p["documentSnapshotId"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && exact(
            v,
            &[
                "kind",
                "contextId",
                "layerUid",
                "frame",
                "workId",
                "viewGeneration",
                "cursor",
                "limit",
            ],
            &["preview"],
        )
        .is_some()
        && v.get("preview").is_none_or(|p| {
            exact(p, &["stableTarget", "intent"], &[]).is_some()
                && super::target(&p["stableTarget"])
                && intent(&p["intent"])
        })
        && v["kind"] == "subselect"
        && v["contextId"] == "scene-root"
        && v["layerUid"].as_str().is_some_and(|s| !s.is_empty())
        && v["workId"]
            .as_str()
            .is_some_and(crate::contract::bounded_identifier)
        && v["frame"]
            .as_u64()
            .is_some_and(|n| n <= u64::from(u32::MAX))
        && revision(v.get("viewGeneration")).is_some_and(|n| n > 0)
        && v["cursor"].as_u64().is_some_and(|n| n <= 1024)
        && v["limit"].as_u64().is_some_and(|n| (1..=8).contains(&n))
}
pub(super) fn view_result(document_id: &str, p: &Value, r: &Value) -> bool {
    if !view_request(p)
        || exact(
            r,
            &[
                "kind",
                "atRevision",
                "documentSnapshotId",
                "contextId",
                "layerUid",
                "frame",
                "workId",
                "viewGeneration",
                "cursor",
                "total",
                "nextCursor",
                "items",
                "preview",
            ],
            &[],
        )
        .is_none()
        || r["preview"].as_bool() != Some(p["view"].get("preview").is_some())
        || r["atRevision"] != p["atRevision"]
        || r["documentSnapshotId"] != p["documentSnapshotId"]
        || r["documentSnapshotId"].as_str()
            != Some(format!("native-object:{document_id}:{}", p["atRevision"]).as_str())
        || [
            "kind",
            "contextId",
            "layerUid",
            "frame",
            "workId",
            "viewGeneration",
            "cursor",
        ]
        .iter()
        .any(|k| r[*k] != p["view"][*k])
    {
        return false;
    }
    let Some(total) = r["total"].as_u64().filter(|n| *n <= 1024) else {
        return false;
    };
    let Some(items) = r["items"].as_array() else {
        return false;
    };
    let cursor = r["cursor"].as_u64().unwrap();
    let end = cursor + items.len() as u64;
    end <= total
        && items.len() as u64 == (total - cursor).min(p["view"]["limit"].as_u64().unwrap())
        && (if end == total {
            r["nextCursor"].is_null()
        } else {
            r["nextCursor"].as_u64() == Some(end)
        })
        && items.iter().all(|i| {
            exact(i, &["stableTarget", "closed", "count", "matrix"], &[]).is_some()
                && super::target(&i["stableTarget"])
                && i["stableTarget"]["contextId"] == r["contextId"]
                && i["stableTarget"]["layerUid"] == r["layerUid"]
                && i["closed"].is_boolean()
                && i["count"].as_u64().is_some_and(|n| n <= 256)
                && i["matrix"].as_array().is_some_and(|m| {
                    m.len() == 6 && m.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)) && {
                        let det = m[0].as_f64().unwrap() * m[3].as_f64().unwrap()
                            - m[1].as_f64().unwrap() * m[2].as_f64().unwrap();
                        det.is_finite() && det.abs() >= 1e-12
                    }
                })
        })
}
fn index(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n < 256)
}
fn point(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| a.len() == 2 && a.iter().all(|n| n.as_f64().is_some_and(f64::is_finite)))
}
fn indices(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.len() <= 256
            && a.iter().all(index)
            && a.iter().enumerate().all(|(i, n)| !a[..i].contains(n))
    })
}
pub(super) fn intent(v: &Value) -> bool {
    let (required, valid): (&[&str], bool) = match v["kind"].as_str() {
        Some("move") => (
            &["kind", "indices", "delta"],
            indices(&v["indices"]) && point(&v["delta"]),
        ),
        Some("scale") => (
            &["kind", "indices", "scale", "anchor"],
            indices(&v["indices"]) && point(&v["scale"]) && point(&v["anchor"]),
        ),
        Some("rotate") => (
            &["kind", "indices", "degrees", "center"],
            indices(&v["indices"])
                && v["degrees"].as_f64().is_some_and(f64::is_finite)
                && point(&v["center"]),
        ),
        Some("pointType") => (
            &["kind", "indices", "pointType"],
            indices(&v["indices"])
                && matches!(
                    v["pointType"].as_str(),
                    Some("corner" | "smooth" | "symmetric")
                ),
        ),
        Some("handle") => (
            &["kind", "index", "handle", "point", "breakPair"],
            index(&v["index"])
                && matches!(v["handle"].as_str(), Some("handleIn" | "handleOut"))
                && point(&v["point"])
                && v["breakPair"].is_boolean(),
        ),
        Some("toggle") => (&["kind", "index"], index(&v["index"])),
        Some("insert") => (
            &["kind", "curve", "time"],
            index(&v["curve"])
                && v["time"]
                    .as_f64()
                    .is_some_and(|t| (1e-8..=1.0 - 1e-8).contains(&t)),
        ),
        Some("delete") => (&["kind", "indices"], indices(&v["indices"])),
        _ => return false,
    };
    valid && exact(v, required, &[]).is_some()
}
pub(crate) fn edit_request(v: &Value) -> bool {
    exact(v, &["command", "stableTarget", "intent"], &["atFrame"]).is_some()
        && v["command"] == "object.subselect.edit"
        && super::target(&v["stableTarget"])
        && intent(&v["intent"])
        && v.get("atFrame")
            .is_none_or(|n| n.as_u64().is_some_and(|n| n <= u64::from(u32::MAX)))
}
pub(super) fn request(v: &Value) -> bool {
    exact(
        v,
        &[
            "atRevision",
            "documentSnapshotId",
            "stableTarget",
            "subselect",
        ],
        &[],
    )
    .is_some()
        && revision(v.get("atRevision")).is_some()
        && v["documentSnapshotId"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && super::target(&v["stableTarget"])
        && exact(&v["subselect"], &["cursor", "limit"], &["preview"]).is_some()
        && v["subselect"]["cursor"].as_u64().is_some_and(|n| n <= 256)
        && v["subselect"]["limit"]
            .as_u64()
            .is_some_and(|n| (1..=16).contains(&n))
        && v["subselect"].get("preview").is_none_or(intent)
}
pub(super) fn result(document_id: &str, p: &Value, r: &Value) -> bool {
    if !request(p)
        || exact(r, &["atRevision", "documentSnapshotId", "subselect"], &[]).is_none()
        || r["atRevision"] != p["atRevision"]
        || r["documentSnapshotId"] != p["documentSnapshotId"]
        || r["documentSnapshotId"].as_str()
            != Some(format!("native-object:{document_id}:{}", p["atRevision"]).as_str())
    {
        return false;
    }
    let s = &r["subselect"];
    if exact(
        s,
        &[
            "stableTarget",
            "closed",
            "total",
            "cursor",
            "nextCursor",
            "segments",
            "preview",
        ],
        &[],
    )
    .is_none()
        || s["stableTarget"] != p["stableTarget"]
        || !s["closed"].is_boolean()
        || s["cursor"] != p["subselect"]["cursor"]
        || s["preview"].as_bool() != Some(p["subselect"].get("preview").is_some())
    {
        return false;
    }
    let Some(total) = s["total"].as_u64().filter(|n| *n <= 256) else {
        return false;
    };
    let Some(items) = s["segments"].as_array() else {
        return false;
    };
    let end = s["cursor"].as_u64().unwrap() + items.len() as u64;
    end <= total
        && items.len() as u64
            == (total - s["cursor"].as_u64().unwrap())
                .min(p["subselect"]["limit"].as_u64().unwrap())
        && (if end == total {
            s["nextCursor"].is_null()
        } else {
            s["nextCursor"].as_u64() == Some(end)
        })
        && items.iter().all(|a| {
            exact(a, &["point", "handleIn", "handleOut"], &[]).is_some()
                && ["point", "handleIn", "handleOut"]
                    .iter()
                    .all(|k| point(&a[*k]))
        })
}

#[cfg(test)]
#[path = "native_subselect_contract_tests.rs"]
mod tests;
