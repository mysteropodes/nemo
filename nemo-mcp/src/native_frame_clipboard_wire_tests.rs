// Exercise the actual catalog and authenticated wire validator. Native I/O here is
// a real bounded loopback connection, not native document execution or installed UI.
use super::*;
use crate::native_contract::frame_edit::{CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE};

fn clipboard_pair(operation: &str) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(operation).unwrap();
    let example = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|example| example["request"]["operation"] == operation)
        .unwrap();
    (
        serde_json::from_value(example["request"].clone()).unwrap(),
        serde_json::from_value(example["response"].clone()).unwrap(),
    )
}
fn empty_packet(request: &NativeApplicationRequest) -> Value {
    json!({"instanceId":request.instance_id,"documentId":request.document_id,"cells":[
        {"relLayer":0,"relFrame":0,"keyframe":true,"records":[],"orderElements":[]}]})
}
fn clipboard_reply(
    request: &NativeApplicationRequest,
    applied: bool,
    packet: Value,
) -> NativeApplicationResponse {
    serde_json::from_value(json!({"apiVersion":2,"requestId":request.request_id,
        "instanceId":request.instance_id,"documentId":request.document_id,
        "contentRevision":request.expected_revision.unwrap() + u64::from(applied),"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":u64::from(applied),"clipboard":packet}}))
    .unwrap()
}

#[test]
fn clipboard_wire_accepts_each_actual_catalog_example_and_closed_operation_result() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let (request, response) = clipboard_pair(operation);
        assert!(request.validate_input().is_ok(), "{operation}");
        assert!(request.validate().is_ok(), "{operation}");
        assert!(
            validate_native_response(&response, &request).is_ok(),
            "{operation}"
        );
        if operation == CLIPBOARD_PASTE {
            assert_eq!(
                response.result.as_ref().unwrap().as_object().unwrap().len(),
                2
            );
        } else {
            assert_eq!(
                response.result.as_ref().unwrap().as_object().unwrap().len(),
                3
            );
        }
        for field in ["requestId", "instanceId", "documentId"] {
            let mut forged = serde_json::to_value(&response).unwrap();
            forged[field] = json!("foreign");
            let forged: NativeApplicationResponse = serde_json::from_value(forged).unwrap();
            assert!(
                validate_native_response(&forged, &request).is_err(),
                "outer {field}"
            );
        }
        for revision in [response.content_revision + 1, MAX_SAFE_REVISION + 1] {
            let mut forged = response.clone();
            forged.content_revision = revision;
            assert!(validate_native_response(&forged, &request).is_err());
        }
        let mut missing = request.clone();
        missing.expected_revision = None;
        assert!(validate_native_response(&response, &missing).is_err());
        let mut extra = response.clone();
        extra.result.as_mut().unwrap()["extra"] = json!(true);
        assert!(validate_native_response(&extra, &request).is_err());
    }
}

#[test]
fn clipboard_wire_checks_inner_owner_even_when_outer_identity_and_revision_match() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT] {
        let (request, _) = clipboard_pair(operation);
        let good = clipboard_reply(&request, false, empty_packet(&request));
        assert!(validate_native_response(&good, &request).is_ok());
        for field in ["instanceId", "documentId"] {
            let mut forged = good.clone();
            forged.result.as_mut().unwrap()["clipboard"][field] = json!("foreign");
            assert_eq!(forged.instance_id, request.instance_id);
            assert_eq!(forged.document_id, request.document_id);
            assert_eq!(forged.content_revision, request.expected_revision.unwrap());
            assert!(
                validate_native_response(&forged, &request).is_err(),
                "inner {field}"
            );
        }
        let null = clipboard_reply(&request, false, Value::Null);
        assert!(validate_native_response(&null, &request).is_ok());
        assert!(
            validate_native_response(&clipboard_reply(&request, true, Value::Null), &request)
                .is_err()
        );
        let changed = clipboard_reply(&request, true, empty_packet(&request));
        assert_eq!(
            validate_native_response(&changed, &request).is_ok(),
            operation == CLIPBOARD_CUT
        );
        let mut wrong_history = good.clone();
        wrong_history.result.as_mut().unwrap()["historyEntriesAdded"] = json!(1);
        assert!(validate_native_response(&wrong_history, &request).is_err());
        for result in [
            json!({"applied":true,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":0,"skippedComponentLayers":0}),
        ] {
            let mut wrong = good.clone();
            wrong.result = Some(result);
            assert!(validate_native_response(&wrong, &request).is_err());
        }
    }
    let (paste, mut response) = clipboard_pair(CLIPBOARD_PASTE);
    response.result =
        Some(json!({"applied":true,"historyEntriesAdded":1,"clipboard":empty_packet(&paste)}));
    assert!(validate_native_response(&response, &paste).is_err());
}

#[test]
fn clipboard_raw_request_and_response_reject_duplicate_unknown_and_positional_bytes() {
    let (mut request, _) = clipboard_pair(CLIPBOARD_PASTE);
    request.payload["cells"] = empty_packet(&request)["cells"].clone();
    let raw = serde_json::to_string(&request).unwrap();
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(raw.as_bytes(), 4096)
            .unwrap()
            .validate_input()
            .is_ok()
    );
    for bad in [
        raw.replace("\"keyframe\":", "\"keyframe\":false,\"keyframe\":"),
        raw.replace("\"relFrame\":", "\"relFr\\u0061me\":0,\"relFrame\":"),
        raw.replace(
            "\"orderElements\":",
            "\"orderElements\":[],\"orderElements\":",
        ),
        raw.replace(
            "\"expectedRevision\":",
            "\"expectedRevision\":7,\"expectedRevision\":",
        ),
        raw.replace("\"payload\":", "\"unknown\":true,\"payload\":"),
    ] {
        assert_ne!(bad, raw);
        assert!(decode_json_bounded::<NativeApplicationRequest>(bad.as_bytes(), 4096).is_err());
    }
    let mut positional = serde_json::to_value(&request).unwrap();
    positional["payload"]["cells"][0] = json!([0, 0, true, []]);
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        positional.to_string().as_bytes(),
        4096
    )
    .unwrap()
    .validate_input()
    .is_err());
    let (copy, _) = clipboard_pair(CLIPBOARD_COPY);
    let response = clipboard_reply(&copy, false, empty_packet(&copy));
    let raw = serde_json::to_string(&response).unwrap();
    for bad in [
        raw.replace("\"clipboard\":", "\"clipboard\":null,\"clipboard\":"),
        raw.replace(
            "\"historyEntriesAdded\":",
            "\"historyEntriesAdded\":0,\"historyEntriesAdded\":",
        ),
        raw.replace("\"cells\":", "\"cells\":[],\"cells\":"),
    ] {
        assert_ne!(bad, raw);
        assert!(decode_json_bounded::<NativeApplicationResponse>(bad.as_bytes(), 4096).is_err());
    }
}

#[tokio::test]
async fn clipboard_wire_preserves_4096_compatibility_and_an_explicit_4096_reader_stays_bounded() {
    let (request, _) = clipboard_pair(CLIPBOARD_COPY);
    let mut packet = empty_packet(&request);
    // An opaque object tests the public shell's byte boundary, not native record admission.
    packet["cells"][0]["records"] = json!([{"padding":""}]);
    let mut response = clipboard_reply(&request, false, packet);
    let length = serde_json::to_vec(&response).unwrap().len();
    let padding = "x".repeat(4096 - length);
    response.result.as_mut().unwrap()["clipboard"]["cells"][0]["records"][0]["padding"] =
        json!(padding.clone());
    assert_eq!(serde_json::to_vec(&response).unwrap().len(), 4096);
    assert!(validate_native_response(&response, &request).is_ok());
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    let received: NativeApplicationResponse =
        read_json_bounded(bytes.as_slice(), 4096).await.unwrap();
    assert_eq!(received, response);
    assert!(validate_native_response(&received, &request).is_ok());
    let original = response.clone();
    let value = response.result.as_mut().unwrap();
    value["clipboard"]["cells"][0]["records"][0]["padding"] = json!(padding + "x");
    assert_eq!(serde_json::to_vec(&response).unwrap().len(), 4097);
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    assert!(
        read_json_bounded::<NativeApplicationResponse>(bytes.as_slice(), 4096)
            .await
            .is_err()
    );
    assert_eq!(received, original);
}

#[tokio::test]
async fn clipboard_catalog_copy_cut_and_paste_use_existing_authenticated_tcp_route() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let (request, response) = clipboard_pair(operation);
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: request.instance_id.clone(),
            port: listener.local_addr().unwrap().port(),
            secret: "clipboard-test".into(),
            build_id: "clipboard-test".into(),
        };
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let received: NativeWireRequest = read_json(&mut stream).await.unwrap();
            assert_eq!(received.secret, endpoint.secret);
            assert_eq!(received.native_request, request);
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
}

#[tokio::test]
async fn clipboard_malformed_payload_refuses_before_any_native_endpoint_connection() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let (mut request, _) = clipboard_pair(operation);
        if operation == CLIPBOARD_PASTE {
            request.payload["cells"] = Value::Null;
        } else {
            request.payload["selection"] = Value::Null;
        }
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: request.instance_id.clone(),
            port: listener.local_addr().unwrap().port(),
            secret: "must-not-connect".into(),
            build_id: "test".into(),
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
}
