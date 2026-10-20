//! Public gesture shells and full shipped-schema oracles. Native semantic tests live with the owner.
use super::*;
use crate::contract::NativeApplicationRequest;
use crate::native_contract::frame_edit::{self, RANGE_DRAG};
use serde_json::json;
fn payload() -> Value {
    json!({"command":"object.layer.range.drag","contextId":"scene-root","layerUid":"L0","activeLayerUid":"L1",
    "part":"both","barSelection":[],"samples":[0,2,1,2],"plainClick":false,"alt":false,"frame":1,"selectedKeys":[]})
}
fn envelope(value: Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"bar-shell","instanceId":"instance",
    "documentId":"document","expectedRevision":7,"operation":RANGE_DRAG,"payload":value}),
    )
    .unwrap()
}
#[test]
fn closed_bar_gesture_preserves_ordered_samples_and_requires_each_captured_intent_field() {
    let p = payload();
    assert!(request(&p));
    assert!(envelope(p.clone()).validate().is_ok());
    assert_eq!(p["samples"], json!([0, 2, 1, 2]));
    for field in p.as_object().unwrap().keys() {
        let mut bad = p.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(!request(&bad), "{field}");
    }
    for (field, bad) in [
        ("part", json!("unknown")),
        ("samples", json!([0.5])),
        ("samples", json!([9007199254740992i64])),
        ("samples", json!([true])),
        ("plainClick", Value::Null),
        ("frame", json!(4294967296u64)),
        ("contextId", json!("")),
        ("activeLayerUid", json!("")),
        ("delta", json!(2)),
        ("command", json!("object.layer.frames.shift")),
    ] {
        let mut v = p.clone();
        v[field] = bad;
        assert!(!request(&v), "{field}");
        assert!(envelope(v).validate_input().is_err());
    }
    for samples in [
        json!([]),
        json!([-9007199254740991i64, 9007199254740991i64]),
    ] {
        let mut v = p.clone();
        v["samples"] = samples;
        assert!(request(&v));
    }
}
#[test]
fn selected_key_addresses_are_closed_real_variants_with_numeric_frame_identity_and_unique_bar_uids()
{
    let mut p = payload();
    let property = json!({"kind":"property","target":{"contextId":"scene-root","layerUid":"L0","strokeId":"stroke"},"property":"rotation"});
    for target in [
        property.clone(),
        json!({"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"}),
        json!({"kind":"timeRemap","contextId":"scene-root","layerUid":"L0"}),
        json!({"kind":"elementOrder","contextId":"scene-root","layerUid":"L0","frameScope":{"kind":"authored","frame":0},"strokeId":"stroke"}),
    ] {
        p["selectedKeys"] = json!([{"target":target,"frame":0.5}]);
        assert!(request(&p));
        p["selectedKeys"][0]["target"]["extra"] = json!(1);
        assert!(!request(&p));
    }
    p["selectedKeys"] =
        json!([{"target":property.clone(),"frame":1},{"target":property,"frame":1.0}]);
    assert!(!request(&p));
    p["selectedKeys"] = json!([]);
    p["barSelection"] = json!([{"layerUid":"L0","part":"in"},{"layerUid":"L0","part":"out"}]);
    assert!(!request(&p));
    p["barSelection"] = json!([{"layerUid":"L0","part":"in"},{"layerUid":"L1","part":"out"}]);
    assert!(request(&p));
}
#[test]
fn sample_container_cap_never_lifts_complete_transport_and_success_always_checkpoints_once() {
    let mut p = payload();
    p["samples"] = json!(vec![0; 4096]);
    assert!(request(&p));
    assert!(envelope(p.clone()).validate_input().is_err());
    p["samples"] = json!(vec![0; 4097]);
    assert!(!request(&p));
    let good = json!({"applied":true,"historyEntriesAdded":1});
    assert!(frame_edit::result_for_operation(RANGE_DRAG, &good));
    assert!(frame_edit::revision_matches_for_operation(
        RANGE_DRAG,
        Some(7),
        8,
        &good
    ));
    for bad in [
        json!({"applied":false,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":1,"selection":[]}),
    ] {
        assert!(!frame_edit::result_for_operation(RANGE_DRAG, &bad));
    }
    for before in [None, Some(6), Some(8)] {
        assert!(!frame_edit::revision_matches_for_operation(
            RANGE_DRAG, before, 8, &good
        ));
    }
}
fn transport_validator() -> jsonschema::Validator {
    let transport: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let mut resources = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        let id = value["$id"].as_str().unwrap().to_owned();
        resources = resources
            .add(id, jsonschema::Resource::from_contents(value))
            .unwrap();
    }
    let registry = resources.prepare().unwrap();
    jsonschema::draft202012::options()
        .with_registry(&registry)
        .build(&transport)
        .unwrap()
}
#[test]
fn full_shipped_schema_keeps_original_operation_order_and_exact_cell_query_branches() {
    let validator = transport_validator();
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let catalog = crate::capabilities::native_catalog();
    let d = catalog.capability_for_operation(RANGE_DRAG).unwrap();
    assert_eq!(d["operations"].as_array().unwrap().len(), 17);
    assert_eq!(d["operations"][14], frame_edit::MOVE);
    assert_eq!(d["operations"][15], RANGE_DRAG);
    assert_eq!(d["operations"][16], frame_edit::REPEAT);
    for example in d["examples"].as_array().unwrap() {
        assert!(
            validator.is_valid(&example["request"]),
            "{}",
            example["request"]
        );
        assert!(validator.is_valid(&example["response"]));
    }
    assert_eq!(
        schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        31
    );
    assert_eq!(
        schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"][28],
        json!({"$ref":"#/$defs/ObjectProjectCompositionReadResult"})
    );
    assert_eq!(
        schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"][29],
        json!({"$ref":"#/$defs/ObjectProjectPaletteReadResult"})
    );
    assert_eq!(
        schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"][30],
        json!({"$ref":"#/$defs/ObjectProjectGuideReadResult"})
    );
    let q = serde_json::to_value(envelope(payload())).unwrap();
    assert!(validator.is_valid(&q));
    let mut bad = q;
    bad["payload"]["part"] = json!("unknown");
    assert!(!validator.is_valid(&bad));
    let t = &schema["$defs"]["ObjectLayerTimingCellPayload"];
    assert_eq!(t["required"].as_array().unwrap().len(), 5);
    assert_eq!(t["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["ObjectLayerTimingCellResult"]["required"]
            .as_array()
            .unwrap()
            .len(),
        11
    );
}
