//! #2203 real host/public API join controls. Authored UNRUN; not installed proof.
use crate::{application_mcp::ApplicationMcp, native_dispatch::NativeObjectHost};
use native_engine::{commands::OpacityRequest, object_snapshot::ObjectSnapshot};
use serde_json::{json, Value};

const REMOVE: &str = "command.document.object.asset.remove";
fn with_host<T>(state: &ApplicationMcp, read: impl FnOnce(&mut NativeObjectHost) -> T) -> T {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    read(
        authority
            .active_mut(generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .unwrap(),
    )
}
fn snapshot(state: &ApplicationMcp) -> ObjectSnapshot {
    with_host(state, |host| {
        host.history
            .acquire_snapshot(host.history.content_revision())
            .unwrap()
    })
}
fn pin(state: &ApplicationMcp) -> Value {
    state
        .native_project_info(snapshot(state).document_id())
        .unwrap()
}
fn saved(state: &ApplicationMcp) -> Vec<u8> {
    let p = pin(state);
    state
        .native_project_file_snapshot(
            p["documentId"].as_str().unwrap(),
            p["contentRevision"].as_u64().unwrap(),
            p["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn oracle(name: &str) -> Value {
    include_str!("../../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == name)
        .unwrap()
}
fn seed() -> ApplicationMcp {
    let state = ApplicationMcp::default();
    state.install_native_blank_project(320, 180, 24.).unwrap();
    // Fixture setup uses the existing native byte provider; the tested removal
    // below always enters the actual public application API and RevisionSync.
    for (id, name) in [("blue", "blue-2x1"), ("red", "red-2x1")] {
        let row = oracle(name);
        let hex = row["hex"].as_str().unwrap();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        with_host(&state, |host| {
            let current = host
                .history
                .acquire_snapshot(host.history.content_revision())
                .unwrap();
            let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,
                "requestId":id,"instanceId":state.instance_id(),"documentId":current.document_id(),
                "expectedRevision":current.content_revision(),"operation":"command.document.object.asset.attach",
                "payload":{"documentSnapshotId":current.snapshot_id(),"name":name,"mediaType":"image/png"}})).unwrap();
            assert!(host
                .history
                .dispatch_asset_command(&request, Some(&bytes))
                .unwrap()
                .is_ok());
        });
    }
    state
}
fn request(state: &ApplicationMcp, id: &str, asset: &str) -> Value {
    let p = pin(state);
    json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":p["documentId"],"expectedRevision":p["contentRevision"],"operation":REMOVE,
        "payload":{"documentSnapshotId":p["documentSnapshotId"],"assetId":asset}})
}
fn invoke(state: &ApplicationMcp, request: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn history(state: &ApplicationMcp, id: &str, action: &str) -> Value {
    let mut r = request(state, id, "unused");
    r["operation"] = json!(format!("command.document.object.{action}"));
    r["payload"] = json!({"command":format!("object.{action}")});
    serde_json::to_value(
        crate::application_mcp::object_client::dispatch_raw(state, &r.to_string()).unwrap(),
    )
    .unwrap()
}
fn depths(state: &ApplicationMcp) -> (usize, usize) {
    with_host(state, |host| host.history.history_depths())
}

#[test]
fn public_remove_saves_real_remaining_bytes_and_undo_redo_restore_original_records() {
    let state = seed();
    let before = saved(&state);
    let original: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(depths(&state), (2, 0));
    let removed = invoke(&state, request(&state, "remove-blue", "asset-blue"));
    assert_eq!(removed["ok"], true);
    assert_eq!(removed["contentRevision"], 3);
    assert_eq!(
        removed["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let after = saved(&state);
    let file: Value = serde_json::from_slice(&after).unwrap();
    assert_eq!(file["assetInventory"]["assetIds"], json!(["asset-red"]));
    assert_eq!(file["assets"].as_array().unwrap().len(), 1);
    assert_eq!(
        file["assets"][0]["source"]["data"],
        oracle("red-2x1")["b64"]
    );
    assert_eq!(file["assets"][0], original["assets"][1]);
    let reopened = ApplicationMcp::default();
    reopened.install_native_project_file(&after).unwrap();
    assert_eq!(saved(&reopened), after);
    assert_eq!(history(&state, "undo-remove", "undo")["ok"], true);
    assert_eq!(saved(&state), before);
    // This host crate observes bounded public persistence, not the private
    // project representation. Exact Arc record identity remains covered by the
    // unchanged native provider test remove_drops_the_asset_and_undo_redo_restore_the_very_same_records
    // in native-engine/tests/object_raster_asset_attach_dispatch.rs (e339 proof retained).
    let restored: Value = serde_json::from_slice(&saved(&state)).unwrap();
    assert_eq!(restored["assets"], original["assets"]);
    assert_eq!(history(&state, "redo-remove", "redo")["ok"], true);
    assert_eq!(saved(&state), after);
    assert_eq!(depths(&state), (3, 0));
}

#[test]
fn public_exact_replay_after_undo_and_absent_noop_preserve_redo_and_changed_body_is_typed() {
    let state = seed();
    let r = request(&state, "remove-replay", "asset-blue");
    let first = invoke(&state, r.clone());
    assert_eq!(first["ok"], true);
    assert_eq!(invoke(&state, r.clone()), first);
    history(&state, "undo-replay", "undo");
    let before = saved(&state);
    let revision = pin(&state)["contentRevision"].clone();
    assert_eq!(depths(&state), (2, 1));
    assert_eq!(invoke(&state, r.clone()), first);
    assert_eq!(pin(&state)["contentRevision"], revision);
    let noop = invoke(&state, request(&state, "absent-noop", "absent"));
    assert_eq!(
        noop["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(noop["contentRevision"], revision);
    assert_eq!(depths(&state), (2, 1));
    let mut changed = r;
    changed["payload"]["assetId"] = json!("asset-red");
    assert_eq!(invoke(&state, changed)["error"]["code"], "invalid_request");
    assert_eq!(saved(&state), before);
    assert_eq!(depths(&state), (2, 1));
    assert_eq!(history(&state, "redo-after-noop", "redo")["ok"], true);
    assert_eq!(depths(&state), (3, 0));
}

#[test]
fn public_owner_stale_snapshot_and_cancel_refusals_publish_nothing() {
    for (mode, code) in [
        ("instance", "wrong_instance"),
        ("document", "wrong_document"),
        ("revision", "stale_revision"),
        ("snapshot", "stale_revision"),
        ("cancel", "cancelled_before_dispatch"),
    ] {
        let state = seed();
        let before = saved(&state);
        let revision = pin(&state)["contentRevision"].clone();
        let mut r = request(&state, mode, "asset-blue");
        match mode {
            "instance" => r["instanceId"] = json!("other-instance"),
            "document" => r["documentId"] = json!("other-document"),
            "revision" => r["expectedRevision"] = json!(0),
            "snapshot" => r["payload"]["documentSnapshotId"] = json!("other-snapshot"),
            "cancel" => r["cancelledBeforeDispatch"] = json!(true),
            _ => unreachable!(),
        }
        let first = invoke(&state, r.clone());
        assert_eq!(first["error"]["code"], code, "{mode}");
        assert_eq!(invoke(&state, r), first);
        assert_eq!(saved(&state), before);
        assert_eq!(pin(&state)["contentRevision"], revision);
        assert_eq!(depths(&state), (2, 0));
    }
}

#[test]
fn public_remove_refuses_real_viewport_reference_and_placement_without_partial_publication() {
    for reference in [true, false] {
        let original = seed();
        let mut file: Value = serde_json::from_slice(&saved(&original)).unwrap();
        if reference {
            file["viewportReference"] = json!({"kind":"still-image","assetId":"asset-blue",
                "name":"plate.png","visible":true,"opacity":0.5,"offsetFrames":0});
        } else {
            // A real placed raster created by the native item command, then
            // serialized and reopened, exercises the provider's placement guard.
            let p = pin(&original);
            let create = json!({"apiVersion":2,"requestId":"place-blue","instanceId":original.instance_id(),
                "documentId":p["documentId"],"expectedRevision":p["contentRevision"],
                "operation":"command.document.object.item.edit","payload":{"command":"object.raster.create",
                "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
                "layerUid":p["layers"][0]["layerUid"],"strokeId":""},
                "item":{"assetId":"asset-blue","x":0,"y":0,"width":2,"height":1}}});
            assert_eq!(invoke(&original, create)["ok"], true);
            file = serde_json::from_slice(&saved(&original)).unwrap();
        }
        let state = ApplicationMcp::default();
        state
            .install_native_project_file(&serde_json::to_vec(&file).unwrap())
            .unwrap();
        let before = saved(&state);
        let reopened: Value = serde_json::from_slice(&before).unwrap();
        if reference {
            assert_eq!(reopened["viewportReference"]["assetId"], "asset-blue");
        } else {
            assert_eq!(
                reopened["drawing"]["items"]["raster"][0]["item"]["assetId"],
                "asset-blue"
            );
        }
        let r = request(&state, "in-use", "asset-blue");
        let first = invoke(&state, r.clone());
        assert_eq!(first["ok"], false, "reference={reference}: {first}");
        assert_eq!(
            first["error"]["code"], "busy_conflict",
            "reference={reference}"
        );
        assert_eq!(first["error"]["details"]["reason"], "asset_in_use");
        assert_eq!(invoke(&state, r), first);
        assert_eq!(saved(&state), before);
        assert_eq!(depths(&state), (0, 0));
        assert_eq!(pin(&state)["contentRevision"], 0);
    }
}

#[tokio::test]
async fn public_remove_external_wire_requires_subscriber_and_waits_for_exact_ack() {
    use nemo_mcp::{
        registry::Endpoint,
        wire::{self, AuthenticatedWireRequest},
    };
    use tokio::{net::TcpListener, sync::oneshot};
    for subscribed in [false, true] {
        let state = seed();
        let before = saved(&state);
        let binding = state
            .control_revision_for_test(json!({"action":"binding"}))
            .unwrap();
        if subscribed {
            state
                .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
                .unwrap();
        }
        let command = request(&state, "wire-remove", "asset-blue");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            instance_id: state.instance_id().into(),
            port: listener.local_addr().unwrap().port(),
            secret: "isolated-remove".into(),
            build_id: "isolated-remove".into(),
        };
        let (sent, received) = oneshot::channel();
        let client_endpoint = endpoint.clone();
        let mut client = tokio::spawn(async move {
            wire::call_native(
                &client_endpoint,
                serde_json::from_value(command).unwrap(),
                wire::CancellationToken::new(),
            )
            .await
            .unwrap()
        });
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let message: AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
            let native = match message {
                AuthenticatedWireRequest::Native(message) => {
                    assert_eq!(message.secret, endpoint.secret);
                    message.native_request
                }
                _ => panic!("remove did not use native wire"),
            };
            state
                .serve_external_for_test(&mut stream, native, |event| {
                    assert!(
                        subscribed,
                        "absent subscriber must refuse before publication"
                    );
                    sent.send(event).map_err(|_| "subscriber closed".into())
                })
                .await
                .unwrap();
        };
        let observe = async {
            if subscribed {
                let event = received.await.unwrap();
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(20), &mut client)
                        .await
                        .is_err()
                );
                state
                    .control_revision_for_test(json!({"action":"acknowledge",
                    "subscriptionId":binding["subscriptionId"],"event":event}))
                    .unwrap();
            }
            client.await.unwrap()
        };
        let (_, response) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(server, observe)
        })
        .await
        .unwrap();
        assert_eq!(response.ok, subscribed);
        if subscribed {
            assert_eq!(response.content_revision, 3);
            assert_eq!(depths(&state), (3, 0));
        } else {
            assert_eq!(response.error.unwrap().code, "unavailable");
            assert_eq!(saved(&state), before);
            assert_eq!(depths(&state), (2, 0));
        }
    }
}
