//! #2372 MULTI-LAYER-BATCH: one `object.vector.batch` may carry steps for several scene-root layers (a Select over a marquee or a
//! Shift-click across layers): one revision, one undo entry, each step guarded by its own layer, a refusal refuses everything.
use super::support::*;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn restyle(layer: &str, id: &str) -> Value {
    json!({"op":"restyle","stableTarget":target(layer, 0, id),"style":{"width":9.0}})
}
fn move_by(layer: &str, id: &str) -> Value {
    json!({"op":"reshape","stableTarget":target(layer, 0, id),"matrix":[1.0,0.0,0.0,1.0,5.0,7.0]})
}
fn at(state: &Value, id: &str) -> Value {
    objects(state).iter().find(|r| r["target"]["strokeId"] == id).unwrap().clone()
}

#[test]
fn a_two_layer_restyle_and_move_are_each_one_revision_and_one_exact_undo() {
    for ops in [
        vec![restyle("L1", "s-a"), restyle("L2", "s-b")],
        vec![move_by("L1", "s-a"), move_by("L2", "s-b")],
        vec![restyle("L1", "s-a"), move_by("L2", "s-b")],
    ] {
        let mut owner = owner_of(&document(1, seeds(), None));
        let before = state(&owner);
        let r = send(&mut owner, "two", batch(ops.clone()));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["strokeIds"], json!(["s-a", "s-b"]), "one receipt id per requested step");
        assert_eq!(r["result"]["historyEntriesAdded"], 1);
        assert_eq!(owner.history_depths(), (1, 0), "one history entry for both layers");
        let after = state(&owner);
        for (id, layer) in [("s-a", "L1"), ("s-b", "L2")] {
            assert_ne!(at(&after, id), at(&before, id), "{id} on {layer} changed");
        }
        assert_eq!(at(&after, "s-c"), at(&before, "s-c"), "an unnamed record is untouched");
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before, "one undo restores both layers exactly");
        owner.redo(2).unwrap();
        assert_eq!(state(&owner), after);
    }
}

#[test]
fn a_locked_layer_in_the_batch_refuses_the_whole_batch_with_nothing_written() {
    for second in [restyle("L2", "s-b"), move_by("L2", "s-b")] {
        for locked in [0usize, 1] {
            let mut source = document(1, seeds(), None);
            source["layers"][locked]["locked"] = json!(true);
            let mut owner = owner_of(&source);
            let before = state(&owner);
            let r = send(&mut owner, "locked", batch(vec![restyle("L1", "s-a"), second.clone()]));
            assert_eq!(r["ok"], false, "{r}");
            assert_eq!((state(&owner), owner.history_depths(), owner.content_revision()), (before, (0, 0), 0), "layer {locked} locked");
        }
    }
}

#[test]
fn a_missing_target_on_the_second_layer_refuses_everything() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let before = state(&owner);
    let r = send(&mut owner, "missing", batch(vec![restyle("L1", "s-a"), restyle("L2", "nope")]));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)));
}

#[test]
fn copy_keyed_edits_on_held_cells_of_two_layers_key_each_layer_and_undo_as_one() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a"), record_at("B", "L2", 0, "s-b")], None);
    for layer in 0..2 {
        doc["layers"][layer]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    }
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    let held = |layer: &str, id: &str| json!({"op":"reshape","stableTarget":target(layer, 0, id),"matrix":[1.0,0.0,0.0,1.0,3.0,0.0],"frame":3});
    let mut payload = batch(vec![held("L1", "s-a"), held("L2", "s-b")]);
    payload["keyframe"] = json!("copy");
    let r = send(&mut owner, "held", payload);
    assert_eq!(r["ok"], true, "{r}");
    let after = state(&owner);
    for layer in 0..2 {
        assert_eq!(after["layers"][layer]["timing"]["keys"], json!([{"frame":0},{"frame":3}]), "layer {layer} keyed at the edited frame");
    }
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
}

#[test]
fn a_trimmed_batch_stays_on_one_layer_and_another_context_is_refused() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let before = state(&owner);
    let mut trimmed = batch(vec![json!({"op":"create","record":record_at("B", "L1", 0, "")}), json!({"op":"create","record":record_at("C", "L2", 0, "")})]);
    trimmed["trimEnds"] = json!(4);
    let mut other_context = batch(vec![restyle("L1", "s-a"), restyle("L2", "s-b")]);
    other_context["ops"][1]["stableTarget"]["contextId"] = json!("component:c1");
    for payload in [trimmed, other_context] {
        let r = send(&mut owner, "bad", payload);
        assert_eq!(r["ok"], false, "{r}");
        assert_eq!((state(&owner), owner.history_depths()), (before.clone(), (0, 0)));
    }
}
