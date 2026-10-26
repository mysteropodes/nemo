//! #2361: a Draw/Pen/shape create on a held or unexposed cell first makes that cell an empty key, in the same revision
//! and undo entry (baseline `ensureKeyframe(true)` in draw-bridge.js commitStroke and tools.js @1065e9d8). Before, the
//! record was stored at a cell that still showed the previous key, so the new stroke was saved but never shown.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn timing(keys: &[u32], samples: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|k| json!({"frame":k})).collect::<Vec<_>>(),
        "samples":samples.iter().map(|s| json!({"frame":s})).collect::<Vec<_>>()})
}
fn timed(version: u32, keys: &[u32], samples: &[u32], objects: Vec<Value>) -> Value {
    let mut d = document(version, objects, None);
    d["layers"][0]["timing"] = timing(keys, samples);
    d
}
fn keys(owner: &NativeObjectHistory) -> Value {
    state(owner)["layers"][0]["timing"]["keys"].clone()
}
fn at(owner: &NativeObjectHistory, frame: u64) -> Vec<String> {
    objects(&state(owner))
        .iter()
        .filter(|r| {
            r["target"]["layerUid"] == "L1"
                && r["target"]["frameScope"]["frame"].as_u64() == Some(frame)
        })
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_create_on_a_held_cell_keys_it_empty_in_the_same_undo_entry() {
    for version in [1, 2] {
        let mut owner = owner_of(&timed(
            version,
            &[0, 5],
            &[],
            vec![
                record_at("A", "L1", 0, "s-a"),
                record_at("C", "L1", 5, "s-c"),
            ],
        ));
        let before = state(&owner);
        let id = created_id(&send(
            &mut owner,
            "held",
            create(record_at("B", "L1", 2, "")),
        ));
        assert_eq!(
            keys(&owner),
            json!([{"frame":0},{"frame":2},{"frame":5}]),
            "v{version}"
        );
        assert_eq!(
            at(&owner, 2),
            vec![id],
            "a new drawing, not the held one carried forward"
        );
        assert_eq!(at(&owner, 0), vec!["s-a".to_string()]);
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(
            state(&owner),
            before,
            "one undo removes the stroke and the key"
        );
    }
}

#[test]
fn an_unexposed_cell_is_keyed_and_an_inbetween_or_an_untimed_layer_is_left_as_it_is() {
    let mut owner = owner_of(&timed(1, &[3], &[], vec![record_at("A", "L1", 3, "s-a")]));
    created_id(&send(
        &mut owner,
        "blank",
        create(record_at("B", "L1", 1, "")),
    ));
    assert_eq!(keys(&owner), json!([{"frame":1},{"frame":3}]));
    let mut owner = owner_of(&timed(
        1,
        &[0],
        &[4],
        vec![
            record_at("A", "L1", 0, "s-a"),
            record_at("C", "L1", 4, "s-i"),
        ],
    ));
    created_id(&send(
        &mut owner,
        "inbetween",
        create(record_at("B", "L1", 4, "")),
    ));
    assert_eq!(keys(&owner), json!([{"frame":0}]));
    assert_eq!(
        state(&owner)["layers"][0]["timing"]["samples"],
        json!([{"frame":4}])
    );
    let mut owner = owner_of(&document(1, seeds(), None));
    created_id(&send(
        &mut owner,
        "untimed",
        create(record_at("B", "L2", 3, "")),
    ));
    assert!(state(&owner)["layers"][1].get("timing").is_none());
}

#[test]
fn symmetry_copies_key_the_held_cell_once() {
    let mut owner = owner_of(&timed(2, &[0], &[], vec![record_at("A", "L1", 0, "s-a")]));
    let response = send(
        &mut owner,
        "many",
        json!({"command":"object.vector.createMany",
        "records":[record_at("B", "L1", 6, ""), record_at("C", "L1", 6, "")]}),
    );
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(keys(&owner), json!([{"frame":0},{"frame":6}]));
    assert_eq!(at(&owner, 6).len(), 2);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn keyframe_copy_keys_the_held_cell_with_what_it_showed_and_is_admitted_on_creates_only() {
    for version in [1, 2] {
        let mut owner = owner_of(&timed(version, &[0], &[], vec![record_at("A", "L1", 0, "s-a")]));
        let mut payload = create(record_at("B", "L1", 3, ""));
        payload["keyframe"] = json!("copy");
        let id = created_id(&send(&mut owner, "copy", payload));
        assert_eq!(keys(&owner), json!([{"frame":0},{"frame":3}]), "v{version}");
        assert_eq!(at(&owner, 3), vec!["s-a".to_string(), id], "the held drawing is kept below the new stroke");
        assert_eq!(owner.history_depths(), (1, 0));
    }
    let base = timed(1, &[0], &[], vec![record_at("A", "L1", 0, "s-a")]);
    for payload in [
        { let mut p = create(record_at("B", "L1", 3, "")); p["keyframe"] = json!("blank"); p },
        { let mut p = replace(record_at("A", "L1", 0, "s-a")); p["keyframe"] = json!("copy"); p },
    ] {
        let mut owner = owner_of(&base);
        let before = state(&owner);
        let reply = send(&mut owner, "bad", payload.clone());
        assert_eq!(reply["ok"], false, "{payload}");
        assert_eq!(state(&owner), before);
    }
}

// #2361 G15 Mask mode (tools.js tagOwner @1065e9d8): a Draw/Pen/shape commit drawn while Mask is on carries
// `isMask` and the chosen combine mode; the native create stores them as authored and undo removes the mask.
#[test]
fn a_mask_mode_create_stores_its_mask_members() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let before = state(&owner);
    let mut mask = record_at("B", "L1", 0, "");
    mask["vectorExt"] = json!({"isMask":true,"maskMode":"subtract"});
    let response = send(&mut owner, "mask", json!({"command":"object.vector.create","record":mask}));
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let stored = objects(&after).iter().find(|r| r["target"]["strokeId"] == "native-stroke:1:0").cloned().unwrap();
    assert_eq!(stored["vectorExt"], json!({"isMask":true,"maskMode":"subtract"}));
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
}
