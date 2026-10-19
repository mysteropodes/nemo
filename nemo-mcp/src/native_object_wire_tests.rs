use super::*;
use serde_json::{json, Value};

fn pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut request = cases["read"]["request"].clone();
    request["payload"]["atRevision"] = json!(0);
    let response = json!({"apiVersion":2,"requestId":request["requestId"],
        "instanceId":request["instanceId"],"documentId":request["documentId"],
        "contentRevision":0,"ok":true,"result":{"atRevision":0,
        "documentSnapshotId":format!("native-object:{}:0",request["documentId"].as_str().unwrap()),
        "object":cases["records"][0]}});
    (
        serde_json::from_value(request).unwrap(),
        serde_json::from_value(response).unwrap(),
    )
}

#[tokio::test]
async fn authenticated_native_envelopes_require_named_objects_at_both_levels() {
    async fn accepts(value: Value) -> bool {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        read_json::<AuthenticatedWireRequest>(bytes.as_slice())
            .await
            .is_ok()
    }
    let (mut request, _) = pair();
    request.operation = "command.document.object.layer.visibility.set".into();
    request.payload =
        json!({"command":"object.layer.visibility.set","layerUid":"l","visible":false});
    request.expected_revision = Some(0);
    assert!(request.validate().is_ok());
    let native = serde_json::to_value(&request).unwrap();
    let positional = json!([
        request.api_version,
        request.request_id,
        request.instance_id,
        request.document_id,
        0,
        request.operation,
        request.payload,
        false
    ]);
    assert!(serde_json::from_value::<NativeApplicationRequest>(positional.clone()).is_ok());
    assert!(accepts(json!({"secret":"test","nativeRequest":native.clone()})).await);
    assert!(!accepts(json!(["test", native])).await);
    assert!(!accepts(json!({"secret":"test","nativeRequest":positional.clone()})).await);
    assert!(!accepts(json!(["test", positional])).await);
    let status = json!({"apiVersion":2,"requestId":"status-1","instanceId":"instance"});
    let status_seq = json!([2, "status-1", "instance"]);
    assert!(serde_json::from_value::<NativeStatusRequest>(status_seq.clone()).is_ok());
    assert!(accepts(json!({"secret":"test","nativeStatus":status.clone()})).await);
    assert!(!accepts(json!(["test", status])).await);
    assert!(!accepts(json!({"secret":"test","nativeStatus":status_seq})).await);
}

#[test]
fn object_response_requires_the_complete_correlated_immutable_result() {
    let (request, response) = pair();
    assert!(validate_native_response(&response, &request).is_ok());
    let mut later = response.clone();
    later.content_revision = 1;
    assert!(validate_native_response(&later, &request).is_err());
    let mut wrong = response.clone();
    wrong.result.as_mut().unwrap()["object"]["target"]["strokeId"] = json!("replacement");
    assert!(validate_native_response(&wrong, &request).is_err());
    let mut extra = response.clone();
    extra.result.as_mut().unwrap()["truncated"] = json!(true);
    assert!(validate_native_response(&extra, &request).is_err());
    let mut missing = response;
    missing
        .result
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("object");
    assert!(validate_native_response(&missing, &request).is_err());
}

#[tokio::test]
async fn available_object_call_uses_authenticated_native_endpoint() {
    let (request, response) = pair();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = Endpoint {
        instance_id: request.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "test".into(),
        build_id: "test".into(),
    };
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let message: NativeWireRequest = read_json(&mut stream).await.unwrap();
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(message.native_request, request);
        write_json(&mut stream, &response).await.unwrap();
    };
    let client = call_native(&endpoint, request.clone(), CancellationToken::new());
    let (_, actual) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    assert_eq!(actual.unwrap(), response);
}

#[tokio::test]
async fn object_reader_preserves_complete_bytes_and_rejects_oversized_envelopes() {
    let (request, response) = pair();
    let original = serde_json::to_vec(&response).unwrap();
    assert!(original.len() < NATIVE_MAX_MESSAGE_BYTES);
    let mut bytes = original;
    bytes.push(b'\n');
    let received: NativeApplicationResponse =
        read_json_bounded(bytes.as_slice(), NATIVE_MAX_MESSAGE_BYTES)
            .await
            .unwrap();
    assert_eq!(received, response);
    assert!(validate_native_response(&received, &request).is_ok());
    let mut too_large = response.clone();
    let segments = &mut too_large.result.as_mut().unwrap()["object"]["geometry"]["segments"];
    *segments = Value::Array(vec![segments[0].clone(); 256]);
    // Still a complete schema-valid record; encoded transport refuses it.
    assert!(validate_native_response(&too_large, &request).is_ok());
    let before = too_large.clone();
    let mut bytes = serde_json::to_vec(&too_large).unwrap();
    assert!(bytes.len() > NATIVE_MAX_MESSAGE_BYTES);
    bytes.push(b'\n');
    assert!(read_json_bounded::<NativeApplicationResponse>(
        bytes.as_slice(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .await
    .is_err());
    assert_eq!(too_large, before);
}

fn create_pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let (mut request, mut response) = pair();
    request.operation = "command.document.object.layer.create".into();
    request.expected_revision = Some(0);
    request.payload = json!({"command":"object.layer.create","name":"data:display name"});
    response.content_revision = 1;
    response.result =
        Some(json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"}));
    (request, response)
}

#[test]
fn creation_original_bytes_are_closed_bounded_named_and_duplicate_safe() {
    let (request, _) = create_pair();
    let raw = serde_json::to_string(&request).unwrap();
    let decode = |raw: &str| {
        decode_json_bounded::<NativeApplicationRequest>(raw.as_bytes(), NATIVE_MAX_MESSAGE_BYTES)
    };
    assert!(decode(&raw).unwrap().validate_input().is_ok());
    assert!(decode(&raw).unwrap().validate().is_ok());
    for invalid in [
        raw.replace("\"name\":", "\"name\":\"first\",\"name\":"),
        raw.replace("\"name\":", "\"na\\u006de\":\"first\",\"name\":"),
        raw.replace(
            "\"expectedRevision\":0",
            "\"expectedRevision\":0,\"expectedRevision\":0",
        ),
        raw.replace("\"command\":", "\"command\":\"first\",\"command\":"),
        raw.replace("\"payload\":", "\"unknown\":true,\"payload\":"),
    ] {
        assert!(decode(&invalid).is_err(), "{invalid}");
    }
    let mut positional = serde_json::to_value(&request).unwrap();
    positional["payload"] = json!(["object.layer.create", "name"]);
    assert!(decode(&positional.to_string())
        .unwrap()
        .validate_input()
        .is_err());
    for malformed in ["null", "[]", "{}", "{", "{} {}"] {
        assert!(decode(malformed).is_err());
    }
    let padding = " ".repeat(NATIVE_MAX_MESSAGE_BYTES - raw.len());
    assert!(decode(&format!("{raw}{padding}")).is_ok());
    assert!(decode(&format!("{raw}{padding} ")).is_err());
    let wire = json!({"secret":"test","nativeRequest":request}).to_string();
    assert!(decode_json_bounded::<NativeWireRequest>(wire.as_bytes(), wire.len()).is_ok());
    assert!(decode_json_bounded::<NativeWireRequest>(
        wire.replace("\"secret\":", "\"secret\":\"first\",\"secret\":")
            .as_bytes(),
        8192
    )
    .is_err());
}

#[test]
fn creation_receipt_correlates_outer_revision_and_keeps_actual_native_identity() {
    let (request, response) = create_pair();
    assert!(validate_native_response(&response, &request).is_ok());
    for revision in [0, 2, MAX_SAFE_REVISION + 1] {
        let mut invalid = response.clone();
        invalid.content_revision = revision;
        assert!(validate_native_response(&invalid, &request).is_err());
    }
    let mut missing_revision = request.clone();
    missing_revision.expected_revision = None;
    assert!(validate_native_response(&response, &missing_revision).is_err());
    let mut wrong = response.clone();
    wrong.result.as_mut().unwrap()["layerUid"] = json!("");
    assert!(validate_native_response(&wrong, &request).is_err());
    let raw = serde_json::to_string(&response).unwrap();
    let duplicate = raw.replace("\"layerUid\":", "\"layerUid\":\"wrong\",\"layerUid\":");
    assert!(decode_json_bounded::<NativeApplicationResponse>(duplicate.as_bytes(), 4096).is_err());
}

#[tokio::test]
async fn available_creation_uses_the_same_authenticated_tcp_receipt_route() {
    let (request, response) = create_pair();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = Endpoint {
        instance_id: request.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "test".into(),
        build_id: "test".into(),
    };
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let message: NativeWireRequest = read_json(&mut stream).await.unwrap();
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(message.native_request, request);
        write_json(&mut stream, &response).await.unwrap();
    };
    let client = call_native(&endpoint, request.clone(), CancellationToken::new());
    let (_, actual) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    assert_eq!(actual.unwrap(), response);
}

fn timing_pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let declaration: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-layer-timing.json"
    ))
    .unwrap();
    (
        serde_json::from_value(declaration["examples"][0]["request"].clone()).unwrap(),
        serde_json::from_value(declaration["examples"][0]["response"].clone()).unwrap(),
    )
}
#[test]
fn timing_wire_retains_full_owner_snapshot_and_selector_correlation() {
    let (request, response) = timing_pair();
    assert!(request.validate().is_ok());
    assert!(validate_native_response(&response, &request).is_ok());
    for (key, value) in [
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("other")),
        ("contextId", json!("other")),
        ("layerUid", json!("other")),
        ("frame", json!(1)),
        ("sourceFrame", json!(null)),
        ("manualEdit", json!(true)),
        ("extra", json!(true)),
    ] {
        let mut forged = response.clone();
        forged.result.as_mut().unwrap()[key] = value;
        assert!(
            validate_native_response(&forged, &request).is_err(),
            "{key}"
        );
    }
    let mut forged = response.clone();
    forged.content_revision = 1;
    assert!(validate_native_response(&forged, &request).is_err());
    let mut forged = response;
    forged.instance_id = "other".into();
    assert!(validate_native_response(&forged, &request).is_err());
}
#[test]
fn timing_wire_original_byte_admission_rejects_duplicate_unknown_and_positional_shapes() {
    let (request, _) = timing_pair();
    let raw = serde_json::to_string(&request).unwrap();
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(raw.as_bytes(), 4096)
            .unwrap()
            .validate()
            .is_ok()
    );
    for bad in [
        raw.replace("\"frame\":0", "\"frame\":0,\"frame\":0"),
        raw.replace("\"frame\":0", "\"fr\\u0061me\":0,\"frame\":0"),
        raw.replace("\"payload\":", "\"extra\":true,\"payload\":"),
        raw.replace("\"payload\":", "\"expectedRevision\":null,\"payload\":"),
        "[2,\"request\",\"instance\",\"document\"]".into(),
    ] {
        assert_ne!(bad, raw);
        assert!(decode_json_bounded::<NativeApplicationRequest>(bad.as_bytes(), 4096).is_err());
    }
}
#[tokio::test]
async fn timing_wire_full_utf8_response_limit_is_inclusive_and_never_truncated() {
    let (mut request, _) = timing_pair();
    let mut uid = "data:雪".to_string();
    let response = loop {
        request.payload["layerUid"] = json!(uid);
        let (_, mut response) = timing_pair();
        response.result.as_mut().unwrap()["layerUid"] = json!(uid);
        let bytes = serde_json::to_vec(&response).unwrap();
        if bytes.len() == 4096 {
            break response;
        }
        assert!(bytes.len() < 4096);
        uid.push('x');
    };
    assert!(request.validate().is_ok());
    assert!(validate_native_response(&response, &request).is_ok());
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    let parsed: NativeApplicationResponse =
        read_json_bounded(bytes.as_slice(), 4096).await.unwrap();
    assert_eq!(parsed, response);
    let mut oversized = response;
    oversized.result.as_mut().unwrap()["layerUid"] = json!(uid + "x");
    let mut bytes = serde_json::to_vec(&oversized).unwrap();
    bytes.push(b'\n');
    assert!(
        read_json_bounded::<NativeApplicationResponse>(bytes.as_slice(), 4096)
            .await
            .is_err()
    );
    while serde_json::to_vec(&request).unwrap().len() < 4096 {
        let mut uid = request.payload["layerUid"].as_str().unwrap().to_owned();
        uid.push('x');
        request.payload["layerUid"] = json!(uid);
    }
    assert_eq!(serde_json::to_vec(&request).unwrap().len(), 4096);
    assert!(request.validate().is_ok());
    let oversized_uid = request.payload["layerUid"].as_str().unwrap().to_owned() + "x";
    request.payload["layerUid"] = json!(oversized_uid);
    assert!(request.validate().is_err());
}

#[tokio::test]
async fn malformed_timing_selector_refuses_before_the_mcp_wire_connects() {
    let (mut request, _) = timing_pair();
    request.payload["frame"] = Value::Null;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: request.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "invalid-timing".into(),
        build_id: "invalid-timing".into(),
    };
    assert!(call_native(&endpoint, request, CancellationToken::new())
        .await
        .is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn an_object_edit_receipt_is_correlated_and_a_flawed_one_is_refused_as_an_invalid_response() {
    let request: NativeApplicationRequest = serde_json::from_value(json!({
        "apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
        "expectedRevision":7,"operation":"command.document.object.layer.rename",
        "payload":{"command":"object.layer.rename","layerUid":"layer-b","name":"Ink"}}))
    .unwrap();
    let reply = |revision: u64, result: Value| -> NativeApplicationResponse {
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance-A",
            "documentId":"document-A","contentRevision":revision,"ok":true,"result":result}),
        )
        .unwrap()
    };
    let applied = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(validate_native_response(&reply(8, applied.clone()), &request).is_ok());
    assert!(validate_native_response(&reply(7, noop.clone()), &request).is_ok());
    for refused in [
        // Not the compact command result of a rename.
        reply(
            8,
            json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"}),
        ),
        reply(8, json!({"applied":true,"historyEntriesAdded":0})),
        // The revision does not advance by exactly the applied edit.
        reply(7, applied.clone()),
        reply(9, applied),
        reply(8, noop),
    ] {
        let error = validate_native_response(&refused, &request).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "invalid native application response");
    }
}

#[test]
fn repeat_wire_accepts_actual_catalog_counts_and_rejects_shape_revision_or_identity_drift() {
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog
        .capability_for_operation(crate::native_contract::frame_edit::REPEAT)
        .unwrap();
    let example = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|example| {
            example["request"]["operation"] == crate::native_contract::frame_edit::REPEAT
        })
        .unwrap();
    let request: NativeApplicationRequest =
        serde_json::from_value(example["request"].clone()).unwrap();
    let response: NativeApplicationResponse =
        serde_json::from_value(example["response"].clone()).unwrap();
    assert!(request.validate_input().is_ok());
    assert!(validate_native_response(&response, &request).is_ok());
    let mut growth = response.clone();
    growth.result.as_mut().unwrap()["cycledLayers"] = json!(0);
    growth.result.as_mut().unwrap()["skippedComponentLayers"] = json!(3);
    assert!(validate_native_response(&growth, &request).is_ok());
    let mut noop = growth.clone();
    noop.content_revision = request.expected_revision.unwrap();
    noop.result.as_mut().unwrap()["applied"] = json!(false);
    noop.result.as_mut().unwrap()["historyEntriesAdded"] = json!(0);
    assert!(validate_native_response(&noop, &request).is_ok());
    for mut bad in [response.clone(), noop.clone()] {
        bad.content_revision += 1;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for field in ["cycledLayers", "skippedComponentLayers"] {
        let mut missing = response.clone();
        missing
            .result
            .as_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(validate_native_response(&missing, &request).is_err());
        for bad in [json!(-1), json!(1.5), json!(4294967296u64), json!(null)] {
            let mut wrong = response.clone();
            wrong.result.as_mut().unwrap()[field] = bad;
            assert!(validate_native_response(&wrong, &request).is_err());
        }
    }
    let mut extra = response.clone();
    extra.result.as_mut().unwrap()["clipboard"] = json!(null);
    assert!(validate_native_response(&extra, &request).is_err());
    for field in ["requestId", "instanceId", "documentId"] {
        let mut wrong = serde_json::to_value(&response).unwrap();
        wrong[field] = json!("foreign");
        let wrong: NativeApplicationResponse = serde_json::from_value(wrong).unwrap();
        assert!(validate_native_response(&wrong, &request).is_err());
    }
    let insert = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|example| {
            example["request"]["operation"] == crate::native_contract::frame_edit::INSERT
        })
        .unwrap();
    let insert_request: NativeApplicationRequest =
        serde_json::from_value(insert["request"].clone()).unwrap();
    let mut insert_response: NativeApplicationResponse =
        serde_json::from_value(insert["response"].clone()).unwrap();
    assert!(validate_native_response(&insert_response, &insert_request).is_ok());
    insert_response.result = response.result;
    assert!(validate_native_response(&insert_response, &insert_request).is_err());
}

#[cfg(test)]
#[path = "native_frame_edit_blank_wire_tests.rs"]
mod blank_key;
