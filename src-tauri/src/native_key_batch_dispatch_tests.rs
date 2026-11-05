//! #2524: shift, delete, flip and distribute of selected keys through the sole host, history and project-file projection; not installed proof.
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const TRACK: &str = "command.document.object.property.track.edit";
fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{"property":"opacity","track":{"keys":[
        {"frame":0,"v":[0]},{"frame":4,"v":[80]},{"frame":8,"v":[40]},{"frame":12,"v":[20]}]}}]}]}});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"key-batch-install","instanceId":state.instance_id(),
        "documentJson":document.to_string()})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn batch(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    frames: &[u64],
    action: Value,
) -> NativeApplicationRequest {
    let selections: Vec<Value> = frames
        .iter()
        .map(|frame| json!({"target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","frame":frame}))
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
fn frames(file: &Value) -> Vec<u64> {
    file["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect()
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
fn shift_moves_keys_replays_projects_into_the_file_and_undoes() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let first = batch(
        &state,
        &id,
        "shift-2",
        0,
        &[4, 8],
        json!({"kind":"shift","delta":2}),
    );
    let reply = state.dispatch_native(first.clone()).unwrap();
    receipt(&reply, 1, true);
    assert_eq!(
        state.dispatch_native(first).unwrap(),
        reply,
        "an exact retry answers the stored receipt"
    );
    assert_eq!(frames(&saved(&state, &id)), vec![0, 6, 10, 12]);
    // A zero-distance drag release adds no revision and no history entry.
    receipt(
        &state
            .dispatch_native(batch(
                &state,
                &id,
                "shift-0",
                1,
                &[6],
                json!({"kind":"shift","delta":0}),
            ))
            .unwrap(),
        1,
        false,
    );
    assert!(undo(&state, &id, "undo-shift", 1).ok);
    assert_eq!(saved(&state, &id), initial);
}
#[test]
fn collisions_negative_time_and_stale_revisions_publish_nothing() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    for (rid, selected, delta) in [
        ("onto-key", 4u64, 4),
        ("negative", 0, -1),
        ("past-end", 12, 4),
    ] {
        let refused = state
            .dispatch_native(batch(
                &state,
                &id,
                rid,
                0,
                &[selected],
                json!({"kind":"shift","delta":delta}),
            ))
            .unwrap();
        assert!(!refused.ok, "{rid}");
        assert_eq!(refused.error.unwrap().code, "unavailable", "{rid}");
        assert_eq!(saved(&state, &id), before, "{rid}");
    }
    let stale = state
        .dispatch_native(batch(
            &state,
            &id,
            "stale",
            7,
            &[4],
            json!({"kind":"shift","delta":1}),
        ))
        .unwrap();
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    let missing = state
        .dispatch_native(batch(
            &state,
            &id,
            "missing",
            0,
            &[4, 5],
            json!({"kind":"delete"}),
        ))
        .unwrap();
    assert_eq!(missing.error.unwrap().code, "not_found");
    assert_eq!(saved(&state, &id), before);
}
#[test]
fn delete_flip_and_distribute_are_one_history_entry_each() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    receipt(
        &state
            .dispatch_native(batch(
                &state,
                &id,
                "flip",
                0,
                &[0, 12],
                json!({"kind":"flip"}),
            ))
            .unwrap(),
        1,
        true,
    );
    let flipped = saved(&state, &id);
    let values: Vec<Value> = flipped["drawing"]["propertyHolders"][0]["properties"][0]["track"]
        ["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["v"][0].clone())
        .collect();
    assert_eq!(frames(&flipped), vec![0, 4, 8, 12]);
    assert_eq!(
        values,
        vec![json!(20), json!(80), json!(40), json!(0)],
        "the end keys carry their own values"
    );
    // 0, 8 and 12 spread to 0, 6 and 12: the key at 4 stays, and 6 is free.
    receipt(
        &state
            .dispatch_native(batch(
                &state,
                &id,
                "spread",
                1,
                &[0, 8, 12],
                json!({"kind":"distribute"}),
            ))
            .unwrap(),
        2,
        true,
    );
    assert_eq!(frames(&saved(&state, &id)), vec![0, 4, 6, 12]);
    // 0, 4 and 12 spread onto the key now at 6, which stays: the baseline assigned the frames without a collision check, so the
    // moved key replaces it (one key per frame) and the batch is one more history entry.
    receipt(
        &state
            .dispatch_native(batch(
                &state,
                &id,
                "spread-replaces",
                2,
                &[0, 4, 12],
                json!({"kind":"distribute"}),
            ))
            .unwrap(),
        3,
        true,
    );
    assert_eq!(frames(&saved(&state, &id)), vec![0, 6, 12]);
    assert!(undo(&state, &id, "undo-replace", 3).ok);
    assert!(undo(&state, &id, "undo-spread", 4).ok);
    assert!(undo(&state, &id, "undo-flip", 5).ok);
    assert_eq!(saved(&state, &id), initial);
}
#[test]
fn malformed_key_batches_are_refused_before_the_owner() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    for bad in [
        json!({"kind":"shift"}),
        json!({"kind":"shift","delta":1.5}),
        json!({"kind":"shift","delta":"1"}),
        json!({"kind":"shift","delta":1000001}),
        json!({"kind":"delete","keep":true}),
        json!({"kind":"flip","times":2}),
    ] {
        assert!(
            batch(&state, &id, "bad", 0, &[4], bad.clone())
                .validate()
                .is_err(),
            "{bad}"
        );
    }
    assert_eq!(saved(&state, &id), before);
}
