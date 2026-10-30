use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

#[test]
fn history_payload_matcher_refuses_legacy_and_foreign_operations() {
    let undo = json!({"command":"object.undo"});
    let redo = json!({"command":"object.redo"});
    assert!(history_request("command.document.object.undo", &undo));
    assert!(history_request("command.document.object.redo", &redo));
    for operation in [
        "history.undo",
        "history.redo",
        "command.document.undo",
        "command.document.redo",
    ] {
        assert!(!history_request(operation, &undo));
        assert!(!history_request(operation, &redo));
    }
}

#[test]
fn history_payload_admits_only_a_positive_safe_integer_steps_member() {
    let op = "command.document.object.undo";
    assert!(history_request(
        op,
        &json!({"command":"object.undo","steps":3})
    ));
    assert!(history_request(
        op,
        &json!({"command":"object.undo","steps":9007199254740991u64})
    ));
    for steps in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("2"),
        json!(null),
        json!(9007199254740992u64),
    ] {
        assert!(!history_request(
            op,
            &json!({"command":"object.undo","steps":steps})
        ));
    }
    assert!(!history_request(
        op,
        &json!({"command":"object.undo","count":2})
    ));
    assert!(!history_request(
        op,
        &json!({"command":"object.redo","steps":1})
    ));
    assert!(history_request(
        "command.document.object.redo",
        &json!({"command":"object.redo","steps":2})
    ));
}

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}

fn changed(mut value: Value, change: &Value) -> Option<Value> {
    let path = change["path"].as_array().unwrap();
    let mut parent = &mut value;
    for key in &path[..path.len() - 1] {
        parent = match key {
            Value::String(key) => &mut parent[key],
            _ => &mut parent[key.as_u64().unwrap() as usize],
        };
    }
    let key = path.last().unwrap().as_str().unwrap();
    if change["delete"] == true {
        parent.as_object_mut().unwrap().remove(key);
    } else {
        parent[key] = if let Some(raw) = change["jsonValue"].as_str() {
            // serde_json rejects overflow before a typed Value exists.
            serde_json::from_str(raw).ok()?
        } else {
            change["value"].clone()
        };
    }
    Some(value)
}

#[test]
fn fixed_records_and_every_negative_corpus_case_keep_the_accepted_boundary() {
    let corpus = cases();
    for value in corpus["records"].as_array().unwrap() {
        assert!(record(value));
        assert_eq!(
            serde_json::from_slice::<Value>(&serde_json::to_vec(value).unwrap()).unwrap(),
            *value
        );
        let payload = json!({"atRevision":0,"stableTarget":value["target"]});
        let response =
            json!({"atRevision":0,"documentSnapshotId":"native-object:document:0","object":value});
        assert!(super::result("document", &payload, &response));
    }
    let mutations = corpus["invalidRecords"].as_array().unwrap();
    assert_eq!(mutations.len(), 31);
    let mut typed_rejections = 0;
    let mut parser_rejections = 0;
    for change in mutations {
        if let Some(value) = changed(corpus["records"][0].clone(), change) {
            assert!(!record(&value), "{}", change["label"]);
            typed_rejections += 1;
        } else {
            assert!(change["jsonValue"].is_string());
            parser_rejections += 1;
        }
    }
    assert_eq!((typed_rejections, parser_rejections), (29, 2));
}

#[test]
fn selectors_are_closed_and_keep_opaque_object_ids_separate_from_envelope_ids() {
    let base = cases()["read"]["request"]["payload"].clone();
    assert!(request(&base));
    for (path, replacement) in [
        ("/atRevision", json!(-1)),
        ("/atRevision", json!(1.5)),
        ("/atRevision", json!(9007199254740992u64)),
        ("/atRevision", Value::Null),
        ("/stableTarget/contextId", json!("other")),
        ("/stableTarget/strokeId", json!(1)),
        ("/stableTarget/layerUid", json!("")),
        ("/stableTarget/frameScope/frame", json!(-1)),
        ("/stableTarget/frameScope/frame", json!(4294967296u64)),
        ("/stableTarget/frameScope/frame", json!(0.5)),
        ("/stableTarget/frameScope/kind", json!({"authored":true})),
        ("/stableTarget/frameScope/kind", json!("all")),
    ] {
        let mut value = base.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(!request(&value), "{path}");
    }
}

#[test]
fn every_typed_object_map_requires_its_fields_and_rejects_extra_members() {
    fn walk(value: &Value, pointer: &str, paths: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                paths.push(pointer.to_owned());
                for (key, child) in fields {
                    walk(child, &format!("{pointer}/{key}"), paths);
                }
            }
            Value::Array(values) => {
                for (i, child) in values.iter().enumerate() {
                    walk(child, &format!("{pointer}/{i}"), paths);
                }
            }
            _ => {}
        }
    }
    for (base, is_record) in [
        (cases()["records"][0].clone(), true),
        (cases()["read"]["request"]["payload"].clone(), false),
    ] {
        let check = |value: &Value| {
            if is_record {
                record(value)
            } else {
                request(value)
            }
        };
        let mut paths = vec![];
        walk(&base, "", &mut paths);
        for path in paths {
            let mut extra = base.clone();
            extra.pointer_mut(&path).unwrap()["unknown"] = json!(true);
            assert!(!check(&extra), "extra member at {path}");
            let keys: Vec<_> = base
                .pointer(&path)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for key in keys {
                let mut missing = base.clone();
                missing
                    .pointer_mut(&path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(&key);
                assert!(!check(&missing), "missing {path}/{key}");
            }
            let mut positional = base.clone();
            *positional.pointer_mut(&path).unwrap() = json!([]);
            assert!(!check(&positional), "positional map at {path}");
        }
    }
}

#[test]
fn result_correlates_the_revision_incarnation_and_full_scoped_target() {
    let payload = cases()["read"]["request"]["payload"].clone();
    let valid = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7",
        "object":cases()["records"][0]});
    assert!(super::result("document", &payload, &valid));
    assert!(!super::result("replacement", &payload, &valid));
    for (path, replacement) in [
        ("/atRevision", json!(8)),
        ("/documentSnapshotId", json!("native-object:document:8")),
        ("/object/target/strokeId", json!("different")),
        ("/object/target/layerUid", json!("different")),
        ("/object/target/frameScope/kind", json!("reference")),
        ("/object/target/frameScope/frame", json!(8)),
    ] {
        let mut value = valid.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(!super::result("document", &payload, &value), "{path}");
    }
    for field in ["atRevision", "documentSnapshotId", "object"] {
        let mut value = valid.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(!super::result("document", &payload, &value), "{field}");
    }
    let mut extra = valid;
    extra["truncated"] = json!(false);
    assert!(!super::result("document", &payload, &extra));
}

#[test]
fn full_segment_and_numeric_boundaries_never_simplify_a_record() {
    let corpus = cases();
    for control in corpus["segmentCountControls"].as_array().unwrap() {
        let mut value = corpus["records"][0].clone();
        let source = value["geometry"]["segments"].as_array().unwrap();
        value["geometry"]["segments"] = Value::Array(
            (0..control["count"].as_u64().unwrap())
                .map(|i| source[i as usize % source.len()].clone())
                .collect(),
        );
        assert_eq!(record(&value), control["valid"].as_bool().unwrap());
    }
    let mut value = corpus["records"][1].clone();
    value["target"]["layerUid"] = json!("opaque layer ".repeat(20));
    value["target"]["strokeId"] = json!("0001");
    value["target"]["frameScope"]["frame"] = json!(u32::MAX);
    assert!(record(&value));
    for channel in ["r", "g", "b", "a"] {
        for invalid in [json!(-0.01), json!(1.01), json!("0.5"), Value::Null] {
            let mut invalid_record = value.clone();
            invalid_record["fill"][channel] = invalid;
            assert!(!record(&invalid_record));
        }
    }
}

#[test]
fn request_admission_accepts_objects_without_hiding_malformed_or_oversized_requests() {
    let mut value = cases()["read"]["request"].clone();
    let admission = |value: &Value| {
        serde_json::from_value::<NativeApplicationRequest>(value.clone())
            .unwrap()
            .validate()
    };
    assert!(admission(&value).is_ok());
    for payload in [json!({}), Value::Null, json!([]), json!("{}")] {
        let mut invalid = value.clone();
        invalid["payload"] = payload;
        assert_eq!(admission(&invalid).unwrap_err().code(), "invalid_request");
    }
    value["expectedRevision"] = json!(7);
    assert_eq!(admission(&value).unwrap_err().code(), "invalid_request");
    value.as_object_mut().unwrap().remove("expectedRevision");
    value["operation"] = json!("query.document.object.fill");
    assert_eq!(admission(&value).unwrap_err().code(), "invalid_request");
    value["operation"] = json!("query.document.object");
    value["payload"]["stableTarget"]["strokeId"] = json!("x".repeat(4096));
    assert_eq!(admission(&value).unwrap_err().code(), "invalid_request");
    assert!(crate::capabilities::validate_native_operation("query.document.opacity").is_ok());
}

#[test]
fn generic_object_result_front_door_rejects_a_malformed_selector() {
    let payload = cases()["read"]["request"]["payload"].clone();
    let value = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7",
        "object":cases()["records"][0]});
    assert!(crate::native_contract::validate_result(
        "query.document.object",
        "document",
        &payload,
        &value
    ));
    for invalid in [
        json!({}),
        json!([]),
        json!({"atRevision":7,"stableTarget":{}}),
    ] {
        assert!(!crate::native_contract::validate_result(
            "query.document.object",
            "document",
            &invalid,
            &value
        ));
    }
}

#[test]
fn registered_examples_keep_full_admission_for_all_available_features() {
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let request: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            let outcome = request.validate();
            if descriptor["availability"]["state"] == "available" {
                assert!(
                    outcome.is_ok(),
                    "available feature {}: {outcome:?}",
                    descriptor["id"]
                );
            } else {
                assert!(request.validate_input().is_ok());
                assert_eq!(outcome.unwrap_err().code(), "unavailable");
            }
        }
    }
}

#[test]
fn creation_shape_and_result_are_exact_through_the_available_declaration() {
    let payload = json!({"command":"object.layer.create","name":""});
    let mut request: NativeApplicationRequest = serde_json::from_value(json!({
        "apiVersion":2,"requestId":"create","instanceId":"instance","documentId":"document",
        "expectedRevision":0,"operation":"command.document.object.layer.create","payload":payload
    }))
    .unwrap();
    assert!(create_request(&payload));
    assert!(request.validate_input().is_ok());
    assert!(request.validate().is_ok());
    for invalid in [
        json!({}),
        json!([]),
        Value::Null,
        json!({"command":"object.layer.create","name":null}),
        json!({"command":"object.layer.create","name":false}),
        json!({"command":"object.layer.create","name":0}),
        json!({"command":"object.layer.create","name":"a","layerUid":"caller"}),
        json!({"command":"object.layer.create","name":"a","extra":true}),
        json!({"command":"layer.create","name":"a"}),
    ] {
        assert!(!create_request(&invalid));
        request.payload = invalid;
        assert!(request.validate_input().is_err());
    }
    request.payload = payload.clone();
    request.expected_revision = None;
    assert!(request.validate_input().is_err());
    request.expected_revision = Some(crate::contract::MAX_SAFE_REVISION + 1);
    assert!(request.validate_input().is_err());
    request.expected_revision = Some(0);
    request.payload["name"] = json!("层".repeat(1500));
    assert!(request.validate_input().is_err());
    let result = json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"});
    assert!(create_result(&result));
    assert!(crate::native_contract::validate_result(
        "command.document.object.layer.create",
        "document",
        &payload,
        &result
    ));
    for invalid in [
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
        json!({"applied":true,"historyEntriesAdded":0,"layerUid":"x"}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":0}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x","extra":true}),
    ] {
        assert!(!create_result(&invalid));
    }
    assert_eq!(
        crate::capabilities::native_catalog()
            .capability_for_operation("command.document.object.layer.create")
            .unwrap()["availability"]["state"],
        "available"
    );
    assert!(crate::capabilities::validate_native_operation("query.document.object").is_ok());
    assert!(crate::capabilities::validate_native_operation(
        "command.document.object.layer.visibility.set"
    )
    .is_ok());
}

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
    assert!(super::result("document", &payload, &valid));

    let mut negative_width = vector.clone();
    negative_width["vector"]["strokeWidth"] = json!(-1);
    assert!(!record(&negative_width));
    let mut bodiless = vector.clone();
    bodiless.as_object_mut().unwrap().remove("vector");
    assert!(!record(&bodiless));
    let mut elsewhere = valid.clone();
    elsewhere["object"]["target"]["strokeId"] = json!("different");
    assert!(!super::result("document", &payload, &elsewhere));
    // The closed-fill family stays closed: it does not take the vector body.
    let mut mixed = cases()["records"][0].clone();
    mixed["vector"] = vector["vector"].clone();
    assert!(!record(&mixed));
}
