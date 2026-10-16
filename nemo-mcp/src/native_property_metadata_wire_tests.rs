//! The common original-byte decoder rejects malformed metadata wire requests.
use super::*;
use serde_json::json;
fn pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let request=serde_json::from_value(json!({"apiVersion":2,"requestId":"metadata-1","instanceId":"instance","documentId":"doc",
        "operation":"query.document.object.property.metadata","payload":{"atRevision":0,"documentSnapshotId":"native-object:doc:0","view":{"kind":"targets","offset":0,"limit":2}}})).unwrap();
    let categories = [
        "conditionalMotionRows",
        "pathVerticesTrimStrokeBrushMesh",
        "referenceOnlyElements",
        "expressionDiagnostics",
        "layerEffectMatteReveal",
        "propertyWrites",
    ];
    let response=serde_json::from_value(json!({"apiVersion":2,"requestId":"metadata-1","instanceId":"instance","documentId":"doc","contentRevision":0,"ok":true,
        "result":{"instanceId":"instance","documentId":"doc","atRevision":0,"documentSnapshotId":"native-object:doc:0","kind":"targets","target":null,
        "items":[{"kind":"context","contextId":"data:opaque 雪"}],"nextOffset":1,
        "unavailableCategories":categories.map(|category|json!({"category":category,"state":"unavailable","reason":"not-admitted"}))}})).unwrap();
    (request, response)
}
#[test]
fn metadata_original_bytes_reject_recursive_decoded_duplicates_positional_and_oversize() {
    let (request, _) = pair();
    let raw = serde_json::to_string(&request).unwrap();
    let decode = |raw: &str| {
        decode_json_bounded::<NativeApplicationRequest>(raw.as_bytes(), NATIVE_MAX_MESSAGE_BYTES)
    };
    assert!(decode(&raw).unwrap().validate_input().is_ok());
    for bad in [
        raw.replace("\"offset\":0", "\"offset\":0,\"offset\":1"),
        raw.replace("\"offset\":0", "\"offset\":0,\"off\\u0073et\":1"),
        raw.replace("\"atRevision\":0", "\"atRevision\":0,\"atRevision\":1"),
        raw.replace(
            "\"documentId\":\"doc\"",
            "\"documentId\":\"doc\",\"documentId\":\"other\"",
        ),
        "[2,\"metadata-1\",\"instance\",\"doc\"]".into(),
        format!("{}{}", raw, " ".repeat(4096)),
    ] {
        assert!(decode(&bad).is_err(), "{bad}");
    }
}
#[tokio::test]
async fn metadata_authenticated_outer_duplicates_and_positional_native_request_refuse() {
    let (request, _) = pair();
    let raw = serde_json::to_string(&request).unwrap();
    let valid = format!("{{\"secret\":\"test\",\"nativeRequest\":{raw}}}\n");
    assert!(read_json::<AuthenticatedWireRequest>(valid.as_bytes())
        .await
        .is_ok());
    for bad in [
        format!("{{\"secret\":\"test\",\"nativeRequest\":{raw},\"native\\u0052equest\":{raw}}}\n"),
        format!("[\"test\",{raw}]\n"),
        "{\"secret\":\"test\",\"nativeRequest\":[]}\n".into(),
    ] {
        assert!(read_json::<AuthenticatedWireRequest>(bad.as_bytes())
            .await
            .is_err());
    }
}
#[test]
fn metadata_response_requires_original_request_and_complete_page_identity() {
    let (request, response) = pair();
    assert!(validate_native_response(&response, &request).is_ok());
    for (key, value) in [
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("other")),
        ("nextOffset", json!(0)),
        ("truncated", json!(true)),
    ] {
        let mut bad = response.clone();
        bad.result.as_mut().unwrap()[key] = value;
        assert!(validate_native_response(&bad, &request).is_err(), "{key}");
    }
    let mut bad = response.clone();
    bad.request_id = "other".into();
    assert!(validate_native_response(&bad, &request).is_err());
    let mut bad = response.clone();
    bad.result.as_mut().unwrap()["items"][0]["contextId"] = json!("雪".repeat(1400));
    assert!(validate_native_response(&bad, &request).is_err());
}
