//! Range-only query shell/correlation, with complete unchanged original cell read branch retained.
use super::*;
use serde_json::json;
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L0","frame":1,"view":"range"})
}
fn output() -> Value {
    json!({"kind":"layerRange","instanceId":"instance","documentId":"document","atRevision":7,
    "documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L0","frame":1,
    "range":{"rawInPoint":-2.5,"rawOutPoint":20.5,"inPoint":0.0,"outPoint":11.0,"hasCustomRange":true,"hasTimeLink":true}})
}
#[test]
fn range_selector_is_exact_and_result_carries_only_same_snapshot_requested_edges() {
    let q = query();
    let r = output();
    assert!(request(&q));
    assert!(result("document", &q, &r));
    assert!(super::super::timing_request(&q));
    assert!(super::super::timing_result("document", &q, &r));
    for field in q.as_object().unwrap().keys() {
        let mut bad = q.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(!request(&bad));
    }
    for field in [
        "atRevision",
        "documentSnapshotId",
        "contextId",
        "layerUid",
        "frame",
    ] {
        let mut bad = r.clone();
        bad[field] = Value::Null;
        assert!(!result("document", &q, &bad));
    }
    for (field, value) in [
        ("kind", json!("timing")),
        ("documentId", json!("foreign")),
        ("cellKind", json!("key")),
    ] {
        let mut bad = r.clone();
        bad[field] = value;
        assert!(!result("document", &q, &bad));
    }
    for field in r["range"].as_object().unwrap().keys() {
        let mut bad = r.clone();
        bad["range"].as_object_mut().unwrap().remove(field);
        assert!(!result("document", &q, &bad));
    }
    let mut bad = r;
    bad["range"]["inPoint"] = json!("0");
    assert!(!result("document", &q, &bad));
}
#[test]
fn full_range_schema_examples_and_original_cell_examples_remain_shipped_and_disjoint() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-layer-timing-v1.schema.json"
    ))
    .unwrap();
    let mut selectors = schema.clone();
    selectors["$ref"] = json!("#/$defs/Selector");
    selectors.as_object_mut().unwrap().remove("type");
    selectors
        .as_object_mut()
        .unwrap()
        .remove("additionalProperties");
    selectors.as_object_mut().unwrap().remove("required");
    selectors.as_object_mut().unwrap().remove("properties");
    let sv = jsonschema::draft202012::options()
        .build(&selectors)
        .unwrap();
    assert!(sv.is_valid(&query()));
    let mut old = query();
    old.as_object_mut().unwrap().remove("view");
    assert!(sv.is_valid(&old));
    let mut bad = query();
    bad["view"] = json!("other");
    assert!(!sv.is_valid(&bad));
    assert_eq!(
        schema["$defs"]["CellSelector"]["required"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        schema["$defs"]["CellReadResult"]["required"]
            .as_array()
            .unwrap()
            .len(),
        11
    );
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-layer-timing.json"
    ))
    .unwrap();
    assert_eq!(descriptor["examples"].as_array().unwrap().len(), 4);
    assert!(super::super::timing_result(
        "document-A",
        &descriptor["examples"][0]["request"]["payload"],
        &descriptor["examples"][0]["response"]["result"]
    ));
    assert!(super::super::timing_result(
        "document-A",
        &descriptor["examples"][1]["request"]["payload"],
        &descriptor["examples"][1]["response"]["result"]
    ));
    assert!(super::super::timing_result(
        "document-A",
        &descriptor["examples"][2]["request"]["payload"],
        &descriptor["examples"][2]["response"]["result"]
    ));
}
