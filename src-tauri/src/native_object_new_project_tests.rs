//! Scalar New through the real bootstrap, authority, replacement and save ports.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{host_error, HostResult};
use crate::native_object_bootstrap::{
    admission, bootstrap_raw, reopen, replace, ObjectBootstrapReceipt,
};
use serde_json::{json, Value};
use std::cell::Cell;

fn request(state: &ApplicationMcp) -> Value {
    json!({"apiVersion":2,"requestId":"new-a","instanceId":state.instance_id(),
        "newProject":{"canvasW":64,"canvasH":48,"fps":24.0}})
}
fn initial(state: &ApplicationMcp) -> ObjectBootstrapReceipt {
    reopen::bootstrap_request(state, &request(state).to_string(), |_| {
        panic!("New must not consult filesystem admission")
    })
    .unwrap()
}
fn replacement(state: &ApplicationMcp, receipt: &ObjectBootstrapReceipt) -> Value {
    let info = state.native_project_info(&receipt.document_id).unwrap();
    let mut value = request(state);
    value["requestId"] = json!("new-b");
    value["newProject"] = json!({"canvasW":80,"canvasH":60,"fps":30.0});
    value["replace"] = json!({"documentId":receipt.document_id,
        "expectedRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],
        "lifecycleGeneration":info["lifecycleGeneration"]});
    value
}
fn prepare(state: &ApplicationMcp, value: &Value) -> HostResult<admission::Admission> {
    admission::prepare_blank(state, admission::new_request(&value.to_string()).unwrap())
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

#[test]
fn initial_new_uses_native_project_owner_and_roundtrips_without_file_intent() {
    let state = ApplicationMcp::default();
    let a = initial(&state);
    assert_eq!(a.api_version, 2);
    assert_eq!(a.request_id, "new-a");
    assert_eq!(a.instance_id, state.instance_id());
    assert_eq!(a.content_revision, 0);
    assert_eq!(a.resource_count, 0);
    assert!(!a.viewport_available);
    let info = state.native_project_info(&a.document_id).unwrap();
    for (field, expected) in [
        ("canvasW", 64),
        ("canvasH", 48),
        ("totalFrames", 120),
        ("contentRevision", 0),
        ("undoDepth", 0),
        ("redoDepth", 0),
    ] {
        assert_eq!(info[field], expected, "{field}");
    }
    assert_eq!(info["fps"], 24.0);
    let bytes = saved(&state, &a.document_id);
    let project: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(project["drawing"]["formatVersion"], 2);
    assert_eq!(
        project["drawing"]["layers"],
        json!([{"layerUid":"layer-1","name":"Layer 1","visible":true,
            "color":"#5FA875","locked":false,"parentLayerUid":null,
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}])
    );
    assert_eq!(project["drawing"]["objects"], json!([]));
    assert_eq!(
        project["drawing"]["compositionOrder"]["contexts"][0]["scopes"],
        json!([{"frameScope":{"kind":"authored","frame":0},"elements":[]}])
    );
    let reopened = ApplicationMcp::default();
    let (id, revision, count) = reopened.install_native_project_file(&bytes).unwrap();
    assert_ne!(id, a.document_id);
    assert_eq!((revision, count), (0, 0));
    assert_eq!(saved(&reopened, &id), bytes);
    let mut again = request(&state);
    again["requestId"] = json!("new-without-replace");
    assert!(bootstrap_raw(&state, &again.to_string()).is_err());
    assert_eq!(state.native_project_info(&a.document_id).unwrap(), info);
    assert_eq!(saved(&state, &a.document_id), bytes);
}

#[test]
fn original_byte_budget_and_named_objects_refuse_ambiguous_new_input() {
    let state = ApplicationMcp::default();
    let raw = request(&state).to_string();
    let mut padded = raw.clone();
    padded.push_str(&" ".repeat(4096 - raw.len()));
    assert!(admission::new_request(&padded).is_some());
    padded.push(' ');
    assert!(admission::new_request(&padded).is_none());
    assert!(bootstrap_raw(&state, &padded).is_err());
    assert!(state.native_state().lock().unwrap().active().is_none());
    padded.pop();
    bootstrap_raw(&state, &padded).unwrap();

    let state = ApplicationMcp::default();
    let raw = request(&state).to_string();
    let pins = json!({"documentId":"doc","expectedRevision":0,
        "documentSnapshotId":"snap","lifecycleGeneration":1});
    let sequence = json!([2,"new-a",state.instance_id(),
        {"canvasW":64,"canvasH":48,"fps":24.0},pins,false])
    .to_string();
    // The derived DTO accepts this complete sequence. The original-byte
    // wrapper guard must be what rejects it, not a malformed nested value.
    assert!(serde_json::from_str::<admission::NewRequest>(&sequence).is_ok());
    let mut named = request(&state);
    named["replace"] = pins;
    assert!(admission::new_request(&named.to_string()).is_some());
    let mut bad = vec![
        raw.replacen(
            "\"requestId\":",
            "\"requestId\":\"duplicate\",\"requestId\":",
            1,
        ),
        raw.replacen("\"newProject\":", "\"newProject\":{},\"newProject\":", 1),
        raw.replacen("\"canvasW\":", "\"canvas\\u0057\":64,\"canvasW\":", 1),
        sequence,
    ];
    for value in [
        json!([64, 48, 24.0]),
        Value::Null,
        json!({"canvasW":64,"canvasH":48}),
        json!({"canvasW":64,"canvasH":48,"fps":24.0,"documentJson":"{}"}),
    ] {
        let mut changed = request(&state);
        changed["newProject"] = value;
        bad.push(changed.to_string());
    }
    for (field, value) in [
        ("sourcePath", json!("/not-used")),
        ("documentJson", json!("{}")),
        ("projectOnly", json!(true)),
        ("extra", json!(false)),
        ("replace", Value::Null),
        ("replace", json!(["doc", 0, "snap", 1])),
        ("replace", json!({})),
    ] {
        let mut changed = request(&state);
        changed[field] = value;
        bad.push(changed.to_string());
    }
    for raw in bad {
        assert!(admission::new_request(&raw).is_none(), "{raw}");
        assert!(
            reopen::bootstrap_request(&state, &raw, |_| panic!("unexpected file route")).is_err()
        );
        assert!(state.native_state().lock().unwrap().active().is_none());
    }
}

#[test]
fn invalid_initial_intent_never_installs_and_valid_retry_still_works() {
    for (field, value) in [
        ("canvasW", json!(0)),
        ("canvasH", json!(0)),
        ("fps", json!(0)),
        ("fps", json!(-1)),
        ("fps", json!(1e20)),
    ] {
        let state = ApplicationMcp::default();
        let mut raw = request(&state);
        raw["newProject"][field] = value;
        assert!(bootstrap_raw(&state, &raw.to_string()).is_err());
        assert!(state.native_state().lock().unwrap().active().is_none());
        initial(&state);
    }
    for (field, value) in [
        ("apiVersion", json!(1)),
        ("instanceId", json!("foreign")),
        ("requestId", json!("")),
        ("cancelledBeforeDispatch", json!(true)),
    ] {
        let state = ApplicationMcp::default();
        let mut raw = request(&state);
        raw[field] = value;
        assert!(bootstrap_raw(&state, &raw.to_string()).is_err());
        assert!(state.native_state().lock().unwrap().active().is_none());
        initial(&state);
    }
}

#[test]
fn new_replacement_preflight_preserves_a_and_cannot_bypass_executor() {
    let state = ApplicationMcp::default();
    let a = initial(&state);
    let request = replacement(&state, &a);
    let info = state.native_project_info(&a.document_id).unwrap();
    let bytes = saved(&state, &a.document_id);
    for (field, value) in [
        ("documentId", json!("foreign")),
        ("documentSnapshotId", json!("foreign")),
        ("expectedRevision", json!(1)),
        ("lifecycleGeneration", json!(999)),
    ] {
        let mut bad = request.clone();
        bad["replace"][field] = value;
        let error = prepare(&state, &bad).err().unwrap();
        assert_eq!(error.code, "replacement_refused");
        assert_eq!(error.details.unwrap()["disposition"], "notDispatched");
        assert_eq!(state.native_project_info(&a.document_id).unwrap(), info);
    }
    for (field, value) in [
        ("canvasW", json!(0)),
        ("canvasH", json!(0)),
        ("fps", json!(-1)),
        ("fps", json!(1e20)),
    ] {
        let mut bad = request.clone();
        bad["newProject"][field] = value;
        assert_eq!(
            prepare(&state, &bad).err().unwrap().details.unwrap()["reasonCode"],
            "invalid_request"
        );
        assert_eq!(state.native_project_info(&a.document_id).unwrap(), info);
    }
    let mut cancelled = request.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        prepare(&state, &cancelled).err().unwrap().details.unwrap()["reasonCode"],
        "cancelled_before_dispatch"
    );
    assert!(bootstrap_raw(&state, &request.to_string()).is_err());
    assert!(
        reopen::bootstrap_request(&state, &request.to_string(), |_| panic!("file route")).is_err()
    );
    assert_eq!(state.native_project_info(&a.document_id).unwrap(), info);
    assert_eq!(saved(&state, &a.document_id), bytes);
    let raw = request.to_string();
    for raw in [
        raw.replacen("\"replace\":", "\"replace\":{},\"replace\":", 1),
        raw.replacen(
            "\"documentId\":",
            "\"document\\u0049d\":\"foreign\",\"documentId\":",
            1,
        ),
    ] {
        assert!(admission::new_request(&raw).is_none());
    }
    let mut unknown = request;
    unknown["replace"]["extra"] = json!(true);
    assert!(admission::new_request(&unknown.to_string()).is_none());
}

#[test]
fn new_replacement_commits_once_after_cleanup_and_exact_replay_preserves_b() {
    let state = ApplicationMcp::default();
    let a = initial(&state);
    let request = replacement(&state, &a);
    let admission::Admission::Execute {
        generation,
        request_id,
        fingerprint,
        prepared,
    } = prepare(&state, &request).unwrap()
    else {
        panic!("expected execution")
    };
    assert!(state.native_project_info(&a.document_id).is_err());
    assert_eq!(
        prepare(&state, &request).err().unwrap().code,
        "replacement_pending"
    );
    let calls = Cell::new(0);
    let b = replace::complete(
        &state.native_state(),
        generation,
        &request_id,
        prepared,
        || {
            calls.set(calls.get() + 1);
            Ok((vec![], "already_absent"))
        },
    )
    .unwrap();
    assert_ne!(a.document_id, b.document_id);
    let info = state.native_project_info(&b.document_id).unwrap();
    for (field, expected) in [
        ("canvasW", 80),
        ("canvasH", 60),
        ("totalFrames", 150),
        ("contentRevision", 0),
        ("undoDepth", 0),
        ("redoDepth", 0),
    ] {
        assert_eq!(info[field], expected, "{field}");
    }
    assert_eq!(info["lifecycleGeneration"], generation);
    let bytes = saved(&state, &b.document_id);
    let admission::Admission::Replay(replayed) = prepare(&state, &request).unwrap() else {
        panic!("must replay")
    };
    assert_eq!(
        serde_json::to_value(replayed).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    assert_eq!(
        serde_json::to_value(
            replace::reconcile_executor(
                &state.native_state(),
                generation,
                &request_id,
                &fingerprint
            )
            .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    assert_eq!(calls.get(), 1);
    let mut changed = request.clone();
    changed["newProject"]["canvasW"] = json!(81);
    assert_eq!(
        prepare(&state, &changed).err().unwrap().code,
        "replacement_refused"
    );
    changed["requestId"] = json!("new-stale");
    assert_eq!(
        prepare(&state, &changed).err().unwrap().code,
        "replacement_refused"
    );
    assert_eq!(state.native_project_info(&b.document_id).unwrap(), info);
    assert_eq!(saved(&state, &b.document_id), bytes);
}

#[test]
fn new_cleanup_failure_fences_without_activation_or_retry() {
    let state = ApplicationMcp::default();
    let a = initial(&state);
    let request = replacement(&state, &a);
    let admission::Admission::Execute {
        generation,
        request_id,
        prepared,
        ..
    } = prepare(&state, &request).unwrap()
    else {
        panic!("expected execution")
    };
    let error = replace::complete(
        &state.native_state(),
        generation,
        &request_id,
        prepared,
        || Err(host_error("cleanup_failed", "injected refusal")),
    )
    .unwrap_err();
    assert_eq!(error.code, "replacement_indeterminate");
    assert!(state.native_state().lock().unwrap().active().is_none());
    assert_eq!(
        prepare(&state, &request).err().unwrap().code,
        "replacement_indeterminate"
    );
}
