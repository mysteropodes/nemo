//! G23 (#2361): `object.vector.createMany` admission and its receipt, in the transport contract.
use super::*;
use serde_json::{json, Value};

fn target() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":""})
}
fn record() -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path","target":target(),
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
                        {"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":null,"vector":{"stroke":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1},"hasRealStroke":true,
            "strokeWidth":2,"strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1}})
}
#[test]
fn create_many_is_bounded_to_one_layer_and_frame_and_its_receipt_lists_every_owner_id() {
    let many = |records: Vec<Value>| json!({"command":"object.vector.createMany","records":records});
    assert!(!request(VECTOR, &many(vec![record()])), "one record is a plain create");
    assert!(!request(VECTOR, &many((0..33).map(|_| record()).collect())));
    let mut other = record();
    other["target"]["layerUid"] = json!("L2");
    assert!(!request(VECTOR, &many(vec![record(), other])));
    assert!(!request(VECTOR, &json!({"command":"object.vector.createMany","records":record()})));
    let payload = many(vec![record(), record()]);
    let ids = json!(["native-stroke:8:0", "native-stroke:8:1"]);
    let good = json!({"applied":true,"historyEntriesAdded":1,"operation":"createMany","strokeId":"native-stroke:8:0","strokeIds":ids});
    assert!(result(VECTOR, &payload, &good));
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("strokeIds");
    assert!(!result(VECTOR, &payload, &missing), "a createMany receipt lists its ids");
    let mut extra = good.clone();
    extra["unexpected"] = json!(1);
    assert!(!result(VECTOR, &payload, &extra));
    let mut foreign = good.clone();
    foreign["strokeIds"] = json!(["native-stroke:8:0", "other:8:1"]);
    assert!(!result(VECTOR, &payload, &foreign));
    for (key, bad) in [
        ("strokeIds", json!(["native-stroke:8:0"])),
        ("strokeIds", json!(["native-stroke:8:1", "native-stroke:8:0"])),
        ("strokeIds", json!(["native-stroke:8:0", "native-stroke:8:0"])),
        ("strokeId", json!("native-stroke:8:1")),
        ("operation", json!("create")),
        ("applied", json!(false)),
    ] {
        let mut value = good.clone();
        value[key] = bad;
        assert!(!result(VECTOR, &payload, &value), "{key}");
    }
}

#[test]
fn placement_is_an_optional_front_or_behind_on_the_two_creates_only() {
    let records = json!([record(), record()]);
    for command in ["object.vector.create", "object.vector.createMany"] {
        let body = if command == "object.vector.create" { json!({"record":record()}) } else { json!({"records":records}) };
        for (placement, ok) in [(json!("behind"), true), (json!("front"), true), (json!("inside"), false), (json!(null), false)] {
            let mut payload = body.clone();
            payload["command"] = json!(command);
            payload["placement"] = placement.clone();
            assert_eq!(request(VECTOR, &payload), ok, "{command} {placement}");
        }
    }
    let delete = json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":"s"},"placement":"behind"});
    assert!(!request(VECTOR, &delete));
}
