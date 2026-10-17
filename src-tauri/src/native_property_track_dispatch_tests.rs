//! D12 real native owner/history and authenticated wire/ACK; not installed proof.
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const OP: &str = "command.document.object.property.track.edit";
fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0","locked":true}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{
        "property":"opacity","static":[17],"track":{"keys":[
        {"frame":0,"v":[0],"hold":false,"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[3,4],"hOut":[5,6]},
        {"frame":4,"v":[80],"curvePoints":[]},{"frame":12,"v":[0],"curvePoints":[]}]}}]}]}});
    open(state, document.to_string())
}
fn open(state: &ApplicationMcp, document: String) -> String {
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"track-install",
        "instanceId":state.instance_id(),"documentJson":document})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn request(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    action: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":OP,"payload":{"command":"object.property.track.edit",
        "selections":[{"target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","frame":0}],"action":action}})).unwrap()
}
fn preset(name: &str) -> Value {
    json!({"kind":"preset","preset":name})
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
fn dispatch(
    state: &ApplicationMcp,
    request: NativeApplicationRequest,
) -> NativeApplicationResponse {
    state.dispatch_native(request).unwrap()
}
fn history(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    redo: bool,
) -> NativeApplicationResponse {
    // Public history remains undeclared: use only the existing bounded desktop route.
    crate::native_object_client::dispatch_raw(
        state,
        &json!({"apiVersion":2,"requestId":rid,
        "instanceId":state.instance_id(),"documentId":id,"expectedRevision":revision,
        "operation":if redo {"command.document.object.redo"} else {"command.document.object.undo"},
        "payload":{"command":if redo {"object.redo"} else {"object.undo"}}})
        .to_string(),
    )
    .unwrap()
}
fn receipt(response: &NativeApplicationResponse, revision: u64, applied: bool) {
    assert!(response.ok, "{response:?}");
    assert_eq!(response.content_revision, revision);
    assert_eq!(
        response.result,
        Some(json!({"applied":applied,"historyEntriesAdded":u64::from(applied)}))
    );
}
#[test]
fn track_public_dispatch_preserves_unrelated_source_replay_noop_redo_and_reopen() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let command = request(&state, &id, "ease", 0, preset("ease"));
    let first = dispatch(&state, command.clone());
    receipt(&first, 1, true);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    let changed = saved(&state, &id);
    assert_ne!(changed, initial);
    let mut projected: Value = serde_json::from_slice(&changed).unwrap();
    let old: Value = serde_json::from_slice(&initial).unwrap();
    let points = &projected["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"][0]
        ["curvePoints"];
    let coordinates: Vec<_> = points
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()))
        .collect();
    assert_eq!(
        coordinates,
        vec![(0., 0.), (0.25, 0.09), (0.5, 0.5), (0.75, 0.91), (1., 1.)]
    );
    projected["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"][0]
        ["curvePoints"] = old["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"][0]
        ["curvePoints"]
        .clone();
    assert_eq!(
        projected, old,
        "only the selected curve changes; handles, static, locked layer and envelope stay authored"
    );
    assert_eq!(dispatch(&state, command.clone()), first);
    receipt(
        &dispatch(&state, request(&state, &id, "ease-noop", 1, preset("ease"))),
        1,
        false,
    );
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "linear-noop", 2, preset("linear")),
        ),
        2,
        false,
    );
    assert_eq!(saved(&state, &id), initial);
    let redo = history(&state, &id, "redo", 2, true);
    assert!(redo.ok);
    assert_eq!(redo.content_revision, 3);
    assert_eq!(
        saved(&state, &id),
        changed,
        "no-op did not destroy native redo"
    );
    assert_eq!(
        dispatch(&state, command),
        first,
        "retained original receipt is not rewritten to current revision"
    );
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), changed);
}
#[test]
fn track_shape_valid_native_refusals_are_atomic_and_do_not_fence_a_later_valid_edit() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let source = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let mut cases = Vec::new();
    let mut stale = request(&state, &id, "stale", 1, preset("ease"));
    cases.push((stale.clone(), "stale_revision"));
    stale.request_id = "cancel".into();
    stale.expected_revision = Some(0);
    stale.cancelled_before_dispatch = true;
    cases.push((stale, "cancelled_before_dispatch"));
    for (rid, key, code) in [
        ("instance", "instanceId", "wrong_instance"),
        ("document", "documentId", "wrong_document"),
    ] {
        let r = request(&state, &id, rid, 0, preset("ease"));
        let mut value = serde_json::to_value(r).unwrap();
        value[key] = json!("foreign");
        cases.push((serde_json::from_value(value).unwrap(), code));
    }
    let mut duplicate = request(&state, &id, "duplicate-key", 0, preset("hold"));
    let mut equal = duplicate.payload["selections"][0].clone();
    equal["frame"] = json!(0.0);
    duplicate.payload["selections"]
        .as_array_mut()
        .unwrap()
        .push(equal);
    cases.push((duplicate, "invalid_request"));
    let mut missing = request(&state, &id, "atomic-missing-key", 0, preset("ease"));
    let mut bad = missing.payload["selections"][0].clone();
    bad["frame"] = json!(3.25);
    missing.payload["selections"]
        .as_array_mut()
        .unwrap()
        .push(bad);
    cases.push((missing, "not_found"));
    for property in ["order", "timeRemap"] {
        let mut r = request(&state, &id, property, 0, preset("ease"));
        r.payload["selections"][0]["property"] = json!(property);
        cases.push((r, "unavailable"));
    }
    let mut missing = request(&state, &id, "missing-holder", 0, preset("ease"));
    missing.payload["selections"][0]["target"]["layerUid"] = json!("absent");
    cases.push((missing, "not_found"));
    for (r, code) in cases {
        r.validate().unwrap();
        let response = dispatch(&state, r);
        assert!(!response.ok);
        assert_eq!(response.content_revision, 0);
        assert_eq!(response.error.unwrap().code, code);
        assert_eq!(saved(&state, &id), source);
        assert_eq!(state.native_project_info(&id).unwrap(), info);
    }
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "valid-after-refusal", 0, preset("hold")),
        ),
        1,
        true,
    );
    assert!(history(&state, &id, "undo-after-refusal", 1, false).ok);
    assert_eq!(saved(&state, &id), source);
}
#[test]
fn track_public_influence_clamps_only_in_native_and_subdivision_uses_original_samples() {
    for (outside, edge) in [(-1000.5, 0.0), (1000.5, 100.0)] {
        let mut outputs = Vec::new();
        for percent in [outside, edge] {
            let state = ApplicationMcp::default();
            let id = install(&state);
            let r = request(
                &state,
                &id,
                "influence",
                0,
                json!({"kind":"influence","side":"out","percent":percent}),
            );
            r.validate().unwrap();
            receipt(&dispatch(&state, r), 1, true);
            outputs.push(saved(&state, &id));
        }
        assert_eq!(outputs[0], outputs[1]);
    }
    let state = ApplicationMcp::default();
    let id = install(&state);
    let mut r = request(&state, &id, "subdivide", 0, json!({"kind":"subdivide"}));
    let mut second = r.payload["selections"][0].clone();
    second["frame"] = json!(4);
    r.payload["selections"].as_array_mut().unwrap().push(second);
    receipt(&dispatch(&state, r.clone()), 1, true);
    let after: Value = serde_json::from_slice(&saved(&state, &id)).unwrap();
    let keys = &after["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"];
    assert_eq!(keys.as_array().unwrap().len(), 4);
    assert_eq!(keys[1]["frame"].as_f64(), Some(2.0));
    assert_eq!(keys[1]["v"][0].as_f64(), Some(40.0));
    assert_eq!(keys[1]["hIn"], json!([0, 0]));
    assert_eq!(keys[1]["hOut"], json!([0, 0]));
    assert!(keys[1].get("hold").is_none());
    r.request_id = "subdivide-existing-midpoint".into();
    r.expected_revision = Some(1);
    receipt(&dispatch(&state, r), 1, false);
}
/// Drives the actual external host route and its retained ACK receiver over TCP.
async fn round_trip(
    state: &ApplicationMcp,
    original: NativeApplicationRequest,
    binding: Option<&Value>,
) -> (NativeApplicationResponse, usize) {
    use nemo_mcp::{registry::Endpoint, wire};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-track-wire".into(),
        build_id: "track-public-test".into(),
    };
    let mut acknowledgments = 0;
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let wire::AuthenticatedWireRequest::Native(message) = envelope else {
            panic!("non-native envelope")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(
            serde_json::to_value(&message.native_request).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        state
            .serve_external_for_test(&mut stream, message.native_request, |event| {
                let binding = binding.expect("unsubscribed/noop/replay commands must not publish");
                let observed = serde_json::to_value(&event).unwrap();
                assert_eq!(observed["instanceId"], original.instance_id);
                assert_eq!(observed["documentId"], original.document_id);
                assert_eq!(observed["requestId"], original.request_id);
                assert_eq!(
                    observed["lifecycleGeneration"],
                    binding["lifecycleGeneration"]
                );
                assert_eq!(
                    observed["fromRevision"],
                    original.expected_revision.unwrap()
                );
                assert_eq!(
                    observed["toRevision"],
                    original.expected_revision.unwrap() + 1
                );
                assert_eq!(
                    state.native_project_info(&original.document_id).unwrap()["contentRevision"],
                    observed["toRevision"]
                );
                let mut wrong = observed;
                wrong["requestId"] = json!("wrong-ack");
                assert!(state
                    .revisions
                    .control(
                        &state.native,
                        RevisionControl::Acknowledge {
                            subscription_id: binding["subscriptionId"].as_str().unwrap().into(),
                            event: serde_json::from_value(wrong).unwrap(),
                        }
                    )
                    .is_err());
                let blocked = dispatch(
                    state,
                    request(
                        state,
                        &original.document_id,
                        "pending-write",
                        original.expected_revision.unwrap() + 1,
                        preset("hold"),
                    ),
                );
                assert!(!blocked.ok);
                assert_eq!(blocked.error.unwrap().code, "unavailable");
                state.revisions.control(
                    &state.native,
                    RevisionControl::Acknowledge {
                        subscription_id: binding["subscriptionId"].as_str().unwrap().into(),
                        event,
                    },
                )?;
                acknowledgments += 1;
                Ok(())
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
    (response.unwrap(), acknowledgments)
}
#[tokio::test]
async fn track_public_wire_requires_live_subscriber_and_acks_exactly_once_with_real_history() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let command = request(&state, &id, "wire-ease", 0, preset("ease"));
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
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
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let (retry, events) = round_trip(&state, command.clone(), None).await;
    assert_eq!(retry, first);
    assert_eq!(events, 0);
    let (noop, events) = round_trip(
        &state,
        request(&state, &id, "wire-noop", 1, preset("ease")),
        None,
    )
    .await;
    receipt(&noop, 1, false);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "wire-bounded-undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    let (noop, events) = round_trip(
        &state,
        request(&state, &id, "wire-linear-noop", 2, preset("linear")),
        None,
    )
    .await;
    receipt(&noop, 2, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "wire-bounded-redo", 2, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let current = saved(&state, &id);
    let mut duplicate = request(&state, &id, "wire-duplicate", 3, preset("hold"));
    let mut equal = duplicate.payload["selections"][0].clone();
    equal["frame"] = json!(0.0);
    duplicate.payload["selections"]
        .as_array_mut()
        .unwrap()
        .push(equal);
    let mut cancelled = request(&state, &id, "wire-cancel", 3, preset("hold"));
    cancelled.cancelled_before_dispatch = true;
    for (r, code) in [
        (duplicate, "invalid_request"),
        (
            request(&state, &id, "wire-stale", 0, preset("hold")),
            "stale_revision",
        ),
        (cancelled, "cancelled_before_dispatch"),
    ] {
        let (refused, events) = round_trip(&state, r, None).await;
        assert!(!refused.ok);
        assert_eq!(refused.error.unwrap().code, code);
        assert_eq!(refused.content_revision, 3);
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), current);
    }
    let (retry, events) = round_trip(&state, command, None).await;
    assert_eq!(retry, first);
    assert_eq!(events, 0);
    assert_eq!(
        state.native_project_info(&id).unwrap()["contentRevision"],
        3
    );
}
