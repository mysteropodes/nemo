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
        "timeRemap":{"keyCount":2,"keyAtFrame":false,"value":12.5,
            "keys":[{"frame":0,"value":0},{"frame":24,"value":47}]},"interpMode":"blend","frameBlend":true,
        "sourceFrameCount":48})
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
fn a_listed_remap_key_may_carry_a_tint_and_a_hold_and_nothing_else() {
    let q = query();
    let mut tinted = video();
    tinted["timeRemap"]["keys"] = json!([{"frame":0,"value":0,"color":"#4ea9ff","hold":true},{"frame":24,"value":47,"color":"#4ea9ff80"}]);
    assert!(result("document", &q, &output(tinted)));
    for key in [
        json!({"frame":0,"value":0,"color":"blue"}),
        json!({"frame":0,"value":0,"color":5}),
        json!({"frame":0,"value":0,"hold":false}),
        json!({"frame":0,"value":0,"hold":"yes"}),
        json!({"frame":0,"value":0,"lockTo":"in"}),
    ] {
        let mut bad = video();
        bad["timeRemap"]["keys"] = json!([key, {"frame":24,"value":47}]);
        assert!(!result("document", &q, &output(bad)));
    }
}
#[test]
fn result_facts_are_bounded_and_belong_to_the_requested_snapshot() {
    let q = query();
    let mut plain = video();
    for k in ["timeRemap", "interpMode", "frameBlend", "sourceFrameCount"] {
        plain[k] = Value::Null;
    }
    assert!(result("document", &q, &output(plain)));
    let mut unlisted = video();
    unlisted["timeRemap"] = json!({"keyCount":300,"keyAtFrame":true,"value":7.5,"keys":null});
    assert!(result("document", &q, &output(unlisted)), "past the budget");
    for (field, value) in [
        ("assetKind", json!("image")),
        ("offsetFrames", json!("5")),
        ("interpMode", json!("")),
        ("frameBlend", json!(1)),
        ("sourceFrameCount", json!(0)),
        ("sourceFrameCount", json!(1.5)),
        ("sourceFrameCount", json!(4_294_967_296_u64)),
        ("timeRemap", json!({"keyCount":2})),
        ("timeRemap", json!({"keyCount":2,"keyAtFrame":false})),
        (
            "timeRemap",
            json!({"keyCount":-1,"keyAtFrame":true,"value":null,"keys":null}),
        ),
        (
            "timeRemap",
            json!({"keyCount":1,"keyAtFrame":false,"value":"3","keys":null}),
        ),
        (
            "timeRemap",
            json!({"keyCount":1,"keyAtFrame":false,"value":3,"keys":[]}),
        ),
        (
            "timeRemap",
            json!({"keyCount":1,"keyAtFrame":false,"value":3,"keys":[{"frame":0}]}),
        ),
        (
            "timeRemap",
            json!({"keyCount":1,"keyAtFrame":false,"value":3,
                "keys":[{"frame":0,"value":3,"lockTo":"in"}]}),
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
