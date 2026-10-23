//! Original custom brush catalog metadata. This contract supplies no drawing provider.
use super::revision;
use crate::contract::bounded_identifier;
use serde_json::{Map, Value};
pub(crate) const READ: &str = "query.document.object.project.customBrushPreset.read";
pub(crate) const EDIT: &str = "command.document.object.project.customBrushPreset.edit";
const NUMBERS: [&str; 30] = [
    "bristleCount",
    "dashGap",
    "edgeNoise",
    "fixedAngle",
    "grain",
    "nibSize",
    "noise",
    "opacity",
    "opacityJitter",
    "polySides",
    "pressureEnd",
    "pressureMid",
    "pressureStart",
    "rotationJitter",
    "roughness",
    "roundness",
    "scatter",
    "scribbleAngleSpread",
    "scribbleCount",
    "scribbleLen",
    "scribbleLenJitter",
    "scribbleSpread",
    "scribbleWidth",
    "sharpness",
    "sizeJitter",
    "spaceJitter",
    "spacing",
    "tipCorner",
    "valueJitter",
    "savedBrushSize",
];
const STRINGS: [&str; 4] = ["rotationMode", "scatterDistribution", "tipShape", "label"];
fn exact<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str())))
    .then_some(object)
}
fn finite(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn pair(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.len() == 2 && items.iter().all(finite))
}
fn stamp(value: &Value) -> bool {
    let Some(object) = exact(value, &["segments"], &["closed", "pointCount"]) else {
        return false;
    };
    object.get("closed").is_none_or(Value::is_boolean)
        && object.get("pointCount").is_none_or(finite)
        && value["segments"].as_array().is_some_and(|segments| {
            segments.iter().all(|segment| {
                exact(segment, &["point"], &["handleIn", "handleOut"]).is_some_and(|object| {
                    pair(&segment["point"])
                        && object.get("handleIn").is_none_or(pair)
                        && object.get("handleOut").is_none_or(pair)
                })
            })
        })
}
fn preset(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.iter().all(|(key, value)| {
            if NUMBERS.contains(&key.as_str()) {
                finite(value)
            } else if STRINGS.contains(&key.as_str()) {
                value.is_string()
            } else if key == "markerTip" {
                value.is_boolean()
            } else if key == "customStamp" {
                stamp(value)
            } else {
                false
            }
        })
    })
}
fn selector(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("settings") => exact(value, &["kind"], &[]).is_some(),
        Some("record") => exact(value, &["kind", "id"], &[]).is_some() && value["id"].is_string(),
        Some("catalogPage") => {
            exact(value, &["kind", "cursor", "limit"], &[]).is_some()
                && revision(value.get("cursor")).is_some()
                && value["limit"]
                    .as_u64()
                    .is_some_and(|n| (1..=16).contains(&n))
        }
        _ => false,
    }
}
pub(super) fn request(operation: &str, value: &Value) -> bool {
    if operation == EDIT {
        if exact(value, &["command", "action"], &[]).is_none()
            || value["command"] != "object.project.customBrushPreset.edit"
        {
            return false;
        }
        let action = &value["action"];
        return match action["kind"].as_str() {
            Some("upsert") => {
                exact(action, &["kind", "id", "preset"], &[]).is_some()
                    && action["id"].is_string()
                    && preset(&action["preset"])
            }
            Some("delete") => {
                exact(action, &["kind", "id"], &[]).is_some() && action["id"].is_string()
            }
            _ => false,
        };
    }
    operation == READ
        && exact(
            value,
            &["atRevision", "documentSnapshotId", "contextId", "selector"],
            &[],
        )
        .is_some()
        && revision(value.get("atRevision")).is_some()
        && value["documentSnapshotId"]
            .as_str()
            .is_some_and(bounded_identifier)
        && value["contextId"] == "scene-root"
        && selector(&value["selector"])
}
fn counts(value: &Value) -> Option<u64> {
    let present = value["customBrushPresetsPresent"].as_bool()?;
    let count = revision(value.get("presetCount"))?;
    (present || count == 0).then_some(count)
}
fn page(value: &Value, select: &Value) -> bool {
    if exact(
        value,
        &[
            "kind",
            "customBrushPresetsPresent",
            "presetCount",
            "cursor",
            "items",
            "nextCursor",
        ],
        &[],
    )
    .is_none()
        || value["kind"] != "customBrushPresetPage"
        || value["cursor"] != select["cursor"]
    {
        return false;
    }
    let Some(count) = counts(value) else {
        return false;
    };
    let Some(cursor) = revision(value.get("cursor")) else {
        return false;
    };
    let Some(items) = value["items"].as_array() else {
        return false;
    };
    let limit = select["limit"].as_u64().unwrap_or(0);
    if items.len() > limit as usize || cursor > count {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = exact(item, &["ordinal", "id"], &["label"]) else {
            return false;
        };
        let Some(id) = item["id"].as_str() else {
            return false;
        };
        if !seen.insert(id)
            || item["ordinal"].as_u64() != Some(cursor + index as u64 + 1)
            || !object.get("label").is_none_or(Value::is_string)
        {
            return false;
        }
    }
    let end = cursor + items.len() as u64;
    end <= count
        && if end < count {
            !items.is_empty() && value["nextCursor"].as_u64() == Some(end)
        } else {
            value["nextCursor"].is_null()
        }
}
pub(super) fn result(operation: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if operation == EDIT {
        return super::settings::result(value);
    }
    if operation != READ
        || !request(operation, payload)
        || exact(
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
        .is_none()
        || !value["instanceId"].as_str().is_some_and(bounded_identifier)
        || value["documentId"].as_str() != Some(document)
        || !["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .all(|key| value.get(*key) == payload.get(*key))
    {
        return false;
    }
    let body = &value["result"];
    match payload["selector"]["kind"].as_str() {
        Some("settings") => {
            exact(body, &["customBrushPresetsPresent", "presetCount"], &[]).is_some()
                && counts(body).is_some()
        }
        Some("catalogPage") => page(body, &payload["selector"]),
        Some("record") => {
            exact(body, &["kind", "id", "preset"], &[]).is_some()
                && body["kind"] == "customBrushPresetRecord"
                && body["id"] == payload["selector"]["id"]
                && preset(&body["preset"])
        }
        _ => false,
    }
}
#[cfg(test)]
#[path = "native_custom_brush_contract_tests.rs"]
mod tests;
