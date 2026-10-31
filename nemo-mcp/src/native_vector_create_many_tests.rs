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

#[test]
fn trim_ends_is_an_optional_positive_limit_on_the_two_creates_only() {
    let records = json!([record(), record()]);
    for command in ["object.vector.create", "object.vector.createMany"] {
        let body = if command == "object.vector.create" { json!({"record":record()}) } else { json!({"records":records}) };
        for (trim, ok) in [(json!(24), true), (json!(0.5), true), (json!(0), false), (json!(-1), false), (json!("24"), false), (json!(null), false)] {
            let mut payload = body.clone();
            payload["command"] = json!(command);
            payload["trimEnds"] = trim.clone();
            payload["placement"] = json!("behind");
            assert_eq!(request(VECTOR, &payload), ok, "{command} {trim}");
        }
    }
    let delete = json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":"s"},"trimEnds":24});
    assert!(!request(VECTOR, &delete));
}

#[test]
fn trim_clip_is_a_well_formed_axis_clip_beside_trim_ends_on_the_creates_and_the_batch() {
    let clip = json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0,1]]});
    let many = json!({"command":"object.vector.createMany","records":[record(), record()],"trimEnds":24,"trimClip":clip});
    assert!(request(VECTOR, &many));
    let mut one = many.clone();
    one["command"] = json!("object.vector.create");
    one["record"] = record();
    one.as_object_mut().unwrap().remove("records");
    assert!(request(VECTOR, &one));
    let with = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut payload = many.clone();
        f(&mut payload);
        request(VECTOR, &payload)
    };
    assert!(!with(&|p| { p.as_object_mut().unwrap().remove("trimEnds"); }), "needs trimEnds");
    for (name, bad) in [
        ("short axis", json!({"axis":[60.0,0.0,60.0],"mirrors":[[0,1]]})),
        ("text axis", json!({"axis":[60.0,0.0,60.0,"100"],"mirrors":[[0,1]]})),
        ("no mirrors", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[]})),
        ("no axis", json!({"mirrors":[[0,1]]})),
        ("odd pair", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0]]})),
        ("negative index", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0,-1]]})),
        ("fractional index", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0,1.5]]})),
        ("reused index", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0,1],[1,2]]})),
        ("same index", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[1,1]]})),
        ("extra member", json!({"axis":[60.0,0.0,60.0,100.0],"mirrors":[[0,1]],"extend":true})),
        ("not an object", json!(null)),
    ] {
        assert!(!with(&|p| p["trimClip"] = bad.clone()), "{name}");
    }
    let delete = json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":"s"},"trimClip":clip});
    assert!(!request(VECTOR, &delete));
}

#[test]
fn keyframe_copy_is_an_optional_member_of_the_two_creates_only() {
    let records = json!([record(), record()]);
    for command in ["object.vector.create", "object.vector.createMany"] {
        let body = if command == "object.vector.create" { json!({"record":record()}) } else { json!({"records":records}) };
        for (keyframe, ok) in [(json!("copy"), true), (json!("blank"), false), (json!(true), false), (json!(null), false)] {
            let mut payload = body.clone();
            payload["command"] = json!(command);
            payload["keyframe"] = keyframe.clone();
            assert_eq!(request(VECTOR, &payload), ok, "{command} {keyframe}");
        }
    }
    let delete = json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":"s"},"keyframe":"copy"});
    assert!(!request(VECTOR, &delete));
}

#[test]
fn brush_texture_members_are_admitted_as_serp_writes_them_and_nothing_else() {
    for (tags, ok) in [
        (json!({"brushGroupId":"bg1","brushTexturePreset":"charcoal","preTextureOpacity":1}), true),
        (json!({"brushGroupId":"bg2","brushTexturePreset":"ink","preTextureStroke":"#112233ff"}), true),
        (json!({"brushGroupId":"bg3","brushTexturePreset":"ink","preTextureStroke":null}), true),
        (json!({"brushGroupId":"bg1","isBrushTextureCopy":true}), true),
        (json!({"brushGroupId":"lone"}), true),
        (json!({"brushGroupId":"bg4","brushTexturePreset":"ink","preTextureOpacity":1,"strokeSeed":4294967295u64}), true),
        (json!({"brushGroupId":"g","isBrushTextureCopy":true,"strokeSeed":1}), false),
        (json!({"brushGroupId":"g","strokeSeed":1}), false),
        (json!({"brushTexturePreset":"p","strokeSeed":4294967296u64}), false),
        (json!({"brushTexturePreset":"p","strokeSeed":1.5}), false),
        (json!({}), false),
        (json!({"isBrushTextureCopy":false}), false),
        (json!({"brushGroupId":""}), false),
        (json!({"brushGroupId":"g","isBrushTextureCopy":true,"brushTexturePreset":"p"}), false),
        (json!({"preTextureStroke":"red"}), false),
        (json!({"preTextureOpacity":"1"}), false),
        (json!({"brushGroupId":"g","extra":1}), false),
    ] {
        let mut r = record();
        r["vectorExt"] = json!({"brushTexture":tags});
        let payload = json!({"command":"object.vector.create","record":r});
        assert_eq!(request(VECTOR, &payload), ok, "{tags}");
    }
}

#[test]
fn shadow_channel_members_are_admitted_as_serp_writes_them_and_nothing_else() {
    for (shadow, ok) in [
        (json!({"channelTag":"shadow","shadowSwatchId":"sh1"}), true),
        (json!({"channelTag":"shadow"}), true),
        (json!({"channelTag":"fill"}), false),
        (json!({"shadowSwatchId":"sh1"}), false),
        (json!({"channelTag":"shadow","shadowSwatchId":""}), false),
        (json!({"channelTag":"shadow","shadowSwatchId":7}), false),
        (json!({"channelTag":"shadow","extra":1}), false),
    ] {
        let mut r = record();
        r["vectorExt"] = json!({"shadow":shadow});
        let payload = json!({"command":"object.vector.create","record":r});
        assert_eq!(request(VECTOR, &payload), ok, "{shadow}");
    }
}

#[test]
fn the_fill_brush_flag_is_admitted_only_as_serp_writes_it() {
    for (flag, ok) in [(json!(true), true), (json!(false), false), (json!("true"), false), (json!(null), false)] {
        let mut r = record();
        r["vectorExt"] = json!({"isFillShape":flag});
        let payload = json!({"command":"object.vector.create","record":r});
        assert_eq!(request(VECTOR, &payload), ok, "{flag}");
    }
}
