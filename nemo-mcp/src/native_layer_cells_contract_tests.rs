//! Cells-view shell/correlation (U31, #2374); the cell, range and placement branches stay disjoint.
use super::*;
use serde_json::json;
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L0","frame":2,"view":"cells"})
}
fn output() -> Value {
    json!({"kind":"layerCells","instanceId":"instance","documentId":"document","atRevision":7,
    "documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L0","frame":2,
    "cells":{"runs":[[2,1,"key",2,true,false],[3,4,"held",2,true,false],[7,1,"interpolated",7,false,true]],"nextFrame":8}})
}
#[test]
fn cells_selector_is_exact_and_runs_tile_the_page_from_the_requested_frame() {
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
    let mut done = r.clone();
    done["cells"]["nextFrame"] = Value::Null;
    assert!(result("document", &q, &done));
    for (path, value) in [
        ("/kind", json!("layerRange")),
        ("/documentId", json!("foreign")),
        ("/frame", json!(3)),
        ("/cells/nextFrame", json!(9)),
        ("/cells/runs", json!([])),
        ("/cells/runs/0/0", json!(1)),
        ("/cells/runs/1/1", json!(0)),
        ("/cells/runs/1/2", json!("blank")),
        ("/cells/runs/1/3", json!("2")),
        ("/cells/runs/2/5", json!(1)),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(!result("document", &q, &bad), "{path}");
    }
    let mut extra = r;
    extra["cells"]["runs"][0]
        .as_array_mut()
        .unwrap()
        .push(json!(0));
    assert!(!result("document", &q, &extra));
}
#[test]
fn cells_schema_branch_and_shipped_example_are_admitted() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-layer-timing-v1.schema.json"
    ))
    .unwrap();
    let mut results = schema.clone();
    results["$ref"] = json!("#/$defs/RowCellsReadResult");
    for k in ["type", "additionalProperties", "required", "properties"] {
        results.as_object_mut().unwrap().remove(k);
    }
    let rv = jsonschema::draft202012::options().build(&results).unwrap();
    assert!(rv.is_valid(&output()));
    let mut bad = output();
    bad["cells"]["runs"][0][2] = json!("blank");
    assert!(!rv.is_valid(&bad));
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-layer-timing.json"
    ))
    .unwrap();
    let example = &descriptor["examples"][3];
    assert_eq!(example["request"]["payload"]["view"], "cells");
    assert!(rv.is_valid(&example["response"]["result"]));
    assert!(super::super::timing_result(
        "document-A",
        &example["request"]["payload"],
        &example["response"]["result"]
    ));
}
