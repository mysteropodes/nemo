//! #2372 SEL-STYLE2: the brush preset and the mask feather members of a `restyle` step (each an edit of its own), and the malformed detail
//! members: one entry for the selection, undo exact, ineligible families left alone, what the engine cannot stamp a typed refusal.
use super::batch_style::{at, paint, refused, restyle, ribbon_doc};
use super::batch_style_detail::{apply, trio, undo_to};
use super::support::*;
use serde_json::{json, Value};

#[test]
fn a_brush_preset_makes_a_plain_stroke_an_anchor_swaps_to_another_and_strips_back_exactly() {
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    let r = apply(
        &mut owner,
        &["s-a", "s-b"],
        json!({"brushPreset":"charcoal-pencil"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "one entry for the selection"
    );
    let (a, b) = (at(&owner, "s-a"), at(&owner, "s-b"));
    for r in [&a, &b] {
        let t = &r["vectorExt"]["brushTexture"];
        assert_eq!(t["brushTexturePreset"], "charcoal-pencil");
        assert!(
            t["strokeSeed"].is_u64() && t["brushGroupId"].as_str().unwrap().starts_with("bg"),
            "{t}"
        );
    }
    assert_eq!(a["vector"]["opacity"], 0, "an unfilled anchor hides itself");
    assert_eq!(a["vectorExt"]["brushTexture"]["preTextureOpacity"], 1);
    assert_eq!(
        b["vector"]["stroke"],
        Value::Null,
        "a filled anchor hides only its stroke"
    );
    assert_eq!(b["vector"]["hasRealStroke"], false);
    assert_eq!(
        b["vectorExt"]["brushTexture"]["preTextureStroke"],
        "#445566ff"
    );
    assert_eq!(
        paint(&b["fill"]),
        paint(&before["objects"][1]["fill"]),
        "its fill stays visible"
    );
    let again = apply(
        &mut owner,
        &["s-a", "s-b"],
        json!({"brushPreset":"chalk-round"}),
    );
    assert_eq!(again["ok"], true, "{again}");
    let (a2, b2) = (at(&owner, "s-a"), at(&owner, "s-b"));
    assert_eq!(
        a2["vectorExt"]["brushTexture"]["brushTexturePreset"],
        "chalk-round"
    );
    assert_eq!(
        a2["vectorExt"]["brushTexture"]["preTextureOpacity"], 1,
        "the first look is what a strip gives back, not the camouflage"
    );
    assert_ne!(
        a2["vectorExt"]["brushTexture"]["strokeSeed"],
        a["vectorExt"]["brushTexture"]["strokeSeed"]
    );
    assert_eq!(
        b2["vectorExt"]["brushTexture"]["preTextureStroke"],
        "#445566ff"
    );
    let none = apply(&mut owner, &["s-a", "s-b"], json!({"brushPreset":"none"}));
    assert_eq!(none["ok"], true, "{none}");
    assert_eq!(
        at(&owner, "s-a"),
        before["objects"][0],
        "none strips back to the stored stroke"
    );
    assert_eq!(
        at(&owner, "s-b")["vector"]["stroke"],
        before["objects"][1]["vector"]["stroke"]
    );
    assert_eq!(at(&owner, "s-b")["vector"]["hasRealStroke"], true);
    assert!(
        at(&owner, "s-b").get("vectorExt").is_none()
            || at(&owner, "s-b")["vectorExt"].get("brushTexture").is_none()
    );
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before, "three undos restore the original");
    while owner.history_depths().1 > 0 {
        owner.redo(owner.content_revision()).unwrap();
    }
    assert_eq!(
        state(&owner_of(&state(&owner))),
        state(&owner),
        "the saved document reopens unchanged"
    );
}

#[test]
fn a_brush_preset_leaves_ribbons_and_unstroked_records_alone_and_refuses_what_it_cannot_stamp() {
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    let r = apply(&mut owner, &["s-c"], json!({"brushPreset":"chalk-round"}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        at(&owner, "s-c"),
        before["objects"][2],
        "a record with no stroke is not eligible"
    );
    let mut ribbon = ribbon_doc();
    let was = at(&ribbon, "native-stroke:1:1");
    let r = apply(
        &mut ribbon,
        &["native-stroke:1:1"],
        json!({"brushPreset":"chalk-round"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        at(&ribbon, "native-stroke:1:1"),
        was,
        "a pressure ribbon is not eligible"
    );
    let doc = trio();
    refused(
        &doc,
        vec![restyle("s-a", json!({"brushPreset":"no-such-brush"}))],
        "invalid_request",
    );
    refused(
        &doc,
        vec![restyle("s-a", json!({"brushPreset":""}))],
        "invalid_request",
    );
    refused(
        &doc,
        vec![restyle(
            "s-a",
            json!({"brushPreset":"chalk-round","fill":"#ff0000"}),
        )],
        "invalid_request",
    );
    refused(
        &doc,
        vec![restyle(
            "s-a",
            json!({"brushPreset":"chalk-round","cap":"round"}),
        )],
        "invalid_request",
    );
    refused(
        &doc,
        vec![restyle(
            "s-a",
            json!({"brushPreset":"chalk-round","maskFeather":3}),
        )],
        "invalid_request",
    );
    let anchor = json!({"brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1,"strokeSeed":1}});
    let copy = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
    let with = |id: &str, ext: &Value| {
        let mut r = record_at("A", "L1", 0, id);
        r["vectorExt"] = ext.clone();
        r
    };
    let stored = document(1, vec![with("anchor", &anchor), with("copy", &copy)], None);
    refused(
        &stored,
        vec![restyle("anchor", json!({"brushPreset":"chalk-round"}))],
        "unavailable",
    );
    refused(
        &stored,
        vec![restyle("copy", json!({"brushPreset":"chalk-round"}))],
        "unavailable",
    );
    refused(
        &stored,
        vec![restyle(
            "anchor",
            json!({"strokeEnabled":true,"stroke":"#ff0000"}),
        )],
        "unavailable",
    );
}

fn masks() -> Value {
    let mask = |label: &str, id: &str, on: bool, feather: Value| {
        let mut r = record_at(label, "L1", 0, id);
        if on {
            r["vectorExt"] = json!({"isMask":true,"maskMode":"add","maskFeather":feather});
        }
        r
    };
    let mut other = mask("B", "other-layer", true, json!(2));
    other["target"]["layerUid"] = json!("L2");
    document(
        1,
        vec![
            mask("B", "m1", true, json!(0)),
            mask("A", "plain", false, Value::Null),
            mask("B", "m2", true, json!(7)),
            other,
        ],
        None,
    )
}

#[test]
fn a_mask_feather_is_the_layers_shared_value_every_mask_of_the_cell_takes_it() {
    let mut owner = owner_of(&masks());
    let before = state(&owner);
    let r = apply(&mut owner, &["m1"], json!({"maskFeather":12.5}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["result"]["strokeIds"],
        json!(["m1"]),
        "the siblings are not steps of their own"
    );
    assert_eq!(owner.history_depths(), (1, 0));
    for id in ["m1", "m2"] {
        assert_eq!(at(&owner, id)["vectorExt"]["maskFeather"], 12.5, "{id}");
    }
    assert_eq!(
        at(&owner, "plain"),
        before["objects"][1],
        "a plain record of the layer is not a mask"
    );
    assert_eq!(
        at(&owner, "other-layer"),
        before["objects"][3],
        "another layer keeps its own feather"
    );
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before);
    refused(
        &masks(),
        vec![restyle("plain", json!({"maskFeather":3}))],
        "unavailable",
    );
    refused(
        &masks(),
        vec![restyle("m1", json!({"maskFeather":-1}))],
        "invalid_request",
    );
    refused(
        &masks(),
        vec![restyle("m1", json!({"maskFeather":501}))],
        "invalid_request",
    );
}

#[test]
fn malformed_detail_members_are_refused_with_nothing_written() {
    let doc = trio();
    for style in [
        json!({"fillEnabled":true}),
        json!({"strokeEnabled":true}),
        json!({"fillEnabled":"yes","fill":"#ff0000"}),
        json!({"cap":"flat"}),
        json!({"join":"sharp"}),
        json!({"miter":0.5}),
        json!({"miter":1e9}),
        json!({"dashOffset":1e12}),
        json!({"paintOrder":"fill-stroke"}),
        json!({"strokeStyle":"wavy"}),
        json!({"cap":null}),
    ] {
        refused(&doc, vec![restyle("s-b", style.clone())], "invalid_request");
    }
}
