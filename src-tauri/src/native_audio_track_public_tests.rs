//! Actual shared host and authenticated MCP Audio metadata producer controls.
//! Source-level tests only; no ordinary editor or installed acceptance.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};
#[path = "native_project_placement_audio_fixture.rs"]
mod fixture;
use fixture::{audio_wire, bootstrap_audio_wire};
const OP: &str = "command.document.object.audio.track.edit";
fn request(receipt: &Value, id: &str, revision: u64, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
        "expectedRevision":revision,"operation":OP,"payload":payload})
}
fn dispatch(state: &ApplicationMcp, r: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(r).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn saved(state: &ApplicationMcp, receipt: &Value) -> Value {
    let id = receipt["documentId"].as_str().unwrap();
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
fn undo(state: &ApplicationMcp, receipt: &Value, revision: u64) {
    let mut r = request(
        receipt,
        "audio-edit-undo",
        revision,
        json!({"command":"object.undo"}),
    );
    r["operation"] = json!("command.document.object.undo");
    assert_eq!(dispatch(state, r)["ok"], true);
}
#[test]
fn audio_metadata_public_host_reaches_actual_record_save_reopen_undo_and_retained_replay() {
    let state = ApplicationMcp::default();
    let receipt = bootstrap_audio_wire(&state, &audio_wire(true), "edit-install");
    let initial = saved(&state, &receipt);
    let r = request(
        &receipt,
        "mute",
        0,
        json!({"kind":"setMuted","audioId":"track-a","muted":true}),
    );
    let first = dispatch(&state, r.clone());
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let mut expected = initial.clone();
    expected["drawing"]["audioTracks"][0]["muted"] = json!(true);
    let changed = saved(&state, &receipt);
    assert_eq!(changed, expected);
    assert_eq!(dispatch(&state, r.clone()), first);
    let reopened = ApplicationMcp::default();
    let pin = bootstrap_audio_wire(&reopened, &changed, "edit-reopen");
    assert_eq!(saved(&reopened, &pin), changed);
    undo(&state, &receipt, 1);
    assert_eq!(saved(&state, &receipt), initial);
    assert_eq!(dispatch(&state, r), first);
    assert_eq!(saved(&state, &receipt), initial);
    let noop = request(
        &receipt,
        "mute-noop",
        2,
        json!({"kind":"setMuted","audioId":"track-a","muted":false}),
    );
    assert_eq!(
        dispatch(&state, noop)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(saved(&state, &receipt), initial);
    assert_eq!(
        state
            .native_project_info(receipt["documentId"].as_str().unwrap())
            .unwrap()["redoDepth"],
        1
    );
}
#[test]
fn audio_public_host_refuses_duplicate_bytes_and_foreign_target_without_partial_publication() {
    let state = ApplicationMcp::default();
    let receipt = bootstrap_audio_wire(&state, &audio_wire(true), "edit-refusals");
    let before = saved(&state, &receipt);
    let r = request(
        &receipt,
        "raw-duplicate",
        0,
        json!({"kind":"setMuted","audioId":"track-a","muted":true}),
    );
    let raw = r.to_string();
    let duplicate = raw.replace("\"muted\":true", "\"muted\":true,\"muted\":false");
    assert_ne!(duplicate, raw);
    assert!(state.dispatch_object_request_json(&duplicate).is_err());
    assert_eq!(saved(&state, &receipt), before);
    let missing = request(
        &receipt,
        "missing-audio",
        0,
        json!({"kind":"rename","audioId":"missing","name":"x"}),
    );
    let response = dispatch(&state, missing.clone());
    assert_eq!(response["error"]["code"], "not_found");
    assert_eq!(dispatch(&state, missing), response);
    assert_eq!(saved(&state, &receipt), before);
    let good = state.dispatch_object_request_json(&raw).unwrap();
    assert!(good.ok);
    assert_eq!(state.dispatch_object_request_json(&raw).unwrap(), good);
}
async fn external_audio_round_trip(
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
        secret: "isolated-audio-edit-test".into(),
        build_id: "audio-edit-wire-test".into(),
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
            _ => panic!("Audio metadata edit must use the existing authenticated native envelope"),
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
                let actual = saved(state, &request);
                assert_eq!(actual["drawing"]["audioTracks"][0]["muted"], true);
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
async fn external_audio_metadata_edit_requires_subscriber_and_acks_the_actual_revision() {
    use crate::application_mcp::revision_sync::RevisionControl;
    let state = ApplicationMcp::default();
    let receipt = bootstrap_audio_wire(&state, &audio_wire(true), "wire-audio");
    let before = saved(&state, &receipt);
    let r = request(
        &receipt,
        "external-audio",
        0,
        json!({"kind":"setMuted","audioId":"track-a","muted":true}),
    );
    let (refused, events) = external_audio_round_trip(&state, r.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), before);
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
    let (first, events) = external_audio_round_trip(&state, r.clone(), Some(&binding)).await;
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, 1);
    assert_eq!(events.len(), 1);
    assert_eq!(
        first.result.as_ref().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1})
    );
    let mut expected = before.clone();
    expected["drawing"]["audioTracks"][0]["muted"] = json!(true);
    assert_eq!(saved(&state, &receipt), expected);
    let (replay, events) = external_audio_round_trip(&state, r.clone(), None).await;
    assert_eq!(replay, first);
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), expected);
    undo(&state, &receipt, 1);
    let (replay, events) = external_audio_round_trip(&state, r, None).await;
    assert_eq!(replay, first);
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), before);
}
