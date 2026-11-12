use super::*;

fn timing_payload() -> Value {
    json!({"atRevision":0,"documentSnapshotId":"native-object:document:0",
        "contextId":"scene-root","layerUid":"data:雪","frame":2})
}
fn timing_result_fixture() -> Value {
    let mut value = timing_payload();
    let object = value.as_object_mut().unwrap();
    object.extend(
        json!({"instanceId":"instance","documentId":"document","cellKind":"held",
        "sourceFrame":1,"hasContent":true,"manualEdit":false})
        .as_object()
        .unwrap()
        .clone(),
    );
    value
}
#[test]
fn timing_contract_keeps_strict_input_and_current_head_result_correlation() {
    let payload = timing_payload();
    assert!(timing_request(&payload));
    let response = timing_result_fixture();
    assert!(timing_result("document", &payload, &response));
    for (key, value) in [
        ("atRevision", json!(-1)),
        ("atRevision", json!(9_007_199_254_740_992u64)),
        ("frame", json!(4_294_967_296u64)),
        ("frame", json!(null)),
        ("layerUid", json!("")),
        ("documentSnapshotId", json!("")),
        ("contextId", json!("")),
        ("lifecycleGeneration", json!(1)),
    ] {
        let mut invalid = payload.clone();
        invalid[key] = value;
        assert!(!timing_request(&invalid));
    }
    for (key, value) in [
        ("documentId", json!("other")),
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("other")),
        ("contextId", json!("other")),
        ("layerUid", json!("other")),
        ("frame", json!(3)),
        ("sourceFrame", json!(2)),
        ("cellKind", json!("key")),
        ("manualEdit", json!(true)),
        ("manualEdit", json!(null)),
        ("cellKind", json!("future-cell-kind")),
        ("hasContent", json!(null)),
        ("extra", json!(true)),
    ] {
        let mut invalid = response.clone();
        invalid[key] = value;
        assert!(!timing_result("document", &payload, &invalid), "{key}");
    }
    let mut request: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":"timing","instanceId":"instance","documentId":"document",
        "operation":"query.document.object.layer.timing","payload":payload}))
    .unwrap();
    assert!(request.validate().is_ok());
    request.expected_revision = Some(0);
    assert!(request.validate().is_err());
    assert_eq!(
        crate::capabilities::native_catalog()
            .capability_for_operation("command.document.object.layer.create")
            .unwrap()["availability"]["state"],
        "available"
    );
}
#[test]
fn timing_contract_accepts_the_frozen_exposure_oracle_including_blank_keys() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-layer-timing-v1/cases.json"
    ))
    .unwrap();
    for expected in corpus["drawing"]["expected"].as_array().unwrap() {
        let mut payload = timing_payload();
        payload["frame"] = expected["frame"].clone();
        let mut value = timing_result_fixture();
        value
            .as_object_mut()
            .unwrap()
            .extend(expected.as_object().unwrap().clone());
        assert!(timing_result("document", &payload, &value), "{expected}");
    }
}

#[test]
fn timing_result_front_door_refuses_an_invalid_original_selector() {
    let response = timing_result_fixture();
    for payload in [
        json!({}),
        json!({"atRevision":0,"documentSnapshotId":"native-object:document:0",
            "contextId":"scene-root","layerUid":"data:雪","frame":null}),
    ] {
        assert!(!timing_result("document", &payload, &response));
        assert!(!crate::native_contract::validate_result(
            "query.document.object.layer.timing",
            "document",
            &payload,
            &response
        ));
    }
}

#[test]
fn timing_selector_accepts_closed_frame_bounds_and_refuses_out_of_domain_selectors() {
    for frame in [0, u32::MAX] {
        let mut payload = timing_payload();
        payload["frame"] = json!(frame);
        let mut result = timing_result_fixture();
        result["frame"] = json!(frame);
        result["sourceFrame"] = json!(frame);
        result["cellKind"] = json!("key");
        result["manualEdit"] = json!(false);
        assert!(timing_request(&payload));
        assert!(timing_result("document", &payload, &result));
        for bad in [
            json!(-1),
            json!(u32::MAX as u64 + 1),
            json!(0.5),
            json!("0"),
        ] {
            payload["frame"] = bad;
            assert!(!timing_request(&payload));
            assert!(!timing_result("document", &payload, &result));
        }
    }
}

#[test]
fn a_vector_family_record_is_judged_by_the_vector_validator_and_keeps_the_read_target() {
    let payload = cases()["read"]["request"]["payload"].clone();
    let segment = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let vector = json!({"schemaVersion":1,"family":"editable-vector-path","target":payload["stableTarget"],
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[segment(10.0, 20.0), segment(110.0, 70.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0.0,"g":0.5,"b":1.0,"a":1.0},"hasRealStroke":true,"strokeWidth":4,
            "strokeCap":"round","strokeJoin":"miter","opacity":1}});
    assert!(record(&vector));
    let valid =
        json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","object":vector});
    assert!(super::super::result("document", &payload, &valid));

    let mut negative_width = vector.clone();
    negative_width["vector"]["strokeWidth"] = json!(-1);
    assert!(!record(&negative_width));
    let mut bodiless = vector.clone();
    bodiless.as_object_mut().unwrap().remove("vector");
    assert!(!record(&bodiless));
    let mut elsewhere = valid.clone();
    elsewhere["object"]["target"]["strokeId"] = json!("different");
    assert!(!super::super::result("document", &payload, &elsewhere));
    // The closed-fill family stays closed: it does not take the vector body.
    let mut mixed = cases()["records"][0].clone();
    mixed["vector"] = vector["vector"].clone();
    assert!(!record(&mixed));
}

#[test]
fn public_layer_creation_keeps_the_entered_component_context_closed_and_nonempty() {
    let admitted=|context: Value| {
        let request=json!({"apiVersion":2,"requestId":"create-context","instanceId":"instance","documentId":"document",
            "expectedRevision":7,"operation":"command.document.object.layer.create",
            "payload":{"command":"object.layer.create","name":"component layer","contextId":context}});
        crate::wire::decode_native_request(&serde_json::to_vec(&request).unwrap()).is_ok()
    };
    assert!(admitted(json!("comp-a")));
    for invalid in [Value::Null,json!(""),json!(7),json!([])] { assert!(!admitted(invalid)); }
}
