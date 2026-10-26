//! `object.vector.batch` (#2361, #2362): ordered create, replace and delete steps on ONE scene-root layer, applied by the
//! native owner as one revision and one undo entry. Shapes are the single commands' own; the receipt lists one stroke id
//! per step, in order: an owner-allocated id for a create, the step's named target for a replace or delete.
use super::shapes::record_shape;
use super::target;
use crate::native_contract::exact;
use serde_json::Value;

const MAX_OPS: usize = 64;

/// One step's addressed target, when the step is one of the three admitted shapes.
fn step(op: &Value) -> Option<&Value> {
    match op["op"].as_str()? {
        "create" => (exact(op, &["op", "record"], &["placement", "keyframe"]).is_some()
            && record_shape(&op["record"], true)
            && op.get("placement").is_none_or(|p| p == "front" || p == "behind")
            && op.get("keyframe").is_none_or(|k| k == "copy"))
        .then(|| &op["record"]["target"]),
        "replace" => (exact(op, &["op", "record"], &[]).is_some()
            && record_shape(&op["record"], false)
            && op["record"]["target"]["frameScope"]["kind"] == "authored")
        .then(|| &op["record"]["target"]),
        "delete" => (exact(op, &["op", "stableTarget"], &[]).is_some() && target(&op["stableTarget"], false))
            .then(|| &op["stableTarget"]),
        _ => None,
    }
}

pub(super) fn request(payload: &Value) -> bool {
    exact(payload, &["command", "ops"], &["keyframe"]).is_some()
        && payload.get("keyframe").is_none_or(|k| k == "copy")
        && payload["ops"].as_array().is_some_and(|ops| {
            let targets: Option<Vec<&Value>> = ops.iter().map(step).collect();
            (1..=MAX_OPS).contains(&ops.len())
                && targets.is_some_and(|targets| {
                    targets.iter().all(|t| t["contextId"] == "scene-root" && t["layerUid"] == targets[0]["layerUid"])
                })
        })
}

pub(super) fn result(payload: &Value, value: &Value) -> bool {
    let (Some(ops), Some(ids)) = (payload["ops"].as_array(), value["strokeIds"].as_array()) else {
        return false;
    };
    exact(value, &["applied", "historyEntriesAdded", "operation", "strokeId", "strokeIds"], &[]).is_some()
        && value["applied"].is_boolean()
        && value["historyEntriesAdded"].as_u64() == Some(u64::from(value["applied"] == true))
        && value["operation"] == "batch"
        && ids.len() == ops.len()
        && ids.first() == Some(&value["strokeId"])
        && ops.iter().zip(ids).all(|(op, id)| match op["op"].as_str() {
            Some("create") => id.as_str().is_some_and(|s| s.starts_with("native-stroke:")),
            Some("replace") => *id == op["record"]["target"]["strokeId"],
            Some("delete") => *id == op["stableTarget"]["strokeId"],
            _ => false,
        })
}

#[cfg(test)]
#[path = "native_vector_batch_contract_tests.rs"]
mod tests;
