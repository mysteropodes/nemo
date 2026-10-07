use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::object_codec::{decode_project, encode_project, ObjectCodecErrorKind};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases()["records"]})
}
fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "fill-candidate",
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn payload() -> Value {
    cases()["command"]["request"]["payload"].clone()
}
fn prepare(
    owner: &ObjectSnapshot,
    payload: &Value,
) -> Result<PreparedObjectFill, DispatchErrorCode> {
    prepare_object_fill_json(owner, &serde_json::to_vec(payload).unwrap())
}
fn as_json(candidate: &PreparedObjectFill) -> Value {
    serde_json::from_slice(&encode_project(candidate.document()).unwrap()).unwrap()
}

#[test]
fn frozen_after_record_and_preservation_oracle_hold_without_source_mutation() {
    let source = document();
    let owner = snapshot(&source);
    let before = encode_project(owner.document()).unwrap();
    let candidate = prepare(&owner, &payload()).unwrap();
    let fixed = cases();
    let oracle = &fixed["command"]["preservationOracle"];
    let index = oracle["beforeRecordIndex"].as_u64().unwrap() as usize;
    let mut expected = source.clone();
    expected["objects"][index] = fixed["command"]["afterRecord"].clone();
    let actual = as_json(&candidate);
    assert_eq!(actual, expected);
    for key in oracle["unchanged"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        assert_eq!(actual["objects"][index][key], source["objects"][index][key]);
    }
    assert_ne!(
        actual["objects"][index]["fill"],
        source["objects"][index]["fill"]
    );
    assert!(candidate.changed());
    assert_eq!(
        candidate.target(),
        owner.document().objects()[index].target()
    );
    assert_eq!(candidate.base_instance_id(), owner.instance_id());
    assert_eq!(candidate.base_document_id(), owner.document_id());
    assert_eq!(candidate.base_snapshot_id(), owner.snapshot_id());
    assert_eq!(candidate.base_content_revision(), 0);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(prepare(&owner, &payload()).unwrap(), candidate);
    let encoded = encode_project(candidate.document()).unwrap();
    assert_eq!(decode_project(&encoded).unwrap(), *candidate.document());
    let read = json!({"apiVersion":2,"requestId":"after-prepare","instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":0,"stableTarget":payload()["stableTarget"]}});
    let retained = serde_json::to_value(
        owner
            .query_json(&serde_json::to_vec(&read).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(retained["result"]["object"], source["objects"][index]);
}

#[test]
fn same_ids_in_other_frames_scopes_and_layers_are_not_cross_frame_identity() {
    let mut source = document();
    let mut later = source["objects"][0].clone();
    later["target"]["frameScope"]["frame"] = json!(8);
    let mut reference = source["objects"][0].clone();
    reference["target"]["frameScope"]["kind"] = json!("reference");
    let mut layer = source["objects"][0].clone();
    layer["target"]["layerUid"] = json!("0");
    source["objects"]
        .as_array_mut()
        .unwrap()
        .extend([later, reference, layer]);
    let owner = snapshot(&source);
    for index in [0usize, 1, 2, 3, 4] {
        let mut request = payload();
        request["stableTarget"] = source["objects"][index]["target"].clone();
        let candidate = prepare(&owner, &request).unwrap();
        let actual = as_json(&candidate);
        for other in 0..5 {
            if index == other {
                assert_eq!(actual["objects"][other]["fill"], request["fill"]);
                assert_eq!(
                    actual["objects"][other]["target"],
                    source["objects"][other]["target"]
                );
                assert_eq!(
                    actual["objects"][other]["geometry"],
                    source["objects"][other]["geometry"]
                );
            } else {
                assert_eq!(actual["objects"][other], source["objects"][other]);
            }
        }
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&encode_project(owner.document()).unwrap()).unwrap(),
        source
    );
}

#[test]
fn semantic_no_op_keeps_original_numbers_and_document_bytes() {
    let mut source = document();
    source["objects"][0]["fill"] = json!({"kind":"solid","r":1,"g":0.0,"b":0,"a":1.0});
    let owner = snapshot(&source);
    let before = encode_project(owner.document()).unwrap();
    let mut request = payload();
    request["fill"] = json!({"kind":"solid","r":1.0,"g":0,"b":0.0,"a":1});
    let candidate = prepare(&owner, &request).unwrap();
    assert!(!candidate.changed());
    assert_eq!(encode_project(candidate.document()).unwrap(), before);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn opaque_long_and_numeric_looking_ids_survive_preparation() {
    let mut source = document();
    let id = "opaque layer / ".repeat(20);
    source["layers"][0]["layerUid"] = json!(id);
    source["objects"][0]["target"]["layerUid"] = source["layers"][0]["layerUid"].clone();
    source["objects"][0]["target"]["strokeId"] = json!("0001");
    let owner = snapshot(&source);
    let mut request = payload();
    request["stableTarget"] = source["objects"][0]["target"].clone();
    let candidate = prepare(&owner, &request).unwrap();
    assert_eq!(
        as_json(&candidate)["objects"][0]["target"],
        request["stableTarget"]
    );
    assert!(candidate.target().layer_uid().len() > 128);
    assert_eq!(candidate.target().stroke_id(), "0001");
}

#[test]
fn invalid_payload_maps_and_field_types_fail_without_a_partial_candidate() {
    let owner = snapshot(&document());
    let before = encode_project(owner.document()).unwrap();
    for value in [
        json!([]),
        Value::Null,
        json!(true),
        json!(3),
        json!("{}"),
        json!({}),
        cases()["command"]["request"].clone(),
    ] {
        assert_eq!(
            prepare(&owner, &value),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    for path in ["", "/stableTarget", "/stableTarget/frameScope", "/fill"] {
        let value = payload();
        let keys: Vec<_> = value
            .pointer(path)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        for key in keys {
            let mut invalid = value.clone();
            invalid
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(&key);
            assert_eq!(
                prepare(&owner, &invalid),
                Err(DispatchErrorCode::InvalidRequest),
                "{path}/{key}"
            );
        }
        for replacement in [json!([]), Value::Null, json!(42)] {
            let mut invalid = value.clone();
            *invalid.pointer_mut(path).unwrap() = replacement;
            assert_eq!(
                prepare(&owner, &invalid),
                Err(DispatchErrorCode::InvalidRequest),
                "{path}"
            );
        }
        let mut extra = value;
        extra.pointer_mut(path).unwrap()["unknown"] = json!(true);
        assert_eq!(
            prepare(&owner, &extra),
            Err(DispatchErrorCode::InvalidRequest),
            "{path}"
        );
    }
    for (path, value) in [
        ("/command", json!("layer.opacity.set")),
        ("/stableTarget/contextId", json!("other")),
        ("/stableTarget/layerUid", json!(1)),
        ("/stableTarget/layerUid", json!("")),
        ("/stableTarget/strokeId", json!(1)),
        ("/stableTarget/strokeId", json!("")),
        ("/stableTarget/frameScope/frame", json!(21)),
        ("/stableTarget/frameScope/frame", json!(-1)),
        ("/stableTarget/frameScope/frame", json!(0.5)),
        ("/stableTarget/frameScope/frame", json!(4294967296u64)),
        ("/stableTarget/frameScope/kind", json!({"authored":true})),
        ("/stableTarget/frameScope/kind", json!("all")),
        ("/fill/kind", json!("gradient")),
    ] {
        let mut invalid = payload();
        *invalid.pointer_mut(path).unwrap() = value;
        assert_eq!(
            prepare(&owner, &invalid),
            Err(DispatchErrorCode::InvalidRequest),
            "{path}"
        );
    }
    for channel in ["r", "g", "b", "a"] {
        for value in [
            json!(-0.1),
            json!(1.1),
            json!("0.5"),
            json!(true),
            Value::Null,
        ] {
            let mut invalid = payload();
            invalid["fill"][channel] = value;
            assert_eq!(
                prepare(&owner, &invalid),
                Err(DispatchErrorCode::InvalidRequest),
                "{channel}"
            );
        }
    }
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn duplicate_raw_members_and_overflow_fail_before_value_collapse() {
    let owner = snapshot(&document());
    let request = payload();
    let bytes = serde_json::to_string(&request).unwrap();
    let fields = [
        ("command", request["command"].clone()),
        ("stableTarget", request["stableTarget"].clone()),
        ("fill", request["fill"].clone()),
        ("contextId", request["stableTarget"]["contextId"].clone()),
        ("frameScope", request["stableTarget"]["frameScope"].clone()),
        ("kind", json!("authored")),
        ("frame", json!(7)),
        ("layerUid", request["stableTarget"]["layerUid"].clone()),
        ("strokeId", request["stableTarget"]["strokeId"].clone()),
        ("kind", json!("solid")),
        ("r", json!(0.8)),
        ("g", json!(0.1)),
        ("b", json!(0.3)),
        ("a", json!(1)),
    ];
    for (key, value) in fields {
        let fragment = format!("\"{key}\":{}", serde_json::to_string(&value).unwrap());
        assert!(bytes.contains(&fragment));
        let duplicate = bytes.replacen(&fragment, &format!("{fragment},{fragment}"), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&duplicate).unwrap(),
            request,
            "Value loses the duplicate"
        );
        assert_eq!(
            prepare_object_fill_json(&owner, duplicate.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest),
            "{key}"
        );
    }
    for raw in [
        bytes.replace("\"r\":0.8", "\"r\":1e400"),
        bytes.replace("\"a\":1", "\"a\":-1e400"),
        format!("{bytes} {{}}"),
    ] {
        assert_eq!(
            prepare_object_fill_json(&owner, raw.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
}

#[test]
fn well_shaped_missing_scoped_targets_fail_without_source_mutation() {
    let owner = snapshot(&document());
    let before = encode_project(owner.document()).unwrap();
    for (path, replacement) in [
        ("/stableTarget/strokeId", json!("missing")),
        ("/stableTarget/layerUid", json!("missing")),
        ("/stableTarget/frameScope/frame", json!(8)),
        ("/stableTarget/frameScope/kind", json!("reference")),
    ] {
        let mut request = payload();
        *request.pointer_mut(path).unwrap() = replacement;
        assert_eq!(
            prepare(&owner, &request),
            Err(DispatchErrorCode::NotFound),
            "{path}"
        );
    }
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn shared_fill_channel_validation_preserves_existing_codec_error_categories() {
    let mut unsupported = document();
    unsupported["objects"][0]["fill"]["kind"] = json!("gradient");
    assert_eq!(
        decode_project(&serde_json::to_vec(&unsupported).unwrap())
            .unwrap_err()
            .kind(),
        ObjectCodecErrorKind::Unsupported
    );
    for channel in ["r", "g", "b", "a"] {
        let mut invalid = document();
        invalid["objects"][0]["fill"][channel] = json!(1.1);
        assert_eq!(
            decode_project(&serde_json::to_vec(&invalid).unwrap())
                .unwrap_err()
                .kind(),
            ObjectCodecErrorKind::Invalid
        );
    }
}
