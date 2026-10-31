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
    let creates = json!({"command":"object.vector.batch","trimEnds":24,"ops":[{"op":"create","record":record("L1","")}]});
    assert!(vector_request(VECTOR, &creates), "an all-create batch may trim");
    let mut own = payload.clone();
    own["ops"][1]["keyframe"] = json!("copy");
    assert!(vector_request(VECTOR, &own), "a create may carry its own copy key");
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
fn malformed_steps_trimmed_layers_bounds_and_members_are_refused() {
    let many: Vec<Value> = (0..65).map(|_| json!({"op":"create","record":record("L1","")})).collect();
    let mut cases = vec![
        json!({"command":"object.vector.batch","ops":[]}),
        json!({"command":"object.vector.batch","ops":many}),
        json!({"command":"object.vector.batch","trimEnds":4,"ops":[{"op":"create","record":record("L1","")},{"op":"create","record":record("L2","")}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"move","record":record("L1","")}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"delete","stableTarget":target("L1","s"),"placement":"behind"}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"create","record":record("L1",""),"placement":"under"}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"replace","record":record("L1","")}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"create","record":record("L1",""),"keyframe":"blank"}]}),
        json!({"command":"object.vector.batch","ops":[{"op":"delete","stableTarget":target("L1","s"),"keyframe":"copy"}]}),
    ];
    cases.push(json!({"command":"object.vector.batch","trimEnds":0,"ops":[{"op":"create","record":record("L1","")}]}));
    for (key, value) in [("keyframe", json!("blank")), ("trimEnds", json!(4)), ("placement", json!("behind"))] {
        let mut payload = cut();
        payload[key] = value;
        cases.push(payload);
    }
    for payload in cases {
        assert!(!vector_request(VECTOR, &payload), "{payload}");
    }
}

#[test]
fn a_copy_step_names_an_earlier_create_and_a_frame() {
    let ops = |copy: Value| json!({"command":"object.vector.batch","trimEnds":24,"ops":[{"op":"create","record":record("L1","")},copy]});
    let good = ops(json!({"op":"copy","fromStep":0,"frame":4,"keyframe":"copy","placement":"behind"}));
    assert!(vector_request(VECTOR, &good), "a copy of the drawn stroke, trimmed with it");
    let receipt = json!({"applied":true,"historyEntriesAdded":1,"operation":"batch","strokeId":"native-stroke:8:0",
        "strokeIds":["native-stroke:8:0","native-stroke:8:1"]});
    assert!(vector_result(VECTOR, &good, &receipt), "a copy's id is owner-allocated like a create's");
    for bad in [
        json!({"op":"copy","fromStep":1,"frame":4}),
        json!({"op":"copy","fromStep":0}),
        json!({"op":"copy","fromStep":0,"frame":-1}),
        json!({"op":"copy","fromStep":0,"frame":4,"record":record("L1","")}),
        json!({"op":"copy","fromStep":0,"frame":4,"keyframe":"move"}),
    ] {
        assert!(!vector_request(VECTOR, &ops(bad.clone())), "{bad}");
    }
    let from_delete = json!({"command":"object.vector.batch","ops":[{"op":"delete","stableTarget":target("L1","s-a")},
        {"op":"copy","fromStep":0,"frame":4}]});
    assert!(!vector_request(VECTOR, &from_delete), "only a create's record is copied");
}

#[test]
fn reshape_and_clone_name_a_stroke_and_new_geometry() {
    let geometry = record("L1", "s-a")["geometry"].clone();
    let payload = json!({"command":"object.vector.batch","ops":[
        {"op":"reshape","stableTarget":target("L1","s-a"),"geometry":geometry},
        {"op":"clone","stableTarget":target("L1","s-a"),"geometry":geometry,"frame":3,"placement":"front"}]});
    assert!(vector_request(VECTOR, &payload));
    let receipt = json!({"applied":true,"historyEntriesAdded":1,"operation":"batch","strokeId":"s-a","strokeIds":["s-a","native-stroke:8:0"]});
    assert!(vector_result(VECTOR, &payload, &receipt), "a reshape keeps its id, a clone is owner-allocated");
    let mut stamp = payload.clone();
    stamp["ops"][1] = json!({"op":"clone","stableTarget":target("L1","s-a"),"frame":3,"offset":[5,-7]});
    assert!(vector_request(VECTOR, &stamp), "a pose stamp clones any stroke shifted, without geometry");
    let anchor = json!({"point":{"x":1,"y":2},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3});
    let mut sculpt = payload.clone();
    sculpt["ops"][0] = json!({"op":"reshape","stableTarget":target("L1","s-a"),"center":[anchor, anchor]});
    assert!(vector_request(VECTOR, &sculpt), "a sculpt reshapes a ribbon by its centreline");
    assert!(vector_result(VECTOR, &sculpt, &receipt));
    for bad in [json!([anchor]), json!([anchor, {"point":{"x":1,"y":2},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}])] {
        let mut p = payload.clone();
        p["ops"][0] = json!({"op":"reshape","stableTarget":target("L1","s-a"),"center":bad});
        assert!(!vector_request(VECTOR, &p), "{p}");
    }
    // #2372 Select: a reshape by a record-space affine of six finite numbers; one receipt id for the named record.
    let mut moved = payload.clone();
    moved["ops"][0] = json!({"op":"reshape","stableTarget":target("L1","s-a"),"matrix":[1,0,0,1,12.5,-4],"frame":2});
    assert!(vector_request(VECTOR, &moved), "a Select move names its record and a matrix");
    assert!(vector_result(VECTOR, &moved, &receipt));
    for bad in [json!([1, 0, 0, 1, 0]), json!([1, 0, 0, 1, 0, 0, 0]), json!([1, 0, 0, 1, 0, "x"]), json!("identity")] {
        let mut p = payload.clone();
        p["ops"][0] = json!({"op":"reshape","stableTarget":target("L1","s-a"),"matrix":bad});
        assert!(!vector_request(VECTOR, &p), "{p}");
    }
    let mut both = payload.clone();
    both["ops"][0]["center"] = json!([anchor, anchor]);
    assert!(!vector_request(VECTOR, &both), "a reshape takes geometry or a centreline, never both");
    both = payload.clone();
    both["ops"][0]["matrix"] = json!([1, 0, 0, 1, 0, 0]);
    assert!(!vector_request(VECTOR, &both), "a matrix excludes geometry too");
    both = payload.clone();
    both["ops"][0]["center"] = json!([anchor, anchor]);
    assert!(!vector_request(VECTOR, &both), "a reshape takes geometry or a centreline, never both");
    for bad in [
        json!({"op":"reshape","stableTarget":target("L1","s-a")}),
        json!({"op":"reshape","stableTarget":target("L1","s-a"),"geometry":geometry,"frame":-3}),
        json!({"op":"reshape","stableTarget":target("L1","s-a"),"geometry":geometry,"placement":"front"}),
        json!({"op":"clone","stableTarget":target("L1","s-a"),"geometry":geometry}),
        json!({"op":"clone","stableTarget":target("L1","s-a"),"geometry":geometry,"frame":3,"offset":[1,2]}),
        json!({"op":"clone","stableTarget":target("L1","s-a"),"frame":3,"offset":[1]}),
        json!({"op":"clone","stableTarget":target("L1","s-a"),"geometry":{"kind":"cubic-path"},"frame":3}),
    ] {
        let mut p = payload.clone();
        p["ops"][1] = bad.clone();
        assert!(!vector_request(VECTOR, &p), "{bad}");
    }
}

/// #2372 SEL-STYLE: a restyle names a record or item and only the style members to change.
#[test]
fn a_restyle_names_its_target_and_valid_style_members_and_its_receipt_id_is_the_target() {
    let step = |style: Value| json!({"op":"restyle","stableTarget":target("L1","s-a"),"style":style});
    let batch = |ops: Vec<Value>| json!({"command":"object.vector.batch","ops":ops});
    let good = batch(vec![step(json!({"fill":"#ff000080","stroke":"#00ff00","width":6,"opacity":0.4})), step(json!({"opacity":0}))]);
    assert!(vector_request(VECTOR, &good));
    let mut framed = good.clone();
    framed["ops"][0]["frame"] = json!(2);
    assert!(vector_request(VECTOR, &framed));
    let receipt = json!({"applied":true,"historyEntriesAdded":1,"operation":"batch","strokeId":"s-a","strokeIds":["s-a","s-a"]});
    assert!(vector_result(VECTOR, &good, &receipt));
    let mut wrong = receipt.clone();
    wrong["strokeIds"] = json!(["s-a","native-stroke:8:0"]);
    assert!(!vector_result(VECTOR, &good, &wrong), "a restyle reports its named target, never an allocated id");
    for bad in [json!({}), json!({"fill":"red"}), json!({"fill":"#12"}), json!({"stroke":"#12345g"}), json!({"fill":null}), json!({"width":0}),
        json!({"width":-1}), json!({"width":20000}), json!({"opacity":1.5}), json!({"opacity":"1"}), json!({"colour":"#000000"})] {
        assert!(!vector_request(VECTOR, &batch(vec![step(bad.clone())])), "{bad}");
    }
    // SEL-STYLE2: the switches, stroke details, preset and feather.
    for ok in [json!({"fillEnabled":true,"fill":"#ff0000"}), json!({"fillEnabled":false}), json!({"strokeEnabled":true,"stroke":"#00ff0080"}),
        json!({"cap":"square","join":"bevel","miter":4,"dashOffset":-3,"paintOrder":"strokeFirst","strokeStyle":"dashed"}),
        json!({"brushPreset":"charcoal-pencil"}), json!({"brushPreset":"none"}), json!({"maskFeather":12.5})] {
        assert!(vector_request(VECTOR, &batch(vec![step(ok.clone())])), "{ok}");
    }
    for bad in [json!({"fillEnabled":true}), json!({"strokeEnabled":true}), json!({"fillEnabled":"yes","fill":"#ff0000"}), json!({"cap":"flat"}),
        json!({"join":"sharp"}), json!({"miter":0.5}), json!({"miter":5000}), json!({"paintOrder":"fill-stroke"}), json!({"strokeStyle":"wavy"}),
        json!({"brushPreset":""}), json!({"brushPreset":7}), json!({"maskFeather":501}), json!({"maskFeather":-1}),
        json!({"brushPreset":"chalk-round","cap":"round"}), json!({"brushPreset":"chalk-round","maskFeather":1}), json!({"maskFeather":3,"fill":"#ff0000"})] {
        assert!(!vector_request(VECTOR, &batch(vec![step(bad.clone())])), "{bad}");
    }
    let mut no_style = good.clone();
    no_style["ops"][0].as_object_mut().unwrap().remove("style");
    assert!(!vector_request(VECTOR, &no_style));
    let mut extra = good.clone();
    extra["ops"][0]["matrix"] = json!([1, 0, 0, 1, 0, 0]);
    assert!(!vector_request(VECTOR, &extra), "a restyle takes no geometry or matrix");
    let mut other_layer = good;
    other_layer["ops"][1]["stableTarget"] = target("L2", "s-b");
    assert!(vector_request(VECTOR, &other_layer), "a Select batch may span layers (#2372)");
    other_layer["ops"][1]["stableTarget"]["contextId"] = json!("component:c1");
    assert!(!vector_request(VECTOR, &other_layer), "every step stays on the scene root");
}
