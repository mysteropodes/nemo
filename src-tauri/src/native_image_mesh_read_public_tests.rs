//! Actual public wire refusal without a presented surface; no installed acceptance is inferred.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    open, round_trip, saved,
};
use crate::application_mcp::ApplicationMcp;
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
async fn raw_mesh_read_cannot_manufacture_a_presented_target_or_mutate_the_owner() {
    let state = ApplicationMcp::default();
    let id = open(&state, project().to_string());
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let raw = json!({"apiVersion":2,"requestId":"mesh-read-without-surface","instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object","payload":{
            "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],
            "view":{"kind":"imageMesh","contextId":"scene-root","layerUid":"L0","frame":0,
                "workId":"native-work-900","viewGeneration":1,"page":{"kind":"selection"}}}}).to_string();
    let command = nemo_mcp::wire::decode_native_request(raw.as_bytes()).unwrap();
    command.validate().unwrap();
    let (response, events) = round_trip(&state, command.clone(), None).await;
    assert!(!response.ok);
    assert_eq!(response.request_id, command.request_id);
    assert_eq!(response.instance_id, command.instance_id);
    assert_eq!(response.document_id, command.document_id);
    assert_eq!(response.content_revision, 0);
    assert_eq!(response.error.unwrap().code, "unavailable");
    assert_eq!(events, 0, "a read never creates a revision notification");
    assert_eq!(saved(&state, &id), before);
    let after = state.native_project_info(&id).unwrap();
    for field in [
        "contentRevision",
        "documentSnapshotId",
        "documentDigest",
        "byteCount",
    ] {
        assert_eq!(after[field], info[field], "{field}");
    }
    // A successful response still requires the real host/surface run, including invalidation.
}
