//! `object.vector.batch` request and receipt shells (#2361, #2362).
use super::super::{request as vector_request, result as vector_result, VECTOR};
use serde_json::{json, Value};

fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":layer,"strokeId":stroke})
}
fn record(layer: &str, stroke: &str) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path","target":target(layer, stroke),
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative","segments":[
            {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":5,"y":5},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":null,"vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":2,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1}})
}
fn cut() -> Value {
    json!({"command":"object.vector.batch","ops":[{"op":"delete","stableTarget":target("L1","s-a")},
        {"op":"create","record":record("L1","")},{"op":"replace","record":record("L1","s-b")}]})
}

#[test]
fn a_batch_names_admitted_steps_on_one_layer_and_its_receipt_follows_them() {
    let payload = cut();
    assert!(vector_request(VECTOR, &payload));
    let mut copy = payload.clone();
    copy["keyframe"] = json!("copy");
    assert!(vector_request(VECTOR, &copy));
    let receipt = json!({"applied":true,"historyEntriesAdded":1,"operation":"batch","strokeId":"s-a",
        "strokeIds":["s-a","native-stroke:8:0","s-b"]});
    assert!(vector_result(VECTOR, &payload, &receipt));
    for (key, bad) in [
        ("strokeIds", json!(["s-a","native-stroke:8:0"])),
        ("strokeIds", json!(["s-a","s-x","s-b"])),
        ("strokeIds", json!(["s-a","native-stroke:8:0","s-c"])),
        ("operation", json!("createMany")),
        ("strokeId", json!("s-b")),
        ("historyEntriesAdded", json!(0)),
    ] {
        let mut value = receipt.clone();
        value[key] = bad;
        assert!(!vector_result(VECTOR, &payload, &value), "{key}");
    }
}

#[test]
fn malformed_steps_mixed_layers_bounds_and_members_are_refused() {
    let many: Vec<Value> = (0..65).map(|_| json!({"op":"create","record":record("L1","")})).collect();
    let mut cases = vec![
        json!({"command":"object.vector.batch","ops":[]}),
        json!({"command":"object.vector.batch","ops":many}),
        json!({"command":"object.vector.batch","ops":[{"op":"create","record":record("L1","")},{"op":"create","record":record("L2","")}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"move","record":record("L1","")}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"delete","stableTarget":target("L1","s"),"placement":"behind"}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"create","record":record("L1",""),"placement":"under"}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"replace","record":record("L1","")}]}),
    ];
    for (key, value) in [("keyframe", json!("blank")), ("trimEnds", json!(4)), ("placement", json!("behind"))] {
        let mut payload = cut();
        payload[key] = value;
        cases.push(payload);
    }
    for payload in cases {
        assert!(!vector_request(VECTOR, &payload), "{payload}");
    }
}
