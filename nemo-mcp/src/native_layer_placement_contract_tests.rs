//! T12 placement view: exact selector, same-snapshot correlation, bounded facts.
use super::*;
use serde_json::json;
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L","frame":3,"view":"placement"})
}
fn output(placement: Value) -> Value {
    json!({"kind":"layerPlacement","instanceId":"instance","documentId":"document","atRevision":7,
    "documentSnapshotId":"snapshot","contextId":"scene-root","layerUid":"L","frame":3,"placement":placement})
}
fn video() -> Value {
    json!({"placementId":"p1","assetId":"v1","assetKind":"video","offsetFrames":5,
        "timeRemap":{"keyCount":2,"keyAtFrame":false},"interpMode":"blend","frameBlend":true})
}
#[test]
fn placement_selector_is_exact_and_routed_by_the_timing_contract() {
    let q = query();
    assert!(request(&q));
    assert!(super::super::timing_request(&q));
    assert!(super::super::timing_result(
        "document",
        &q,
        &output(video())
    ));
    assert!(result("document", &q, &output(Value::Null)));
    for field in q.as_object().unwrap().keys() {
        let mut bad = q.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(!request(&bad));
    }
    let mut range = q.clone();
    range["view"] = json!("range");
    assert!(!request(&range));
}
#[test]
fn result_facts_are_bounded_and_belong_to_the_requested_snapshot() {
    let q = query();
    let mut plain = video();
    for k in ["timeRemap", "interpMode", "frameBlend"] {
        plain[k] = Value::Null;
    }
    assert!(result("document", &q, &output(plain)));
    for (field, value) in [
        ("assetKind", json!("image")),
        ("offsetFrames", json!("5")),
        ("interpMode", json!("")),
        ("frameBlend", json!(1)),
        ("timeRemap", json!({"keyCount":2})),
        ("timeRemap", json!({"keyCount":-1,"keyAtFrame":true})),
        (
            "timeRemap",
            json!({"keys":[],"keyCount":0,"keyAtFrame":false}),
        ),
        ("extra", json!(1)),
    ] {
        let mut bad = video();
        bad[field] = value;
        assert!(!result("document", &q, &output(bad)), "{field}");
    }
    for (field, value) in [
        ("documentId", json!("foreign")),
        ("frame", json!(4)),
        ("kind", json!("layerRange")),
    ] {
        let mut bad = output(video());
        bad[field] = value;
        assert!(!result("document", &q, &bad), "{field}");
    }
}
