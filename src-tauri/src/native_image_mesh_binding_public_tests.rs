//! Actual original-byte admission and authenticated ApplicationMcp wire/ACK join.
//! This positive gate is authored UNRUN; it does not claim installed UI acceptance.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use serde_json::{json, Value};

fn project() -> Value {
    let png: Value =
        include_str!("../../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|row| row["name"] == "blue-2x1")
            .unwrap();
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,
        "assets":[{"id":"pixels","name":"blue-2x1","kind":"image",
            "source":{"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png["b64"]}}],
        "assetInventory":{"assetIds":["pixels"],"placements":[]},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":"L0"}],"objects":[],"items":{"raster":[{
                "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"pixels-0",
                    "frameScope":{"kind":"authored","frame":0}},
                "item":{"assetId":"pixels","x":3,"y":5,"width":20,"height":10,"opacity":1}}]}}})
}

#[tokio::test]
async fn raw_application_mesh_attach_commits_one_correlated_atomic_history_and_saved_state() {
    let state = ApplicationMcp::default();
    let id = open(&state, project().to_string());
    let initial = saved(&state, &id);
    let before: Value = serde_json::from_slice(&initial).unwrap();
    let target = before["drawing"]["items"]["raster"][0]["target"].clone();
    // The existing literal fan fixture has eight boundary vertices and one interior vertex.
    let mesh = json!({"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2});
    let raw = json!({"apiVersion":2,"requestId":"raw-mesh-attach","instanceId":state.instance_id(),
        "documentId":id,"expectedRevision":0,"operation":"command.document.object.image.mesh.edit",
        "payload":{"command":"object.image.mesh.edit","action":{"kind":"attach","target":target,"mesh":mesh}}})
        .to_string();
    assert!(raw.len() <= 4096);
    // No typed deserialization shortcut or lower-level owner retry can bypass this gate.
    let command = nemo_mcp::wire::decode_native_request(raw.as_bytes())
        .expect("the closed public input contract admits the implemented attach action");
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = state
        .revisions
        .control(&state.native, RevisionControl::Binding)
        .unwrap();
    state
        .revisions
        .control(
            &state.native,
            serde_json::from_value(json!({"action":"subscribe","binding":binding})).unwrap(),
        )
        .unwrap();
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let actual: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(actual["drawing"]["items"]["raster"][0]["target"], target);
    assert_eq!(
        actual["drawing"]["items"]["raster"][0]["item"]["meshId"],
        "im_1"
    );
    assert_eq!(actual["drawing"]["imageMeshes"], json!({"im_1":mesh}));
    let mut expected = before;
    expected["drawing"]["items"]["raster"][0]["item"]["meshId"] = json!("im_1");
    expected["drawing"]["imageMeshes"] = actual["drawing"]["imageMeshes"].clone();
    assert_eq!(
        actual, expected,
        "attach changes only the binding and shared mesh store"
    );
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    // Public MCP history is undeclared; this is the existing bounded desktop history adapter.
    assert!(history(&state, &id, "undo-raw-mesh-attach", 1, false).ok);
    assert_eq!(
        saved(&state, &id),
        initial,
        "one undo restores tag and store together"
    );
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), changed);
}
