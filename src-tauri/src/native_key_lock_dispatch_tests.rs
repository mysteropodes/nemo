//! D37: layer keyLock and per-key lockTo through the sole host, history and project-file projection; not installed proof.
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const POLICY: &str = "command.document.object.layer.policy.edit";
const TRACK: &str = "command.document.object.property.track.edit";
fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{"property":"opacity","track":{"keys":[
        {"frame":0,"v":[0],"lockTo":"in"},{"frame":4,"v":[80]},{"frame":12,"v":[0]}]}}]}]}});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"key-lock-install","instanceId":state.instance_id(),
        "documentJson":document.to_string()})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn policy(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    value: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":POLICY,"payload":{"command":"object.layer.policy.edit",
        "layerUid":"L0","flag":"keyLock","value":value}}))
    .unwrap()
}
fn track(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    frames: &[u64],
    mode: Value,
) -> NativeApplicationRequest {
    let selections: Vec<Value> = frames
        .iter()
        .map(|frame| json!({"target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","frame":frame}))
        .collect();
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":TRACK,"payload":{"command":"object.property.track.edit",
        "selections":selections,"action":{"kind":"lockTo","mode":mode}}}))
    .unwrap()
}
fn saved(state: &ApplicationMcp, id: &str) -> Value {
    let info = state.native_project_info(id).unwrap();
    let bytes = state
        .native_project_file_snapshot(
            id,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn keys(file: &Value) -> Value {
    file["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"].clone()
}
fn receipt(response: &NativeApplicationResponse, revision: u64, applied: bool) {
    assert!(response.ok, "{response:?}");
    assert_eq!(response.content_revision, revision);
    assert_eq!(
        response.result,
        Some(json!({"applied":applied,"historyEntriesAdded":u64::from(applied)}))
    );
}
fn undo(state: &ApplicationMcp, id: &str, rid: &str, revision: u64) -> NativeApplicationResponse {
    crate::application_mcp::object_client::dispatch_raw(
        state,
        &json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":"command.document.object.undo","payload":{"command":"object.undo"}})
        .to_string(),
    )
    .unwrap()
}
#[test]
fn layer_key_lock_is_written_replayed_projected_in_the_layer_row_and_undone() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    assert!(state.native_project_info(&id).unwrap()["layers"][0]
        .get("keyLock")
        .is_none());
    let first = policy(&state, &id, "lock-in", 0, json!("in"));
    let reply = state.dispatch_native(first.clone()).unwrap();
    receipt(&reply, 1, true);
    assert_eq!(
        state.dispatch_native(first).unwrap(),
        reply,
        "an exact retry answers the stored receipt"
    );
    assert_eq!(
        state.native_project_info(&id).unwrap()["layers"][0]["keyLock"],
        "in",
        "the project row is the read the menu ticks"
    );
    assert_eq!(saved(&state, &id)["drawing"]["layers"][0]["keyLock"], "in");
    receipt(
        &state
            .dispatch_native(policy(&state, &id, "lock-in-again", 1, json!("in")))
            .unwrap(),
        1,
        false,
    );
    receipt(
        &state
            .dispatch_native(policy(&state, &id, "lock-clear", 1, Value::Null))
            .unwrap(),
        2,
        true,
    );
    assert!(state.native_project_info(&id).unwrap()["layers"][0]
        .get("keyLock")
        .is_none());
    receipt(
        &state
            .dispatch_native(policy(&state, &id, "lock-clear-again", 2, Value::Null))
            .unwrap(),
        2,
        false,
    );
    assert!(undo(&state, &id, "undo-clear", 2).ok);
    assert_eq!(
        state.native_project_info(&id).unwrap()["layers"][0]["keyLock"],
        "in"
    );
    assert!(undo(&state, &id, "undo-set", 3).ok);
    assert_eq!(saved(&state, &id), initial);
}
#[test]
fn key_lock_to_is_written_atomically_and_a_stale_or_unknown_key_publishes_nothing() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    receipt(
        &state
            .dispatch_native(track(&state, &id, "to-out", 0, &[4, 12], json!("out")))
            .unwrap(),
        1,
        true,
    );
    let got = keys(&saved(&state, &id));
    assert_eq!(
        (
            got[0]["lockTo"].clone(),
            got[1]["lockTo"].clone(),
            got[2]["lockTo"].clone()
        ),
        (json!("in"), json!("out"), json!("out"))
    );
    let before = saved(&state, &id);
    let unknown = state
        .dispatch_native(track(&state, &id, "to-missing", 1, &[4, 5], json!("layer")))
        .unwrap();
    assert!(!unknown.ok);
    assert_eq!(unknown.error.unwrap().code, "not_found");
    let stale = state
        .dispatch_native(track(&state, &id, "to-stale", 0, &[4], json!("layer")))
        .unwrap();
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(
        saved(&state, &id),
        before,
        "no partial selection was published"
    );
    receipt(
        &state
            .dispatch_native(track(&state, &id, "to-same", 1, &[4, 12], json!("out")))
            .unwrap(),
        1,
        false,
    );
    receipt(
        &state
            .dispatch_native(track(&state, &id, "to-clear", 1, &[0, 4], Value::Null))
            .unwrap(),
        2,
        true,
    );
    let cleared = keys(&saved(&state, &id));
    assert_eq!(
        (
            cleared[0]["lockTo"].clone(),
            cleared[1]["lockTo"].clone(),
            cleared[2]["lockTo"].clone()
        ),
        (Value::Null, Value::Null, json!("out"))
    );
    assert!(undo(&state, &id, "undo-clear", 2).ok);
    assert!(undo(&state, &id, "undo-set", 3).ok);
    assert_eq!(saved(&state, &id), initial);
}
#[test]
fn malformed_key_lock_requests_are_refused_before_the_owner() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    for bad in [json!("both"), json!(true), json!(""), json!(["in"])] {
        assert!(
            policy(&state, &id, "bad-layer", 0, bad.clone())
                .validate()
                .is_err(),
            "{bad}"
        );
        assert!(
            track(&state, &id, "bad-key", 0, &[4], bad.clone())
                .validate()
                .is_err(),
            "{bad}"
        );
    }
    assert_eq!(saved(&state, &id), before);
}
