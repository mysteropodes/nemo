//! F04 public shared-service tests over the real installed-in-memory native host.
//! These are source tests, not packaged desktop acceptance.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};
const OP: &str = "command.document.object.project.settings.edit";
fn drawing() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,"layers":[{"layerUid":"L"}],"objects":[]})
}
fn install(state: &ApplicationMcp, project: bool) -> Value {
    let document = if project {
        json!({"format":"nemo.native-project-document","formatVersion":1,
      "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":drawing(),"viewportReference":null,"assets":[]})
    } else {
        drawing()
    };
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"install-settings",
      "instanceId":state.instance_id(),"documentJson":document.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(receipt: &Value, id: &str, revision: u64, action: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
      "expectedRevision":revision,"operation":OP,"payload":{"command":"object.project.settings.edit","action":action}})
}
fn dispatch(state: &ApplicationMcp, r: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(r).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn saved(state: &ApplicationMcp, r: &Value) -> Value {
    let id = r["documentId"].as_str().unwrap();
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
#[test]
fn public_settings_reach_actual_owner_and_preserve_optional_metadata_until_exact_edit() {
    let state = ApplicationMcp::default();
    let receipt = install(&state, true);
    let initial = saved(&state, &receipt);
    assert!(initial["project"].get("canvasBg").is_none());
    assert!(initial["project"].get("waIn").is_none());
    assert!(initial["project"].get("waOut").is_none());
    let actions = [
        json!({"kind":"setFps","fps":29.97}),
        json!({"kind":"setCanvasSize","width":640,"height":360}),
        json!({"kind":"setCanvasBackground","color":"#AbC12380"}),
        json!({"kind":"setWorkArea","inFrame":20.5,"outFrame":-3.25}),
    ];
    for (i, action) in actions.into_iter().enumerate() {
        let r = request(&receipt, &format!("setting-{i}"), i as u64, action);
        let response = dispatch(&state, r.clone());
        assert_eq!(response["ok"], true);
        assert_eq!(response["contentRevision"], i as u64 + 1);
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
        for field in ["requestId", "instanceId", "documentId"] {
            assert_eq!(response[field], r[field]);
        }
        assert_eq!(
            dispatch(&state, r),
            response,
            "exact public retry must use retained receipt"
        );
        let after = saved(&state, &receipt);
        assert_eq!(after["drawing"], initial["drawing"]);
        assert_eq!(after["assets"], initial["assets"]);
        assert_eq!(after["viewportReference"], initial["viewportReference"]);
        if i < 2 {
            assert!(after["project"].get("canvasBg").is_none());
            assert!(after["project"].get("waIn").is_none());
        }
    }
    let value = saved(&state, &receipt);
    let p = &value["project"];
    assert_eq!(
        p,
        &json!({"canvasW":640,"canvasH":360,"fps":29.97,"canvasBg":"#AbC12380","waIn":20.5,"waOut":-3.25})
    );
    let noop = dispatch(
        &state,
        request(
            &receipt,
            "settings-noop",
            4,
            json!({"kind":"setFps","fps":29.97}),
        ),
    );
    assert_eq!(noop["contentRevision"], 4);
    assert_eq!(
        noop["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(info["undoDepth"], 4);
    assert_eq!(info["redoDepth"], 0);
}
#[test]
fn public_settings_stale_cancel_foreign_and_invalid_input_preserve_actual_project() {
    let state = ApplicationMcp::default();
    let receipt = install(&state, true);
    let before = saved(&state, &receipt);
    let good = request(
        &receipt,
        "settings-good",
        0,
        json!({"kind":"setFps","fps":48}),
    );
    for (id, field, value, code) in [
        ("stale", "expectedRevision", json!(1), "stale_revision"),
        (
            "cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        (
            "foreign",
            "documentId",
            json!("different-document"),
            "wrong_document",
        ),
    ] {
        let mut r = good.clone();
        r["requestId"] = json!(id);
        r[field] = value;
        let response = dispatch(&state, r);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], code);
        assert_eq!(saved(&state, &receipt), before);
    }
    for action in [
        json!({"kind":"setFps","fps":null}),
        json!({"kind":"setCanvasSize","width":0,"height":100}),
        json!({"kind":"setCanvasBackground","color":null}),
        json!({"kind":"setWorkArea","inFrame":2}),
    ] {
        let r = request(&receipt, "invalid", 0, action);
        assert!(state
            .dispatch_native(serde_json::from_value(r).unwrap())
            .is_err());
        assert_eq!(saved(&state, &receipt), before);
    }
    let response = dispatch(&state, good.clone());
    assert_eq!(response["ok"], true);
    let after = saved(&state, &receipt);
    let mut changed = good;
    changed["payload"]["action"]["fps"] = json!(30);
    let refused = dispatch(&state, changed);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(saved(&state, &receipt), after);
}
#[test]
fn public_metadata_unavailable_without_project_never_creates_synthetic_settings() {
    let state = ApplicationMcp::default();
    let receipt = install(&state, false);
    let r = dispatch(
        &state,
        request(
            &receipt,
            "bare-settings",
            0,
            json!({"kind":"setFps","fps":30}),
        ),
    );
    assert_eq!(r["ok"], false);
    assert_eq!(r["error"]["code"], "unavailable");
    assert_eq!(r["contentRevision"], 0);
}
#[test]
fn public_fps_command_clamps_without_changing_storage_work_area_semantics() {
    let state = ApplicationMcp::default();
    let receipt = install(&state, true);
    for (i, input, wanted) in [(0, 0.5, 1.0), (1, 100.0, 60.0)] {
        assert_eq!(
            dispatch(
                &state,
                request(
                    &receipt,
                    &format!("fps-clamp-{i}"),
                    i,
                    json!({"kind":"setFps","fps":input})
                )
            )["ok"],
            true
        );
        assert_eq!(
            saved(&state, &receipt)["project"]["fps"].as_f64(),
            Some(wanted)
        );
    }
}

/// Actual authenticated native wire and external revision barrier, following
/// the accepted Add fixture at812812a7. No owner/dispatch/ACK substitute.
async fn external_settings_round_trip(
    state: &ApplicationMcp,
    request: Value,
    binding: Option<&Value>,
) -> (nemo_mcp::contract::NativeApplicationResponse, Vec<Value>) {
    use crate::application_mcp::revision_sync::RevisionControl;
    use nemo_mcp::{registry::Endpoint, wire};
    use std::time::Duration;
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-settings-test".into(),
        build_id: "settings-wire-test".into(),
    };
    let mut acknowledged = Vec::new();
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let native = match envelope {
            wire::AuthenticatedWireRequest::Native(message) => {
                assert_eq!(message.secret, endpoint.secret);
                message.native_request
            }
            _ => panic!("settings command must use the existing authenticated native envelope"),
        };
        state
            .serve_external_for_test(&mut stream, native, |event| {
                let binding =
                    binding.expect("subscriber-absent refusal/replay/no-op must not emit an event");
                let observed = serde_json::to_value(&event).unwrap();
                assert_eq!(observed["instanceId"], request["instanceId"]);
                assert_eq!(observed["documentId"], request["documentId"]);
                assert_eq!(observed["requestId"], request["requestId"]);
                assert_eq!(
                    observed["lifecycleGeneration"],
                    binding["lifecycleGeneration"]
                );
                assert_eq!(observed["fromRevision"], request["expectedRevision"]);
                assert_eq!(
                    observed["toRevision"],
                    request["expectedRevision"].as_u64().unwrap() + 1
                );
                // The actual UI metadata reader observes the committed head BEFORE
                // releasing this exact live event; no invented/stale presentation ACK.
                let info = state
                    .native_project_info(observed["documentId"].as_str().unwrap())
                    .unwrap();
                assert_eq!(info["contentRevision"], observed["toRevision"]);
                assert_eq!(info["fps"].as_f64(), Some(29.97));
                assert_eq!(info["canvasW"], 320);
                assert_eq!(info["canvasH"], 180);
                state.revisions.control(
                    &state.native,
                    RevisionControl::Acknowledge {
                        subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
                        event,
                    },
                )?;
                acknowledged.push(json!({"event":observed,"metadata":info}));
                Ok(())
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(
        &endpoint,
        serde_json::from_value(request.clone()).unwrap(),
        wire::CancellationToken::new(),
    );
    let (_, response) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    (response.unwrap(), acknowledged)
}
#[tokio::test]
async fn external_mcp_settings_requires_real_subscriber_then_reads_and_acks_exact_live_revision() {
    use crate::application_mcp::revision_sync::RevisionControl;
    let state = ApplicationMcp::default();
    let receipt = install(&state, true);
    let before = saved(&state, &receipt);
    let id = receipt["documentId"].as_str().unwrap();
    let command = request(
        &receipt,
        "external-settings",
        0,
        json!({"kind":"setFps","fps":29.97}),
    );
    // Negative first: a valid public wire command must not write without a UI consumer.
    let (refused, events) = external_settings_round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
    let error = refused.error.unwrap();
    assert_eq!(error.code, "unavailable");
    assert_eq!(error.details.unwrap()["committed"], false);
    assert_eq!(refused.content_revision, 0);
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), before);
    let binding = state
        .revisions
        .control(&state.native, RevisionControl::Binding)
        .unwrap();
    assert_eq!(binding["instanceId"], receipt["instanceId"]);
    assert_eq!(binding["documentId"], receipt["documentId"]);
    assert_eq!(binding["contentRevision"], 0);
    state
        .revisions
        .control(
            &state.native,
            serde_json::from_value(json!({"action":"subscribe","binding":binding})).unwrap(),
        )
        .unwrap();
    let (changed, events) =
        external_settings_round_trip(&state, command.clone(), Some(&binding)).await;
    assert!(changed.ok, "{changed:?}");
    assert_eq!(changed.content_revision, 1);
    assert_eq!(
        changed.result.as_ref().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["metadata"]["fps"].as_f64(), Some(29.97));
    let after = saved(&state, &receipt);
    assert_eq!(after["drawing"], before["drawing"]);
    assert_eq!(after["project"]["fps"].as_f64(), Some(29.97));
    for field in ["canvasBg", "waIn", "waOut"] {
        assert!(after["project"].get(field).is_none());
    }
    let (replay, events) = external_settings_round_trip(&state, command, None).await;
    assert_eq!(replay, changed);
    assert!(
        events.is_empty(),
        "retained retry cannot re-emit a revision"
    );
    let (noop, events) = external_settings_round_trip(
        &state,
        request(
            &receipt,
            "external-settings-noop",
            1,
            json!({"kind":"setFps","fps":29.97}),
        ),
        None,
    )
    .await;
    assert!(noop.ok);
    assert_eq!(noop.content_revision, 1);
    assert_eq!(
        noop.result.unwrap(),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert!(events.is_empty());
    let info = state.native_project_info(id).unwrap();
    assert_eq!(info["undoDepth"], 1);
    assert_eq!(info["redoDepth"], 0);
    assert_eq!(saved(&state, &receipt), after);
    state
        .revisions
        .control(
            &state.native,
            RevisionControl::Disconnect {
                subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
            },
        )
        .unwrap();
}
