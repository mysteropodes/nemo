//! Host/file refusals preserve actual authority and owned filesystem effects.
use crate::native_object_bootstrap::v2_tests::*;

#[test]
fn host_v2_invalid_original_raw_documents_never_install_and_v1_stays_v1() {
    let state = ApplicationMcp::default();
    let mut invalid: Value = serde_json::from_str(ORIGINAL).unwrap();
    invalid["compositionOrder"]["contexts"][0]["layers"][0]["naturalPosition"] = json!(2);
    let wrapper = json!({"apiVersion":2,"requestId":"invalid","instanceId":state.instance_id(),"documentJson":invalid.to_string()});
    assert!(bootstrap_raw(&state, &wrapper.to_string()).is_err());
    assert_eq!(identity(&state), None);
    let mut missing: Value = serde_json::from_str(ORIGINAL).unwrap();
    missing.as_object_mut().unwrap().remove("compositionOrder");
    let request = json!({"apiVersion":2,"requestId":"missing","instanceId":state.instance_id(),"documentJson":missing.to_string()});
    assert!(bootstrap_raw(&state, &request.to_string()).is_err());
    let duplicate = ORIGINAL.replacen(
        "\"formatVersion\":2",
        "\"formatVersion\":2,\"formatVersion\":2",
        1,
    );
    let request = json!({"apiVersion":2,"requestId":"duplicate","instanceId":state.instance_id(),"documentJson":duplicate});
    assert!(bootstrap_raw(&state, &request.to_string()).is_err());
    assert_eq!(identity(&state), None);
    missing["formatVersion"] = json!(1);
    let legacy = install(&state, &missing.to_string());
    let before = identity(&state);
    let value: Value = serde_json::from_str(
        snapshot(&state, &legacy, 0)["documentJson"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value, missing);
    assert!(bootstrap_raw(&state, &wrapper.to_string()).is_err());
    assert_eq!(identity(&state), before);
}

#[test]
fn host_v2_cancel_scope_stale_and_duplicate_save_requests_have_zero_disk_effect() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let request = save_json(&state, &pins, 0, &scratch.0.join("new.json"));
    let before = identity(&state);
    let mut cancelled = request.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        save_request(
            &state,
            &cancelled.to_string(),
            |_| panic!("cancel scope"),
            |_, _| panic!("cancel effect")
        )
        .unwrap_err()
        .code,
        "cancelled_before_dispatch"
    );
    assert!(save_request(
        &state,
        &request.to_string(),
        |_| false,
        |_, _| panic!("scope effect")
    )
    .is_err());
    let raw = request.to_string().replacen(
        "\"atRevision\":0",
        r#""atRevision":0,"at\u0052evision":0"#,
        1,
    );
    assert_ne!(raw, request.to_string());
    assert!(save_request(
        &state,
        &raw,
        |_| panic!("duplicate scope"),
        |_, _| panic!("duplicate effect")
    )
    .is_err());
    let mut badpin = request.clone();
    badpin["documentSnapshotId"] = json!("stale-snapshot");
    assert!(save_request(
        &state,
        &badpin.to_string(),
        |_| true,
        |_, _| panic!("badpin effect")
    )
    .is_err());
    assert_eq!(scratch.count(), 0);
    assert_eq!(identity(&state), before);
    assert_eq!(snapshot(&state, &pins, 0)["documentJson"], ORIGINAL);
    advance(&state, &pins, 0, "fill.set");
    assert!(save_request(
        &state,
        &request.to_string(),
        |_| true,
        |_, _| panic!("stale effect")
    )
    .is_err());
    assert_eq!(scratch.count(), 0);
    assert_eq!(snapshot(&state, &pins, 1)["documentJson"], EDITED);
}

#[test]
fn host_v2_existing_destination_and_file_reopen_refusals_preserve_data_owner_and_history() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let path = scratch.0.join("foreign.json");
    fs::write(&path, b"foreign").unwrap();
    let request = save_json(&state, &pins, 0, &path);
    assert!(save_request(&state, &request.to_string(), |_| true, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"foreign");
    assert_eq!(scratch.count(), 1);
    let source = scratch.0.join("valid-v2.json");
    fs::write(&source, ORIGINAL).unwrap();
    let request = json!({"apiVersion":2,"requestId":"file","instanceId":state.instance_id(),"sourcePath":source.to_str().unwrap()});
    let before = identity(&state);
    assert!(bootstrap_request(&state, &request.to_string(), |_| false).is_err());
    assert!(bootstrap_request(&state, &request.to_string(), |_| true).is_err()); // Active owner cannot be replaced.
    let mut cancelled = request.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        bootstrap_request(&state, &cancelled.to_string(), |_| panic!("cancelled open"))
            .unwrap_err()
            .code,
        "cancelled_before_dispatch"
    );
    assert_eq!(identity(&state), before);
    assert_eq!(snapshot(&state, &pins, 0)["documentJson"], ORIGINAL);
    release(&state, &pins, 0);
    assert_eq!(identity(&state), None);
    let mut malformed: Value = serde_json::from_str(ORIGINAL).unwrap();
    malformed["compositionOrder"]["contexts"] = json!([]);
    fs::write(&source, malformed.to_string()).unwrap();
    assert!(bootstrap_request(&state, &request.to_string(), |_| true).is_err());
    assert_eq!(identity(&state), None);
    assert_eq!(fs::read(&path).unwrap(), b"foreign");
}

#[test]
fn host_v2_saved_acquisition_survives_actual_advance_and_terminal_release_before_io() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let path = scratch.0.join("acquired.json");
    let request = save_json(&state, &pins, 0, &path);
    let mut triggered = false;
    let receipt = serde_json::to_value(
        save_request(
            &state,
            &request.to_string(),
            |path| path.starts_with(&scratch.0),
            |stage, _| {
                if stage == "snapshot" && !triggered {
                    triggered = true;
                    advance(&state, &pins, 0, "fill.set");
                }
                Ok(())
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert!(triggered);
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["atRevision"], 0);
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(snapshot(&state, &pins, 1)["documentJson"], EDITED);
    let path = scratch.0.join("released-acquisition.json");
    let request = save_json(&state, &pins, 1, &path);
    let mut triggered = false;
    let receipt = serde_json::to_value(
        save_request(
            &state,
            &request.to_string(),
            |path| path.starts_with(&scratch.0),
            |stage, _| {
                if stage == "snapshot" && !triggered {
                    triggered = true;
                    release(&state, &pins, 1);
                }
                Ok(())
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert!(triggered);
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["atRevision"], 1);
    assert_eq!(identity(&state), None);
    assert_eq!(fs::read(&path).unwrap(), EDITED.as_bytes());
}

#[test]
fn host_v2_oversize_serializer_and_released_owner_cannot_stage_or_install_partial_bytes() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let mut large: Value = serde_json::from_str(ORIGINAL).unwrap();
    large["compositionOrder"]["contexts"][0]["layers"][0]["order"]["expression"]["code"] =
        json!("x".repeat(4096));
    let pins = install(&state, &large.to_string());
    let before = identity(&state);
    let serialized = call(
        &state,
        &pins,
        "query.document.object.serialize",
        0,
        json!({"atRevision":0}),
    );
    assert_eq!(serialized["ok"], false);
    assert_eq!(serialized["error"]["code"], "unavailable");
    let request = json!({"apiVersion":2,"requestId":"large-save","instanceId":pins["instanceId"],"documentId":pins["documentId"],
        "atRevision":0,"documentSnapshotId":format!("native-object:{}:0",pins["documentId"].as_str().unwrap()),
        "destinationPath":scratch.0.join("new.json").to_str().unwrap()});
    assert!(save_request(
        &state,
        &request.to_string(),
        |_| true,
        |_, _| panic!("oversize effect")
    )
    .is_err());
    assert_eq!(identity(&state), before);
    assert_eq!(scratch.count(), 0);
    release(&state, &pins, 0);
    assert!(save_request(
        &state,
        &request.to_string(),
        |_| true,
        |_, _| panic!("released effect")
    )
    .is_err());
    assert_eq!(identity(&state), None);
    assert_eq!(scratch.count(), 0);
}
