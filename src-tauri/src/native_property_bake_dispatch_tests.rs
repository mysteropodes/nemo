//! Actual bundled MCP native wire client -> subscribed owner -> retained VM.
//! No installed UI, filesystem SaveAs or common render/export claim.
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use super::property_edit_tests::{history, open, receipt, request, round_trip, saved};
use serde_json::{json, Value};

#[tokio::test]
async fn expression_bake_real_mcp_client_requires_ack_and_preserves_history_and_reopen() {
    let state = ApplicationMcp::default();
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
            "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{
                "property":"opacity","static":[17],"expression":{"enabled":true,"code":"value + frame"}}]}]}});
    let id = open(&state, project.to_string());
    let initial = saved(&state, &id);
    let command = request(&state, &id, "wire-bake", 0, json!({"kind":"bake"}));
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = state.revisions.control(&state.native, RevisionControl::Binding).unwrap();
    state.revisions.control(&state.native,
        serde_json::from_value(json!({"action":"subscribe","binding":binding})).unwrap()).unwrap();
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let project: Value = serde_json::from_slice(&changed).unwrap();
    let property = &project["drawing"]["propertyHolders"][0]["properties"][0];
    assert_eq!(property["expression"], json!({"enabled":false,"code":"value + frame"}));
    let keys = property["track"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["frame"], 0);
    assert_eq!(keys[0]["v"], json!([17.]));
    assert_eq!(keys[1]["frame"], 15);
    assert_eq!(keys[1]["v"], json!([32.]));
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "undo-wire-bake", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), changed);
}
