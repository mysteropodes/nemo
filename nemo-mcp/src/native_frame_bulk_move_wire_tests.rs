//! Existing authenticated wire/operation validator for ordinary Move; not native execution.
use super::*;
use crate::native_contract::frame_edit::MOVE;
fn move_pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(MOVE).unwrap();
    let example = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["request"]["operation"] == MOVE)
        .unwrap();
    (
        serde_json::from_value(example["request"].clone()).unwrap(),
        serde_json::from_value(example["response"].clone()).unwrap(),
    )
}
#[test]
fn move_wire_qualifies_actual_catalog_and_closed_receipt_owner_revision() {
    let (request, response) = move_pair();
    assert!(request.validate_input().is_ok());
    assert!(request.validate().is_ok());
    assert!(validate_native_response(&response, &request).is_ok());
    for field in ["requestId", "instanceId", "documentId"] {
        let mut bad = serde_json::to_value(&response).unwrap();
        bad[field] = json!("foreign");
        assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &request).is_err());
    }
    for value in [
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":0,"selection":[]}),
        json!({"applied":true,"historyEntriesAdded":1,"selection":null}),
        json!({"applied":false,"historyEntriesAdded":0,"selection":[]}),
        json!({"applied":true,"historyEntriesAdded":1,"selection":[{"layerUid":"a","frame":0,"extra":1}]}),
    ] {
        let mut bad = response.clone();
        bad.result = Some(value);
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for revision in [7, 9, MAX_SAFE_REVISION + 1] {
        let mut bad = response.clone();
        bad.content_revision = revision;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    let mut noop = response.clone();
    noop.content_revision = 7;
    noop.result = Some(json!({"applied":false,"historyEntriesAdded":0,"selection":null}));
    assert!(validate_native_response(&noop, &request).is_ok());
    let mut empty = response.clone();
    empty.result = Some(json!({"applied":true,"historyEntriesAdded":1,"selection":[]}));
    assert!(validate_native_response(&empty, &request).is_ok());
}
#[test]
fn move_wire_duplicate_nested_cells_and_top_result_members_are_rejected_from_original_bytes() {
    let (_, response) = move_pair();
    let raw = serde_json::to_string(&response).unwrap();
    for bad in [
        raw.replace("\"selection\":", "\"selection\":[],\"selection\":"),
        raw.replace("\"layerUid\":", "\"layerUid\":\"duplicate\",\"layerUid\":"),
        raw.replace("\"applied\":", "\"applied\":true,\"applied\":"),
    ] {
        assert_ne!(bad, raw);
        assert!(decode_json_bounded::<NativeApplicationResponse>(bad.as_bytes(), 4096).is_err());
    }
}
#[tokio::test]
async fn move_wire_keeps_exact_complete_4096_byte_response_boundary() {
    let (request, mut response) = move_pair();
    let size = serde_json::to_vec(&response).unwrap().len();
    // Opaque UID padding tests the public wire shell, not actual native layer admission.
    let uid = response.result.as_ref().unwrap()["selection"][0]["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    response.result.as_mut().unwrap()["selection"][0]["layerUid"] =
        json!(uid + &"x".repeat(4096 - size));
    assert_eq!(serde_json::to_vec(&response).unwrap().len(), 4096);
    assert!(validate_native_response(&response, &request).is_ok());
    let mut raw = serde_json::to_vec(&response).unwrap();
    raw.push(b'\n');
    let received: NativeApplicationResponse =
        read_json_bounded(raw.as_slice(), 4096).await.unwrap();
    assert_eq!(received, response);
    let uid = response.result.as_ref().unwrap()["selection"][0]["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    response.result.as_mut().unwrap()["selection"][0]["layerUid"] = json!(uid + "x");
    let mut raw = serde_json::to_vec(&response).unwrap();
    assert_eq!(raw.len(), 4097);
    raw.push(b'\n');
    assert!(
        read_json_bounded::<NativeApplicationResponse>(raw.as_slice(), 4096)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn move_catalog_uses_existing_authenticated_endpoint_without_alternate_route() {
    let (request, response) = move_pair();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = Endpoint {
        instance_id: request.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "move-test".into(),
        build_id: "move-test".into(),
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
#[tokio::test]
async fn malformed_move_refuses_before_any_native_connection() {
    let (mut request, _) = move_pair();
    request.payload["selection"] = Value::Null;
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
