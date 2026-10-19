//! Actual shared host and authenticated wire placement producer; not packaged desktop acceptance.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};
const OP: &str = "command.document.object.project.placement.edit";
fn install(state: &ApplicationMcp) -> Value {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},
        "viewportReference":null,"assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}}],
        "assetInventory":{"assetIds":["v1","a1"],"placements":[{
            "id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":5}]}});
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"placement-install",
        "instanceId":state.instance_id(),"documentJson":wire.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(receipt: &Value, id: &str, revision: u64, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
        "expectedRevision":revision,"operation":OP,"payload":payload})
}
fn shift(delta: i64) -> Value {
    json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":delta})
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
        "placement-undo",
        revision,
        json!({"command":"object.undo"}),
    );
    r["operation"] = json!("command.document.object.undo");
    assert_eq!(dispatch(state, r)["ok"], true);
}
#[test]
fn public_placement_dispatch_reaches_actual_owner_and_replays_after_undo_without_reexecution() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let initial = saved(&state, &receipt);
    let req = request(&receipt, "public-shift", 0, shift(-7));
    let first = dispatch(&state, req.clone());
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(first["contentRevision"], 1);
    for field in ["requestId", "instanceId", "documentId"] {
        assert_eq!(first[field], req[field]);
    }
    let changed = saved(&state, &receipt);
    assert_eq!(
        changed["assetInventory"]["placements"][0]["offsetFrames"],
        -2
    );
    for key in ["assets", "drawing", "project", "viewportReference"] {
        assert_eq!(changed[key], initial[key]);
    }
    assert_eq!(dispatch(&state, req.clone()), first);
    undo(&state, &receipt, 1);
    assert_eq!(saved(&state, &receipt), initial);
    assert_eq!(dispatch(&state, req), first);
    assert_eq!(saved(&state, &receipt), initial);
    let noop = request(&receipt, "public-noop", 2, shift(0));
    assert_eq!(
        dispatch(&state, noop)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(info["contentRevision"], 2);
    assert_eq!(info["undoDepth"], 0);
    assert_eq!(info["redoDepth"], 1);
}
#[test]
fn public_placement_cancel_stale_foreign_document_and_audio_target_refuse_without_partial_state() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let before = saved(&state, &receipt);
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
            json!("replaced-document"),
            "wrong_document",
        ),
    ] {
        let mut r = request(&receipt, id, 0, shift(1));
        r[field] = value;
        let response = dispatch(&state, r.clone());
        assert_eq!(response["error"]["code"], code);
        assert_eq!(dispatch(&state, r), response);
        assert_eq!(saved(&state, &receipt), before);
    }
    let audio = request(
        &receipt,
        "audio-unavailable",
        0,
        json!({"kind":"place","placement":{
        "id":"pa","assetId":"a1","target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":0}}),
    );
    let first = dispatch(&state, audio.clone());
    assert_eq!(first["error"]["code"], "unavailable");
    assert_eq!(dispatch(&state, audio), first);
    assert_eq!(saved(&state, &receipt), before);
}
#[test]
fn original_byte_object_client_admits_route_but_refuses_nested_duplicates_before_dispatch() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let req = request(&receipt, "raw-placement", 0, shift(1));
    let raw = req.to_string();
    let duplicate = raw.replace("\"deltaFrames\":1", "\"deltaFrames\":1,\"deltaFrames\":2");
    assert_ne!(raw, duplicate);
    let before = saved(&state, &receipt);
    assert!(state.dispatch_object_request_json(&duplicate).is_err());
    assert_eq!(saved(&state, &receipt), before);
    let first = state.dispatch_object_request_json(&raw).unwrap();
    assert!(first.ok);
    assert_eq!(state.dispatch_object_request_json(&raw).unwrap(), first);
    assert_eq!(
        saved(&state, &receipt)["assetInventory"]["placements"][0]["offsetFrames"],
        6
    );
}

// Existing authenticated native wire and real revision subscription/ACK, not a mock owner.
async fn external_placement_round_trip(
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
        secret: "isolated-placement-test".into(),
        build_id: "placement-wire-test".into(),
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
            _ => panic!("placement command must use the existing authenticated native envelope"),
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
                assert_eq!(
                    actual["assetInventory"]["placements"][0]["offsetFrames"],
                    -2
                );
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
async fn external_placement_wire_requires_subscriber_then_observes_and_acks_real_revision() {
    use crate::application_mcp::revision_sync::RevisionControl;
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let before = saved(&state, &receipt);
    let req = request(&receipt, "external-placement", 0, shift(-7));
    let (refused, events) = external_placement_round_trip(&state, req.clone(), None).await;
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
    let (first, events) = external_placement_round_trip(&state, req.clone(), Some(&binding)).await;
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, 1);
    assert_eq!(
        first.result.as_ref().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(events.len(), 1);
    let changed = saved(&state, &receipt);
    assert_eq!(
        changed["assetInventory"]["placements"][0]["offsetFrames"],
        -2
    );
    let (replay, events) = external_placement_round_trip(&state, req.clone(), None).await;
    assert_eq!(replay, first);
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), changed);
    undo(&state, &receipt, 1);
    let (replay, events) = external_placement_round_trip(&state, req, None).await;
    assert_eq!(replay, first);
    assert!(events.is_empty());
    assert_eq!(saved(&state, &receipt), before);
}

// Audio source/record/placement joins below use the actual shared host. They do
// not establish playback, ordinary editor integration or installed acceptance.
#[path = "native_project_placement_audio_fixture.rs"]
mod audio_fixture;
use audio_fixture::{audio_offset, audio_wire, bootstrap_audio_wire, AUDIO_WAV};

#[test]
fn public_audio_bootstrap_persists_real_wave_and_authored_record_through_actual_reopen() {
    use base64::Engine;
    let state = ApplicationMcp::default();
    let original = audio_wire(true);
    let receipt = bootstrap_audio_wire(&state, &original, "audio-install");
    assert_eq!(receipt["contentRevision"], 0);
    let file = saved(&state, &receipt);
    assert_eq!(file, original);
    let source = &file["assets"][0]["source"];
    assert_eq!(source["mediaType"], "audio/wav");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(source["data"].as_str().unwrap())
            .unwrap(),
        AUDIO_WAV
    );
    let track = &file["drawing"]["audioTracks"][0];
    assert_eq!(track["audioId"], "track-a");
    assert_eq!(track["trimEnd"], Value::Null);
    assert!(track.get("dataB64").is_none());
    assert!(track.get("offsetFrames").is_none());
    assert_eq!(audio_offset(&file), -1.25);
    let reopened = ApplicationMcp::default();
    let new_receipt = bootstrap_audio_wire(&reopened, &file, "audio-reopen");
    assert_eq!(new_receipt["contentRevision"], 0);
    assert_eq!(saved(&reopened, &new_receipt), file);
}
#[test]
fn public_audio_place_fractional_shift_set_and_replay_use_the_actual_history_owner() {
    let state = ApplicationMcp::default();
    let receipt = bootstrap_audio_wire(&state, &audio_wire(false), "audio-unplaced");
    let before = saved(&state, &receipt);
    let place = request(
        &receipt,
        "audio-place",
        0,
        json!({"kind":"place","placement":{
        "id":"audio-p","assetId":"wave-a","target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":-1.25}}),
    );
    let first = dispatch(&state, place.clone());
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(dispatch(&state, place), first);
    let shift = request(
        &receipt,
        "audio-shift",
        1,
        json!({"kind":"shiftOffset","placementId":"audio-p","deltaFrames":0.625}),
    );
    let shifted = dispatch(&state, shift.clone());
    assert_eq!(shifted["ok"], true);
    assert_eq!(shifted["contentRevision"], 2);
    let after_shift = saved(&state, &receipt);
    assert_eq!(audio_offset(&after_shift), -0.625);
    assert_eq!(dispatch(&state, shift.clone()), shifted);
    undo(&state, &receipt, 2);
    let undone = saved(&state, &receipt);
    assert_eq!(audio_offset(&undone), -1.25);
    assert_eq!(dispatch(&state, shift), shifted);
    assert_eq!(saved(&state, &receipt), undone);
    let noop = request(
        &receipt,
        "audio-zero",
        3,
        json!({"kind":"shiftOffset","placementId":"audio-p","deltaFrames":0.0}),
    );
    let no_change = dispatch(&state, noop.clone());
    assert_eq!(no_change["ok"], true);
    assert_eq!(
        no_change["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(no_change["contentRevision"], 3);
    assert_eq!(dispatch(&state, noop), no_change);
    assert_eq!(saved(&state, &receipt), undone);
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(info["redoDepth"], 1);
    let set = request(
        &receipt,
        "audio-set",
        3,
        json!({"kind":"setOffset","placementId":"audio-p","offsetFrames":-3.875}),
    );
    let response = dispatch(&state, set);
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], 4);
    let changed = saved(&state, &receipt);
    assert_eq!(audio_offset(&changed), -3.875);
    for field in ["drawing", "assets", "project", "viewportReference"] {
        assert_eq!(changed[field], before[field]);
    }
    let reopened = ApplicationMcp::default();
    let reopened_receipt = bootstrap_audio_wire(&reopened, &changed, "audio-edited-reopen");
    assert_eq!(saved(&reopened, &reopened_receipt), changed);
}
#[test]
fn public_audio_bad_target_scalar_cancel_stale_and_foreign_identity_leave_saved_state_unchanged() {
    let state = ApplicationMcp::default();
    let receipt = bootstrap_audio_wire(&state, &audio_wire(true), "audio-refusal-install");
    let before = saved(&state, &receipt);
    for (index, payload, code) in [
        (
            0,
            json!({"kind":"place","placement":{"id":"missing-track","assetId":"wave-a",
            "target":{"kind":"audioTrack","trackUid":"absent"},"offsetFrames":0.5}}),
            "not_found",
        ),
        (
            1,
            json!({"kind":"setOffset","placementId":"audio-p","offsetFrames":null}),
            "invalid_request",
        ),
        (
            2,
            json!({"kind":"shiftOffset","placementId":"audio-p","deltaFrames":9007199254740992_u64}),
            "invalid_request",
        ),
    ] {
        let req = request(&receipt, &format!("audio-refused-{index}"), 0, payload);
        let first = dispatch(&state, req.clone());
        assert_eq!(first["error"]["code"], code);
        assert_eq!(dispatch(&state, req), first);
        assert_eq!(saved(&state, &receipt), before);
    }
    for (field, value, code) in [
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("expectedRevision", json!(1), "stale_revision"),
        ("documentId", json!("foreign-audio"), "wrong_document"),
    ] {
        let mut req = request(
            &receipt,
            field,
            0,
            json!({"kind":"shiftOffset","placementId":"audio-p","deltaFrames":0.5}),
        );
        req[field] = value;
        let response = dispatch(&state, req.clone());
        assert_eq!(response["error"]["code"], code);
        assert_eq!(dispatch(&state, req), response);
        assert_eq!(saved(&state, &receipt), before);
    }
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(info["contentRevision"], 0);
    assert_eq!(info["undoDepth"], 0);
}
#[test]
fn public_audio_bootstrap_refuses_mime_only_wave_before_install_and_allows_a_valid_successor() {
    let state = ApplicationMcp::default();
    let mut invalid = audio_wire(true);
    // Valid base64 and advertised MIME, but bytes are not a RIFF/WAVE container.
    invalid["assets"][0]["source"]["data"] = json!("AAAA");
    assert!(bootstrap_raw(
        &state,
        &json!({"apiVersion":2,"requestId":"bad-wave",
        "instanceId":state.instance_id(),"documentJson":invalid.to_string()})
        .to_string()
    )
    .is_err());
    let valid = audio_wire(true);
    let receipt = bootstrap_audio_wire(&state, &valid, "valid-after-bad-wave");
    assert_eq!(receipt["contentRevision"], 0);
    assert_eq!(saved(&state, &receipt), valid);
}
