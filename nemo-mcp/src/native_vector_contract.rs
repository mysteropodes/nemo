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

#[path = "native_vector_contract_shapes.rs"]
mod shapes;
use shapes::{record_shape, shape};

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

fn mask_edit(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("mode") => {
            exact(value, &["kind", "mode"], &[]).is_some()
                && one_of(&value["mode"], &["add", "subtract", "intersect"])
        }
        Some("feather") => {
            exact(value, &["kind", "value"], &[]).is_some() && finite(&value["value"])
        }
        Some("unset") => exact(value, &["kind"], &[]).is_some(),
        _ => false,
    }
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
        (VECTOR, "object.vector.mask.edit") => {
            named(&["stableTarget", "maskEdit"])
                && target(&payload["stableTarget"], false)
                && mask_edit(&payload["maskEdit"])
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
                "delete" | "mask.edit" => {
                    payload["stableTarget"]["strokeId"].as_str() == Some(stroke)
                }
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
