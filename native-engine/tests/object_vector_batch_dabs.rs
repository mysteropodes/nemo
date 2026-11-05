//! #2372 (A-bitmap-brush-edit): a texture anchor whose dabs are STORED records (an imported project, a tween frame) stamps them again from
//! its preset after a width, a stroke colour or a preset edit (the stored dabs are deleted in the same revision, the seeded anchor paints
//! natively), and a dab with no anchor left is an ordinary record. Baseline `regenerateBrushTexture`.
use super::super::support::{document, owner_of, record_at, send, state as bare_state, target};
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn textured(extra: Value) -> Value {
    let anchor_ext = json!({"brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1}});
    let copy_ext = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
    let with = |id: &str, ext: &Value| {
        let mut r = record_at("A", "L1", 0, id);
        r["vectorExt"] = ext.clone();
        r
    };
    let mut records = vec![
        with("anchor", &anchor_ext),
        with("dab-1", &copy_ext),
        with("dab-2", &copy_ext),
    ];
    if extra.is_object() {
        records.push(extra);
    }
    document(1, records, None)
}
fn ids(owner: &NativeObjectHistory) -> Vec<String> {
    bare_state(owner)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}
fn rs(id: &str, style: Value) -> Value {
    json!({"op":"restyle","stableTarget":target("L1", 0, id),"style":style})
}

#[test]
fn stored_dabs_are_stamped_again_from_the_anchor_after_a_width_ink_or_preset_edit() {
    for (name, style) in [
        ("width", json!({"width":12})),
        ("ink", json!({"stroke":"#ff0000"})),
        ("preset", json!({"brushPreset":"chalk-round"})),
    ] {
        let mut owner = owner_of(&textured(Value::Null));
        let before = bare_state(&owner);
        let r = send(&mut owner, name, batch(vec![rs("anchor", style)]));
        assert_eq!(r["ok"], true, "{name}: {r}");
        assert_eq!(owner.history_depths(), (1, 0), "{name}: one entry");
        assert_eq!(
            ids(&owner),
            ["anchor"],
            "{name}: the stored dabs go, the seeded anchor paints natively"
        );
        let anchor = bare_state(&owner)["objects"][0].clone();
        assert!(
            anchor["vectorExt"]["brushTexture"]["strokeSeed"].is_u64(),
            "{name}: {anchor}"
        );
        match name {
            "width" => assert_eq!(anchor["vector"]["strokeWidth"], 12),
            "ink" => assert_eq!(anchor["vector"]["stroke"]["r"], 1),
            _ => assert_eq!(
                anchor["vectorExt"]["brushTexture"]["brushTexturePreset"],
                "chalk-round"
            ),
        }
        owner.undo(1).unwrap();
        assert_eq!(
            bare_state(&owner),
            before,
            "{name}: undo brings the stored dabs back"
        );
    }
    // Edits that do not touch the dabs' look leave them in place.
    let mut owner = owner_of(&textured(Value::Null));
    let r = send(
        &mut owner,
        "fill",
        batch(vec![rs("anchor", json!({"fill":"#ff0000"}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(ids(&owner), ["anchor", "dab-1", "dab-2"]);
}

#[test]
fn an_anchor_with_no_preset_to_stamp_from_keeps_its_stored_dabs() {
    let mut doc = textured(Value::Null);
    doc["objects"][0]["vectorExt"]["brushTexture"]
        .as_object_mut()
        .unwrap()
        .remove("brushTexturePreset");
    let mut owner = owner_of(&doc);
    let r = send(
        &mut owner,
        "width",
        batch(vec![rs("anchor", json!({"width":12}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        ids(&owner),
        ["anchor", "dab-1", "dab-2"],
        "nothing can re-derive them"
    );
}

#[test]
fn a_dab_without_its_anchor_is_an_ordinary_record_and_one_with_its_anchor_is_not_edited_alone() {
    let mut orphan = textured(Value::Null);
    orphan["objects"].as_array_mut().unwrap().remove(0);
    let mut owner = owner_of(&orphan);
    let before = bare_state(&owner);
    let moved = send(
        &mut owner,
        "move",
        batch(vec![
            json!({"op":"reshape","stableTarget":target("L1", 0, "dab-1"),"matrix":[1., 0., 0., 1., 10., 20.]}),
        ]),
    );
    assert_eq!(moved["ok"], true, "{moved}");
    let after = bare_state(&owner);
    assert_ne!(
        after["objects"][0]["geometry"], before["objects"][0]["geometry"],
        "the orphan dab moved"
    );
    assert_eq!(after["objects"][1], before["objects"][1], "and only it");
    let recoloured = send(
        &mut owner,
        "fill",
        batch(vec![rs("dab-2", json!({"fill":"#00ff00"}))]),
    );
    assert_eq!(recoloured["ok"], true, "{recoloured}");
    // With its anchor present a dab alone is refused by type and nothing is written.
    let mut owner = owner_of(&textured(Value::Null));
    let before = bare_state(&owner);
    for step in [
        json!({"op":"reshape","stableTarget":target("L1", 0, "dab-1"),"matrix":[1., 0., 0., 1., 10., 20.]}),
        rs("dab-1", json!({"width":3})),
    ] {
        let r = send(&mut owner, "alone", batch(vec![step]));
        assert_eq!(r["ok"], false, "{r}");
    }
    assert_eq!(
        (bare_state(&owner), owner.history_depths()),
        (before, (0, 0))
    );
}
