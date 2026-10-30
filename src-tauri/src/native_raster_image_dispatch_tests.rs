//! Actual native authority/barrier with a controlled byte-source boundary. Authored UNRUN.
use crate::application_mcp::{ApplicationMcp, NativeApplicationRequest};
use serde_json::{json, Value};

fn state() -> ApplicationMcp {
    let state = ApplicationMcp::default();
    state.install_native_blank_project(320, 180, 24.).unwrap();
    state
}
fn info(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let lock = native.lock().unwrap();
    let id = lock.active().unwrap().1.document_id().to_owned();
    drop(lock);
    state.native_project_info(&id).unwrap()
}
fn save(state: &ApplicationMcp) -> Vec<u8> {
    let pin = info(state);
    state
        .native_project_file_snapshot(
            pin["documentId"].as_str().unwrap(),
            pin["contentRevision"].as_u64().unwrap(),
            pin["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn png() -> Vec<u8> {
    let corpus =
        include_str!("../../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt");
    let value = corpus
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == "blue-2x1")
        .unwrap();
    let hex = value["hex"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
fn input(state: &ApplicationMcp, id: &str) -> NativeApplicationRequest {
    let pin = info(state);
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],"operation":"command.document.object.asset.attach",
        "payload":{"documentSnapshotId":pin["documentSnapshotId"],"lifecycleGeneration":pin["lifecycleGeneration"],
            "name":"still.png","mediaType":"image/png","source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}}})).unwrap()
}
#[tokio::test]
async fn image_byte_port_runs_outside_both_locks_and_real_pair_survives_save_reload() {
    let state = state();
    let native = state.native_state();
    let request = input(&state, "host-image");
    let reply = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, request, false, |_| {
            assert!(native.try_lock().is_ok());
            assert!(state.revisions.0.try_lock().is_ok());
            async { Ok(png()) }
        })
        .await
        .unwrap()
        .response;
    assert!(reply.ok);
    let result = reply.result.unwrap();
    assert_eq!(result["naturalWidth"], 2);
    assert_eq!(result["naturalHeight"], 1);
    let bytes = save(&state);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(file["assets"].as_array().unwrap().len(), 1);
    assert_eq!(file["drawing"]["layers"].as_array().unwrap().len(), 2);
    let reopened = ApplicationMcp::default();
    reopened.install_native_project_file(&bytes).unwrap();
    assert_eq!(save(&reopened), bytes);
}
#[tokio::test]
async fn retained_image_request_replays_before_another_source_access() {
    let state = state();
    let native = state.native_state();
    let request = input(&state, "replay");
    let first = state
        .revisions
        .dispatch_image_using(
            state.instance_id(),
            &native,
            request.clone(),
            false,
            |_| async { Ok(png()) },
        )
        .await
        .unwrap()
        .response;
    assert!(first.ok);
    let again = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, request, false, |_| async {
            panic!("replay must not access source");
            #[allow(unreachable_code)]
            Ok(Vec::new())
        })
        .await
        .unwrap()
        .response;
    assert_eq!(
        serde_json::to_value(again).unwrap(),
        serde_json::to_value(first).unwrap()
    );
}
#[tokio::test]
async fn stale_lifecycle_cancel_and_absent_external_subscriber_stop_before_source_work() {
    for mode in ["lifecycle", "cancel", "subscriber"] {
        let state = state();
        let original = save(&state);
        let mut request = input(&state, mode);
        if mode == "lifecycle" {
            request.payload["lifecycleGeneration"] = 999.into();
        }
        if mode == "cancel" {
            request.cancelled_before_dispatch = true;
        }
        let reply = state
            .revisions
            .dispatch_image_using(
                state.instance_id(),
                &state.native_state(),
                request,
                mode == "subscriber",
                |_| async {
                    panic!("inadmissible source read");
                    #[allow(unreachable_code)]
                    Ok(Vec::new())
                },
            )
            .await
            .unwrap()
            .response;
        assert!(!reply.ok);
        assert_eq!(save(&state), original);
    }
}
#[tokio::test]
async fn releasing_the_real_owner_while_resolving_image_bytes_blocks_late_publication() {
    use crate::{
        native_application::admit_release_request,
        native_application_contract::NativeReleaseRequest,
    };
    let state = state();
    let pin = info(&state);
    let native = state.native_state();
    let reply=state.revisions.dispatch_image_using(state.instance_id(),&native,input(&state,"late-image"),false,|_|{
        let request:NativeReleaseRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"close-image","instanceId":state.instance_id(),
            "documentId":pin["documentId"],"expectedRevision":0})).unwrap();
        admit_release_request(&native,&request).unwrap();async{Ok(png())}
    }).await.unwrap().response;
    assert!(!reply.ok);
    assert!(native.lock().unwrap().active().is_none());
}
#[tokio::test]
async fn corrupt_image_source_leaves_no_partial_native_asset_or_layer() {
    let state = state();
    let original = save(&state);
    let reply = state
        .revisions
        .dispatch_image_using(
            state.instance_id(),
            &state.native_state(),
            input(&state, "corrupt"),
            false,
            |_| async { Ok(b"not a PNG".to_vec()) },
        )
        .await
        .unwrap()
        .response;
    assert!(!reply.ok);
    assert_eq!(save(&state), original);
}

#[tokio::test]
async fn a_refused_source_reports_its_reason_and_not_dispatched_so_the_client_may_correct_and_retry() {
    let state = state();
    let original = save(&state);
    let reply = state
        .revisions
        .dispatch_image_using(
            state.instance_id(),
            &state.native_state(),
            input(&state, "refused"),
            false,
            |_| async { Err("native image source outside filesystem scope".to_owned()) },
        )
        .await
        .unwrap()
        .response;
    let error = reply.error.unwrap();
    assert_eq!(error.code, "unavailable");
    assert!(error.message.ends_with("outside filesystem scope"), "{}", error.message);
    let details = error.details.unwrap();
    assert_eq!(details["disposition"], "notDispatched");
    assert_eq!(details["committed"], false);
    assert_eq!(save(&state), original);
}

#[tokio::test]
async fn wrong_owner_and_changed_body_return_typed_terminal_envelopes_before_source_io_without_poisoning_replay(
) {
    // Like the existing positive host controls, execution waits for reviewed PNG activation.
    let state = state();
    let native = state.native_state();
    let genuine = input(&state, "typed-errors");
    let initial = save(&state);
    for (field, code) in [
        ("instanceId", "wrong_instance"),
        ("documentId", "wrong_document"),
    ] {
        let mut altered = serde_json::to_value(&genuine).unwrap();
        altered[field] = json!("foreign");
        let altered: NativeApplicationRequest = serde_json::from_value(altered).unwrap();
        let response = state
            .revisions
            .dispatch_image_using(state.instance_id(), &native, altered, false, |_| async {
                panic!("wrong owner must not invoke source resolution");
                #[allow(unreachable_code)]
                Ok(Vec::new())
            })
            .await
            .unwrap()
            .response;
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, code);
        assert_eq!(response.instance_id, state.instance_id());
        assert_eq!(response.document_id, genuine.document_id);
        assert_eq!(save(&state), initial);
    }
    let first = state
        .revisions
        .dispatch_image_using(
            state.instance_id(),
            &native,
            genuine.clone(),
            false,
            |_| async { Ok(png()) },
        )
        .await
        .unwrap()
        .response;
    assert!(
        first.ok,
        "foreign-id failures must not reserve the genuine request id"
    );
    let committed = save(&state);
    let mut changed = genuine.clone();
    changed.payload["source"]["path"] = json!("/another.png");
    let refused = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, changed, false, |_| async {
            panic!("changed body must not invoke source resolution");
            #[allow(unreachable_code)]
            Ok(Vec::new())
        })
        .await
        .unwrap()
        .response;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "invalid_request");
    assert_eq!(save(&state), committed);
    let replay = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, genuine, false, |_| async {
            panic!("genuine retained replay must not invoke source resolution");
            #[allow(unreachable_code)]
            Ok(Vec::new())
        })
        .await
        .unwrap()
        .response;
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(first).unwrap()
    );
    assert_eq!(save(&state), committed);
}
/// #2361: a bitmap-brush Draw stroke rides the same byte port: the anchor, its companion and the PNG land in one
/// revision, survive save/reload, and a replay never reads the source again.
#[tokio::test]
async fn bitmap_brush_stroke_lands_anchor_companion_and_png_through_the_byte_port() {
    let state = state();
    let native = state.native_state();
    let pin = info(&state);
    let file: Value = serde_json::from_slice(&save(&state)).unwrap();
    let layer = file["drawing"]["layers"][0]["layerUid"].clone();
    let point = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":layer,"strokeId":"","frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[point(10.0, 10.0), point(60.0, 20.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":1,"a":1},"hasRealStroke":true,"strokeWidth":8,"strokeCap":"round",
            "strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":0},
        "vectorExt":{"bitmapBrush":{"bitmapBrushSpec":{"tip":"soft","size":8,"spacing":15,"scatter":20,"opacity":1,
            "color":"#0000ffff","pressure":false,"seed":7},"brushGroupId":"bmb_host_1","preTextureOpacity":1}}});
    let request: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"host-bitmap",
        "instanceId":state.instance_id(),"documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],
        "operation":"command.document.object.brush.bitmap.stroke",
        "payload":{"documentSnapshotId":pin["documentSnapshotId"],"lifecycleGeneration":pin["lifecycleGeneration"],
            "record":record,"companion":{"x":35,"y":15,"width":58,"height":18,"opacity":1},
            "source":{"kind":"file","path":"/admitted/brush.png"}}}))
    .unwrap();
    let first = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, request.clone(), false, |_| {
            assert!(native.try_lock().is_ok());
            async { Ok(png()) }
        })
        .await
        .unwrap()
        .response;
    assert!(first.ok, "{}", serde_json::to_value(&first).unwrap());
    let bytes = save(&state);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(file["assets"].as_array().unwrap().len(), 1);
    assert_eq!(file["drawing"]["objects"].as_array().unwrap().len(), 1);
    let companion = &file["drawing"]["items"]["raster"][0]["item"];
    assert_eq!(
        (&companion["isBrushTextureCopy"], &companion["brushGroupId"]),
        (&json!(true), &json!("bmb_host_1"))
    );
    let again = state
        .revisions
        .dispatch_image_using(state.instance_id(), &native, request, false, |_| async {
            panic!("replay must not access source");
            #[allow(unreachable_code)]
            Ok(Vec::new())
        })
        .await
        .unwrap()
        .response;
    assert_eq!(
        serde_json::to_value(again).unwrap(),
        serde_json::to_value(first).unwrap()
    );
    let reopened = ApplicationMcp::default();
    reopened.install_native_project_file(&bytes).unwrap();
    assert_eq!(save(&reopened), bytes);
}
