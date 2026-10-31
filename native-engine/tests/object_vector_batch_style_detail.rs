//! #2372 SEL-STYLE2: the paint switches, stroke details, brush preset and mask feather of the Select selection as members of a `restyle`
//! step: each is one revision and one history entry over a mixed selection, undo is exact, and the families the engine cannot follow are
//! typed refusals with nothing written.
use super::batch_style::{at, batch, byte, close, paint, restyle, ribbon_doc};
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

/// A (open, stroke only), B (closed, filled and stroked), C (B without its stroke) on layer L1, frame 0.
pub(super) fn trio() -> Value {
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
pub(super) fn apply(owner: &mut NativeObjectHistory, ids: &[&str], style: Value) -> Value {
    let steps = ids.iter().map(|id| restyle(id, style.clone())).collect();
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = format!(
        "detail-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    send(owner, &id, batch(steps))
}
/// Undoes every entry made since the history was `depth` deep, each against the revision it was made at.
pub(super) fn undo_to(owner: &mut NativeObjectHistory, depth: usize) {
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
fn a_stroke_style_pick_changes_only_a_stroked_plain_record_and_nothing_of_an_unstroked_one_or_a_ribbon(
) {
    let mut owner = owner_of(&trio());
    let before = state(&owner);
    // The pick sends the offset, the miter and the paint order beside the style (the baseline's applyStrokeStyle, stroked paths only).
    let pick = json!({"strokeStyle":"dashed","dashOffset":0,"miter":4,"paintOrder":"strokeFirst"});
    let r = apply(&mut owner, &["s-a", "s-c"], pick.clone());
    assert_eq!(r["ok"], true, "{r}");
    let a = &at(&owner, "s-a")["vector"];
    assert_eq!(a["miterLimit"], 4);
    assert_eq!(a["paintOrder"], "strokeFirst");
    assert_eq!(a["dashArray"], json!([10, 7.2]));
    assert_eq!(
        at(&owner, "s-c"),
        before["objects"][2],
        "an unstroked record is untouched"
    );
    // The same members without a style are the plain miter / paint order edits, which still reach an unstroked record.
    let r = apply(
        &mut owner,
        &["s-c"],
        json!({"miter":4,"paintOrder":"strokeFirst"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(at(&owner, "s-c")["vector"]["miterLimit"], 4);
    assert_eq!(at(&owner, "s-c")["vector"]["paintOrder"], "strokeFirst");
    let mut ribbon = ribbon_doc();
    let id = "native-stroke:1:1";
    let was = at(&ribbon, id);
    let r = apply(&mut ribbon, &[id], pick);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        at(&ribbon, id),
        was,
        "a ribbon is not restyled by a stroke style pick"
    );
}
