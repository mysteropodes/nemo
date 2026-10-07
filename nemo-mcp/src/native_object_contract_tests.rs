use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

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
fn request_admission_denies_staged_objects_without_hiding_malformed_or_oversized_requests() {
    let mut value = cases()["read"]["request"].clone();
    let admission = |value: &Value| {
        serde_json::from_value::<NativeApplicationRequest>(value.clone())
            .unwrap()
            .validate()
    };
    let error = admission(&value).unwrap_err();
    assert_eq!(error.code(), "unavailable");
    assert_eq!(
        error.message(),
        crate::capabilities::native_catalog()
            .capability_for_operation("query.document.object")
            .unwrap()["availability"]["reason"]
    );
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
