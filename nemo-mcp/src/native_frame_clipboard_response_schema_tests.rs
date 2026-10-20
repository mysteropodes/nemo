// Complete transport schema consumers, extracted unchanged to preserve the existing module limit.
use super::*;

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
fn complete_clipboard_response(result: Value) -> Value {
    json!({"apiVersion":2,"requestId":"clipboard-schema","instanceId":"instance","documentId":"document",
        "contentRevision":7,"ok":true,"result":result})
}

#[test]
fn complete_transport_schema_reaches_one_clipboard_result_branch_without_copy_cut_overlap() {
    let (schema, validator) = complete_transport_schema();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    let retained = [
        "#/$defs/DiagnosticsResult",
        "#/$defs/ReproductionStatusResult",
        "#/$defs/ReproductionExportResult",
        "#/$defs/ReproductionReportResult",
        "#/$defs/ReproductionReplayResult",
        "#/$defs/ObjectLayerCreateResult",
        "#/$defs/ObjectImportBatchResult",
        "#/$defs/ObjectLayerTimingResult",
        "#/$defs/ObjectPropertyMetadataResult",
        "native-bitmap-brush-resources-v1.schema.json#/$defs/PublishResult",
        "native-bitmap-brush-resources-v1.schema.json#/$defs/InspectResult",
        "native-bitmap-brush-resources-v1.schema.json#/$defs/SceneResult",
        "#/$defs/CommandResult",
        "#/$defs/OpacityQueryResult",
        "#/$defs/RevisionQueryResult",
        "#/$defs/SnapshotQueryResult",
        "#/$defs/SerializeQueryResult",
        "#/$defs/EvaluateQueryResult",
        "#/$defs/TransactionResult",
        "#/$defs/JobReceipt",
        "native-object-item-v1.schema.json#/$defs/ItemEditResult",
        "#/$defs/ObjectVectorEditResult",
        "#/$defs/ObjectPropertyReadResult",
        "#/$defs/ObjectTextReadResult",
        "native-object-item-v1.schema.json#/$defs/ImageImportResult",
        "#/$defs/ObjectFrameRepeatResult",
        "#/$defs/ObjectFrameClipboardCutResult",
        "#/$defs/ObjectFrameBulkMoveResult",
        "#/$defs/ObjectProjectCompositionReadResult",
    ];
    assert_eq!(branches.len(), retained.len() + 2);
    for (branch, expected) in branches.iter().zip(retained) {
        assert_eq!(branch, &json!({"$ref":expected}));
    }
    assert_eq!(
        &branches[29],
        &json!({"$ref":"#/$defs/ObjectProjectPaletteReadResult"})
    );
    assert_eq!(
        branches.last().unwrap(),
        &json!({"$ref":"#/$defs/ObjectProjectGuideReadResult"})
    );
    assert!(!branches
        .iter()
        .any(|branch| branch["$ref"] == "#/$defs/ObjectFrameClipboardCopyResult"));
    let catalog = crate::capabilities::native_catalog();
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let descriptor = catalog.capability_for_operation(operation).unwrap();
        let example = descriptor["examples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|example| example["request"]["operation"] == operation)
            .unwrap();
        assert!(
            validator.is_valid(&example["response"]),
            "complete catalog response {operation}"
        );
    }
    for result in [
        clipboard_result(false, Value::Null),
        clipboard_result(false, packet()),
        clipboard_result(true, packet()),
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":false,"historyEntriesAdded":0}),
    ] {
        assert!(
            validator.is_valid(&complete_clipboard_response(result.clone())),
            "{result}"
        );
    }
    let mut omitted_order = packet();
    omitted_order["cells"][0]
        .as_object_mut()
        .unwrap()
        .remove("orderElements");
    assert!(
        validator.is_valid(&complete_clipboard_response(clipboard_result(
            false,
            omitted_order
        )))
    );
    // Response has no operation. The common union must accept a changed Cut-shaped body,
    // while the existing operation-aware validator still rejects that body for Copy.
    let changed = clipboard_result(true, packet());
    assert!(validator.is_valid(&complete_clipboard_response(changed.clone())));
    assert!(!result_for_operation(CLIPBOARD_COPY, &changed));
}

#[test]
fn complete_transport_schema_refuses_malformed_clipboard_results_and_full_envelopes() {
    let (_, validator) = complete_transport_schema();
    let good = complete_clipboard_response(clipboard_result(false, packet()));
    assert!(validator.is_valid(&good));
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
        assert!(
            !validator.is_valid(&missing),
            "missing complete envelope {field}"
        );
    }
    for (field, value) in [
        ("apiVersion", json!(1)),
        ("requestId", json!("bad request id")),
        ("instanceId", Value::Null),
        ("documentId", json!([])),
        ("contentRevision", json!(-1)),
        ("contentRevision", json!(0.5)),
        ("contentRevision", json!(9007199254740992u64)),
        ("ok", json!(1)),
        ("ok", json!(false)),
        ("extra", json!(true)),
        ("error", json!({"code":"unavailable","message":"no"})),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(!validator.is_valid(&bad), "complete envelope {field}");
    }
    assert!(!validator.is_valid(&Value::Null));
    assert!(!validator.is_valid(&json!([])));
    for result in [
        clipboard_result(true, Value::Null),
        json!({"applied":false,"clipboard":null}),
        json!({"historyEntriesAdded":0,"clipboard":null}),
        json!({"applied":true,"historyEntriesAdded":0,"clipboard":packet()}),
        json!({"applied":false,"historyEntriesAdded":1,"clipboard":packet()}),
        json!({"applied":"false","historyEntriesAdded":0,"clipboard":null}),
        json!({"applied":false,"historyEntriesAdded":0,"clipboard":null,"extra":0}),
    ] {
        assert!(
            !validator.is_valid(&complete_clipboard_response(result.clone())),
            "result {result}"
        );
    }
    for field in ["instanceId", "documentId", "cells"] {
        let mut missing = packet();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            !validator.is_valid(&complete_clipboard_response(clipboard_result(
                false, missing
            ))),
            "packet {field}"
        );
    }
    for (field, value) in [
        ("instanceId", json!("")),
        ("documentId", Value::Null),
        ("cells", Value::Null),
        ("cells", json!([null])),
        ("extra", json!(true)),
    ] {
        let mut bad = packet();
        bad[field] = value;
        assert!(
            !validator.is_valid(&complete_clipboard_response(clipboard_result(false, bad))),
            "packet {field}"
        );
    }
    for field in ["relLayer", "relFrame", "keyframe", "records"] {
        let mut missing = packet();
        missing["cells"][0].as_object_mut().unwrap().remove(field);
        assert!(
            !validator.is_valid(&complete_clipboard_response(clipboard_result(
                false, missing
            ))),
            "cell {field}"
        );
    }
    for (field, value) in [
        ("relLayer", json!(-1)),
        ("relFrame", json!(4294967296u64)),
        ("keyframe", json!("true")),
        ("records", Value::Null),
        ("records", json!([null])),
        ("orderElements", Value::Null),
        ("orderElements", json!([[]])),
        ("extra", json!(0)),
    ] {
        let mut bad = packet();
        bad["cells"][0][field] = value;
        assert!(
            !validator.is_valid(&complete_clipboard_response(clipboard_result(false, bad))),
            "cell {field}"
        );
    }
}
