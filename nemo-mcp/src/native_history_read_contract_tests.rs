use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{decode_json_bounded, decode_native_request, validate_native_response};
use serde_json::json;
fn envelope() -> Value {
    json!({"apiVersion":2,"requestId":"history-read","instanceId":"instance","documentId":"document",
        "operation":OP,"payload":{"atRevision":7,"documentSnapshotId":"native-object:document:7"}})
}
fn response() -> Value {
    json!({"apiVersion":2,"requestId":"history-read","instanceId":"instance","documentId":"document",
        "contentRevision":7,"ok":true,"result":{"atRevision":7,
        "documentSnapshotId":"native-object:document:7","undoDepth":2,"redoDepth":1}})
}
#[test]
fn published_history_schema_keeps_existing_branches_and_closes_new_shapes() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let requests = schema["$defs"]["Request"]["oneOf"].as_array().unwrap();
    let matching: Vec<_> = requests
        .iter()
        .filter(|branch| branch["allOf"][1]["properties"]["operation"]["const"] == OP)
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0]["allOf"][0],
        json!({"$ref":"#/$defs/ForbiddenRevisionRequest"})
    );
    let mut request_schema = matching[0].clone();
    request_schema["$defs"] = schema["$defs"].clone();
    let input = jsonschema::draft202012::options()
        .build(&request_schema)
        .unwrap();
    let output_schema = json!({"$ref":"#/$defs/ObjectHistoryReadResult", "$defs":schema["$defs"]});
    let output = jsonschema::draft202012::options()
        .build(&output_schema)
        .unwrap();
    assert!(input.is_valid(&envelope()));
    assert!(output.is_valid(&response()["result"]));
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), 38);
    assert_eq!(
        branches[37],
        json!({"$ref":"#/$defs/ObjectHistoryReadResult"})
    );
    for field in ["atRevision", "documentSnapshotId"] {
        let mut bad = envelope();
        bad["payload"].as_object_mut().unwrap().remove(field);
        assert!(!input.is_valid(&bad));
    }
    for (field, value) in [
        ("extra", json!(true)),
        ("atRevision", json!(-1)),
        ("atRevision", json!(9007199254740992u64)),
        ("documentSnapshotId", json!("")),
    ] {
        let mut bad = envelope();
        bad["payload"][field] = value;
        assert!(!input.is_valid(&bad));
    }
    let mut bad = envelope();
    bad["expectedRevision"] = json!(7);
    assert!(!input.is_valid(&bad));
    for field in ["undoDepth", "redoDepth"] {
        for value in [
            json!(-1),
            json!(0.5),
            json!(true),
            json!(9007199254740992u64),
        ] {
            let mut bad = response()["result"].clone();
            bad[field] = value;
            assert!(!output.is_valid(&bad));
        }
        let mut bad = response()["result"].clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(!output.is_valid(&bad));
    }
    let mut bad = response()["result"].clone();
    bad["extra"] = json!(true);
    assert!(!output.is_valid(&bad));
}
#[test]
fn registered_public_input_is_closed_bounded_and_duplicate_safe() {
    let r: NativeApplicationRequest =
        decode_native_request(envelope().to_string().as_bytes()).unwrap();
    r.validate().unwrap();
    assert!(crate::native_contract::forbids_expected_revision(OP));
    for (key, value) in [
        ("expectedRevision", json!(7)),
        ("expectedRevision", Value::Null),
        ("apiVersion", json!(1)),
        ("extra", json!(true)),
    ] {
        let mut bad = envelope();
        bad[key] = value;
        assert!(decode_native_request(bad.to_string().as_bytes())
            .and_then(|r| r
                .validate()
                .map_err(|e| std::io::Error::other(e.to_string())))
            .is_err());
    }
    for p in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"atRevision":7,"documentSnapshotId":"s","extra":true}),
    ] {
        assert!(!request(&p));
    }
    for value in [
        Value::Null,
        json!("7"),
        json!(-1),
        json!(0.5),
        json!(true),
        json!(9007199254740992u64),
    ] {
        let mut bad = envelope();
        bad["payload"]["atRevision"] = value;
        assert!(decode_native_request(bad.to_string().as_bytes())
            .and_then(|r| r
                .validate()
                .map_err(|e| std::io::Error::other(e.to_string())))
            .is_err());
    }
    for raw in [
        envelope()
            .to_string()
            .replacen("\"apiVersion\":2", "\"apiVersion\":2,\"apiVersion\":2", 1),
        envelope()
            .to_string()
            .replacen("\"atRevision\":7", "\"atRevision\":7,\"atRevision\":7", 1),
    ] {
        assert!(decode_native_request(raw.as_bytes()).is_err());
    }
    let bytes = format!("{}{}", envelope(), " ".repeat(4096));
    assert!(decode_native_request(bytes.as_bytes()).is_err());
}
#[test]
fn wire_result_rejects_forged_owner_pins_depths_fields_and_original_byte_duplicates() {
    let r: NativeApplicationRequest = serde_json::from_value(envelope()).unwrap();
    let good: NativeApplicationResponse = serde_json::from_value(response()).unwrap();
    validate_native_response(&good, &r).unwrap();
    for (key, value) in [
        ("requestId", json!("foreign")),
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(8)),
    ] {
        let mut bad = response();
        bad[key] = value;
        assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err());
    }
    for (key, value) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("foreign")),
        ("undoDepth", json!(-1)),
        ("undoDepth", json!(0.5)),
        ("undoDepth", json!(true)),
        ("redoDepth", json!(9007199254740992u64)),
        ("extra", json!(true)),
    ] {
        let mut bad = response();
        bad["result"][key] = value;
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err(),
            "{key}"
        );
    }
    for field in ["undoDepth", "redoDepth", "atRevision", "documentSnapshotId"] {
        let mut bad = response();
        bad["result"].as_object_mut().unwrap().remove(field);
        assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err());
    }
    let raw =
        response()
            .to_string()
            .replacen("\"undoDepth\":2", "\"undoDepth\":2,\"undoDepth\":2", 1);
    assert!(decode_json_bounded::<NativeApplicationResponse>(raw.as_bytes(), 4096).is_err());
    let raw = format!("{}{}", response(), " ".repeat(4096));
    assert!(decode_json_bounded::<NativeApplicationResponse>(raw.as_bytes(), 4096).is_err());
}
