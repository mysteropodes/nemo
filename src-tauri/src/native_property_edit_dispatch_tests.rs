//! D36 real native owner/history and authenticated wire/ACK; not installed proof.
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const OP: &str = "command.document.object.property.edit";
fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0","locked":true}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{
        "property":"opacity","static":[17]}]}]}});
    open(state, document.to_string())
}
fn open(state: &ApplicationMcp, document: String) -> String {
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"property-edit-install",
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
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":OP,"payload":{"command":"object.property.edit",
        "target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","action":action}}),
    )
    .unwrap()
}
fn value(number: i32) -> Value {
    json!({"kind":"setValue","frame":-0.5,"values":[number]})
}
fn animation(frame: f64, enabled: bool) -> Value {
    json!({"kind":"setAnimated","frame":frame,"animated":enabled})
}
fn property(state: &ApplicationMcp, id: &str) -> Value {
    let document: Value = serde_json::from_slice(&saved(state, id)).unwrap();
    document["drawing"]["propertyHolders"][0]["properties"][0].clone()
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
    crate::application_mcp::object_client::dispatch_raw(
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
fn property_edit_public_static_history_replay_noop_redo_and_reopen_use_native_owner() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let command = request(&state, &id, "set", 0, value(25));
    command.validate().unwrap();
    let first = dispatch(&state, command.clone());
    receipt(&first, 1, true);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    let changed = saved(&state, &id);
    let mut projected: Value = serde_json::from_slice(&changed).unwrap();
    let old: Value = serde_json::from_slice(&initial).unwrap();
    assert_eq!(
        property(&state, &id),
        json!({"property":"opacity","static":[25]})
    );
    projected["drawing"]["propertyHolders"][0]["properties"][0]["static"] = json!([17]);
    assert_eq!(
        projected, old,
        "only static property value changes, including locked layer preservation"
    );
    assert_eq!(dispatch(&state, command.clone()), first);
    let mut changed_body = command.clone();
    changed_body.payload["action"] = value(26);
    let refused = dispatch(&state, changed_body);
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "invalid_request");
    assert_eq!(saved(&state, &id), changed);
    receipt(
        &dispatch(&state, request(&state, &id, "same", 1, value(25))),
        1,
        false,
    );
    assert!(history(&state, &id, "undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "same-after-undo", 2, value(17)),
        ),
        2,
        false,
    );
    let redo = history(&state, &id, "redo", 2, true);
    assert!(redo.ok);
    assert_eq!(redo.content_revision, 3);
    assert_eq!(
        saved(&state, &id),
        changed,
        "semantic noop preserves native redo"
    );
    assert_eq!(
        dispatch(&state, command),
        first,
        "original receipt retains its original revision"
    );
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), changed);
}
#[test]
fn property_edit_public_key_and_animation_actions_preserve_authored_track_presence() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    // Already static: disabling animation is a no-op and must not author an empty track.
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "static-noop", 0, animation(-1.25, false)),
        ),
        0,
        false,
    );
    assert_eq!(
        property(&state, &id),
        json!({"property":"opacity","static":[17]})
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "key",
                0,
                json!({"kind":"setKey","frame":-1.25,"values":[20]}),
            ),
        ),
        1,
        true,
    );
    let keyed = property(&state, &id);
    let key = &keyed["track"]["keys"][0];
    assert_eq!(key["frame"].as_f64(), Some(-1.25));
    assert_eq!(key["v"], json!([20]));
    assert_eq!(key["hIn"], json!([0, 0]));
    assert_eq!(key["hOut"], json!([0, 0]));
    assert!(key.get("hold").is_none());
    assert_eq!(
        key["curvePoints"],
        json!([{ "x":0.0,"y":0.0},{"x":0.25,"y":0.156},{"x":0.5,"y":0.5},{"x":0.75,"y":0.844},{"x":1.0,"y":1.0}])
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "keyed-value",
                1,
                json!({"kind":"setValue","frame":-1.25,"values":[33]}),
            ),
        ),
        2,
        true,
    );
    let mut expected = keyed;
    expected["track"]["keys"][0]["v"] = json!([33]);
    assert_eq!(property(&state, &id), expected);
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "missing-key",
                2,
                json!({"kind":"removeKey","frame":1.5}),
            ),
        ),
        2,
        false,
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "remove-key",
                2,
                json!({"kind":"removeKey","frame":-1.25}),
            ),
        ),
        3,
        true,
    );
    assert_eq!(
        property(&state, &id),
        json!({"property":"opacity","static":[17],"track":{"keys":[]}})
    );
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "animate", 3, animation(2.5, true)),
        ),
        4,
        true,
    );
    let animated = property(&state, &id);
    let key = &animated["track"]["keys"][0];
    assert_eq!(key["frame"].as_f64(), Some(2.5));
    assert_eq!(key["v"][0].as_f64(), Some(17.0));
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "animation-noop", 4, animation(-8.25, true)),
        ),
        4,
        false,
    );
    assert_eq!(property(&state, &id), animated);
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "disable", 4, animation(2.5, false)),
        ),
        5,
        true,
    );
    let disabled = property(&state, &id);
    assert_eq!(disabled["track"]["keys"], json!([]));
    assert_eq!(disabled["static"][0].as_f64(), Some(17.0));
    assert!(history(&state, &id, "undo-disable", 5, false).ok);
    assert_eq!(property(&state, &id), animated);
}
#[test]
fn property_edit_public_refusals_are_atomic_and_do_not_block_a_later_valid_command() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let source = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let mut cases = Vec::new();
    cases.push((
        request(&state, &id, "stale", 1, value(25)),
        "stale_revision",
    ));
    let mut cancelled = request(&state, &id, "cancel", 0, value(25));
    cancelled.cancelled_before_dispatch = true;
    cases.push((cancelled, "cancelled_before_dispatch"));
    for (rid, key, code) in [
        ("instance", "instanceId", "wrong_instance"),
        ("document", "documentId", "wrong_document"),
    ] {
        let mut raw = serde_json::to_value(request(&state, &id, rid, 0, value(25))).unwrap();
        raw[key] = json!("foreign");
        cases.push((serde_json::from_value(raw).unwrap(), code));
    }
    for (rid, action) in [
        (
            "dimension",
            json!({"kind":"setValue","frame":0,"values":[1,2]}),
        ),
        (
            "key-dimension",
            json!({"kind":"setKey","frame":0,"values":[1,2]}),
        ),
    ] {
        cases.push((request(&state, &id, rid, 0, action), "invalid_request"));
    }
    for name in ["order", "timeRemap", "not-a-builtin"] {
        let mut r = request(&state, &id, name, 0, value(25));
        r.payload["property"] = json!(name);
        cases.push((r, "unavailable"));
    }
    let mut missing = request(&state, &id, "missing", 0, value(25));
    missing.payload["target"]["layerUid"] = json!("absent");
    cases.push((missing, "not_found"));
    for (r, code) in cases {
        r.validate().unwrap();
        let response = dispatch(&state, r);
        assert!(!response.ok, "{code}");
        assert_eq!(response.content_revision, 0);
        assert_eq!(response.error.unwrap().code, code);
        assert_eq!(saved(&state, &id), source);
        assert_eq!(state.native_project_info(&id).unwrap(), info);
    }
    let mut curve = request(
        &state,
        &id,
        "curve",
        0,
        json!({"kind":"setKey","frame":0,"values":[1],"curvePoints":[]}),
    );
    assert!(
        curve.validate().is_err(),
        "curve ownership remains on existing track route"
    );
    curve.payload["action"] = value(25);
    receipt(&dispatch(&state, curve), 1, true);
    assert!(history(&state, &id, "undo-valid", 1, false).ok);
    assert_eq!(saved(&state, &id), source);
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
        secret: "isolated-property-edit-wire".into(),
        build_id: "property-edit-public-test".into(),
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
                        value(33),
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
async fn property_edit_public_tcp_requires_live_subscription_and_correlated_ack_once() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let command = request(&state, &id, "wire-set", 0, value(25));
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
    assert_eq!(
        property(&state, &id),
        json!({"property":"opacity","static":[25]})
    );
    let (retry, events) = round_trip(&state, command, None).await;
    assert_eq!(retry, first);
    assert_eq!(events, 0);
    let (noop, events) = round_trip(
        &state,
        request(&state, &id, "wire-noop", 1, value(25)),
        None,
    )
    .await;
    receipt(&noop, 1, false);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    let (stale, events) = round_trip(
        &state,
        request(&state, &id, "wire-stale", 0, value(33)),
        None,
    )
    .await;
    assert!(!stale.ok);
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
}
