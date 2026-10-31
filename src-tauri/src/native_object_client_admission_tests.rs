//! Strict raw-client negatives and actual owner/lifecycle availability fences.
use crate::application_mcp::{
    object_client::{client_status, dispatch_raw, OPERATIONS},
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
fn closed_query_and_command_shapes_keep_private_operations_off_the_public_route() {
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
    let public_query = state
        .dispatch_native(serde_json::from_value(read(&receipt, "public-query", 0, 0)).unwrap())
        .unwrap();
    assert!(public_query.ok);
    assert_eq!(public_query.content_revision, 0);
    assert!(state
        .dispatch_native(serde_json::from_value(fill(&receipt, "private-fill", 0)).unwrap())
        .is_err());
    // Public history was activated after the original raw-client admission.
    // Empty history reaches the owner and returns its typed refusal.
    for r in [
        history(&receipt, "public-undo", 0, "undo"),
        history(&receipt, "public-redo", 0, "redo"),
    ] {
        let response = state
            .dispatch_native(serde_json::from_value(r.clone()).unwrap())
            .unwrap();
        assert!(!response.ok);
        assert_eq!(response.request_id, r["requestId"].as_str().unwrap());
        assert_eq!(
            response.instance_id,
            receipt["instanceId"].as_str().unwrap()
        );
        assert_eq!(
            response.document_id,
            receipt["documentId"].as_str().unwrap()
        );
        assert_eq!(response.content_revision, 0);
        assert_eq!(response.error.unwrap().code, "unavailable");
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
        "query.document.object.serialize",
        "command.document.object.layer.visibility.set",
        "command.document.object.project.settings.edit",
        "command.document.object.layer.policy.edit",
        "command.document.object.project.placement.edit",
        "command.document.object.audio.track.edit",
        "command.document.object.layer.cut"
    ]);
    assert_eq!(status["operations"], operations);
    for flag in ["publicCapabilityAvailable", "mcpAvailable"] {
        assert_eq!(status[flag], true);
    }
    for flag in ["saveAvailable", "viewportAvailable", "exportAvailable"] {
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
    assert_eq!(
        client_status(&state).unwrap()["publicCapabilityAvailable"],
        false
    );
    assert_eq!(client_status(&state).unwrap()["mcpAvailable"], false);
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
    assert_eq!(
        client_status(&state).unwrap()["publicCapabilityAvailable"],
        true
    );
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

/// Operation -> revisionDelta for every `rawClient` flag in the given descriptors.
fn raw_client_flags(descriptors: &[Value]) -> Result<std::collections::BTreeMap<String, u64>, String> {
    let mut flags = std::collections::BTreeMap::new();
    for descriptor in descriptors {
        let id = descriptor["id"].as_str().unwrap_or("?");
        let Some(raw) = descriptor.get("rawClient") else {
            continue;
        };
        for (operation, meta) in raw.as_object().ok_or(format!("{id}: rawClient is not an object"))? {
            let delta = meta["revisionDelta"]
                .as_u64()
                .ok_or(format!("{id}: {operation} has no revisionDelta"))?;
            let declared = |key: &str| {
                descriptor[key]
                    .as_array()
                    .is_some_and(|ops| ops.iter().any(|v| v == operation.as_str()))
            };
            if !declared("operations") && !declared("hostOperations") {
                return Err(format!("{id}: {operation} is flagged but not declared"));
            }
            if flags.insert(operation.clone(), delta).is_some() {
                return Err(format!("{operation} is flagged twice"));
            }
        }
    }
    Ok(flags)
}
fn raw_client_allowlist_drift(descriptors: &[Value]) -> Result<(), String> {
    let flags = raw_client_flags(descriptors)?;
    let listed: std::collections::BTreeSet<_> = OPERATIONS.iter().copied().collect();
    let flagged: std::collections::BTreeSet<_> = flags.keys().map(String::as_str).collect();
    if listed.len() != OPERATIONS.len() || listed != flagged {
        return Err(format!("OPERATIONS {listed:?} != flagged {flagged:?}"));
    }
    Ok(())
}
#[test]
fn raw_client_allowlist_equals_the_capability_declared_rawclient_set() {
    let descriptors = nemo_mcp::capabilities::native_catalog().descriptors();
    raw_client_allowlist_drift(descriptors).unwrap();
    let flags = raw_client_flags(descriptors).unwrap();
    for operation in OPERATIONS {
        let delta = flags[operation];
        // Reads never advance the revision; every command is one transaction.
        assert_eq!(delta, u64::from(operation.starts_with("command.")), "{operation}");
    }
}
#[test]
fn raw_client_host_operations_stay_out_of_the_public_registration() {
    let catalog = nemo_mcp::capabilities::native_catalog();
    let mut host = 0;
    for descriptor in catalog.descriptors() {
        for operation in descriptor["hostOperations"].as_array().into_iter().flatten() {
            let operation = operation.as_str().unwrap();
            host += 1;
            assert!(catalog.capability_for_operation(operation).is_none(), "{operation}");
            assert!(OPERATIONS.contains(&operation), "{operation}");
        }
    }
    assert_eq!(host, 2);
}
#[test]
fn raw_client_allowlist_drift_is_caught_in_both_directions() {
    let real = nemo_mcp::capabilities::native_catalog().descriptors().to_vec();
    // Unflag one operation: OPERATIONS now lists an operation nothing declares.
    for operation in OPERATIONS {
        let mut unflagged = real.clone();
        for descriptor in &mut unflagged {
            if let Some(raw) = descriptor.get_mut("rawClient").and_then(Value::as_object_mut) {
                raw.remove(operation);
            }
        }
        assert!(raw_client_allowlist_drift(&unflagged).is_err(), "unflag {operation}");
    }
    // Flag an extra declared operation: a flag the allowlist does not honour.
    let mut extra = real.clone();
    let target = extra
        .iter_mut()
        .find(|d| d["id"] == "native.object.fill")
        .unwrap();
    target["rawClient"]["command.document.object.fill.trace"] = json!({"revisionDelta":1});
    assert!(raw_client_allowlist_drift(&extra).is_err());
    // A flag on an operation its descriptor never declares is itself invalid.
    let mut ghost = real;
    ghost[0]["rawClient"] = json!({"command.document.object.ghost":{"revisionDelta":1}});
    assert!(raw_client_flags(&ghost).is_err());
}
