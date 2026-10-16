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

fn property_description() -> serde_json::Value {
    json!({"property":"opacity","dimension":1,"unit":"","defaultValue":[100],
        "animatable":true,"controlType":"slider","controlName":"Opacity",
        "authored":{"availability":"available","keyCount":0,"hasStaticOverride":false,"expressionEnabled":false},
        "availability":{"metadata":"available","applicability":"available","propertyWrite":"unavailable",
            "reason":"read-only-description-not-command-availability"}})
}
fn described_page(kind: &str) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let (mut request, mut response) = pair();
    let target = json!({"contextId":"scene-root","layerUid":"L"});
    let mut item = property_description();
    request.payload["view"] = if kind == "storedProperty" {
        item["availability"]["applicability"] = json!("unavailable");
        item["availability"]["reason"] = json!("stored-metadata-does-not-prove-applicability");
        json!({"kind":kind,"target":target,"property":"opacity"})
    } else {
        json!({"kind":kind,"target":target,"offset":0,"limit":2})
    };
    let result = response.result.as_mut().unwrap();
    result["kind"] = json!(kind);
    result["target"] = target;
    result["items"] = json!([item]);
    result["nextOffset"] = json!(null);
    (request, response)
}
#[test]
fn metadata_properties_and_stored_reads_preserve_authorship_and_applicability() {
    for kind in ["properties", "storedProperty"] {
        let (request, response) = described_page(kind);
        assert!(request.validate_input().is_ok());
        assert!(validate_native_response(&response, &request).is_ok());
        for (field, value) in [
            ("dimension", json!(0)),
            ("dimension", json!(5)),
            ("dimension", json!(null)),
            ("property", json!("")),
            ("unit", json!(false)),
            ("animatable", json!(0)),
            ("defaultValue", json!([1, 2])),
            ("defaultValue", json!(["1"])),
            ("controlType", json!(true)),
            ("controlName", json!(0)),
            ("authored", json!({"availability":"available","keyCount":0})),
            ("availability", json!({"metadata":"available"})),
            ("extra", json!(true)),
        ] {
            let mut bad = response.clone();
            bad.result.as_mut().unwrap()["items"][0][field] = value;
            assert!(
                validate_native_response(&bad, &request).is_err(),
                "{kind}: {field}"
            );
        }
        for (field, value) in [
            ("keyCount", json!(4_294_967_296u64)),
            ("hasStaticOverride", json!(null)),
            ("expressionEnabled", json!(0)),
            ("availability", json!("unknown")),
        ] {
            let mut bad = response.clone();
            bad.result.as_mut().unwrap()["items"][0]["authored"][field] = value;
            assert!(validate_native_response(&bad, &request).is_err());
        }
        for (field, value) in [
            ("metadata", json!("unavailable")),
            ("propertyWrite", json!("available")),
            ("applicability", json!("unknown")),
            ("reason", json!("wrong")),
        ] {
            let mut bad = response.clone();
            bad.result.as_mut().unwrap()["items"][0]["availability"][field] = value;
            assert!(validate_native_response(&bad, &request).is_err());
        }
        let mut null_metadata = response.clone();
        for field in ["defaultValue", "controlType", "controlName"] {
            null_metadata.result.as_mut().unwrap()["items"][0][field] = json!(null);
        }
        assert!(validate_native_response(&null_metadata, &request).is_ok());
    }
    let (request, mut response) = described_page("properties");
    response.result.as_mut().unwrap()["items"][0]["dimension"] = json!(4);
    response.result.as_mut().unwrap()["items"][0]["defaultValue"] = json!([0, 1, 2, 3]);
    assert!(validate_native_response(&response, &request).is_ok());
    for property in ["order", "timeRemap"] {
        let mut separate = response.clone();
        let item = &mut separate.result.as_mut().unwrap()["items"][0];
        item["property"] = json!(property);
        assert!(validate_native_response(&separate, &request).is_err());
        separate.result.as_mut().unwrap()["items"][0]["authored"] =
            json!({"availability":"unavailable","reason":"separate-property-owner"});
        assert!(validate_native_response(&separate, &request).is_ok());
        separate.result.as_mut().unwrap()["items"][0]["authored"]["reason"] = json!("other");
        assert!(validate_native_response(&separate, &request).is_err());
    }
}
#[test]
fn metadata_closed_selectors_reject_wrong_targets_pins_and_page_ranges() {
    let (request, _) = described_page("properties");
    for view in [
        json!(null),
        json!({}),
        json!({"kind":"unknown"}),
        json!({"kind":"targets","offset":-1,"limit":1}),
        json!({"kind":"targets","offset":4_294_967_296u64,"limit":1}),
        json!({"kind":"properties","target":{"contextId":"scene-root"},"offset":0,"limit":1}),
        json!({"kind":"storedProperty","target":{"contextId":"scene-root","layerUid":"L","strokeId":""},"property":"opacity"}),
        json!({"kind":"storedProperty","target":{"contextId":"scene-root","layerUid":"L"},"property":""}),
    ] {
        let mut bad = request.clone();
        bad.payload["view"] = view;
        assert!(bad.validate_input().is_err());
    }
    for (field, value) in [("atRevision", json!(-1)), ("documentSnapshotId", json!(""))] {
        let mut bad = request.clone();
        bad.payload[field] = value;
        assert!(bad.validate_input().is_err());
    }
    let mut element = request;
    element.payload["view"]["target"]["strokeId"] = json!("stroke-a");
    assert!(element.validate_input().is_ok());
}
#[test]
fn metadata_target_kinds_cursor_and_categories_are_closed_correlated_sets() {
    let (mut request, mut response) = pair();
    request.payload["view"]["limit"] = json!(8);
    response.result.as_mut().unwrap()["items"] = json!([
        {"kind":"context","contextId":"scene-root"},
        {"kind":"layer","target":{"contextId":"scene-root","layerUid":"L"}},
        {"kind":"element","target":{"contextId":"scene-root","layerUid":"L","strokeId":"S"}}]);
    response.result.as_mut().unwrap()["nextOffset"] = json!(3);
    assert!(validate_native_response(&response, &request).is_ok());
    for item in [
        json!({"kind":"context","contextId":""}),
        json!({"kind":"other"}),
        json!({"kind":"layer","target":{"contextId":"scene-root","layerUid":"L","strokeId":"S"}}),
        json!({"kind":"element","target":{"contextId":"scene-root","layerUid":"L"}}),
    ] {
        let mut bad = response.clone();
        bad.result.as_mut().unwrap()["items"][0] = item;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for (field, value) in [
        ("items", json!(null)),
        ("items", json!([])),
        ("nextOffset", json!(2)),
        ("nextOffset", json!(4_294_967_296u64)),
        ("kind", json!("properties")),
        ("unavailableCategories", json!({})),
    ] {
        let mut bad = response.clone();
        bad.result.as_mut().unwrap()[field] = value;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for (field, value) in [
        ("state", json!("available")),
        ("reason", json!("")),
        ("category", json!("propertyWrites")),
        ("extra", json!(true)),
    ] {
        let mut bad = response.clone();
        bad.result.as_mut().unwrap()["unavailableCategories"][0][field] = value;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    let mut empty = response;
    empty.result.as_mut().unwrap()["items"] = json!([]);
    empty.result.as_mut().unwrap()["nextOffset"] = json!(null);
    assert!(validate_native_response(&empty, &request).is_ok());
}
#[test]
fn metadata_stored_property_reply_cannot_be_an_unrelated_or_paginated_descriptor() {
    let (request, response) = described_page("storedProperty");
    for (field, value) in [
        ("items", json!([])),
        ("nextOffset", json!(1)),
        ("target", json!(null)),
    ] {
        let mut bad = response.clone();
        bad.result.as_mut().unwrap()[field] = value;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    let mut wrong_property = response;
    wrong_property.result.as_mut().unwrap()["items"][0]["property"] = json!("position");
    assert!(validate_native_response(&wrong_property, &request).is_err());
}
