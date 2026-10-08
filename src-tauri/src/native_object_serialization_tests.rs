//! Actual-owner serialization, exact persisted bytes and independent reopen controls.
use crate::application_mcp::{
    object_client::{client_status, dispatch_raw},
    object_client_tests::{document, fill, history, read},
    ApplicationMcp,
};
use crate::{
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{ReleaseAdmission, TerminalPump},
    native_object_bootstrap::bootstrap_raw,
};
use native_engine::object_codec::decode_project;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn install(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,
        "requestId":"serialize-install","instanceId":state.instance_id(),
        "documentJson":bytes})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn query(pins: &Value, revision: u64) -> Value {
    json!({"apiVersion":2,"requestId":"serialize-read","instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"operation":"query.document.object.serialize",
        "payload":{"atRevision":revision}})
}
fn dispatch(state: &ApplicationMcp, request: &Value) -> Value {
    serde_json::to_value(dispatch_raw(state, &request.to_string()).unwrap()).unwrap()
}
fn advance(state: &ApplicationMcp, request: &Value, revision: u64) {
    let response = dispatch(state, request);
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], revision);
    assert_eq!(client_status(state).unwrap()["contentRevision"], revision);
}
fn refuse(state: &ApplicationMcp, request: &Value, code: &str) {
    let response = dispatch(state, request);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], code);
    assert_eq!(response["requestId"], request["requestId"]);
    assert!(response.get("result").is_none());
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
}
fn serialized(state: &ApplicationMcp, pins: &Value, revision: u64, expected: &Value) -> String {
    let request = query(pins, revision);
    let response = dispatch(state, &request);
    for key in ["apiVersion", "requestId", "instanceId", "documentId"] {
        assert_eq!(response[key], request[key]);
    }
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["contentRevision"], revision);
    assert!(response.get("error").is_none());
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    let result = &response["result"];
    assert_eq!(result.as_object().unwrap().len(), 3);
    assert_eq!(result["atRevision"], revision);
    assert_eq!(
        result["documentSnapshotId"],
        format!(
            "native-object:{}:{revision}",
            pins["documentId"].as_str().unwrap()
        )
    );
    let bytes = result["documentJson"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(bytes).unwrap(), *expected);
    assert_eq!(
        serde_json::to_value(decode_project(bytes.as_bytes()).unwrap()).unwrap(),
        *expected
    );
    assert_eq!(client_status(state).unwrap()["contentRevision"], revision);
    bytes.into()
}
fn edited() -> Value {
    let mut expected = document();
    expected["objects"][0]["fill"] = json!({"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5});
    expected
}
fn release(state: &ApplicationMcp, pins: &Value, revision: u64) -> Value {
    let request: NativeReleaseRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":"serialize-release","instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"expectedRevision":revision}))
    .unwrap();
    let native = state.native_state();
    let generation = match admit_release_request(&native, &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retained release"),
    };
    assert!(!dispatch(state, &query(pins, revision))["ok"]
        .as_bool()
        .unwrap());
    serde_json::to_value(
        complete_release(&native, generation, &request, || {
            Ok((vec![], "already_absent"))
        })
        .unwrap(),
    )
    .unwrap()
}

struct Saved(PathBuf);
impl Drop for Saved {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
#[test]
fn whole_document_fill_history_save_reopen_preserves_records_and_runtime_fences() {
    let state = ApplicationMcp::default();
    let pins = install(&state, &document().to_string());
    let original = serialized(&state, &pins, 0, &document());
    assert_eq!(serialized(&state, &pins, 0, &document()), original);
    assert!(state
        .dispatch_native(serde_json::from_value(query(&pins, 0)).unwrap())
        .is_err());
    advance(&state, &fill(&pins, "serialize-fill", 0), 1);
    let after_fill = serialized(&state, &pins, 1, &edited());
    assert_ne!(after_fill, original);
    advance(&state, &history(&pins, "serialize-undo", 1, "undo"), 2);
    assert_eq!(serialized(&state, &pins, 2, &document()), original);
    advance(&state, &history(&pins, "serialize-redo", 2, "redo"), 3);
    assert_eq!(serialized(&state, &pins, 3, &edited()), after_fill);
    let (old, bytes) = (pins, after_fill);
    let saved =
        Saved(std::env::temp_dir().join(format!("nemo-object-{}.json", uuid::Uuid::new_v4())));
    fs::write(&saved.0, bytes.as_bytes()).unwrap();
    let disk = fs::read(&saved.0).unwrap();
    assert_eq!(disk, bytes.as_bytes());
    let terminal = release(&state, &old, 3);
    assert_eq!(terminal["undoDepth"], 1);
    assert_eq!(terminal["redoDepth"], 0);
    assert_eq!(client_status(&state).unwrap()["available"], false);
    assert_eq!(client_status(&state).unwrap()["operations"], json!([]));
    refuse(&state, &query(&old, 3), "unavailable");
    let fresh = install(&state, std::str::from_utf8(&disk).unwrap());
    assert_ne!(fresh["documentId"], old["documentId"]);
    assert_eq!(fresh["instanceId"], old["instanceId"]);
    assert_eq!(fresh["contentRevision"], 0);
    assert_eq!(serialized(&state, &fresh, 0, &edited()), bytes);
    refuse(&state, &query(&old, 3), "wrong_document");
    for index in 0..document()["objects"].as_array().unwrap().len() {
        assert_eq!(
            dispatch(&state, &read(&fresh, "saved-record", 0, index))["result"]["object"],
            edited()["objects"][index]
        );
    }
    for action in ["undo", "redo"] {
        refuse(
            &state,
            &history(&fresh, &format!("empty-{action}"), 0, action),
            "unavailable",
        );
    }
    assert_eq!(serialized(&state, &fresh, 0, &edited()), bytes);
}

#[test]
fn strict_original_envelope_and_pinned_selector_fail_without_owner_changes() {
    let state = ApplicationMcp::default();
    let pins = install(&state, &document().to_string());
    let original = serialized(&state, &pins, 0, &document());
    let valid = query(&pins, 0);
    let raw = valid.to_string();
    let mut invalid = vec!["null".into(), "[]".into(), format!("{raw} trailing")];
    for payload in [
        json!({}),
        json!({"atRevision":null}),
        json!({"atRevision":-1}),
        json!({"atRevision":0.5}),
        json!({"atRevision":9_007_199_254_740_992u64}),
        json!({"atRevision":0,"extra":true}),
        json!([0]),
    ] {
        let mut altered = valid.clone();
        altered["payload"] = payload;
        invalid.push(altered.to_string());
    }
    for (key, value) in [
        ("expectedRevision", json!(0)),
        ("extra", json!(true)),
        ("requestId", json!("invalid id")),
        ("operation", json!("query.document.serialize")),
    ] {
        let mut altered = valid.clone();
        altered[key] = value;
        invalid.push(altered.to_string());
    }
    for key in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "operation",
        "payload",
        "atRevision",
    ] {
        for encoded in [
            key.to_string(),
            format!("\\u{:04x}{}", key.as_bytes()[0], &key[1..]),
        ] {
            invalid.push(raw.replacen(
                &format!("\"{key}\":"),
                &format!("\"{encoded}\":null,\"{key}\":"),
                1,
            ));
        }
    }
    invalid.push(format!("{raw}{}", " ".repeat(4097 - raw.len())));
    for bad in invalid {
        assert!(dispatch_raw(&state, &bad).is_err(), "accepted {bad}");
    }
    let exact = format!("{raw}{}", " ".repeat(4096 - raw.len()));
    assert!(dispatch_raw(&state, &exact).unwrap().ok);
    assert!(dispatch_raw(&state, &(exact + " ")).is_err());
    assert_eq!(serialized(&state, &pins, 0, &document()), original);
    advance(&state, &fill(&pins, "after-invalid", 0), 1);
    advance(&state, &history(&pins, "only-entry", 1, "undo"), 2);
    refuse(&state, &history(&pins, "extra", 2, "undo"), "unavailable");
}

#[test]
fn identity_cancellation_and_revision_refusals_preserve_full_content_and_history() {
    let state = ApplicationMcp::default();
    let pins = install(&state, &document().to_string());
    for (key, value, code) in [
        ("instanceId", json!("other-instance"), "wrong_instance"),
        ("documentId", json!("other-document"), "wrong_document"),
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
    ] {
        let mut bad = query(&pins, 0);
        bad[key] = value;
        refuse(&state, &bad, code);
    }
    refuse(&state, &query(&pins, 1), "not_found");
    dispatch(&state, &fill(&pins, "stale-fill", 0));
    refuse(&state, &query(&pins, 0), "not_found");
    serialized(&state, &pins, 1, &edited());
    advance(&state, &history(&pins, "one-entry", 1, "undo"), 2);
    serialized(&state, &pins, 2, &document());
    refuse(
        &state,
        &history(&pins, "extra-again", 2, "undo"),
        "unavailable",
    );
}

#[test]
fn oversized_complete_document_refuses_without_truncation_or_history_mutation() {
    let mut large = document();
    let segment = large["objects"][0]["geometry"]["segments"][0].clone();
    large["objects"][0]["geometry"]["segments"] = Value::Array(vec![segment; 256]);
    let state = ApplicationMcp::default();
    let pins = install(&state, &large.to_string());
    let failure = dispatch(&state, &query(&pins, 0));
    assert_eq!(failure["ok"], false);
    assert_eq!(failure["error"]["code"], "unavailable");
    assert!(failure.get("result").is_none());
    assert!(serde_json::to_vec(&failure).unwrap().len() <= 4096);
    assert_eq!(client_status(&state).unwrap()["contentRevision"], 0);
    for action in ["undo", "redo"] {
        refuse(
            &state,
            &history(&pins, &format!("large-{action}"), 0, action),
            "unavailable",
        );
    }
    assert_eq!(
        dispatch(&state, &read(&pins, "untouched-small", 0, 1))["result"]["object"],
        large["objects"][1]
    );
}

#[test]
fn vacant_reserved_and_nonobject_owner_never_dispatch_serialization() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let mut state = ApplicationMcp::default();
    state.instance_id = "pump-fixture".into();
    let pins = json!({"instanceId":"pump-fixture","documentId":"document-fixture"});
    refuse(&state, &query(&pins, 0), "unavailable");
    let reservation = state.reserve_native_install().unwrap();
    refuse(&state, &query(&pins, 0), "unavailable");
    let calls = Arc::new(AtomicUsize::new(0));
    state
        .install_dispatch(
            reservation.generation(),
            Box::new(TerminalPump(calls.clone())),
        )
        .unwrap();
    refuse(&state, &query(&pins, 0), "unavailable");
    assert_eq!(client_status(&state).unwrap()["operations"], json!([]));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
