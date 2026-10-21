//! Original workspace layout-settle capture and pinned raw read contract.
use super::revision;
use crate::contract::bounded_identifier;
use serde_json::{Map, Value};
pub(crate) const READ: &str = "query.document.object.project.workspace.read";
pub(crate) const EDIT: &str = "command.document.object.project.workspace.edit";
const FIELDS: [&str; 7] = [
    "layerPanelWidth",
    "toolsPanelWidth",
    "propsPanelWidth",
    "propsPanelCollapsed",
    "timelineHeight",
    "panelOrder",
    "motionColumns",
];
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
fn workspace(value: &Value, capture: bool) -> bool {
    let required = if capture { &FIELDS[..6] } else { &[] };
    let optional = if capture { &FIELDS[6..] } else { &FIELDS[..] };
    let Some(object) = exact(value, required, optional) else {
        return false;
    };
    object.iter().all(|(key, value)| match key.as_str() {
        "propsPanelCollapsed" => value.is_boolean(),
        "panelOrder" => {
            value.is_null()
                || value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
        }
        _ => value.is_null() || value.is_string(),
    })
}
pub(super) fn request(operation: &str, value: &Value) -> bool {
    if operation == EDIT {
        return exact(value, &["command", "action"], &[]).is_some()
            && value["command"] == "object.project.workspace.edit"
            && exact(&value["action"], &["kind", "workspace"], &[]).is_some()
            && value["action"]["kind"] == "capture"
            && workspace(&value["action"]["workspace"], true);
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
        && exact(&value["selector"], &["kind"], &[]).is_some()
        && value["selector"]["kind"] == "record"
}
fn record(value: &Value) -> bool {
    let Some(object) = exact(value, &["workspacePresent"], &["workspace"]) else {
        return false;
    };
    match value["workspacePresent"].as_bool() {
        Some(false) => !object.contains_key("workspace"),
        Some(true) => object
            .get("workspace")
            .is_some_and(|value| workspace(value, false)),
        None => false,
    }
}
pub(super) fn result(operation: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if operation == EDIT {
        return super::settings::result(value);
    }
    operation == READ
        && request(operation, payload)
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
        && value["instanceId"].as_str().is_some_and(bounded_identifier)
        && value["documentId"].as_str() == Some(document)
        && ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .all(|key| value.get(*key) == payload.get(*key))
        && record(&value["result"])
}
#[cfg(test)]
#[path = "native_workspace_contract_tests.rs"]
mod tests;
