//! Authenticated native wire and complete operation-aware qualification for the real bar operation.
use super::*;
use crate::native_contract::frame_edit::RANGE_DRAG;
fn pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let catalog = crate::capabilities::native_catalog();
    let d = catalog.capability_for_operation(RANGE_DRAG).unwrap();
    let e = d["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["request"]["operation"] == RANGE_DRAG)
        .unwrap();
    (
        serde_json::from_value(e["request"].clone()).unwrap(),
        serde_json::from_value(e["response"].clone()).unwrap(),
    )
}
#[test]
fn bar_wire_requires_correlated_owner_exact_checkpoint_and_next_revision_including_unmoved_click() {
    let (q, r) = pair();
    assert!(q.validate().is_ok());
    assert!(validate_native_response(&r, &q).is_ok());
    for field in ["requestId", "instanceId", "documentId"] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[field] = json!("foreign");
        assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err());
    }
    for value in [
        json!({"applied":false,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
    ] {
        let mut bad = r.clone();
        bad.result = Some(value);
        assert!(validate_native_response(&bad, &q).is_err());
    }
    let mut bad = r.clone();
    bad.content_revision = 7;
    assert!(validate_native_response(&bad, &q).is_err());
    let raw = serde_json::to_string(&r).unwrap();
    let duplicate = raw.replace("\"applied\":", "\"applied\":true,\"applied\":");
    assert_ne!(duplicate, raw);
    assert!(decode_json_bounded::<NativeApplicationResponse>(duplicate.as_bytes(), 4096).is_err());
}
#[tokio::test]
async fn bar_uses_existing_authenticated_endpoint_without_a_new_queue_or_adapter_route() {
    let (q, r) = pair();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = Endpoint {
        instance_id: q.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "bar-wire".into(),
        build_id: "bar-wire".into(),
    };
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let actual: NativeWireRequest = read_json(&mut stream).await.unwrap();
        assert_eq!(actual.secret, endpoint.secret);
        assert_eq!(actual.native_request, q);
        write_json(&mut stream, &r).await.unwrap();
    };
    let client = call_native(&endpoint, q.clone(), CancellationToken::new());
    let (_, actual) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    assert_eq!(actual.unwrap(), r);
}
#[tokio::test]
async fn malformed_or_oversized_gesture_is_refused_before_connection_without_trimming_samples() {
    for oversized in [false, true] {
        let (mut q, _) = pair();
        q.payload["samples"] = if oversized {
            json!(vec![0; 4096])
        } else {
            json!([0.5])
        };
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: q.instance_id.clone(),
            port: listener.local_addr().unwrap().port(),
            secret: "never".into(),
            build_id: "test".into(),
        };
        assert!(call_native(&endpoint, q, CancellationToken::new())
            .await
            .is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
}
