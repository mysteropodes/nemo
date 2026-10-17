//! L01-J closed public contract of the native vector operations (G14, G22, G13) and of the `editable-vector-path` record
//! the object read returns. Shape and receipt validation only: the one native owner repeats every rule on the original
//! bytes. Mirrored by `src/js/adapters/native-object-vector-contract.js`.
use crate::contract::{
    NativeApplicationRequest, NativeApplicationResponse, NATIVE_MAX_MESSAGE_BYTES,
};
use crate::native_contract::{exact, frame};
use serde_json::Value;

pub(crate) const VECTOR: &str = "command.document.object.vector.edit";
pub(crate) const PARAMETRIC: &str = "command.document.object.parametric.edit";
pub(crate) const EFFECTS: &str = "command.document.object.path.effects.edit";
pub(crate) const FAMILY: &str = "editable-vector-path";
const MAX_ANCHORS: usize = 256;
const CAPS: [&str; 3] = ["butt", "round", "square"];
const JOINS: [&str; 3] = ["miter", "round", "bevel"];
const ORDERS: [&str; 2] = ["fillFirst", "strokeFirst"];
const PROFILES: [&str; 5] = ["taper-in", "taper-out", "taper-both", "bulge", "even"];
const EFFECT_TYPES: [&str; 2] = ["zigzag", "roughen"];

pub(crate) fn owns(operation: &str) -> bool {
    matches!(operation, VECTOR | PARAMETRIC | EFFECTS)
}

fn opaque(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}

fn non_negative(value: &Value) -> bool {
    value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
}

fn unit(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
}

fn one_of(value: &Value, list: &[&str]) -> bool {
    value.as_str().is_some_and(|text| list.contains(&text))
}

fn pair(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|parts| parts.len() == 2 && parts.iter().all(finite))
}

fn target(value: &Value, create: bool) -> bool {
    let stroke = if create {
        value["strokeId"] == ""
    } else {
        opaque(&value["strokeId"])
    };
    exact(
        value,
        &["contextId", "frameScope", "layerUid", "strokeId"],
        &[],
    )
    .is_some()
        && value["contextId"] == "scene-root"
        && opaque(&value["layerUid"])
        && stroke
        && exact(&value["frameScope"], &["kind", "frame"], &[]).is_some()
        && one_of(&value["frameScope"]["kind"], &["authored", "reference"])
        && frame(value["frameScope"].get("frame")).is_some()
}

fn point(value: &Value) -> bool {
    exact(value, &["x", "y"], &[]).is_some() && finite(&value["x"]) && finite(&value["y"])
}

fn solid(value: &Value) -> bool {
    exact(value, &["kind", "r", "g", "b", "a"], &[]).is_some()
        && value["kind"] == "solid"
        && ["r", "g", "b", "a"].iter().all(|key| unit(&value[*key]))
}

fn geometry(value: &Value) -> bool {
    exact(
        value,
        &[
            "kind",
            "closed",
            "coordinateSpace",
            "handleSpace",
            "segments",
        ],
        &[],
    )
    .is_some()
        && value["kind"] == "cubic-path"
        && value["closed"].is_boolean()
        && value["coordinateSpace"] == "document-world"
        && value["handleSpace"] == "relative"
        && value["segments"].as_array().is_some_and(|segments| {
            (2..=MAX_ANCHORS).contains(&segments.len())
                && segments.iter().all(|segment| {
                    exact(segment, &["point", "handleIn", "handleOut"], &[]).is_some()
                        && ["point", "handleIn", "handleOut"]
                            .iter()
                            .all(|key| point(&segment[*key]))
                })
        })
}

fn dashes(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        let total: f64 = items.iter().filter_map(Value::as_f64).sum();
        !items.is_empty() && items.iter().all(non_negative) && total > 0.0 && total.is_finite()
    })
}

fn body(value: &Value) -> bool {
    let optional = ["miterLimit", "dashArray", "dashOffset", "paintOrder"];
    let required = [
        "stroke",
        "hasRealStroke",
        "strokeWidth",
        "strokeCap",
        "strokeJoin",
        "opacity",
    ];
    exact(value, &required, &optional).is_some_and(|members| {
        let stroke = &value["stroke"];
        (stroke.is_null() || solid(stroke))
            && value["hasRealStroke"].as_bool() == Some(!stroke.is_null())
            && non_negative(&value["strokeWidth"])
            && one_of(&value["strokeCap"], &CAPS)
            && one_of(&value["strokeJoin"], &JOINS)
            && unit(&value["opacity"])
            && members.get("miterLimit").is_none_or(non_negative)
            && members.get("dashArray").is_none_or(dashes)
            && members.get("dashOffset").is_none_or(finite)
            && members.get("paintOrder").is_none_or(|v| one_of(v, &ORDERS))
    })
}

fn anchor(value: &Value) -> bool {
    exact(value, &["point", "handleIn", "handleOut", "width"], &[]).is_some()
        && ["point", "handleIn", "handleOut"]
            .iter()
            .all(|key| point(&value[*key]))
        && finite(&value["width"])
}

fn width_profile(value: &Value) -> bool {
    value.as_array().is_some_and(|stops| {
        let mut previous = 0.0;
        stops.iter().all(|stop| {
            let t = stop["t"]
                .as_f64()
                .filter(|t| unit(&stop["t"]) && *t >= previous);
            if let Some(t) = t {
                previous = t;
            }
            exact(stop, &["t", "width"], &[]).is_some() && t.is_some() && finite(&stop["width"])
        })
    })
}

fn pressure(value: &Value) -> bool {
    let optional = [
        "centerSegments",
        "widthProfile",
        "strokeProfile",
        "profileBase",
    ];
    exact(value, &[], &optional).is_some_and(|members| {
        members
            .get("centerSegments")
            .is_none_or(|v| v.as_array().is_some_and(|items| items.iter().all(anchor)))
            && members.get("widthProfile").is_none_or(width_profile)
            && members
                .get("strokeProfile")
                .is_none_or(|v| one_of(v, &PROFILES))
            && members.get("profileBase").is_none_or(non_negative)
    })
}

fn shape(value: &Value, for_set: bool) -> bool {
    let kind = value["kind"].as_str().unwrap_or("");
    let members: &[&str] = match kind {
        "rect" => &["tl", "tr", "br", "bl"],
        "ellipse" => &["startAngle", "sweep", "innerRadius"],
        "star" => &["pointCount", "innerRatio", "cornerRadius"],
        _ => return false,
    };
    let mut optional = members.to_vec();
    if !for_set {
        optional.push("box");
    }
    exact(value, &["kind"], &optional).is_some_and(|object| {
        members
            .iter()
            .all(|key| object.get(*key).is_none_or(finite))
            && object.get("box").is_none_or(|bounds| {
                exact(bounds, &["x1", "y1", "x2", "y2"], &[]).is_some()
                    && ["x1", "y1", "x2", "y2"]
                        .iter()
                        .all(|key| finite(&bounds[*key]))
            })
    })
}

fn extension(value: &Value, has_stroke: bool) -> bool {
    exact(value, &[], &["pressure", "paramShape", "boxAngle"]).is_some_and(|object| {
        !object.is_empty()
            && object
                .get("pressure")
                .is_none_or(|v| pressure(v) && !has_stroke)
            && object.get("paramShape").is_none_or(|v| shape(v, false))
            && object.get("boxAngle").is_none_or(finite)
    })
}

fn record_shape(value: &Value, create: bool) -> bool {
    exact(
        value,
        &[
            "schemaVersion",
            "family",
            "target",
            "geometry",
            "fill",
            "vector",
        ],
        &["vectorExt"],
    )
    .is_some_and(|object| {
        value["schemaVersion"] == 1
            && value["family"] == FAMILY
            && target(&value["target"], create)
            && geometry(&value["geometry"])
            && (value["fill"].is_null() || solid(&value["fill"]))
            && body(&value["vector"])
            && object
                .get("vectorExt")
                .is_none_or(|ext| extension(ext, value["vector"]["hasRealStroke"] == true))
    })
}

pub(crate) fn record(value: &Value) -> bool {
    record_shape(value, false)
}

fn effect(value: &Value) -> bool {
    exact(value, &["type"], &["enabled", "p1", "p2", "p3"]).is_some_and(|object| {
        one_of(&value["type"], &EFFECT_TYPES)
            && object.get("enabled").is_none_or(Value::is_boolean)
            && ["p1", "p2", "p3"]
                .iter()
                .all(|key| object.get(*key).is_none_or(|v| v.is_null() || finite(v)))
    })
}

pub(crate) fn request(operation: &str, payload: &Value) -> bool {
    let Some(command) = payload["command"].as_str() else {
        return false;
    };
    let named = |extra: &[&str]| {
        let mut keys = vec!["command"];
        keys.extend_from_slice(extra);
        exact(payload, &keys, &[]).is_some()
    };
    match (operation, command) {
        (VECTOR, "object.vector.create") => {
            named(&["record"]) && record_shape(&payload["record"], true)
        }
        (VECTOR, "object.vector.replace") => {
            named(&["record"])
                && record_shape(&payload["record"], false)
                && payload["record"]["target"]["frameScope"]["kind"] == "authored"
        }
        (VECTOR, "object.vector.delete") => {
            named(&["stableTarget"]) && target(&payload["stableTarget"], false)
        }
        (PARAMETRIC, "object.parametric.set") => {
            named(&["stableTarget", "paramShape"])
                && target(&payload["stableTarget"], false)
                && shape(&payload["paramShape"], true)
        }
        (PARAMETRIC, "object.parametric.translate") => {
            named(&["stableTarget", "delta"])
                && target(&payload["stableTarget"], false)
                && pair(&payload["delta"])
        }
        (PARAMETRIC, "object.parametric.scale") => {
            named(&["stableTarget", "scale", "anchor"])
                && target(&payload["stableTarget"], false)
                && pair(&payload["scale"])
                && pair(&payload["anchor"])
        }
        (EFFECTS, "object.path.effects.set") => {
            named(&["layerUid", "stack"])
                && opaque(&payload["layerUid"])
                && payload["stack"]
                    .as_array()
                    .is_some_and(|stack| stack.iter().all(effect))
        }
        _ => false,
    }
}

fn command_receipt(value: &Value) -> bool {
    value["applied"].is_boolean()
        && value["historyEntriesAdded"].as_u64() == Some(u64::from(value["applied"] == true))
}

/// The vector edit receipt names the operation it ran and the stroke it touched: an owner-allocated id for a create, the
/// stable target for a replace or delete. Parameter and effect-stack receipts are the compact command result.
pub(crate) fn result(operation: &str, payload: &Value, value: &Value) -> bool {
    match operation {
        VECTOR => {
            let Some(ran) = payload["command"]
                .as_str()
                .and_then(|command| command.strip_prefix("object.vector."))
            else {
                return false;
            };
            let Some(stroke) = value["strokeId"].as_str() else {
                return false;
            };
            let stroke_matches = match ran {
                "create" => value["applied"] == true && stroke.starts_with("native-stroke:"),
                "replace" => payload["record"]["target"]["strokeId"].as_str() == Some(stroke),
                "delete" => payload["stableTarget"]["strokeId"].as_str() == Some(stroke),
                _ => false,
            };
            exact(
                value,
                &["applied", "historyEntriesAdded", "operation", "strokeId"],
                &[],
            )
            .is_some()
                && command_receipt(value)
                && value["operation"].as_str() == Some(ran)
                && stroke_matches
        }
        PARAMETRIC | EFFECTS => {
            exact(value, &["applied", "historyEntriesAdded"], &[]).is_some()
                && command_receipt(value)
        }
        _ => true,
    }
}

/// A successful receipt belongs to the request owner and advances the revision by exactly its own disposition: one when
/// applied, none for an exact no-op. Complete responses stay within the public 4096-byte bound.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if !owns(&request.operation) {
        return true;
    }
    let Some(value) = &response.result else {
        return false;
    };
    response.request_id == request.request_id
        && response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && request
            .expected_revision
            .and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(response.content_revision)
        && serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= NATIVE_MAX_MESSAGE_BYTES)
}

#[cfg(test)]
#[path = "native_vector_contract_tests.rs"]
mod tests;
