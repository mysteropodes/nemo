//! #2372 SEL-STYLE2: the paint switches, stroke details, brush preset and mask feather of the Select selection as members of a `restyle`
//! step: each is one revision and one history entry over a mixed selection, undo is exact, and the families the engine cannot follow are
//! typed refusals with nothing written.
use super::batch_style::{at, batch, byte, close, paint, refused, restyle, ribbon_doc};
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

/// A (open, stroke only), B (closed, filled and stroked), C (B without its stroke) on layer L1, frame 0.
fn trio() -> Value {
    let mut c = record_at("B", "L1", 0, "s-c");
    c["vector"]["stroke"] = Value::Null;
    c["vector"]["hasRealStroke"] = json!(false);
    document(
        1,
        vec![
            record_at("A", "L1", 0, "s-a"),
            record_at("B", "L1", 0, "s-b"),
            c,
        ],
        None,
    )
}
fn apply(owner: &mut NativeObjectHistory, ids: &[&str], style: Value) -> Value {
    let steps = ids.iter().map(|id| restyle(id, style.clone())).collect();
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = format!(
        "detail-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    send(owner, &id, batch(steps))
}
/// Undoes every entry made since the history was `depth` deep, each against the revision it was made at.
fn undo_to(owner: &mut NativeObjectHistory, depth: usize) {
    while owner.history_depths().0 > depth {
        owner.undo(owner.content_revision()).unwrap();
    }
}

#[test]
fn the_fill_switch_turns_a_fill_on_with_its_colour_or_off_in_one_entry() {
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    let on = json!({"fillEnabled":true,"fill":"#00ff0080"});
    let r = apply(&mut owner, &["s-a", "s-b", "s-c"], on);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0));
    for id in ["s-a", "s-b", "s-c"] {
        assert_eq!(
            paint(&at(&owner, id)["fill"]),
            [0.0, 1.0, 0.0, byte(128)],
            "{id}"
        );
    }
    assert_eq!(
        at(&owner, "s-b")["vector"]["stroke"],
        before["objects"][1]["vector"]["stroke"],
        "the stroke is untouched"
    );
    let off = apply(
        &mut owner,
        &["s-a", "s-b", "s-c"],
        json!({"fillEnabled":false}),
    );
    assert_eq!(off["ok"], true, "{off}");
    for id in ["s-a", "s-b", "s-c"] {
        assert_eq!(at(&owner, id)["fill"], Value::Null, "{id}");
    }
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before, "undo restores every record exactly");
}

#[test]
fn the_stroke_switch_turns_a_stroke_on_with_its_colour_or_off_and_keeps_the_flag_and_paint_in_step()
{
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    let r = apply(
        &mut owner,
        &["s-a", "s-b", "s-c"],
        json!({"strokeEnabled":true,"stroke":"#ff0000"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    for id in ["s-a", "s-b", "s-c"] {
        let v = &at(&owner, id)["vector"];
        assert_eq!(
            (paint(&v["stroke"]), v["hasRealStroke"].clone()),
            ([1.0, 0.0, 0.0, 1.0], json!(true)),
            "{id}"
        );
    }
    let off = apply(
        &mut owner,
        &["s-a", "s-b", "s-c"],
        json!({"strokeEnabled":false,"stroke":"#00ff00"}),
    );
    assert_eq!(off["ok"], true, "{off}");
    for id in ["s-a", "s-b", "s-c"] {
        let v = &at(&owner, id)["vector"];
        assert_eq!(
            (v["stroke"].clone(), v["hasRealStroke"].clone()),
            (Value::Null, json!(false)),
            "{id}: the colour of an off stroke is ignored"
        );
    }
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before);
    assert_eq!(
        state(&owner_of(&state(&owner))),
        state(&owner),
        "the saved document reopens unchanged"
    );
}

#[test]
fn a_ribbon_stroke_switch_is_its_ink_alpha_and_its_fill_switch_does_nothing() {
    let mut owner = ribbon_doc();
    let id = "native-stroke:1:1";
    let before = state(&owner);
    let bare = apply(&mut owner, &[id], json!({"strokeEnabled":true}));
    assert_eq!(
        bare["error"]["code"], "invalid_request",
        "turning an ink on names its colour: {bare}"
    );
    let off = apply(
        &mut owner,
        &[id],
        json!({"strokeEnabled":false,"stroke":"#ff0000"}),
    );
    assert_eq!(off["ok"], true, "{off}");
    assert_eq!(
        paint(&at(&owner, id)["fill"]),
        [0.1, 0.1, 0.1, 0.0],
        "off keeps the ink colour and zeroes its alpha"
    );
    let on = apply(
        &mut owner,
        &[id],
        json!({"strokeEnabled":true,"stroke":"#ff000040"}),
    );
    assert_eq!(on["ok"], true, "{on}");
    let ink = paint(&at(&owner, id)["fill"]);
    assert!(
        close(&json!(ink[3]), byte(0x40)) && close(&json!(ink[0]), 1.0),
        "on takes the stroke colour back (a zero alpha cannot keep its colour): {ink:?}"
    );
    let on = apply(
        &mut owner,
        &[id],
        json!({"strokeEnabled":false,"stroke":"#ffffff"}),
    );
    assert_eq!(on["ok"], true, "{on}");
    let none = apply(&mut owner, &[id], json!({"fillEnabled":false}));
    assert_eq!(none["ok"], true, "{none}");
    assert!(
        at(&owner, id)["fill"]["a"].as_f64().unwrap() == 0.0,
        "a fill switch leaves a ribbon alone"
    );
    undo_to(&mut owner, 1);
    assert_eq!(state(&owner), before);
}

#[test]
fn cap_join_miter_and_paint_order_reach_the_plain_records_and_a_ribbon_takes_only_its_paint_order()
{
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    let r = apply(
        &mut owner,
        &["s-a", "s-b", "s-c"],
        json!({"cap":"square","join":"bevel","miter":4,"paintOrder":"strokeFirst"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0));
    for id in ["s-a", "s-b", "s-c"] {
        let v = &at(&owner, id)["vector"];
        assert_eq!(
            (
                v["strokeCap"].clone(),
                v["strokeJoin"].clone(),
                v["miterLimit"].clone(),
                v["paintOrder"].clone()
            ),
            (
                json!("square"),
                json!("bevel"),
                json!(4),
                json!("strokeFirst")
            ),
            "{id}"
        );
    }
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before);
    let mut ribbon = ribbon_doc();
    let id = "native-stroke:1:1";
    let was = state(&ribbon)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["target"]["strokeId"] == id)
        .unwrap()["vector"]
        .clone();
    let r = apply(
        &mut ribbon,
        &[id],
        json!({"cap":"square","join":"bevel","miter":4,"paintOrder":"strokeFirst"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    let now = at(&ribbon, id)["vector"].clone();
    assert_eq!(now["paintOrder"], "strokeFirst");
    assert_eq!(
        (
            now["strokeCap"].clone(),
            now["strokeJoin"].clone(),
            now["miterLimit"].clone()
        ),
        (
            was["strokeCap"].clone(),
            was["strokeJoin"].clone(),
            was["miterLimit"].clone()
        ),
        "a ribbon is a filled outline: cap, join and miter do not apply"
    );
}

#[test]
fn the_stroke_style_writes_the_dash_pattern_from_the_width_and_the_dash_offset_follows_only_a_dashed_stroke(
) {
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    // C has no stroke: a style has nothing to dash. A is 4 wide, B 2.5.
    let r = apply(
        &mut owner,
        &["s-a", "s-b", "s-c"],
        json!({"strokeStyle":"dashed","dashOffset":3}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(at(&owner, "s-a")["vector"]["dashArray"], json!([10, 7.2]));
    assert_eq!(at(&owner, "s-a")["vector"]["dashOffset"], 3);
    assert_eq!(at(&owner, "s-b")["vector"]["dashArray"], json!([6.25, 4.5]));
    assert_eq!(
        at(&owner, "s-c"),
        before["objects"][2],
        "no stroke, no dashes"
    );
    let r = apply(&mut owner, &["s-a", "s-c"], json!({"dashOffset":9}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        at(&owner, "s-a")["vector"]["dashOffset"],
        9,
        "a dashed stroke takes the offset"
    );
    assert_eq!(
        at(&owner, "s-c"),
        before["objects"][2],
        "a solid one has none to move"
    );
    let r = apply(&mut owner, &["s-a"], json!({"strokeStyle":"dotted"}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(at(&owner, "s-a")["vector"]["dashArray"], json!([2, 4.8]));
    let r = apply(&mut owner, &["s-a", "s-b"], json!({"strokeStyle":"solid"}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(
        at(&owner, "s-a")["vector"].get("dashArray").is_none(),
        "a solid stroke stores no pattern"
    );
    undo_to(&mut owner, 0);
    assert_eq!(state(&owner), before);
}

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
