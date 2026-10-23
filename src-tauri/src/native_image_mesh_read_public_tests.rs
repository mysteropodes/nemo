//! Actual authenticated read refusal plus real GPU-presented host/public reads.
//! Borrowed test surfaces consume the actual compositor output; no installed/AppKit proof is inferred.
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
    // The positive tests below separately require a real compositor and presented surface.
}

use super::super::tests::{output, present, snapshot, viewport};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use crate::native_object_bootstrap::bootstrap_raw;
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{PresentationReceipt, ViewportStatus},
};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};

// Local helpers use crate-visible application entry points. The unrelated
// revision-dispatch test child's private helpers are intentionally not imported.
fn open(state: &ApplicationMcp, document: String) -> String {
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"mesh-read-install",
        "instanceId":state.instance_id(),"documentJson":document})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn saved(state: &ApplicationMcp, id: &str) -> Vec<u8> {
    let info = state.native_project_info(id).unwrap();
    state
        .native_project_file_snapshot(
            id,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
async fn round_trip(
    state: &ApplicationMcp,
    original: NativeApplicationRequest,
    binding: Option<&Value>,
) -> (NativeApplicationResponse, usize) {
    use nemo_mcp::{registry::Endpoint, wire};
    assert!(
        binding.is_none(),
        "this read-only helper never invents a revision subscription"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "mesh-read-private-wire".into(),
        build_id: "mesh-read-test".into(),
    };
    let mut events = 0;
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let wire::AuthenticatedWireRequest::Native(message) = envelope else {
            panic!("non-native request")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(
            serde_json::to_value(&message.native_request).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        state
            .serve_external_for_test(&mut stream, message.native_request, |_| {
                events += 1;
                Err("an immutable mesh read must not publish a revision event".into())
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(&endpoint, original.clone(), wire::CancellationToken::new());
    let (_, response) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    (response.unwrap(), events)
}
fn presented_project() -> Value {
    let mut value = project();
    value["project"]["canvasH"] = json!(64);
    value["drawing"]["formatVersion"] = json!(2);
    value["drawing"]["totalFrames"] = json!(2);
    value["drawing"]["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
        "formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                {"layerUid":"L0","strokeId":"pixels-0","naturalPosition":0,"order":{}}]},
                {"frameScope":{"kind":"authored","frame":1},"elements":[
                    {"layerUid":"L0","strokeId":"pixels-1","naturalPosition":0,"order":{}}]}]}]});
    value["drawing"]["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
        "family":"drawing","contextId":"scene-root","keys":[{"frame":0},{"frame":1}],"samples":[]});
    let mut second = value["drawing"]["items"]["raster"][0].clone();
    second["target"]["frameScope"]["frame"] = json!(1);
    second["target"]["strokeId"] = json!("pixels-1");
    value["drawing"]["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .push(second);
    for entry in value["drawing"]["items"]["raster"].as_array_mut().unwrap() {
        entry["item"]["meshId"] = json!("im_1");
    }
    value["drawing"]["imageMeshes"] = json!({"im_1":{"outline":[[0,0],[1,0],[1,1],[0,1]],
        "verts":[[0,0],[1,0],[1,1],[0,1]],"tris":[0,1,2,0,2,3],
        "offsets":[[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}});
    value
}
fn host_info(owner: &NativeObjectHost) -> Value {
    let pin = snapshot(owner);
    json!({"instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "contentRevision":owner.content_revision(),"documentSnapshotId":pin.snapshot_id()})
}
fn query(info: &Value, shown: &PresentationReceipt, frame: u32, page: Value) -> Value {
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    let identity = shown.identity();
    json!({"apiVersion":2,"requestId":format!("mesh-read-{}-{}",page["kind"].as_str().unwrap(),page["cursor"].as_u64().unwrap_or(0)),
        "instanceId":info["instanceId"],"documentId":info["documentId"],"operation":"query.document.object",
        "payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{
            "kind":"imageMesh","contextId":"scene-root","layerUid":"L0","frame":frame,
            "workId":format!("native-work-{}",identity.work_id().value()),
            "viewGeneration":identity.view_generation().value(),"page":page}}})
}
fn host_read(owner: &mut NativeObjectHost, request: &Value) -> Value {
    assert!(request.to_string().len() <= 4096);
    let response = owner
        .dispatch(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert!(response.to_string().len() <= 4096);
    response
}
fn host_saved(owner: &NativeObjectHost) -> Vec<u8> {
    let pin = snapshot(owner);
    owner
        .project_file_snapshot(owner.content_revision(), pin.snapshot_id())
        .unwrap()
        .unwrap()
}
fn assert_selected(response: &Value, request: &Value) {
    assert_eq!(response["ok"], true, "{response}");
    for field in ["requestId", "instanceId", "documentId"] {
        assert_eq!(response[field], request[field]);
    }
    let result = &response["result"];
    assert_eq!(result["atRevision"], request["payload"]["atRevision"]);
    assert_eq!(
        result["documentSnapshotId"],
        request["payload"]["documentSnapshotId"]
    );
    for field in ["contextId", "layerUid", "frame", "workId", "viewGeneration"] {
        assert_eq!(result[field], request["payload"]["view"][field]);
    }
    assert_eq!(
        result["target"],
        json!({"contextId":"scene-root","layerUid":"L0","strokeId":"pixels-0",
        "frameScope":{"kind":"authored","frame":0}})
    );
    for (field, expected) in [
        ("x", -7.),
        ("y", 0.),
        ("width", 20.),
        ("height", 10.),
        ("rotation", 0.),
    ] {
        assert_eq!(result["rect"][field].as_f64(), Some(expected));
    }
    assert_eq!(
        result["mesh"],
        json!({"meshId":"im_1","outlineCount":4,"vertexCount":4,
        "triangleIndexCount":6,"offsetsPresent":true,"cols":1,"rows":1})
    );
}
fn mutate(owner: &mut NativeObjectHost, id: &str, undo: bool) {
    let (operation, payload) = if undo {
        (
            "command.document.object.undo",
            json!({"command":"object.undo"}),
        )
    } else {
        (
            "command.document.object.image.mesh.edit",
            json!({"command":"object.image.mesh.edit","action":{"kind":"detach",
            "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"pixels-0","frameScope":{"kind":"authored","frame":0}}}}),
        )
    };
    let request = json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),"operation":operation,"payload":payload});
    let response = host_read(owner, &request);
    assert_eq!(response["ok"], true, "{response}");
}

#[test]
fn gpu_presented_host_mesh_pages_are_correlated_and_mutation_undo_release_invalidate() {
    let source = presented_project();
    let mut owner = NativeObjectHost::from_project_file(
        "mesh-presented-host".into(),
        source.to_string().as_bytes(),
    )
    .unwrap();
    let initial = host_saved(&owner);
    let depths = owner.history.history_depths();
    let mut gpu = Compositor::new().unwrap();
    let (mut surface, trace) = viewport(&gpu, 64);
    trace.borrow_mut().pixel = Some([0, 0, 255, 255]);
    let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    assert!(owner.preview.reference_cache.is_some());
    assert_eq!(
        (trace.borrow().acquisitions, trace.borrow().settlements),
        (1, 1)
    );
    let info = host_info(&owner);
    let selected = query(&info, &shown, 0, json!({"kind":"selection"}));
    assert_selected(&host_read(&mut owner, &selected), &selected);
    let mesh = &source["drawing"]["imageMeshes"]["im_1"];
    for kind in ["outline", "verts", "tris", "offsets"] {
        let mut cursor = 0;
        let mut entries = Vec::new();
        loop {
            let request = query(
                &info,
                &shown,
                0,
                json!({"kind":kind,"meshId":"im_1","cursor":cursor,"limit":2}),
            );
            let response = host_read(&mut owner, &request);
            assert_selected(&response, &request);
            let page = &response["result"]["page"];
            assert_eq!(page["kind"], kind);
            assert_eq!(page["cursor"], cursor);
            assert_eq!(
                page["total"].as_u64(),
                Some(mesh[kind].as_array().unwrap().len() as u64)
            );
            let items = page["items"].as_array().unwrap();
            assert!(items.len() <= 2);
            entries.extend_from_slice(items);
            if page["nextCursor"].is_null() {
                break;
            }
            cursor = page["nextCursor"].as_u64().unwrap();
            assert_eq!(cursor, entries.len() as u64);
        }
        assert_eq!(Value::Array(entries), mesh[kind]);
    }
    for (path, value) in [
        ("/payload/view/workId", json!("native-work-999999999")),
        (
            "/payload/view/viewGeneration",
            json!(shown.identity().view_generation().value() + 1),
        ),
        ("/payload/view/frame", json!(1)),
        ("/payload/atRevision", json!(99)),
        (
            "/payload/documentSnapshotId",
            json!("native-object:stale:0"),
        ),
    ] {
        let mut wrong = selected.clone();
        *wrong.pointer_mut(path).unwrap() = value;
        assert_eq!(
            host_read(&mut owner, &wrong)["error"]["code"],
            "stale_revision"
        );
    }
    assert_eq!(owner.history.history_depths(), depths);
    assert_eq!(host_saved(&owner), initial);
    mutate(&mut owner, "mesh-detach", false);
    let info = host_info(&owner);
    assert_eq!(
        host_read(
            &mut owner,
            &query(&info, &shown, 0, json!({"kind":"selection"}))
        )["error"]["code"],
        "unavailable"
    );
    let detached = present(&mut owner, &mut gpu, &mut surface).unwrap();
    assert_ne!(shown.identity().work_id(), detached.identity().work_id());
    let request = query(&info, &detached, 0, json!({"kind":"selection"}));
    let response = host_read(&mut owner, &request);
    assert_eq!(response["ok"], true);
    assert!(response["result"]["mesh"].is_null());
    mutate(&mut owner, "mesh-undo", true);
    let info = host_info(&owner);
    assert_eq!(
        host_read(
            &mut owner,
            &query(&info, &detached, 0, json!({"kind":"selection"}))
        )["error"]["code"],
        "unavailable"
    );
    let restored = present(&mut owner, &mut gpu, &mut surface).unwrap();
    let request = query(&info, &restored, 0, json!({"kind":"selection"}));
    assert_selected(&host_read(&mut owner, &request), &request);
    assert_eq!(host_saved(&owner), initial);
    owner.release_project().unwrap();
    assert!(owner
        .dispatch(serde_json::from_value(request).unwrap())
        .is_err());
}

#[tokio::test]
async fn gpu_presented_original_application_owner_serves_authenticated_mesh_reads_without_events() {
    let state = ApplicationMcp::default();
    let source = presented_project();
    let id = open(&state, source.to_string());
    let initial = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let generation = info["lifecycleGeneration"].as_u64().unwrap();
    let selected = native_engine::object_frame_packet::ObjectFrameSelector {
        instance_id: state.instance_id(),
        document_id: &id,
        content_revision: info["contentRevision"].as_u64().unwrap(),
        document_snapshot_id: info["documentSnapshotId"].as_str().unwrap(),
        context_id: "scene-root",
        scope_kind: native_engine::object_document::FrameScopeKind::Authored,
        frame: 0,
    };
    let mut gpu = Compositor::new().unwrap();
    let (mut surface, trace) = viewport(&gpu, 64);
    trace.borrow_mut().pixel = Some([0, 0, 255, 255]);
    let shown = state
        .present_object_preview(generation, &selected, output(), &mut gpu, &mut surface)
        .unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    for page in [
        json!({"kind":"selection"}),
        json!({"kind":"outline","meshId":"im_1","cursor":0,"limit":16}),
        json!({"kind":"verts","meshId":"im_1","cursor":0,"limit":16}),
        json!({"kind":"tris","meshId":"im_1","cursor":0,"limit":16}),
        json!({"kind":"offsets","meshId":"im_1","cursor":0,"limit":16}),
    ] {
        let request = query(&info, &shown, 0, page);
        let raw = request.to_string();
        assert!(raw.len() <= 4096);
        let command = nemo_mcp::wire::decode_native_request(raw.as_bytes()).unwrap();
        command.validate().unwrap();
        let (response, events) = round_trip(&state, command, None).await;
        assert_eq!(events, 0);
        let wire = serde_json::to_value(response).unwrap();
        assert!(wire.to_string().len() <= 4096);
        assert_selected(&wire, &request);
        let kind = request["payload"]["view"]["page"]["kind"].as_str().unwrap();
        assert_eq!(wire["result"]["page"]["kind"], kind);
        if kind != "selection" {
            let expected = &source["drawing"]["imageMeshes"]["im_1"][kind];
            assert_eq!(wire["result"]["page"]["items"], *expected);
            assert_eq!(
                wire["result"]["page"]["total"].as_u64(),
                Some(expected.as_array().unwrap().len() as u64)
            );
            assert!(wire["result"]["page"]["nextCursor"].is_null());
        }
    }
    assert_eq!(
        (trace.borrow().acquisitions, trace.borrow().settlements),
        (1, 1)
    );
    assert_eq!(saved(&state, &id), initial);
    let after = state.native_project_info(&id).unwrap();
    for field in [
        "contentRevision",
        "documentSnapshotId",
        "documentDigest",
        "byteCount",
        "undoDepth",
        "redoDepth",
    ] {
        assert_eq!(after[field], info[field], "{field}");
    }
}
