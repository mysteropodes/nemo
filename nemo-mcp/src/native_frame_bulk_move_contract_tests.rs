//! Ordinary Move public shell and complete-schema controls; no native document execution.
use super::*;
fn payload(cells: Value) -> Value {
    json!({"command":"object.frames.move","selection":cells,"layerUid":"data:opaque target 雪","frame":3})
}
fn receipt(applied: bool, cells: Value) -> Value {
    json!({"applied":applied,"historyEntriesAdded":u64::from(applied),"selection":cells})
}
#[test]
fn move_selection_is_unique_closed_u32_and_shell_bound_does_not_enlarge_transport() {
    for cells in [
        json!([]),
        json!([{"layerUid":"data:opaque source 雪","frame":u32::MAX}]),
    ] {
        assert!(request(MOVE, &payload(cells.clone())));
        assert!(envelope(MOVE, payload(cells)).validate_input().is_ok());
    }
    for cells in [
        Value::Null,
        json!({}),
        json!([null]),
        json!([{"layerUid":"","frame":0}]),
        json!([{"layerUid":"a","frame":-1}]),
        json!([{"layerUid":"a","frame":0.5}]),
        json!([{"layerUid":"a","frame":4294967296u64}]),
        json!([{"layerUid":"a","frame":0,"extra":1}]),
        json!([{"layerUid":"a","frame":0},{"layerUid":"a","frame":0}]),
    ] {
        assert!(!request(MOVE, &payload(cells)));
    }
    let full = (0..4096)
        .map(|frame| json!({"layerUid":"a","frame":frame}))
        .collect::<Vec<_>>();
    assert!(request(MOVE, &payload(json!(full))));
    assert!(envelope(MOVE, payload(json!(full)))
        .validate_input()
        .is_err());
    let over = (0..4097)
        .map(|frame| json!({"layerUid":"a","frame":frame}))
        .collect::<Vec<_>>();
    assert!(!request(MOVE, &payload(json!(over))));
    for (field, bad) in [
        ("frame", json!(-1)),
        ("frame", json!(1.5)),
        ("frame", json!(4294967296u64)),
        ("layerUid", json!("")),
        ("delta", json!(1)),
    ] {
        let mut value = payload(json!([]));
        value[field] = bad;
        assert!(!request(MOVE, &value));
    }
}
#[test]
fn move_receipt_requires_actual_selection_exact_checkpoint_and_publication_revision() {
    for value in [
        receipt(true, json!([])),
        receipt(true, json!([{"layerUid":"a","frame":u32::MAX}])),
        receipt(false, Value::Null),
    ] {
        let revision = 7 + u64::from(value["applied"] == true);
        assert!(result_for_operation(MOVE, &value));
        assert!(revision_matches_for_operation(
            MOVE,
            Some(7),
            revision,
            &value
        ));
        assert!(!revision_matches_for_operation(
            MOVE,
            Some(7),
            revision + 1,
            &value
        ));
        assert!(!revision_matches_for_operation(
            MOVE, None, revision, &value
        ));
        for operation in OPERATIONS
            .into_iter()
            .filter(|operation| *operation != MOVE)
        {
            assert!(!result_for_operation(operation, &value));
        }
    }
    for value in [
        receipt(true, Value::Null),
        receipt(false, json!([])),
        receipt(true, json!([{"layerUid":"","frame":0}])),
        receipt(
            true,
            json!([{"layerUid":"a","frame":0},{"layerUid":"a","frame":0}]),
        ),
        json!({"applied":true,"historyEntriesAdded":0,"selection":[]}),
        json!({"applied":false,"historyEntriesAdded":1,"selection":null}),
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"selection":[],"extra":1}),
    ] {
        assert!(!result_for_operation(MOVE, &value), "{value}");
    }
    assert!(!declares("command.document.object.layer.frames.shift"));
    assert!(!request(
        "command.document.object.layer.frames.shift",
        &json!({"command":"object.layer.frames.shift","layerUid":"a","delta":1})
    ));
}
fn complete_transport_schema() -> (Value, jsonschema::Validator) {
    let transport: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    // Resolve unchanged external references by their actual canonical $id. Every resource
    // is the complete shipped file; no true-schema stub, DTO projection or network fetch.
    let mut resources = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let document: Value = serde_json::from_str(text).unwrap();
        let id = document["$id"].as_str().unwrap().to_owned();
        resources = resources
            .add(id, jsonschema::Resource::from_contents(document))
            .unwrap();
    }
    let registry = resources.prepare().unwrap();
    let validator = jsonschema::draft202012::options()
        .with_registry(&registry)
        .build(&transport)
        .unwrap();
    (transport, validator)
}

fn complete_response(value: Value) -> Value {
    json!({"apiVersion":2,"requestId":"move-schema","instanceId":"instance","documentId":"document","contentRevision":8,"ok":true,"result":value})
}
#[test]
fn complete_schema_connects_move_after_all_original27_result_branches_and_accepts_catalog() {
    let (schema, validator) = complete_transport_schema();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), 33);
    assert_eq!(
        branches[30],
        json!({"$ref":"#/$defs/ObjectProjectGuideReadResult"})
    );
    assert_eq!(
        branches[31],
        json!({"$ref":"#/$defs/ObjectProjectShadowReadResult"})
    );
    assert_eq!(
        branches[32],
        json!({"$ref":"#/$defs/ObjectProjectPerspectiveReadResult"})
    );
    assert_eq!(
        branches[28],
        json!({"$ref":"#/$defs/ObjectProjectCompositionReadResult"})
    );
    assert_eq!(
        branches[29],
        json!({"$ref":"#/$defs/ObjectProjectPaletteReadResult"})
    );
    assert_eq!(
        branches[26],
        json!({"$ref":"#/$defs/ObjectFrameClipboardCutResult"})
    );
    assert_eq!(
        branches[27],
        json!({"$ref":"#/$defs/ObjectFrameBulkMoveResult"})
    );
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(MOVE).unwrap();
    assert_eq!(descriptor["operations"], json!(OPERATIONS));
    for example in descriptor["examples"].as_array().unwrap() {
        assert!(
            validator.is_valid(&example["request"]),
            "{}",
            example["request"]["operation"]
        );
        assert!(
            validator.is_valid(&example["response"]),
            "{}",
            example["request"]["operation"]
        );
    }
    for value in [
        receipt(true, json!([])),
        receipt(true, json!([{"layerUid":"a","frame":3}])),
        receipt(false, Value::Null),
    ] {
        assert!(validator.is_valid(&complete_response(value)));
    }
    for value in [
        receipt(true, Value::Null),
        receipt(false, json!([])),
        json!({"applied":true,"historyEntriesAdded":0,"selection":[]}),
        receipt(true, json!([{"layerUid":"a","frame":0,"extra":true}])),
        receipt(
            true,
            json!([{"layerUid":"a","frame":0},{"layerUid":"a","frame":0}]),
        ),
    ] {
        assert!(!validator.is_valid(&complete_response(value)));
    }
    let good = complete_response(receipt(true, json!([])));
    for field in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "contentRevision",
        "ok",
        "result",
    ] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(!validator.is_valid(&missing));
    }
}
