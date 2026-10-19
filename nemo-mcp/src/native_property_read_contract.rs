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
                Some("rawSample" | "expressionContext") => {
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
    let context = view["kind"] == "expressionContext";
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
    } else if context {
        vec!["frame", "self", "comp", "keyCount", "compUnavailable"]
    } else {
        vec!["trackPresent", "items", "nextOffset"]
    });
    if exact(value, &required, if context { &["mesh"] } else { &[] }).is_none()
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
    let valid = if context {
        finite(&value["frame"])
            && value["frame"].as_f64() == view["frame"].as_f64()
            && expression_context(value)
    } else if raw {
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

fn expression_context(value: &Value) -> bool {
    let me = &value["self"];
    let parent = &me["parent"];
    let self_valid = me.is_null()
        || exact(
            me,
            &[
                "name",
                "index",
                "isShape",
                "hasParent",
                "property",
                "parent",
            ],
            &[],
        )
        .is_some_and(|s| {
            (s["name"].is_null() || s["name"].is_string())
                && uint(&s["index"]).is_some()
                && s["isShape"].is_boolean()
                && s["hasParent"].is_boolean()
                && s["property"] == value["property"]
                && (parent.is_null()
                    || exact(parent, &["name", "index"], &[])
                        .is_some_and(|p| p["name"].is_string() && uint(&p["index"]).is_some()))
        });
    let comp = &value["comp"];
    let comp_valid = if comp.is_null() {
        value["compUnavailable"] == "projectMetadata"
    } else {
        exact(
            comp,
            &[
                "width",
                "height",
                "fps",
                "frames",
                "layers",
                "name",
                "frameDuration",
                "duration",
                "numLayers",
            ],
            &[],
        )
        .is_some_and(|c| {
            let fps = c["fps"].as_f64().filter(|v| v.is_finite() && *v > 0.);
            ["width", "height", "frames", "layers", "numLayers"]
                .iter()
                .all(|k| uint(&c[*k]).is_some())
                && uint(&c["width"]).is_some_and(|n| n > 0)
                && uint(&c["height"]).is_some_and(|n| n > 0)
                && c["name"].is_string()
                && finite(&c["frameDuration"]) && finite(&c["duration"])
                && fps.is_some_and(|fps| {
                    c["frameDuration"].as_f64() == Some(1. / fps)
                        && c["duration"].as_f64()
                            == Some(f64::from(uint(&c["frames"]).unwrap() as u32).max(1.) / fps)
                })
                && c["numLayers"] == c["layers"]
                && value["compUnavailable"].is_null()
        })
    };
    self_valid && comp_valid && uint(&value["keyCount"]).is_some_and(|n| n <= 65_536)
        && value.get("mesh").is_none_or(|mesh|mesh_context(mesh,&value["target"],&value["property"]))
}

fn mesh_context(value:&Value,target:&Value,property:&Value) -> bool {
    exact(value,&["meshId","vertexIndex","vertexCount","outlineVertexCount","vertexUV","isOutlineVertex"],&[])
        .is_some_and(|m| {
            let index=m["vertexIndex"].as_i64().filter(|i|(-1..=i64::from(u32::MAX)).contains(i));
            let expected=property.as_str().and_then(|p|p.strip_prefix("vtx"))
                .filter(|p|!p.is_empty() && p.bytes().all(|b|b.is_ascii_digit()))
                .map_or(Some(-1),|p|p.parse::<i64>().ok());
            let count=uint(&m["vertexCount"]);let outline=uint(&m["outlineVertexCount"]);
            text(m.get("meshId")) && m.get("meshId")==target.get("strokeId")
                && index.is_some() && index==expected && count.is_some() && outline.is_some()
                && outline.unwrap()<=count.unwrap() && vector(&m["vertexUV"],2)
                && m["isOutlineVertex"].as_bool()==Some(index.unwrap()>=0 && (index.unwrap() as u64)<outline.unwrap())
                && ((index.unwrap()>=0 && (index.unwrap() as u64)<count.unwrap())
                    || m["vertexUV"].as_array().unwrap().iter().all(|v|v.as_f64()==Some(0.)))
        })
}
#[cfg(test)]
#[path = "native_property_read_contract_tests.rs"]
mod tests;
