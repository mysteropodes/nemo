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

#[test]
fn fixed_authored_and_reference_input_does_not_authorize_object_dispatch() {
    for index in [0, 1] {
        let request = object_request(index);
        let before = serde_json::to_value(&request).unwrap();
        assert!(request.validate_input().is_ok());
        let error = request.validate().unwrap_err();
        assert_eq!(error.code(), "unavailable");
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation("query.document.object")
            .unwrap();
        assert_eq!(descriptor["availability"]["state"], "unavailable");
        assert_eq!(error.message(), descriptor["availability"]["reason"]);
        assert_eq!(serde_json::to_value(&request).unwrap(), before);
        let mut absent_target = request.clone();
        absent_target.payload["stableTarget"]["strokeId"] = json!("not-in-a-document");
        // Existence belongs to the actual owner, not this pure input check.
        assert!(absent_target.validate_input().is_ok());
        assert_eq!(absent_target.validate().unwrap_err().code(), "unavailable");
    }
}

#[test]
fn every_registered_available_example_retains_full_validation() {
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let request: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            assert!(request.validate_input().is_ok(), "{}", descriptor["id"]);
            if descriptor["id"] == "native.object" {
                assert_eq!(request.validate().unwrap_err().code(), "unavailable");
            } else {
                assert!(request.validate().is_ok(), "{}", descriptor["id"]);
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
    assert_eq!(cancelled.validate().unwrap_err().code(), "unavailable");
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
