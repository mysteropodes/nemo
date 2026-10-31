//! #2256: closed contract of the native enter/exit component-context transition
//! (`query.document.object.workspace.context`), the pure view-state op of the workspace (no revision, no history).
//! The caller sends the workspace it holds with a pinned snapshot and one action; the native owner answers the new workspace
//! and the context the viewport presents. The table is mirrored by `src/js/adapters/native-object-workspace-context-contract.js`.
use crate::contract::{bounded_identifier, NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::{exact, revision};
use serde_json::{Map, Value};

pub(crate) const OP: &str = "query.document.object.workspace.context";
const MAX_ID: usize = 128;
const MAX_TABS: usize = 8;
const SCENE: &str = "scene-root";
const OUTCOMES: [&str; 9] = [
    "entered",
    "already_active",
    "refused_nested",
    "exited",
    "not_in_context",
    "closed",
    "close_noop",
    "refreshed",
    "context_gone",
];

fn context_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID && !id.contains('\0'))
}
fn uint32(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .filter(|n| *n <= u64::from(u32::MAX))
}
fn view(value: Option<&Value>) -> Option<(u64, u64, u64)> {
    let object = exact(value?, &["frame", "workIn", "workOut"], &[])?;
    Some((
        uint32(object.get("frame"))?,
        uint32(object.get("workIn"))?,
        uint32(object.get("workOut"))?,
    ))
}
fn workspace_ok(value: &Value) -> bool {
    let Some(object) = exact(
        value,
        &["activeContextId", "openTabs", "view", "sceneView"],
        &[],
    ) else {
        return false;
    };
    let active = match object.get("activeContextId") {
        Some(Value::Null) => false,
        other => context_id(other).is_some(),
    };
    let Some(tabs) = object
        .get("openTabs")
        .and_then(Value::as_array)
        .filter(|tabs| tabs.len() <= MAX_TABS)
    else {
        return false;
    };
    let ids: Vec<Option<&str>> = tabs.iter().map(|tab| context_id(Some(tab))).collect();
    let unique = |ids: &[Option<&str>]| {
        ids.iter()
            .enumerate()
            .all(|(at, id)| id.is_some() && !ids[..at].contains(id))
    };
    let scene_view = !object["sceneView"].is_null();
    unique(&ids)
        && active == scene_view
        && view(object.get("view"))
            .is_some_and(|(frame, lo, hi)| lo <= hi && frame <= u64::from(u32::MAX))
        && (!scene_view || view(object.get("sceneView")).is_some())
        && (!active || ids.contains(&object["activeContextId"].as_str()))
}
fn action_ok(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("enter") => exact(value, &["kind", "contextId", "entryFrame"], &[]).is_some_and(|o| {
            context_id(o.get("contextId")).is_some()
                && (o["entryFrame"].is_null() || uint32(o.get("entryFrame")).is_some())
        }),
        Some("close") => exact(value, &["kind", "contextId"], &[])
            .is_some_and(|o| context_id(o.get("contextId")).is_some()),
        Some("exit" | "refresh") => exact(value, &["kind"], &[]).is_some(),
        _ => false,
    }
}
pub(crate) fn request(payload: &Value) -> bool {
    exact(
        payload,
        &["atRevision", "documentSnapshotId", "workspace", "action"],
        &[],
    )
    .is_some_and(|p| {
        revision(p.get("atRevision")).is_some()
            && p["documentSnapshotId"]
                .as_str()
                .is_some_and(bounded_identifier)
            && workspace_ok(&p["workspace"])
            && action_ok(&p["action"])
    })
}

pub(crate) fn result(payload: &Value, value: &Value) -> bool {
    if !request(payload) {
        return false;
    }
    let Some(o) = exact(
        value,
        &[
            "atRevision",
            "documentSnapshotId",
            "outcome",
            "contextId",
            "totalFrames",
            "fps",
            "workspace",
        ],
        &[],
    ) else {
        return false;
    };
    let ws: &Map<String, Value> = match value["workspace"].as_object() {
        Some(ws) => ws,
        None => return false,
    };
    let active = ws.get("activeContextId").cloned().unwrap_or(Value::Null);
    let scene = Value::from(SCENE);
    let shown = o.get("contextId");
    o.get("atRevision") == payload.get("atRevision")
        && o.get("documentSnapshotId") == payload.get("documentSnapshotId")
        && o["outcome"]
            .as_str()
            .is_some_and(|outcome| OUTCOMES.contains(&outcome))
        && uint32(o.get("totalFrames")).is_some_and(|n| n >= 1)
        && o["fps"].as_f64().is_some_and(|f| f.is_finite() && f > 0.0)
        && workspace_ok(&value["workspace"])
        && shown == Some(if active.is_null() { &scene } else { &active })
}

/// Owner and revision correlation of a successful envelope: same request, instance and document (a refusal may report
/// the replacement document), and the header revision is the revision the answer is pinned to.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if request.operation != OP {
        return true;
    }
    response.request_id == request.request_id
        && response.instance_id == request.instance_id
        && (response.document_id == request.document_id
            || (!response.ok
                && response
                    .error
                    .as_ref()
                    .is_some_and(|error| error.code == "wrong_document")))
        && (!response.ok
            || response.result.as_ref().is_some_and(|result| {
                result.get("atRevision").and_then(Value::as_u64) == Some(response.content_revision)
                    && request.payload.get("atRevision").and_then(Value::as_u64)
                        == Some(response.content_revision)
            }))
}

#[cfg(test)]
#[path = "native_workspace_context_contract_tests.rs"]
mod tests;
