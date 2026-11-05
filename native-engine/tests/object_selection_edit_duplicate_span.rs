//! R-lane-a-fixes: an Alt-drag copy of a dynamic or gradient-filled shape moves the `paramShape.box` frame and the gradient ends with the segments (baseline:
//! the clone is made in place and then runs the ordinary move), and a selection that spans layers is cloned into the destination (active) layer in one entry
//! (baseline `duplicateSelection` materialises every selected path into the active layer).
use crate::history::object_history::object_selection_edit_tests::{
    apply, bytes, corners, duplicate, elements_at, owner_of, prepare, record_at, shape, strokes_at, timed, value,
};
use serde_json::{json, Value};

fn pair(stroke: &str, position: u64) -> (String, u64) {
    (stroke.to_string(), position)
}
/// The payload of an Alt-drag duplicate: the baseline duplicate request plus where the copies land.
fn dragged(layer: &str, mode: &str, ids: &[&str], offset: Value) -> Value {
    let mut payload = duplicate(layer, 0, mode, ids);
    payload["offset"] = offset;
    payload
}
fn base(version: u32) -> Value {
    crate::history::object_history::object_selection_edit_tests::base(version)
}
fn document(version: u32, layers: Vec<Value>, objects: Vec<Value>) -> Value {
    crate::history::object_history::object_selection_edit_tests::document(version, layers, objects)
}

/// A dynamic rectangle that also carries a gradient fill (the Alt-drag baseline runs the ordinary move on the clone, which moves the
/// `paramShape.box` frame and both gradient end points with the segments).
fn dynamic_gradient_rect() -> Value {
    let mut record = shape("L0", "r", 0, 0);
    record["family"] = json!("editable-vector-path");
    record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round",
        "strokeJoin":"round","opacity":1});
    record["vectorExt"] = json!({
        "paramShape":{"kind":"rect","tl":2,"box":{"x1":0,"y1":0,"x2":10,"y2":5}},
        "fillGradient":{"kind":"linear","from":[0,0],"to":[10,5],
            "stops":[{"offset":0,"color":"#ff0000"},{"offset":1,"color":"#0000ff"}]}});
    record
}

#[test]
fn an_alt_drag_duplicate_moves_the_dynamic_shape_frame_and_the_gradient_ends_with_the_copy() {
    let source = document(1, vec![timed("L0", "Hero", &[0], &[])], vec![dynamic_gradient_rect()]);
    let mut owner = owner_of(&source);
    let before = value(&owner);
    apply(&mut owner, "duplicate", &dragged("L0", "animation2d", &["r"], json!([30, -7.5]))).unwrap();
    let after = value(&owner);
    let copy = record_at(&after, "L0", 0, "native-stroke:1:0");
    assert_eq!(corners(&copy), [30.0, -7.5, 40.0, -2.5]);
    assert_eq!(copy["vectorExt"]["paramShape"]["box"], json!({"x1":30.0,"y1":-7.5,"x2":40.0,"y2":-2.5}));
    assert_eq!(copy["vectorExt"]["paramShape"]["tl"], json!(2), "the radius is not a position");
    assert_eq!(copy["vectorExt"]["fillGradient"]["from"], json!([30.0, -7.5]));
    assert_eq!(copy["vectorExt"]["fillGradient"]["to"], json!([40.0, -2.5]));
    assert_eq!(
        copy["vectorExt"]["fillGradient"]["stops"],
        record_at(&before, "L0", 0, "r")["vectorExt"]["fillGradient"]["stops"]
    );
    assert_eq!(record_at(&after, "L0", 0, "r"), record_at(&before, "L0", 0, "r"), "the original is untouched");
    owner.undo(1).unwrap();
    assert_eq!(value(&owner), before, "one undo removes the copy");
}

#[test]
fn the_cmd_d_nudge_still_leaves_the_frame_and_the_gradient_as_authored() {
    let source = document(1, vec![timed("L0", "Hero", &[0], &[])], vec![dynamic_gradient_rect()]);
    let mut owner = owner_of(&source);
    apply(&mut owner, "duplicate", &duplicate("L0", 0, "animation2d", &["r"])).unwrap();
    let copy = record_at(&value(&owner), "L0", 0, "native-stroke:1:0");
    assert_eq!(corners(&copy), [12.0, 12.0, 22.0, 17.0]);
    assert_eq!(copy["vectorExt"]["paramShape"]["box"], json!({"x1":0,"y1":0,"x2":10,"y2":5}));
    assert_eq!(copy["vectorExt"]["fillGradient"]["from"], json!([0, 0]));
}

/// A duplicate whose selection also holds shapes of other layers: `others` is [{layerUid, strokeIds}].
fn spanning(layer: &str, frame: u32, mode: &str, ids: &[&str], others: Value) -> Value {
    let mut payload = duplicate(layer, frame, mode, ids);
    payload["others"] = others;
    payload
}

#[test]
fn a_selection_that_spans_layers_is_cloned_into_the_destination_layer_in_one_entry() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let (before_value, before) = (value(&owner), bytes(&owner));
        // L0 is the destination (the active layer); `x` lives on L1.
        let payload = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]));
        assert_eq!(apply(&mut owner, "duplicate", &payload), Ok(true));
        let after = value(&owner);
        assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c", "native-stroke:1:0", "native-stroke:1:1"], "own copy first, then the other layer's");
        assert_eq!(strokes_at(&after, "L1", 0), ["x"], "the other layer is only read");
        assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:0")), [12.0, 12.0, 22.0, 17.0]);
        assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:1")), [92.0, 12.0, 102.0, 17.0], "x at 80 + the 12 px nudge");
        assert_eq!(record_at(&after, "L1", 0, "x"), record_at(&before_value, "L1", 0, "x"));
        assert_eq!((owner.content_revision(), owner.history_depths()), (1, (1, 0)), "one revision, one history entry");
        if version == 2 {
            assert_eq!(elements_at(&after, "L0", 0).last(), Some(&pair("native-stroke:1:1", 4)), "the copies join the top of the destination cell");
        }
        let applied = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before, "v{version}: one undo removes every copy");
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), applied, "v{version}: redo restores them exactly");
    }
}

#[test]
fn the_destination_may_hold_none_of_the_selection_and_a_drag_distance_applies_to_every_copy() {
    let mut owner = owner_of(&base(2));
    let payload = {
        let mut p = spanning("L0", 0, "animation2d", &[], json!([{"layerUid":"L1","strokeIds":["x"]}]));
        p["offset"] = json!([30, -7.5]);
        p
    };
    assert_eq!(apply(&mut owner, "duplicate", &payload), Ok(true));
    let after = value(&owner);
    assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c", "native-stroke:1:0"]);
    assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:0")), [110.0, -7.5, 120.0, -2.5]);
}

#[test]
fn a_source_layer_is_read_at_the_cell_its_playhead_shows_and_the_destination_held_cell_is_promoted() {
    // L1 shows its key 0 at frame 4 (held); L0 holds key 3 at frame 4: Animation 2D keys the destination, Motion edits the key it inherits.
    let layers = vec![timed("L0", "Hero", &[3], &[]), timed("L1", "Side", &[0], &[])];
    let objects = vec![shape("L0", "d", 3, 60), shape("L1", "x", 0, 80)];
    let others = json!([{"layerUid":"L1","strokeIds":["x"]}]);
    let mut animation = owner_of(&document(2, layers.clone(), objects.clone()));
    assert_eq!(apply(&mut animation, "duplicate", &spanning("L0", 4, "animation2d", &[], others.clone())), Ok(true));
    let after = value(&animation);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":3},{"frame":4}]), "the held destination cell became a key");
    assert_eq!(strokes_at(&after, "L0", 4), ["d", "native-stroke:1:0"], "the key holds a copy of what it showed, then the clone of x");
    let mut motion = owner_of(&document(2, layers, objects));
    assert_eq!(apply(&mut motion, "duplicate", &spanning("L0", 4, "motion", &[], others)), Ok(true));
    let after = value(&motion);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":3}]), "Motion edits the key it inherits");
    assert_eq!(strokes_at(&after, "L0", 3), ["d", "native-stroke:1:0"]);
}

#[test]
fn a_spanning_duplicate_refuses_a_malformed_or_unreachable_selection_and_writes_nothing() {
    let owner = owner_of(&base(2));
    let before = bytes(&owner);
    let bad = [
        ("an empty list", spanning("L0", 0, "animation2d", &["a"], json!([]))),
        ("the destination again", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L0","strokeIds":["b"]}]))),
        ("a layer twice", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]},{"layerUid":"L1","strokeIds":["x"]}]))),
        ("no shapes", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":[]}]))),
        ("a stray member", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"],"extra":1}]))),
        ("a repeated shape", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x","x"]}]))),
        ("not a list", spanning("L0", 0, "animation2d", &["a"], json!({"layerUid":"L1","strokeIds":["x"]}))),
    ];
    for (name, payload) in bad {
        let refused = prepare(&owner, "duplicate", &payload);
        assert!(refused.is_err(), "{name}");
    }
    let unknown_layer = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"nope","strokeIds":["x"]}]));
    let unknown_stroke = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["nope"]}]));
    for payload in [unknown_layer, unknown_stroke] {
        assert!(prepare(&owner, "duplicate", &payload).is_err());
    }
    assert_eq!(bytes(&owner), before);
    // A locked destination refuses it whole, as the single-layer duplicate does.
    let mut locked = base(2);
    locked["layers"][0]["locked"] = json!(true);
    let owner = owner_of(&locked);
    assert!(prepare(&owner, "duplicate", &spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]))).is_err());
    // A locked source layer is only read.
    let mut source_locked = base(2);
    source_locked["layers"][1]["locked"] = json!(true);
    let mut owner = owner_of(&source_locked);
    assert_eq!(apply(&mut owner, "duplicate", &spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]))), Ok(true));
}
