//! #2520/#2525: the 3D layer switch through the sole host, history and project-file projection: one saved revision per toggle and no undo entry (baseline
//! `toggleLayer3D` pushed none), the layer row reports it, the switch is refused with no revision on a layer the frame path cannot place in 3D, and save
//! keeps it. Not installed proof.
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const POLICY: &str = "command.document.object.layer.policy.edit";
fn install(state: &ApplicationMcp) -> String {
    install_layers(state, json!([{"layerUid":"L0"},{"layerUid":"L1"}]))
}
fn install_layers(state: &ApplicationMcp, layers: Value) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":layers,"objects":[],"propertyHolders":[]}});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"three-d-install","instanceId":state.instance_id(),"documentJson":document.to_string()}).to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"].as_str().unwrap().into()
}
fn switch(state: &ApplicationMcp, id: &str, rid: &str, revision: u64, value: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":POLICY,"payload":{"command":"object.layer.policy.edit",
        "layerUid":"L0","flag":"threeD","value":value}}))
    .unwrap()
}
fn saved(state: &ApplicationMcp, id: &str) -> Value {
    let info = state.native_project_info(id).unwrap();
    let bytes = state
        .native_project_file_snapshot(id, info["contentRevision"].as_u64().unwrap(), info["documentSnapshotId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn receipt(response: &NativeApplicationResponse, revision: u64, applied: bool) {
    assert!(response.ok, "{response:?}");
    assert_eq!(response.content_revision, revision);
    // #2520: the switch is saved with its revision but is no undo step.
    assert_eq!(response.result, Some(json!({"applied":applied,"historyEntriesAdded":0})));
}
fn undo(state: &ApplicationMcp, id: &str, rid: &str, revision: u64) -> NativeApplicationResponse {
    crate::application_mcp::object_client::dispatch_raw(
        state,
        &json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":"command.document.object.undo","payload":{"command":"object.undo"}}).to_string(),
    )
    .unwrap()
}
fn row(state: &ApplicationMcp, id: &str, layer: usize) -> Value {
    state.native_project_info(id).unwrap()["layers"][layer].clone()
}
#[test]
fn the_3d_switch_is_written_projected_in_the_layer_row_saved_and_undone() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    assert!(row(&state, &id, 0).get("threeD").is_none());
    let first = switch(&state, &id, "three-d-on", 0, json!(true));
    let reply = state.dispatch_native(first.clone()).unwrap();
    receipt(&reply, 1, true);
    assert_eq!(state.dispatch_native(first).unwrap(), reply, "an exact retry answers the stored receipt");
    assert_eq!(row(&state, &id, 0)["threeD"], json!(true), "the project row is the read the layer list lights its cell from");
    assert!(row(&state, &id, 1).get("threeD").is_none(), "only the toggled layer");
    assert_eq!(saved(&state, &id)["drawing"]["layers"][0]["threeD"], json!(true), "save keeps the switch");
    receipt(&state.dispatch_native(switch(&state, &id, "three-d-on-again", 1, json!(true))).unwrap(), 1, false);
    receipt(&state.dispatch_native(switch(&state, &id, "three-d-off", 1, json!(false))).unwrap(), 2, true);
    assert!(row(&state, &id, 0).get("threeD").is_none(), "off reads as absent in the row");
    let nothing = undo(&state, &id, "undo-nothing", 2);
    assert!(!nothing.ok, "the switch added no undo entry, so there is nothing to undo");
    assert_eq!(state.native_project_info(&id).unwrap()["contentRevision"], json!(2));
}
#[test]
fn a_layer_the_frame_path_cannot_place_refuses_the_switch_with_no_revision_and_no_history() {
    let state = ApplicationMcp::default();
    let id = install_layers(&state, json!([{"layerUid":"L0","isNullLayer":true},{"layerUid":"L1"}]));
    let before = saved(&state, &id);
    let reply = state.dispatch_native(switch(&state, &id, "three-d-null", 0, json!(true))).unwrap();
    assert!(!reply.ok, "{reply:?}");
    assert_eq!(reply.error.as_ref().map(|e| e.code.as_str()), Some("unavailable"));
    assert_eq!(reply.content_revision, 0);
    assert!(row(&state, &id, 0).get("threeD").is_none());
    assert_eq!(saved(&state, &id), before, "a refusal changes nothing");
    assert!(!undo(&state, &id, "undo-after-refusal", 0).ok);
}
#[test]
fn the_public_contract_refuses_a_non_boolean_3d_value_before_the_owner() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    for value in [json!(null), json!("in"), json!(1)] {
        assert!(switch(&state, &id, "three-d-bad", 0, value).validate().is_err());
    }
    assert_eq!(state.native_project_info(&id).unwrap()["contentRevision"], json!(0));
}
