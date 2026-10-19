//! D11 bounded raw sampling and authored key DTOs; no evaluation or writable mirror.
use crate::contract::bounded_identifier;
use crate::native_contract::{exact, revision};
use serde_json::Value;
pub(crate) const OP: &str = "query.document.object.property.read";
fn text(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}
fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn uint(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n <= u32::MAX as u64)
}
fn target(value: &Value) -> bool {
    exact(value, &["contextId", "layerUid"], &["strokeId"]).is_some_and(|t| {
        text(t.get("contextId"))
            && text(t.get("layerUid"))
            && t.get("strokeId").is_none_or(|v| text(Some(v)))
    })
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["atRevision", "documentSnapshotId", "view"], &[]).is_some_and(|p| {
        revision(p.get("atRevision")).is_some()
            && text(p.get("documentSnapshotId"))
            && match p["view"].get("kind").and_then(Value::as_str) {
                Some("rawSample") => {
                    exact(&p["view"], &["kind", "target", "property", "frame"], &[]).is_some_and(
                        |v| target(&v["target"]) && text(v.get("property")) && finite(&v["frame"]),
                    )
                }
                Some("authoredKeys") => exact(
                    &p["view"],
                    &["kind", "target", "property", "offset", "limit"],
                    &[],
                )
                .is_some_and(|v| {
                    target(&v["target"])
                        && text(v.get("property"))
                        && uint(&v["offset"]).is_some()
                        && v["limit"].as_u64().is_some_and(|n| (1..=8).contains(&n))
                }),
                _ => false,
            }
    })
}
fn vector(value: &Value, dimension: usize) -> bool {
    value
        .as_array()
        .is_some_and(|v| v.len() == dimension && v.iter().all(finite))
}
fn point(value: &Value) -> bool {
    exact(value, &["x", "y"], &["tx", "ty"]).is_some_and(|p| {
        finite(&p["x"])
            && finite(&p["y"])
            && ["tx", "ty"].iter().all(|k| p.get(*k).is_none_or(finite))
    })
}
fn curve(value: &Value) -> bool {
    value.is_null()
        || value.as_array().is_some_and(|p| {
            p.iter().all(point)
                && p.windows(2)
                    .all(|v| v[0]["x"].as_f64().unwrap() < v[1]["x"].as_f64().unwrap())
                && (p.len() < 2
                    || p[0]["x"].as_f64() == Some(0.0) && p[p.len() - 1]["x"].as_f64() == Some(1.0))
        })
}
fn key(value: &Value, dimension: usize) -> bool {
    exact(
        value,
        &["frame", "v"],
        &["hold", "curvePoints", "hIn", "hOut", "lockTo"],
    )
    .is_some_and(|k| {
        finite(&k["frame"])
            && vector(&k["v"], dimension)
            && k.get("hold").is_none_or(Value::is_boolean)
            && k.get("lockTo")
                .is_none_or(|v| v.is_null() || matches!(v.as_str(), Some("in" | "out" | "layer")))
            && k.get("curvePoints").is_none_or(curve)
            && ["hIn", "hOut"]
                .iter()
                .all(|name| k.get(*name).is_none_or(|v| vector(v, 2)))
    })
}
pub(super) fn result(document: &str, payload: &Value, value: &Value) -> bool {
    if !request(payload) {
        return false;
    }
    let view = &payload["view"];
    let raw = view["kind"] == "rawSample";
    let mut required = vec![
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
        "kind",
        "target",
        "property",
        "dimension",
    ];
    required.extend(if raw {
        vec!["frame", "value"]
    } else {
        vec!["trackPresent", "items", "nextOffset"]
    });
    if exact(value, &required, &[]).is_none()
        || !value["instanceId"].as_str().is_some_and(bounded_identifier)
        || value["documentId"].as_str() != Some(document)
        || ["atRevision", "documentSnapshotId"]
            .iter()
            .any(|k| value.get(*k) != payload.get(*k))
        || ["kind", "target", "property"]
            .iter()
            .any(|k| value.get(*k) != view.get(*k))
    {
        return false;
    }
    let Some(dimension) = value["dimension"].as_u64().filter(|d| (1..=4).contains(d)) else {
        return false;
    };
    let valid = if raw {
        finite(&value["frame"])
            && value["frame"].as_f64() == view["frame"].as_f64()
            && vector(&value["value"], dimension as usize)
    } else {
        let Some(present) = value["trackPresent"].as_bool() else {
            return false;
        };
        let Some(items) = value["items"].as_array() else {
            return false;
        };
        items.len() <= view["limit"].as_u64().unwrap() as usize
            && items.iter().all(|v| key(v, dimension as usize))
            && items
                .windows(2)
                .all(|p| p[0]["frame"].as_f64().unwrap() < p[1]["frame"].as_f64().unwrap())
            && (present || items.is_empty() && value["nextOffset"].is_null())
            && (value["nextOffset"].is_null()
                || items.len() == view["limit"].as_u64().unwrap() as usize
                    && uint(&value["nextOffset"])
                        == uint(&view["offset"])
                            .and_then(|n| n.checked_add(items.len() as u64))
                            .filter(|n| *n <= u32::MAX as u64))
    };
    valid && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 3072)
}
#[cfg(test)]
#[path = "native_property_read_contract_tests.rs"]
mod tests;
