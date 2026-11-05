//! #2377: the key selection box warp and the key tint through the sole host, history and project-file projection; not installed proof.
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const TRACK: &str = "command.document.object.property.track.edit";
fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":30,
        "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
        {"property":"opacity","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[80]}]}},
        {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[45]}]}}]}]}});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"key-warp-install","instanceId":state.instance_id(),
        "documentJson":document.to_string()})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn edit(state: &ApplicationMcp, id: &str, rid: &str, revision: u64, action: Value) -> NativeApplicationRequest {
    edit_at(state, id, rid, revision, [10, 10], action)
}
/// One selected key per row: opacity at `frames[0]`, rotation at `frames[1]`.
fn edit_at(state: &ApplicationMcp, id: &str, rid: &str, revision: u64, frames: [u64; 2], action: Value) -> NativeApplicationRequest {
    let selections: Vec<Value> = ["opacity", "rotation"]
        .iter()
        .zip(frames)
        .map(|(p, frame)| json!({"target":{"contextId":"scene-root","layerUid":"L0"},"property":p,"frame":frame}))
        .collect();
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":TRACK,"payload":{"command":"object.property.track.edit",
        "selections":selections,"action":action}}))
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
fn key(file: &Value, record: usize, at: usize) -> Value {
    file["drawing"]["propertyHolders"][0]["properties"][record]["track"]["keys"][at].clone()
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
fn warp_and_tint_replay_project_into_the_file_and_undo() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    // Two rows, a skew from the top edge: the first row moves the whole 5 frames, the last stays.
    let skew = edit(&state, &id, "warp-1", 0, json!({"kind":"warp","mode":"top","total":5}));
    let reply = state.dispatch_native(skew.clone()).unwrap();
    receipt(&reply, 1, true);
    assert_eq!(state.dispatch_native(skew).unwrap(), reply, "an exact retry answers the stored receipt");
    let file = saved(&state, &id);
    assert_eq!(key(&file, 0, 1), json!({"frame":15,"v":[80]}));
    assert_eq!(key(&file, 1, 1), json!({"frame":10,"v":[45]}));
    // A tint is one more entry and rides the project file.
    receipt(&state.dispatch_native(edit_at(&state, &id, "tint-1", 1, [15, 10], json!({"kind":"color","color":"#E8B64C"}))).unwrap(), 2, true);
    let tinted = saved(&state, &id);
    assert_eq!(key(&tinted, 0, 1), json!({"frame":15,"v":[80],"color":"#e8b64c"}));
    assert_eq!(key(&tinted, 1, 1)["color"], json!("#e8b64c"));
    // The same tint again, and a zero drag, change nothing.
    receipt(&state.dispatch_native(edit_at(&state, &id, "tint-2", 2, [15, 10], json!({"kind":"color","color":"#e8b64c"}))).unwrap(), 2, false);
    receipt(&state.dispatch_native(edit_at(&state, &id, "warp-0", 2, [15, 10], json!({"kind":"warp","mode":"bottom","total":0}))).unwrap(), 2, false);
    assert!(undo(&state, &id, "undo-tint", 2).ok);
    assert_eq!(key(&saved(&state, &id), 0, 1), json!({"frame":15,"v":[80]}));
    assert!(undo(&state, &id, "undo-warp", 3).ok);
    assert_eq!(saved(&state, &id), initial);
}
#[test]
fn unreachable_warps_and_malformed_tints_publish_nothing() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    for (rid, action) in [
        ("past-end", json!({"kind":"warp","mode":"top","total":25})),
        ("before-start", json!({"kind":"warp","mode":"bottom","total":-11})),
        ("no-grab", json!({"kind":"warp","mode":"liquify","total":1})),
        ("bad-tint", json!({"kind":"color","color":"amber"})),
    ] {
        // A malformed action is refused by the declared contract, an unreachable one by the engine; neither publishes.
        assert!(state.dispatch_native(edit(&state, &id, rid, 0, action)).map_or(true, |r| !r.ok), "{rid}");
    }
    assert_eq!(saved(&state, &id), initial);
    assert_eq!(state.native_project_info(&id).unwrap()["contentRevision"], json!(0));
}
