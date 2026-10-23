//! Hand-computed cases of the Motion-track repeat kernel: every literal below was worked out from the baseline loop in
//! `repeatSelection` (src/js/timeline.js at fbe4b8a3: clone `k.frame + r * span` for each key with `minF <= frame <= maxF`,
//! append, stable sort by frame). The 125 oracle rows are in object_frame_clipboard_tracks_oracle.rs and the negative controls
//! in object_frame_clipboard_tracks_controls.rs.
use super::*;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_tracks_controls.rs"]
mod controls;
#[path = "object_frame_clipboard_tracks_oracle.rs"]
mod oracle;
#[path = "object_frame_clipboard_tracks_rows.rs"]
#[allow(dead_code)]
pub(crate) mod rows;

use rows::{document, plan_json, tables, uid, Form};

fn cycle(
    doc: &mut ObjectDocument,
    layers: &[usize],
    first: u32,
    last: u32,
    times: u32,
) -> Result<(), DispatchErrorCode> {
    let ids: Vec<String> = layers.iter().map(|index| uid(*index)).collect();
    repeat_motion_tracks(doc, &ids, first, last, times)
}
fn keys_of(doc: &ObjectDocument, layer: usize, property: &str) -> Value {
    tables(doc)[layer][property]["keys"].clone()
}
fn frames_of(doc: &ObjectDocument, layer: usize, property: &str) -> Vec<Value> {
    keys_of(doc, layer, property)
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["frame"].clone())
        .collect()
}
fn rotation(keys: Value) -> Vec<Value> {
    vec![json!({"rotation":{"keys":keys}})]
}

#[test]
fn keys_in_the_range_are_cloned_at_frame_plus_round_times_span_with_every_field() {
    // range 1..=2 (span 2), two rounds: key 1 -> 3, 5; key 2 -> 4, 6; key 9 lies outside and stays.
    let rich = json!({"frame":1,"v":[10],"hold":true,"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[1,2],"hOut":[3,4]});
    let plain = json!({"frame":2,"v":[20]});
    let far = json!({"frame":9,"v":[90]});
    let mut doc = document(
        12,
        &rotation(json!([rich.clone(), plain.clone(), far.clone()])),
        Form::V1,
    );
    cycle(&mut doc, &[0], 1, 2, 2).unwrap();
    let clone_of = |key: &Value, frame: u64| {
        let mut copy = key.clone();
        copy["frame"] = json!(frame);
        copy
    };
    assert_eq!(
        keys_of(&doc, 0, "rotation"),
        json!([
            rich.clone(),
            plain.clone(),
            clone_of(&rich, 3),
            clone_of(&plain, 4),
            clone_of(&rich, 5),
            clone_of(&plain, 6),
            far
        ]),
        "clones carry hold, curve points and both handles unchanged"
    );
}

#[test]
fn the_merge_is_sorted_by_frame_with_existing_keys_between_the_clones() {
    // range 1..=4 (span 4), once: clones at 5 and 8; the existing key 6 falls between them.
    let keys = json!([{"frame":1,"v":[1]},{"frame":4,"v":[4]},{"frame":6,"v":[6]}]);
    let mut doc = document(20, &rotation(keys), Form::V1);
    cycle(&mut doc, &[0], 1, 4, 1).unwrap();
    assert_eq!(
        frames_of(&doc, 0, "rotation"),
        vec![json!(1), json!(4), json!(5), json!(6), json!(8)]
    );
    let values: Vec<Value> = keys_of(&doc, 0, "rotation")
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["v"][0].clone())
        .collect();
    assert_eq!(
        values,
        vec![json!(1), json!(4), json!(1), json!(6), json!(4)],
        "frame 5 is the clone of key 1, frame 8 the clone of key 4"
    );
}

#[test]
fn whole_frames_stay_integer_numbers_and_fractional_ones_stay_floats() {
    // the baseline prints 3 for 1 + 2 and 3.5 for 1.5 + 2: a whole result is never "3.0"
    let keys = json!([{"frame":1,"v":[1]},{"frame":1.5,"v":[2]},{"frame":9,"v":[9]}]);
    let mut doc = document(20, &rotation(keys), Form::V1);
    cycle(&mut doc, &[0], 1, 2, 1).unwrap();
    let frames = frames_of(&doc, 0, "rotation");
    assert_eq!(
        frames,
        vec![json!(1), json!(1.5), json!(3), json!(3.5), json!(9)]
    );
    assert!(frames[2].is_i64() && !frames[2].is_f64(), "{:?}", frames[2]);
    assert!(frames[3].is_f64(), "{:?}", frames[3]);
    // a whole frame written as a float in the source still shifts to an integer, like the JSON round trip
    let keys = json!([{"frame":2.0,"v":[1]}]);
    let mut doc = document(20, &rotation(keys), Form::V1);
    cycle(&mut doc, &[0], 2, 2, 1).unwrap();
    let frames = frames_of(&doc, 0, "rotation");
    assert_eq!(frames[1], json!(3));
    assert!(frames[1].is_i64(), "{:?}", frames[1]);
}

#[test]
fn clones_keep_absent_null_empty_and_explicit_fields_distinct() {
    let keys = json!([
        {"frame":0,"v":[1]},
        {"frame":1,"v":[2],"curvePoints":null},
        {"frame":2,"v":[3],"curvePoints":[]},
        {"frame":3,"v":[4],"hold":false}]);
    let mut doc = document(20, &rotation(keys), Form::V1);
    cycle(&mut doc, &[0], 0, 3, 1).unwrap();
    let keys = keys_of(&doc, 0, "rotation");
    let keys = keys.as_array().unwrap();
    assert_eq!(keys.len(), 8);
    for index in 0..4 {
        assert_eq!(
            keys[index + 4]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            keys[index].as_object().unwrap().keys().collect::<Vec<_>>(),
            "key {index} and its clone carry the same fields"
        );
    }
    assert!(keys[4].get("curvePoints").is_none() && keys[4].get("hold").is_none());
    assert!(keys[5].get("curvePoints").is_some_and(Value::is_null));
    assert_eq!(keys[6]["curvePoints"], json!([]));
    assert_eq!(keys[7]["hold"], json!(false));
}

#[test]
fn only_covered_layers_scene_root_layer_level_tracks_are_cycled() {
    // three layers, each with a rotation track at frame 1; the cycle covers L0 and L1 over 1..=1 once.
    let one = |v: i64| rotation(json!([{"frame":1,"v":[v]}])).remove(0);
    let tables_in = vec![one(1), one(2), one(3)];
    let mut doc = document(10, &tables_in, Form::V1);
    cycle(&mut doc, &[0, 1], 1, 1, 1).unwrap();
    assert_eq!(frames_of(&doc, 0, "rotation"), vec![json!(1), json!(2)]);
    assert_eq!(frames_of(&doc, 1, "rotation"), vec![json!(1), json!(2)]);
    assert_eq!(
        frames_of(&doc, 2, "rotation"),
        vec![json!(1)],
        "L2 is outside the bounds"
    );
    assert_eq!(keys_of(&doc, 1, "rotation")[1], json!({"frame":2,"v":[2]}));
}

#[test]
fn static_values_unrelated_properties_and_expressions_are_not_touched() {
    let tables_in = vec![json!({
        "opacity":{"keys":[{"frame":0,"v":[100]},{"frame":4,"v":[0]}]},
        "scale":{"keys":[]}})];
    let mut value = rows::document_json(10, &tables_in, Form::V1);
    let holder = &mut value["propertyHolders"][0];
    for record in holder["properties"].as_array_mut().unwrap() {
        if record["property"] == "opacity" {
            record["static"] = json!([17]);
            record["expression"] = json!({"enabled":true,"code":"1"});
        }
    }
    let mut doc = rows::decode(&value);
    cycle(&mut doc, &[0], 0, 0, 1).unwrap();
    let wire = serde_json::to_value(&doc).unwrap();
    let records = wire["propertyHolders"][0]["properties"].as_array().unwrap();
    let opacity = records.iter().find(|r| r["property"] == "opacity").unwrap();
    assert_eq!(opacity["static"], json!([17]));
    assert_eq!(opacity["expression"], json!({"enabled":true,"code":"1"}));
    assert_eq!(
        opacity["track"]["keys"],
        json!([{"frame":0,"v":[100]},{"frame":1,"v":[100]},{"frame":4,"v":[0]}])
    );
    let scale = records.iter().find(|r| r["property"] == "scale").unwrap();
    assert_eq!(
        scale["track"]["keys"],
        json!([]),
        "an empty track is skipped, as the baseline does"
    );
}

#[test]
fn a_range_without_keys_and_zero_rounds_change_nothing() {
    let keys = json!([{"frame":6,"v":[1]},{"frame":8,"v":[2]}]);
    let mut doc = document(20, &rotation(keys), Form::V1);
    let before = doc.clone();
    cycle(&mut doc, &[0], 1, 4, 3).unwrap();
    assert_eq!(doc, before, "no key lies in 1..=4");
    cycle(&mut doc, &[0], 6, 8, 0).unwrap();
    assert_eq!(doc, before, "no rounds, no clones");
    cycle(&mut doc, &[], 6, 8, 2).unwrap();
    assert_eq!(doc, before, "no layer is covered");
    let mut bare = document(5, &[Value::Null], Form::V2);
    let bare_before = bare.clone();
    cycle(&mut bare, &[0], 0, 4, 2).unwrap();
    assert_eq!(bare, bare_before, "a layer with no tracks");
}

#[test]
fn a_clone_on_an_existing_key_refuses_and_leaves_every_layer_as_it_was() {
    // L0 would be cycled fine (key 0 -> 2), but L1 already has a key at 2 where its key 0 would land.
    let tables_in = vec![
        json!({"rotation":{"keys":[{"frame":0,"v":[1]}]}}),
        json!({"rotation":{"keys":[{"frame":0,"v":[2]},{"frame":2,"v":[3]}]}}),
    ];
    for form in [Form::V1, Form::V2] {
        let mut doc = document(10, &tables_in, form);
        let before = doc.clone();
        assert_eq!(
            cycle(&mut doc, &[0, 1], 0, 1, 1),
            Err(DispatchErrorCode::InvalidRequest),
            "{form:?}"
        );
        assert_eq!(doc, before, "{form:?}: not even L0 changed");
        // the same cycle over L0 alone is fine
        cycle(&mut doc, &[0], 0, 1, 1).unwrap();
        assert_eq!(frames_of(&doc, 0, "rotation"), vec![json!(0), json!(2)]);
    }
}

#[test]
fn bad_ranges_and_more_rounds_than_the_baseline_clamp_refuse() {
    let keys = json!([{"frame":0,"v":[1]}]);
    let mut doc = document(10, &rotation(keys), Form::V1);
    let before = doc.clone();
    assert_eq!(
        cycle(&mut doc, &[0], 3, 2, 1),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        cycle(&mut doc, &[0], 0, 0, MAX_ROUNDS + 1),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(doc, before);
    cycle(&mut doc, &[0], 0, 0, MAX_ROUNDS).unwrap();
    let frames = frames_of(&doc, 0, "rotation");
    assert_eq!(frames.len(), 51);
    assert_eq!(frames[50], json!(50), "round 50 of a one-frame span");
}

#[test]
fn the_scene_root_order_track_is_cycled_and_another_context_is_not() {
    let order = json!({"keys":[{"frame":0,"v":[1]},{"frame":1,"v":[2]},{"frame":7,"v":[3]}]});
    let other = json!({"keys":[{"frame":0,"v":[9]},{"frame":1,"v":[8]}]});
    let mut value = rows::document_json(10, &[Value::Null, Value::Null], Form::V2);
    value["compositionOrder"] = plan_json(&[Some(order), None], Some(other.clone()));
    // a ctx-x holder in range, and a scene-root holder of L1, which is outside the bounds
    value["propertyHolders"] = json!([
        {"target":{"contextId":"ctx-x","layerUid":"L0"},"properties":[{"property":"rotation","track":other}]},
        {"target":{"contextId":"scene-root","layerUid":"L1"},"properties":[{"property":"rotation","track":other}]}]);
    let mut doc = rows::decode(&value);
    let before = serde_json::to_value(&doc).unwrap();
    cycle(&mut doc, &[0], 0, 1, 2).unwrap();
    let after = serde_json::to_value(&doc).unwrap();
    let scene = &after["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"];
    let frames: Vec<&Value> = scene
        .as_array()
        .unwrap()
        .iter()
        .map(|k| &k["frame"])
        .collect();
    assert_eq!(
        frames,
        vec![
            &json!(0),
            &json!(1),
            &json!(2),
            &json!(3),
            &json!(4),
            &json!(5),
            &json!(7)
        ],
        "keys 0 and 1 -> 2,4 and 3,5; key 7 stays"
    );
    assert_eq!(scene[2], json!({"frame":2,"v":[1]}));
    assert_eq!(scene[3], json!({"frame":3,"v":[2]}));
    let mut expected = before.clone();
    expected["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"] =
        scene.clone();
    assert_eq!(
        after, expected,
        "the ctx-x plan layer, the ctx-x holder, L1 and every other field are unchanged"
    );
}

#[test]
fn element_holders_are_not_cycled_the_baseline_loop_never_reads_element_motion() {
    use build::{Build, Cell, Stroke};
    let cell = |flag: char, id: &str| Cell {
        flag,
        strokes: if id.is_empty() {
            Vec::new()
        } else {
            vec![Stroke {
                id: id.to_owned(),
                z: 1,
                g: 10,
            }]
        },
    };
    let layers = vec![vec![cell('K', "s1"), cell('.', ""), cell('.', "")]];
    let mut value = build::document_json(&layers, Build::V1);
    let track = json!({"keys":[{"frame":0,"v":[5]},{"frame":1,"v":[6]}]});
    value["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{"property":"rotation","track":track}]},
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"s1"},"properties":[{"property":"rotation","track":track}]}]);
    let mut doc = rows::decode(&value);
    cycle(&mut doc, &[0], 0, 1, 1).unwrap();
    let wire = serde_json::to_value(&doc).unwrap();
    let layer_frames: Vec<Value> = wire["propertyHolders"][0]["properties"][0]["track"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(layer_frames, vec![json!(0), json!(1), json!(2), json!(3)]);
    assert_eq!(
        wire["propertyHolders"][1]["properties"][0]["track"], track,
        "the element holder (stroke s1) keeps its two keys"
    );
}
