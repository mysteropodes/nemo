//! PRIVATE actual common validator cases, authored UNRUN.
use super::*;
use serde_json::json;
fn import() -> Value {
    json!({"command":"object.asset.font.import","documentSnapshotId":"snapshot-0","lifecycleGeneration":1,
    "source":{"kind":"google","family":"  Roboto  ","weight":700,"italic":true}})
}
fn query(action: &str) -> Value {
    json!({"action":action,"atRevision":1,"documentSnapshotId":"snapshot-1","assetId":"native-font-one"})
}
fn read_result() -> Value {
    json!({"atRevision":1,"documentSnapshotId":"snapshot-1","assetId":"native-font-one","offset":0,
    "returnedLength":3,"totalLength":44724,"encodedSha256":"a".repeat(64),"base64":"QUJD"})
}
fn read_request() -> Value {
    let mut v = query("read");
    v["offset"] = 0.into();
    v["length"] = 1536.into();
    v
}
fn envelope(operation: &str, payload: Value, expected: Option<u64>) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "original".into(),
        instance_id: "font-instance".into(),
        document_id: "font-document".into(),
        expected_revision: expected,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}

#[test]
fn common_import_admits_only_original_scalar_source_descriptors() {
    assert!(request(IMPORT, &import()));
    for key in ["assetId", "url", "bytes", "base64", "path"] {
        let mut value = import();
        value["source"][key] = "caller".into();
        assert!(!request(IMPORT, &value));
    }
    for source in [
        json!({"kind":"google","family":" ","weight":400,"italic":false}),
        json!({"kind":"google","family":"A","weight":1001,"italic":false}),
        json!({"kind":"bundled","fontKey":"../Roboto.ttf"}),
    ] {
        let mut value = import();
        value["source"] = source;
        assert!(!request(IMPORT, &value));
    }
}
#[test]
fn common_font_revision_policies_and_original_byte_bounds_are_enforced() {
    assert!(envelope(IMPORT, import(), Some(0)).validate().is_ok());
    assert!(envelope(IMPORT, import(), None).validate().is_err());
    assert!(envelope(QUERY, query("inspect"), None).validate().is_ok());
    assert!(envelope(QUERY, query("inspect"), Some(1))
        .validate()
        .is_err());
    let mut value = import();
    value["source"]["family"] = "x".repeat(4096).into();
    assert!(envelope(IMPORT, value, Some(0)).validate().is_err());
}
#[test]
fn common_raw_decoder_refuses_nested_duplicate_source_members_before_typing() {
    let request = r#"{"apiVersion":2,"requestId":"dup","instanceId":"I","documentId":"D","expectedRevision":0,"operation":"command.document.object.asset.font.import","payload":{"command":"object.asset.font.import","documentSnapshotId":"S","lifecycleGeneration":1,"source":{"kind":"google","family":"A","family":"B","weight":400,"italic":false}}}"#;
    assert!(
        crate::wire::decode_json_bounded::<NativeApplicationRequest>(request.as_bytes(), 4096)
            .is_err()
    );
}
#[test]
fn exact_opaque_ids_are_preserved_without_generic_request_id_narrowing() {
    let id = format!("font / {}", "\u{1}".repeat(248));
    assert_eq!(id.len(), 255);
    let mut payload = query("inspect");
    payload["assetId"] = id.clone().into();
    let value = json!({"atRevision":1,"documentSnapshotId":"snapshot-1","font":{"assetId":id,"name":"  Exact  ","sourceKind":"linked","available":false,"reason":"linked_font_unavailable"}});
    assert!(request(QUERY, &payload));
    assert!(result(QUERY, &payload, &value));
    let mut bad = value.clone();
    bad["font"]["assetId"] = "normalized".into();
    assert!(!result(QUERY, &payload, &bad));
}
#[test]
fn byte_chunks_require_exact_pins_offsets_positive_count_and_canonical_base64() {
    let payload = read_request();
    assert!(result(QUERY, &payload, &read_result()));
    for (key, value) in [
        ("documentSnapshotId", json!("other")),
        ("assetId", json!("other")),
        ("offset", json!(1)),
        ("returnedLength", json!(0)),
        ("returnedLength", json!(1537)),
        ("base64", json!("QQ==")),
        ("base64", json!("QR==")),
        ("encodedSha256", json!("A".repeat(64))),
    ] {
        let mut changed = read_result();
        changed[key] = value;
        assert!(!result(QUERY, &payload, &changed), "{key}");
    }
    let mut short = read_result();
    short["returnedLength"] = 1.into();
    short["base64"] = "QQ==".into();
    assert!(result(QUERY, &payload, &short));
}
#[test]
fn linked_font_has_no_byte_or_resolver_result_variant() {
    let payload = query("inspect");
    let mut value = json!({"atRevision":1,"documentSnapshotId":"snapshot-1","font":{"assetId":"native-font-one","name":"Original","sourceKind":"linked","available":false,"reason":"linked_font_unavailable"}});
    assert!(result(QUERY, &payload, &value));
    value["font"]["locator"] = "file:///unread".into();
    assert!(!result(QUERY, &payload, &value));
}
#[test]
fn catalog_pagination_cannot_forge_rows_counts_or_next_offset() {
    let payload = json!({"action":"list","atRevision":1,"documentSnapshotId":"snapshot-1","offset":0,"limit":1});
    let row = json!({"assetId":"font-one","name":"One","sourceKind":"linked","available":false,"reason":"linked_font_unavailable"});
    let good = json!({"atRevision":1,"documentSnapshotId":"snapshot-1","offset":0,"nextOffset":1,"totalFontRecords":2,"rows":[row]});
    assert!(result(QUERY, &payload, &good));
    for (key, value) in [
        ("nextOffset", Value::Null),
        ("totalFontRecords", json!(0)),
        ("rows", json!([])),
        ("offset", json!(1)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(!result(QUERY, &payload, &bad));
    }
}
#[test]
fn receipt_results_are_correlated_and_missing_does_not_encode_refusal() {
    let payload = json!({"action":"receipt","forRequestId":"original"});
    let missing = json!({"status":"not_found","forRequestId":"original"});
    assert!(result(QUERY, &payload, &missing));
    let refused = json!({"status":"refused","forRequestId":"original","requestFingerprintSha256":"b".repeat(64),"contentRevision":1,"code":"unavailable"});
    assert!(result(QUERY, &payload, &refused));
    let mut bad = refused.clone();
    bad["forRequestId"] = "other".into();
    assert!(!result(QUERY, &payload, &bad));
    bad = missing;
    bad["code"] = "unavailable".into();
    assert!(!result(QUERY, &payload, &bad));
}
#[test]
fn font_outer_response_identity_and_exact_content_revision_are_bound() {
    let request = envelope(QUERY, read_request(), None);
    let response = NativeApplicationResponse {
        api_version: 2,
        request_id: "original".into(),
        instance_id: "font-instance".into(),
        document_id: "font-document".into(),
        content_revision: 1,
        ok: true,
        result: Some(read_result()),
        error: None,
    };
    assert!(response_matches(&request, &response));
    let mut bad = response.clone();
    bad.content_revision = 2;
    assert!(!response_matches(&request, &bad));
    bad = response;
    bad.document_id = "other".into();
    assert!(!response_matches(&request, &bad));
}
