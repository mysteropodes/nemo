//! #2371 / #2372: the in/out bar context menu (`layer.range.arrange`) through the actual owner, history and dispatcher.
//! The fixture is the L02-F document with a fourth layer, ranges L0 = [2,8], L1 = [5,15],
//! L2 = [10,12] and L3 = [12,20]; every expectation is a hand-derived literal read back through plain `serde_json::Value`
//! projections, and effective edges are read with the same immutable range evaluator the layer bars use.
use super::object_layer_lifecycle_tests::{base, bytes, owner_of, send, timing, value};
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::RangeEvaluator;
use serde_json::{json, Value};

fn fixture() -> Value {
    let mut document = base(2);
    document["layers"].as_array_mut().unwrap().push(json!({
        "layerUid":"L3","name":"D","visible":true,"locked":false,"color":"#5fa875",
        "parentLayerUid":null,"timing":timing()}));
    for context in document["compositionOrder"]["contexts"].as_array_mut().unwrap() {
        context["layers"].as_array_mut().unwrap().push(json!({"layerUid":"L3","naturalPosition":3,"order":{}}));
    }
    for (at, edges) in [[2, 8], [5, 15], [10, 12], [12, 20]].iter().enumerate() {
        document["layers"][at]["range"] = json!({"inPoint":edges[0],"outPoint":edges[1]});
    }
    document
}
fn arrange(action: &str, layers: &[&str]) -> Value {
    json!({"command":"object.layer.range.arrange","contextId":"scene-root","layerUids":layers,
        "action":action,"frame":0})
}
fn stagger(step: i64, layers: &[&str]) -> Value {
    let mut payload = arrange("stagger", layers);
    payload["step"] = json!(step);
    payload
}
fn edges(owner: &NativeObjectHistory, uid: &str) -> (f64, f64) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let range = RangeEvaluator::for_document(snapshot.document(), "scene-root", 0)
        .unwrap()
        .resolve(uid, true)
        .unwrap();
    (range.in_point, range.out_point)
}
fn all(owner: &NativeObjectHistory) -> Vec<(f64, f64)> {
    ["L0", "L1", "L2", "L3"]
        .iter()
        .map(|uid| edges(owner, uid))
        .collect()
}
fn offset(document: &Value, layer: &str, property: &str) -> Option<f64> {
    document["propertyHolders"]
        .as_array()?
        .iter()
        .find(|h| h["target"]["layerUid"] == layer && h["target"].get("strokeId").is_none())?
        ["properties"]
        .as_array()?
        .iter()
        .find(|p| p["property"] == property)?["static"][0]
        .as_f64()
}
fn applied(owner: &mut NativeObjectHistory, id: &str, payload: &Value) {
    let reply = send(owner, id, "range.arrange", payload);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{id}: {reply}"
    );
}
fn refused(owner: &mut NativeObjectHistory, id: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let reply = send(owner, id, "range.arrange", payload);
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(reply["error"]["code"], code, "{id}: {reply}");
    assert_eq!(bytes(owner), before, "{id} bytes");
    assert_eq!(
        (owner.history_depths(), owner.content_revision()),
        (depths, revision),
        "{id}"
    );
}

#[test]
fn each_batch_operation_lands_the_baseline_ranges_as_one_undoable_entry() {
    let cases: [(&str, Value, [(f64, f64); 3]); 6] = [
        // alignBars('left'): every in point to the smallest in, widths kept
        ("left", arrange("alignLeft", &["L0", "L1", "L2"]), [(2.0, 8.0), (2.0, 12.0), (2.0, 4.0)]),
        // alignBars('right'): every out point to the largest out
        ("right", arrange("alignRight", &["L0", "L1", "L2"]), [(9.0, 15.0), (5.0, 15.0), (13.0, 15.0)]),
        // centres 5, 10, 11 average 8.667: starts round(5.667), round(3.667), round(7.667)
        ("center", arrange("alignCenter", &["L0", "L1", "L2"]), [(6.0, 12.0), (4.0, 14.0), (8.0, 10.0)]),
        // distributeBars: starts 2, 6 (the midpoint of 2 and 10) and 10
        ("distribute", arrange("distribute", &["L0", "L1", "L2"]), [(2.0, 8.0), (6.0, 16.0), (10.0, 12.0)]),
        // flipBars: the slots 2, 5, 10 reversed
        ("flip", arrange("flip", &["L0", "L1", "L2"]), [(10.0, 16.0), (5.0, 15.0), (2.0, 4.0)]),
        // staggerBars(3): the leftmost bar anchors, then +3 per bar
        ("stagger", stagger(3, &["L0", "L1", "L2"]), [(2.0, 8.0), (5.0, 15.0), (8.0, 10.0)]),
    ];
    for (name, payload, want) in cases {
        let mut owner = owner_of(&fixture());
        let before = bytes(&owner);
        applied(&mut owner, name, &payload);
        let got = all(&owner);
        assert_eq!(&got[..3], &want[..], "{name}");
        assert_eq!(got[3], (12.0, 20.0), "{name}: a bar outside the selection is untouched");
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0)),
            "{name}: one revision, one entry"
        );
        let kept = bytes(&owner);
        assert_eq!(owner.undo(1).unwrap(), 2, "{name}");
        assert_eq!(bytes(&owner), before, "{name} undo");
        assert_eq!(owner.redo(2).unwrap(), 3, "{name}");
        assert_eq!(bytes(&owner), kept, "{name} redo");
    }
}

#[test]
fn stagger_clamps_at_frame_zero_and_follows_the_listed_order_for_equal_starts() {
    let mut owner = owner_of(&fixture());
    applied(&mut owner, "neg", &stagger(-9, &["L0", "L1", "L2"]));
    assert_eq!(
        &all(&owner)[..3],
        &[(2.0, 8.0), (0.0, 10.0), (0.0, 2.0)],
        "base 2, then max(0, 2 - 9) and max(0, 2 - 18)"
    );
    let mut tie = fixture();
    tie["layers"][1]["range"] = json!({"inPoint":2,"outPoint":6});
    let mut owner = owner_of(&tie);
    // L0 and L1 both start at 2: the stable sort keeps the listed order, so L1 (listed first) anchors and L0 is second
    applied(&mut owner, "tie", &stagger(5, &["L1", "L0"]));
    assert_eq!(&all(&owner)[..2], &[(7.0, 13.0), (2.0, 6.0)]);
}

#[test]
fn a_staggered_child_of_each_of_two_parents_moves_its_offset_not_its_hard_points() {
    let mut owner = owner_of(&fixture());
    // L2 follows parent L0, L3 follows parent L1 (offsets seeded from the current gaps)
    for (id, child, parent) in [("k2", "L2", "L0"), ("k3", "L3", "L1")] {
        let reply = send(
            &mut owner,
            id,
            "timeLink.set",
            &json!({"command":"object.layer.timeLink.set","contextId":"scene-root","layerUid":child,
                "sourceLayerUid":parent,"frame":0}),
        );
        assert_eq!(reply["result"]["applied"], true, "{reply}");
    }
    let linked = value(&owner);
    assert_eq!(offset(&linked, "L2", "timeLinkInOffset"), Some(8.0));
    assert_eq!(offset(&linked, "L3", "timeLinkInOffset"), Some(7.0));
    assert_eq!(offset(&linked, "L3", "timeLinkOutOffset"), Some(5.0));
    let before = bytes(&owner);
    let depths = owner.history_depths();
    applied(&mut owner, "st", &stagger(4, &["L0", "L1", "L2", "L3"]));
    // L0 stays 2, L1 moves to 6 (hard points 6, 16); L2 asks for 10 and already sits there; L3 asks for 14 while its
    // parent L1 now sits at [6,16]: the new offsets are 14 - 6 = 8 and 22 - 16 = 6
    assert_eq!(
        all(&owner),
        vec![(2.0, 8.0), (6.0, 16.0), (10.0, 12.0), (14.0, 22.0)]
    );
    let after = value(&owner);
    assert_eq!(offset(&after, "L3", "timeLinkInOffset"), Some(8.0));
    assert_eq!(offset(&after, "L3", "timeLinkOutOffset"), Some(6.0));
    assert_eq!(after["layers"][3]["range"]["inPoint"], 12, "the child's hard points are inert and untouched");
    assert_eq!(after["layers"][3]["range"]["outPoint"], 20);
    assert_eq!(after["layers"][1]["range"]["inPoint"], 6);
    assert_eq!(owner.history_depths(), (depths.0 + 1, 0), "one entry for the whole stagger");
    let kept = bytes(&owner);
    assert_eq!(owner.undo(owner.content_revision()).unwrap(), 4);
    assert_eq!(bytes(&owner), before, "one undo restores both parents and both children");
    assert_eq!(owner.redo(4).unwrap(), 5);
    assert_eq!(bytes(&owner), kept);
}

#[test]
fn a_batch_edit_survives_save_and_reopen_byte_for_byte() {
    let mut owner = owner_of(&fixture());
    applied(&mut owner, "d", &arrange("distribute", &["L0", "L1", "L2", "L3"]));
    let reopened =
        NativeObjectHistory::new("again", decode_storage_project(&bytes(&owner)).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), bytes(&owner));
    assert_eq!(all(&reopened), all(&owner));
    // starts 2, 5, 10, 12 spread over 2..12 in thirds: 2, 5.333 -> 5, 8.667 -> 9 and 12, each width kept
    assert_eq!(
        all(&owner),
        vec![(2.0, 8.0), (5.0, 15.0), (9.0, 11.0), (12.0, 20.0)]
    );
}

#[test]
fn a_batch_that_changes_nothing_publishes_nothing() {
    let mut document = fixture();
    document["layers"][1]["range"] = json!({"inPoint":2,"outPoint":12});
    let mut owner = owner_of(&document);
    let reply = send(&mut owner, "n", "range.arrange", &arrange("alignLeft", &["L0", "L1"]));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn reset_clears_only_the_hard_points_and_keeps_the_link_markers_and_the_other_bars() {
    let mut document = fixture();
    document["layers"][1]["range"] =
        json!({"inPoint":5,"outPoint":15,"markers":[{"frame":7}],"timeLink":{"uid":"L0","mode":"both"}});
    let mut owner = owner_of(&document);
    let before = bytes(&owner);
    applied(&mut owner, "r0", &arrange("reset", &["L0"]));
    let after = value(&owner);
    assert!(
        after["layers"][0].get("range").is_none(),
        "an emptied range is not stored"
    );
    assert_eq!(edges(&owner, "L0"), (0.0, 23.0), "the full duration");
    assert_eq!(after["layers"][2]["range"], json!({"inPoint":10,"outPoint":12}));
    applied(&mut owner, "r1", &arrange("reset", &["L1"]));
    let range = &value(&owner)["layers"][1]["range"];
    assert_eq!(
        range,
        &json!({"markers":[{"frame":7}],"timeLink":{"uid":"L0","mode":"both"}}),
        "Parent in Time and markers stay, like the baseline reset"
    );
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(owner.undo(2).unwrap(), 3);
    assert_eq!(owner.undo(3).unwrap(), 4);
    assert_eq!(bytes(&owner), before);
    // a layer with no stored range, and a bar already reset, publish nothing
    let mut owner = owner_of(&base(2));
    let reply = send(&mut owner, "r2", "range.arrange", &arrange("reset", &["L0"]));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
}

#[test]
fn batch_refusals_are_typed_and_change_nothing() {
    let mut owner = owner_of(&fixture());
    refused(&mut owner, "one", &arrange("alignLeft", &["L0"]), "invalid_request");
    refused(&mut owner, "none", &arrange("reset", &[]), "invalid_request");
    refused(&mut owner, "dup", &arrange("flip", &["L0", "L0"]), "invalid_request");
    refused(&mut owner, "blank", &arrange("flip", &["L0", ""]), "invalid_request");
    refused(&mut owner, "ghost", &arrange("flip", &["L0", "nope"]), "not_found");
    refused(&mut owner, "ghost-reset", &arrange("reset", &["nope"]), "not_found");
    refused(&mut owner, "action", &arrange("shuffle", &["L0", "L1"]), "invalid_request");
    let mut bare = arrange("stagger", &["L0", "L1"]);
    refused(&mut owner, "no-step", &bare, "invalid_request");
    bare["step"] = json!(0);
    refused(&mut owner, "zero-step", &bare, "invalid_request");
    bare["step"] = json!(1.5);
    refused(&mut owner, "fraction", &bare, "invalid_request");
    let mut stray = arrange("alignLeft", &["L0", "L1"]);
    stray["step"] = json!(2);
    refused(&mut owner, "stray-step", &stray, "invalid_request");
    let mut reset_step = arrange("reset", &["L0"]);
    reset_step["step"] = json!(2);
    refused(&mut owner, "reset-step", &reset_step, "invalid_request");
    for context in ["unused-symbol", "elsewhere"] {
        let mut foreign = arrange("alignLeft", &["L0", "L1"]);
        foreign["contextId"] = json!(context);
        refused(&mut owner, context, &foreign, "invalid_request");
    }
    let mut late = arrange("alignLeft", &["L0", "L1"]);
    late["frame"] = json!(24);
    refused(&mut owner, "frame", &late, "invalid_request");
    let mut extra = arrange("alignLeft", &["L0", "L1"]);
    extra["extra"] = json!(1);
    refused(&mut owner, "extra", &extra, "invalid_request");
    let mut wrong = arrange("alignLeft", &["L0", "L1"]);
    wrong["command"] = json!("object.layer.range.drag");
    refused(&mut owner, "command", &wrong, "invalid_request");
    // a stale pin is refused by the shared envelope before anything is planned
    let mut stale = super::object_layer_lifecycle_tests::request(
        &owner,
        "stale",
        "range.arrange",
        &arrange("alignLeft", &["L0", "L1"]),
    );
    stale["expectedRevision"] = json!(owner.content_revision() + 1);
    let reply = super::object_layer_lifecycle_tests::dispatch(&mut owner, &stale);
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "stale_revision", "{reply}");
}
