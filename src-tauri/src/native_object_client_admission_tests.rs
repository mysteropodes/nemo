//! Strict raw-client negatives and actual owner/lifecycle availability fences.
use crate::application_mcp::{
    object_client::{client_status, dispatch_raw},
    object_client_tests::*,
};
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{ReleaseAdmission, TerminalPump},
};
use serde_json::{json, Value};

fn duplicate(raw: &str, key: &str, escaped: bool) -> String {
    let encoded = if escaped {
        format!("\\u{:04x}{}", key.as_bytes()[0], &key[1..])
    } else {
        key.into()
    };
    raw.replacen(
        &format!("\"{key}\":"),
        &format!("\"{encoded}\":null,\"{key}\":"),
        1,
    )
}
#[test]
fn raw_admission_rejects_every_decoded_duplicate_before_any_owner_effect() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let valid = fill(&receipt, "raw-admitted", 0).to_string();
    for key in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "expectedRevision",
        "operation",
        "payload",
        "command",
        "stableTarget",
        "contextId",
        "frameScope",
        "kind",
        "frame",
        "layerUid",
        "strokeId",
        "fill",
        "r",
        "g",
        "b",
        "a",
    ] {
        for escaped in [false, true] {
            assert!(
                dispatch_raw(&state, &duplicate(&valid, key, escaped)).is_err(),
                "duplicate {key}"
            );
        }
    }
    assert_eq!(client_status(&state).unwrap()["contentRevision"], 0);
    assert_eq!(
        dispatch(&state, &fill(&receipt, "raw-admitted", 0))["ok"],
        true
    );
}
#[test]
fn strict_envelope_and_original_utf8_byte_limit_precede_command_dispatch() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let valid = fill(&receipt, "boundary", 0);
    let mut invalid = vec![
        String::new(),
        "null".into(),
        "[]".into(),
        "{}".into(),
        format!("{} trailing", valid),
        format!("{} {{}}", valid),
    ];
    for key in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "operation",
        "payload",
    ] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        invalid.push(missing.to_string());
        let mut null = valid.clone();
        null[key] = Value::Null;
        invalid.push(null.to_string());
    }
    for (key, value) in [
        ("apiVersion", json!(1)),
        ("extra", json!(true)),
        ("requestId", json!("bad id")),
        ("instanceId", json!("")),
        ("documentId", json!("a".repeat(129))),
        ("operation", json!("command.document.apply")),
        ("expectedRevision", json!(9_007_199_254_740_992u64)),
        ("expectedRevision", Value::Null),
        ("payload", json!([])),
        ("cancelledBeforeDispatch", json!(1)),
    ] {
        let mut altered = valid.clone();
        altered[key] = value;
        invalid.push(altered.to_string());
    }
    invalid.push(valid.to_string().replace("0.9", "1e400"));
    invalid.push(valid.to_string() + &" ".repeat(4097 - valid.to_string().len()));
    for raw in invalid {
        assert!(dispatch_raw(&state, &raw).is_err(), "accepted {raw}");
    }
    assert_eq!(client_status(&state).unwrap()["contentRevision"], 0);
    let exact = valid.to_string() + &" ".repeat(4096 - valid.to_string().len());
    assert!(dispatch_raw(&state, &exact).unwrap().ok);
    // Multibyte padding in an otherwise valid unknown field is counted in bytes.
    let unicode = valid
        .to_string()
        .replace("legacy layer:alpha", &"é".repeat(2100));
    assert!(dispatch_raw(&state, &unicode).is_err());
    assert_eq!(client_status(&state).unwrap()["contentRevision"], 1);
}
#[test]
fn closed_query_and_command_shapes_cannot_open_other_operations_or_public_availability() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    for operation in [
        "query.document.serialize",
        "query.document.evaluate",
        "history.undo",
        "job.export.png.begin",
        "query.viewport",
    ] {
        let bad = request(&receipt, "unsupported", 0, operation, json!({}));
        assert!(dispatch_raw(&state, &bad.to_string()).is_err());
    }
    let mut query = read(&receipt, "bad-query", 0, 0);
    query["expectedRevision"] = json!(0);
    assert!(dispatch_raw(&state, &query.to_string()).is_err());
    query = read(&receipt, "bad-query", 0, 0);
    query["payload"]["extra"] = json!(true);
    assert!(dispatch_raw(&state, &query.to_string()).is_err());
    query = read(&receipt, "bad-query", 0, 0);
    query["payload"]["stableTarget"]["frameScope"] = json!(["authored", 7]);
    assert!(dispatch_raw(&state, &query.to_string()).is_err());
    let mut bad = fill(&receipt, "bad-fill", 0);
    bad["payload"]["extra"] = json!(true);
    assert_eq!(dispatch(&state, &bad)["error"]["code"], "invalid_request");
    bad = fill(&receipt, "bad-target", 0);
    bad["payload"]["stableTarget"]["frameScope"]["frame"] = json!(8);
    assert_eq!(dispatch(&state, &bad)["error"]["code"], "not_found");
    for r in [
        read(&receipt, "public-query", 0, 0),
        fill(&receipt, "public-fill", 0),
        history(&receipt, "public-undo", 0, "undo"),
        history(&receipt, "public-redo", 0, "redo"),
    ] {
        assert!(state
            .dispatch_native(serde_json::from_value(r).unwrap())
            .is_err());
    }
    assert_eq!(client_status(&state).unwrap()["contentRevision"], 0);
}
#[test]
fn route_is_default_denied_for_other_owner_and_every_nonactive_lifecycle_phase() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let mut state = ApplicationMcp::default();
    state.instance_id = "pump-fixture".into();
    let calls = Arc::new(AtomicUsize::new(0));
    let fake = json!({"instanceId":"pump-fixture","documentId":"document-fixture"});
    let command = fill(&fake, "wrong-owner", 0);
    assert_eq!(client_status(&state).unwrap()["available"], false);
    assert!(!dispatch_raw(&state, &command.to_string()).unwrap().ok);
    let reservation = state.reserve_native_install().unwrap();
    assert!(!dispatch_raw(&state, &command.to_string()).unwrap().ok);
    state
        .install_dispatch(
            reservation.generation(),
            Box::new(TerminalPump(calls.clone())),
        )
        .unwrap();
    let status = client_status(&state).unwrap();
    assert_eq!(status["available"], false);
    assert_eq!(status["operations"], json!([]));
    assert!(!dispatch_raw(&state, &command.to_string()).unwrap().ok);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn actual_release_reinstall_changes_incarnation_and_never_replays_into_new_owner() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let status = client_status(&state).unwrap();
    assert_eq!(status["available"], true);
    let operations = json!([
        "query.document.object",
        "command.document.object.fill.set",
        "command.document.object.undo",
        "command.document.object.redo",
        "query.document.object.serialize"
    ]);
    assert_eq!(status["operations"], operations);
    for flag in [
        "publicCapabilityAvailable",
        "mcpAvailable",
        "saveAvailable",
        "viewportAvailable",
        "exportAvailable",
    ] {
        assert_eq!(status[flag], false);
    }
    let command = fill(&receipt, "incarnation-fill", 0);
    dispatch(&state, &command);
    let request: NativeReleaseRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"release-client",
        "instanceId":receipt["instanceId"],"documentId":receipt["documentId"],"expectedRevision":1})).unwrap();
    let native = state.native_state();
    let generation = match admit_release_request(&native, &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retry"),
    };
    assert_eq!(client_status(&state).unwrap()["available"], false);
    assert!(!dispatch_raw(&state, &command.to_string()).unwrap().ok);
    complete_release(&native, generation, &request, || {
        Ok((Vec::new(), "complete"))
    })
    .unwrap();
    assert_eq!(client_status(&state).unwrap()["available"], false);
    assert_eq!(client_status(&state).unwrap()["operations"], json!([]));
    assert!(!dispatch_raw(&state, &command.to_string()).unwrap().ok);
    let next = install(&state);
    assert_eq!(client_status(&state).unwrap()["operations"], operations);
    assert_ne!(next["documentId"], receipt["documentId"]);
    assert_eq!(
        dispatch(&state, &command)["error"]["code"],
        "wrong_document"
    );
    assert_eq!(
        dispatch(&state, &read(&next, "fresh-read", 0, 0))["result"]["object"],
        document()["objects"][0]
    );
    assert!(
        client_status(&state).unwrap()["lifecycleGeneration"]
            .as_u64()
            .unwrap()
            > status["lifecycleGeneration"].as_u64().unwrap()
    );
}
