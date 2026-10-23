//! Independent distinction between registered input and execution availability.
use super::*;
use serde_json::{json, Value};

fn object_request(index: usize) -> NativeApplicationRequest {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    NativeApplicationRequest {
        api_version: 2,
        request_id: "input-read".into(),
        instance_id: "instance-a".into(),
        document_id: "native-object-document-1".into(),
        expected_revision: None,
        operation: "query.document.object".into(),
        payload: json!({"atRevision":0,"stableTarget":cases["records"][index]["target"]}),
        cancelled_before_dispatch: false,
    }
}

fn visibility_request() -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "visibility-7".into(),
        instance_id: "instance-a".into(),
        document_id: "native-object-document-1".into(),
        expected_revision: Some(7),
        operation: "command.document.object.layer.visibility.set".into(),
        payload: json!({
            "command": "object.layer.visibility.set",
            "layerUid": "legacy layer:alpha",
            "visible": false
        }),
        cancelled_before_dispatch: false,
    }
}

#[test]
fn public_visibility_is_registered_and_strict() {
    let base = visibility_request();
    assert!(base.validate_input().is_ok());
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(&base.operation)
        .unwrap();
    assert_eq!(descriptor["id"], "native.object");
    assert_eq!(descriptor["availability"]["state"], "available");
    assert!(base.validate().is_ok());

    let mut missing_revision = base.clone();
    missing_revision.expected_revision = None;
    same_failure(
        &missing_revision,
        "invalid_request",
        "native object visibility command requires expectedRevision",
    );
    let mut unsafe_revision = base.clone();
    unsafe_revision.expected_revision = Some(9_007_199_254_740_992);
    same_failure(
        &unsafe_revision,
        "invalid_request",
        "expectedRevision exceeds the transport maximum",
    );
    for payload in [
        json!({}),
        json!({"command":"object.layer.visibility.set","layerUid":"","visible":false}),
        json!({"command":"object.layer.visibility.set","layerUid":7,"visible":false}),
        json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":0}),
        json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":false,"extra":true}),
        json!({"command":"object.fill.set","layerUid":"legacy layer:alpha","visible":false}),
    ] {
        let mut invalid = base.clone();
        invalid.payload = payload;
        same_failure(
            &invalid,
            "invalid_request",
            "native object visibility command does not match its declared contract",
        );
    }
    for good in [
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":false,"historyEntriesAdded":0}),
    ] {
        assert!(crate::native_contract::validate_result(
            &base.operation,
            &base.document_id,
            &base.payload,
            &good
        ));
    }
    for bad in [
        json!({}),
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"extra":true}),
        json!({"applied":"yes","historyEntriesAdded":1}),
    ] {
        assert!(!crate::native_contract::validate_result(
            &base.operation,
            &base.document_id,
            &base.payload,
            &bad
        ));
    }
}

#[test]
fn fixed_authored_and_reference_input_admits_public_route_without_asserting_existence() {
    for index in [0, 1] {
        let request = object_request(index);
        let before = serde_json::to_value(&request).unwrap();
        assert!(request.validate_input().is_ok());
        assert!(request.validate().is_ok());
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation("query.document.object")
            .unwrap();
        assert_eq!(descriptor["availability"]["state"], "available");
        assert_eq!(serde_json::to_value(&request).unwrap(), before);
        let mut absent_target = request.clone();
        absent_target.payload["stableTarget"]["strokeId"] = json!("not-in-a-document");
        // Existence belongs to the actual owner, not this pure input check.
        assert!(absent_target.validate_input().is_ok());
        assert!(absent_target.validate().is_ok());
    }
}

#[test]
fn every_registered_available_example_retains_full_validation() {
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let request: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            assert!(request.validate_input().is_ok(), "{}", descriptor["id"]);
            if descriptor["availability"]["state"] == "available" {
                assert!(request.validate().is_ok(), "{}", descriptor["id"]);
            } else {
                assert_eq!(
                    request.validate().unwrap_err().code(),
                    "unavailable",
                    "{}",
                    descriptor["id"]
                );
            }
        }
    }
}

fn same_failure(request: &NativeApplicationRequest, code: &str, message: &str) {
    let input = request.validate_input().unwrap_err();
    let full = request.validate().unwrap_err();
    assert_eq!(input.code(), code);
    assert_eq!(full.code(), code);
    assert_eq!(input.message(), message);
    assert_eq!(full.message(), message);
}

#[test]
fn malformed_registered_selectors_preserve_codes_and_precedence() {
    let base = object_request(0);
    for payload in [
        json!({}),
        Value::Null,
        json!([]),
        json!("{}"),
        json!({"atRevision":0,"stableTarget":{}}),
    ] {
        let mut request = base.clone();
        request.payload = payload;
        same_failure(
            &request,
            "invalid_request",
            "native object selector does not match its declared contract",
        );
    }
    for (path, value) in [
        ("/atRevision", json!(-1)),
        ("/atRevision", json!(9_007_199_254_740_992_u64)),
        ("/stableTarget/contextId", json!("other-context")),
        ("/stableTarget/frameScope/kind", json!("all-frames")),
        ("/stableTarget/frameScope/frame", json!(4_294_967_296_u64)),
        ("/stableTarget/frameScope", json!(["authored", 7])),
        ("/stableTarget/layerUid", json!(0)),
        ("/stableTarget/strokeId", json!("")),
    ] {
        let mut request = base.clone();
        *request.payload.pointer_mut(path).unwrap() = value;
        same_failure(
            &request,
            "invalid_request",
            "native object selector does not match its declared contract",
        );
    }
    let mut extra = base.clone();
    extra.payload["extra"] = json!(true);
    same_failure(
        &extra,
        "invalid_request",
        "native object selector does not match its declared contract",
    );
    let mut revised = base.clone();
    revised.expected_revision = Some(0);
    same_failure(
        &revised,
        "invalid_request",
        "native pinned reads forbid expectedRevision",
    );
    revised.expected_revision = Some(9_007_199_254_740_992);
    same_failure(
        &revised,
        "invalid_request",
        "expectedRevision exceeds the transport maximum",
    );
    revised.api_version = 1;
    same_failure(
        &revised,
        "invalid_request",
        "native request requires apiVersion 2",
    );
}

#[test]
fn registration_identity_and_cancellation_remain_separate_from_availability() {
    let base = object_request(0);
    for invalid in ["", "bad id", ".bad", "🟠", &"x".repeat(129)] {
        let mut request = base.clone();
        request.request_id = invalid.into();
        same_failure(
            &request,
            "invalid_request",
            "requestId must match ^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$",
        );
    }
    for field in ["instanceId", "documentId", "operation"] {
        let mut encoded = serde_json::to_value(&base).unwrap();
        encoded[field] = json!("bad id");
        let request = serde_json::from_value(encoded).unwrap();
        same_failure(
            &request,
            "invalid_request",
            &format!("{field} must match ^[A-Za-z0-9][A-Za-z0-9._:/-]{{0,127}}$"),
        );
    }
    let mut unknown = base.clone();
    unknown.operation = "query.feature.not.registered".into();
    unknown.payload = json!({});
    same_failure(
        &unknown,
        "invalid_request",
        "operation query.feature.not.registered is not declared by a registered native capability",
    );
    let mut cancelled = base.clone();
    cancelled.cancelled_before_dispatch = true;
    assert!(cancelled.validate_input().is_ok());
    assert!(
        cancelled.validate().is_ok(),
        "execution handles cancellation after shape admission"
    );
}

#[test]
fn exact_complete_utf8_byte_boundary_is_preserved() {
    let mut request = object_request(0);
    request.operation = "query.document.opacity".into();
    request.payload = json!({"pad":""});
    let base = serde_json::to_vec(&request).unwrap().len();
    request.payload["pad"] = json!("x".repeat(4096 - base));
    assert_eq!(serde_json::to_vec(&request).unwrap().len(), 4096);
    assert!(request.validate_input().is_ok());
    assert!(request.validate().is_ok());
    request.payload["pad"] = json!("x".repeat(4097 - base));
    same_failure(
        &request,
        "invalid_request",
        "native requests are limited to 4096 encoded bytes",
    );
    let padding = 4096 - base;
    request.payload["pad"] = json!(format!(
        "{}{}",
        "🟠".repeat(padding / 4),
        "x".repeat(padding % 4)
    ));
    assert_eq!(serde_json::to_vec(&request).unwrap().len(), 4096);
    assert!(request.validate_input().is_ok());
    request.payload["pad"]
        .as_str()
        .map(str::to_owned)
        .map(|pad| {
            request.payload["pad"] = json!(format!("{pad}x"));
        })
        .unwrap();
    same_failure(
        &request,
        "invalid_request",
        "native requests are limited to 4096 encoded bytes",
    );
}

#[test]
fn typed_deserialization_stays_closed_and_does_not_replace_raw_ingress() {
    let base = serde_json::to_value(object_request(0)).unwrap();
    for (field, value) in [
        ("extra", json!(true)),
        ("expectedRevision", Value::Null),
        ("cancelledBeforeDispatch", json!("false")),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<NativeApplicationRequest>(invalid).is_err());
    }
    let encoded = serde_json::to_string(&base).unwrap();
    let duplicate = format!("{{\"requestId\":\"one\",{}", &encoded[1..]);
    assert!(serde_json::from_str::<NativeApplicationRequest>(&duplicate).is_err());
    assert!(
        serde_json::from_str::<NativeApplicationRequest>(&format!("{encoded} trailing")).is_err()
    );
    assert!(serde_json::from_value::<NativeApplicationRequest>(json!([])).is_err());
}

#[test]
fn import_input_requires_a_revision_and_its_declared_payload_before_availability() {
    let operation = "command.document.object.import.batch";
    let example = &crate::capabilities::native_catalog()
        .capability_for_operation(operation)
        .expect("declared")["examples"][0];
    let base: NativeApplicationRequest =
        serde_json::from_value(example["request"].clone()).unwrap();
    assert_eq!(base.operation, operation);
    assert!(base.validate_input().is_ok());
    assert!(base.validate().is_ok());

    let mut missing_revision = base.clone();
    missing_revision.expected_revision = None;
    same_failure(
        &missing_revision,
        "invalid_request",
        "native object import command requires expectedRevision",
    );
    let mut open = base.clone();
    open.payload["unexpected"] = json!(true);
    let mut foreign_command = base;
    foreign_command.payload["command"] = json!("object.import.other");
    for request in [&open, &foreign_command] {
        same_failure(
            request,
            "invalid_request",
            "native object import command does not match its declared contract",
        );
    }
}

fn history_request(command: &str) -> NativeApplicationRequest {
    let mut request = visibility_request();
    request.operation = format!("command.document.{command}");
    request.payload = json!({"command":command});
    request
}

#[test]
fn public_object_history_requires_registered_strict_revision_pinned_commands() {
    for command in ["object.undo", "object.redo"] {
        let base = history_request(command);
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&base.operation)
            .expect("public native object history registration");
        assert_eq!(descriptor["id"], "native.object");
        assert_eq!(descriptor["availability"]["state"], "available");
        for revision in [0, 7, 9_007_199_254_740_991] {
            let mut request = base.clone();
            request.expected_revision = Some(revision);
            assert!(request.validate_input().is_ok());
            assert!(request.validate().is_ok());
        }
        let mut missing = base.clone();
        missing.expected_revision = None;
        same_failure(
            &missing,
            "invalid_request",
            "native object history command requires expectedRevision",
        );
        let mut unsafe_revision = base.clone();
        unsafe_revision.expected_revision = Some(9_007_199_254_740_992);
        same_failure(
            &unsafe_revision,
            "invalid_request",
            "expectedRevision exceeds the transport maximum",
        );
        let mut encoded = serde_json::to_value(&base).unwrap();
        encoded["expectedRevision"] = Value::Null;
        assert!(serde_json::from_value::<NativeApplicationRequest>(encoded).is_err());
        for payload in [
            json!({}),
            Value::Null,
            json!([]),
            json!({"command":null}),
            json!({"command":7}),
            json!({"command":"history.undo"}),
            json!({"command":if command == "object.undo" {"object.redo"} else {"object.undo"}}),
            json!({"command":command,"extra":true}),
        ] {
            let mut invalid = base.clone();
            invalid.payload = payload;
            same_failure(
                &invalid,
                "invalid_request",
                "native object history command does not match its declared contract",
            );
        }
    }
}

#[test]
fn public_object_history_success_is_exact_and_correlated_to_its_request() {
    for command in ["object.undo", "object.redo"] {
        let request = history_request(command);
        let good = json!({"applied":true,"historyEntriesAdded":0});
        assert!(crate::native_contract::validate_result(
            &request.operation,
            &request.document_id,
            &request.payload,
            &good
        ));
        for result in [
            Value::Null,
            json!({}),
            json!({"applied":false,"historyEntriesAdded":0}),
            json!({"applied":true,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":0.0}),
            json!({"applied":true,"historyEntriesAdded":null}),
            json!({"applied":true}),
            json!({"applied":null,"historyEntriesAdded":0}),
            json!({"applied":true,"historyEntriesAdded":0,"extra":true}),
        ] {
            assert!(!crate::native_contract::validate_result(
                &request.operation,
                &request.document_id,
                &request.payload,
                &result
            ));
        }
        let response = json!({"apiVersion":2,"requestId":request.request_id,
            "instanceId":request.instance_id,"documentId":request.document_id,
            "contentRevision":8,"ok":true,"result":good});
        assert!(crate::wire::validate_native_response(
            &serde_json::from_value(response.clone()).unwrap(),
            &request
        )
        .is_ok());
        for revision in [0_u64, 7, 9, 9_007_199_254_740_991] {
            let mut wrong_revision = response.clone();
            wrong_revision["contentRevision"] = json!(revision);
            assert!(crate::wire::validate_native_response(
                &serde_json::from_value(wrong_revision).unwrap(),
                &request
            )
            .is_err());
        }
        for key in ["requestId", "instanceId", "documentId"] {
            let mut foreign = response.clone();
            foreign[key] = json!("foreign");
            assert!(crate::wire::validate_native_response(
                &serde_json::from_value(foreign).unwrap(),
                &request
            )
            .is_err());
        }
        let mut mismatch = request.payload.clone();
        mismatch["command"] = json!(if command == "object.undo" {
            "object.redo"
        } else {
            "object.undo"
        });
        assert!(!crate::native_contract::validate_result(
            &request.operation,
            &request.document_id,
            &mismatch,
            &good
        ));
    }
}
