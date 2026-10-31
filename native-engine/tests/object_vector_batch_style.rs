//! #2372 SEL-STYLE: a `restyle` step of `object.vector.batch` changes only the style members it names, on every record of a mixed
//! selection, as one revision and one history entry; ribbons rebuild and re-sync their linked fill; families the engine cannot restyle
//! are refused by type with nothing written.
use super::create_many::pressure_record;
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

pub(super) fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
pub(super) fn target(frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L1","strokeId":stroke})
}
pub(super) fn restyle(id: &str, style: Value) -> Value {
    json!({"op":"restyle","stableTarget":target(0, id),"style":style})
}
pub(super) fn mixed() -> Value {
    let mut doc = document(
        1,
        vec![
            record_at("B", "L1", 0, "s-b"),
            record_at("A", "L1", 0, "s-a"),
        ],
        None,
    );
    doc["items"] = json!({"raster":[{"target":target(0, "img"),"item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20}}],
        "text":[{"target":target(0, "txt"),"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
            "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]});
    doc
}
pub(super) fn at(owner: &NativeObjectHistory, id: &str) -> Value {
    objects(&state(owner))
        .iter()
        .find(|r| r["target"]["strokeId"] == id)
        .unwrap()
        .clone()
}
pub(super) fn close(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|v| (v - want).abs() < 1e-9)
}
pub(super) fn paint(v: &Value) -> [f64; 4] {
    ["r", "g", "b", "a"].map(|k| v[k].as_f64().unwrap())
}
pub(super) fn byte(n: u8) -> f64 {
    f64::from(n) / 255.0
}

#[test]
fn one_fill_edit_over_a_mixed_selection_is_one_entry_and_reaches_only_what_paints_a_fill() {
    let mut owner = owner_of(&mixed());
    let before = state(&owner);
    let steps = ["s-b", "s-a", "img", "txt"].map(|id| restyle(id, json!({"fill":"#ff000080"})));
    let r = send(&mut owner, "fill", batch(steps.to_vec()));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["result"]["strokeIds"],
        json!(["s-b", "s-a", "img", "txt"])
    );
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "one entry for the whole selection"
    );
    assert_eq!(
        paint(&at(&owner, "s-b")["fill"]),
        [1.0, 0.0, 0.0, byte(128)]
    );
    assert_eq!(
        at(&owner, "s-b")["vector"]["stroke"],
        before["objects"][0]["vector"]["stroke"],
        "the stroke is untouched"
    );
    assert_eq!(
        at(&owner, "s-a"),
        before["objects"][1],
        "a record with no fill has nothing to recolour"
    );
    assert_eq!(
        state(&owner)["items"]["raster"],
        before["items"]["raster"],
        "an image has no colour"
    );
    assert_eq!(
        state(&owner)["items"]["text"][0]["block"]["color"],
        "#ff000080"
    );
    owner.undo(1).unwrap();
    assert_eq!(
        state(&owner),
        before,
        "one undo restores every record and item exactly"
    );
}

#[test]
fn stroke_width_and_opacity_apply_by_the_baseline_rules_in_one_entry() {
    let mut owner = owner_of(&mixed());
    let before = state(&owner);
    let style = json!({"stroke":"#00ff00","width":6,"opacity":0.4});
    let r = send(
        &mut owner,
        "style",
        batch(
            ["s-b", "s-a", "img", "txt"]
                .map(|id| restyle(id, style.clone()))
                .to_vec(),
        ),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0));
    let (b, a) = (at(&owner, "s-b"), at(&owner, "s-a"));
    assert_eq!(paint(&b["vector"]["stroke"]), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(
        (
            b["vector"]["strokeWidth"].clone(),
            a["vector"]["strokeWidth"].clone()
        ),
        (json!(6), json!(6))
    );
    assert_eq!(paint(&a["vector"]["stroke"]), [0.0, 1.0, 0.0, 1.0]);
    assert!(
        close(&b["fill"]["a"], 0.4),
        "a filled record takes the opacity as its fill alpha: {b}"
    );
    assert_eq!(
        b["vector"]["opacity"], before["objects"][0]["vector"]["opacity"],
        "the item opacity of a filled record stays"
    );
    assert!(
        close(&a["vector"]["opacity"], 0.4),
        "an unfilled record takes it as its item opacity: {a}"
    );
    assert!(close(
        &state(&owner)["items"]["raster"][0]["item"]["opacity"],
        0.4
    ));
    assert_eq!(
        state(&owner)["items"]["text"][0]["block"]["color"],
        "#11223366",
        "an opacity is a text block's colour alpha"
    );
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
    owner.redo(2).unwrap();
    assert_eq!(at(&owner, "s-b")["vector"]["strokeWidth"], 6);
    assert_eq!(
        state(&owner_of(&state(&owner))),
        state(&owner),
        "the saved document reopens unchanged"
    );
}

#[test]
fn a_text_block_takes_a_fill_as_its_colour_and_an_opacity_as_its_alpha() {
    let mut owner = owner_of(&mixed());
    let r = send(
        &mut owner,
        "text",
        batch(vec![restyle(
            "txt",
            json!({"fill":"#336699","opacity":0.4}),
        )]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        state(&owner)["items"]["text"][0]["block"]["color"],
        "#33669966",
        "the opacity is the colour's alpha byte"
    );
    let r = send(
        &mut owner,
        "opaque",
        batch(vec![restyle("txt", json!({"opacity":1}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        state(&owner)["items"]["text"][0]["block"]["color"],
        "#336699"
    );
}

pub(super) fn ribbon_doc() -> NativeObjectHistory {
    let mut owner = owner_of(&document(1, seeds(), None));
    let companion = pressure_record(
        [0.2, 0.4, 0.6],
        json!({"linkedFillId":"lf","isLinkedFillCompanion":true}),
    );
    let ribbon = pressure_record(
        [0.1, 0.1, 0.1],
        json!({"linkedFillId":"lf","pressure":{"centerSegments":[
        {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":40,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}),
    );
    let made = send(
        &mut owner,
        "pair",
        json!({"command":"object.vector.createMany","records":[companion, ribbon]}),
    );
    assert_eq!(made["ok"], true, "{made}");
    owner
}

#[test]
fn a_ribbon_width_rescales_its_profile_rebuilds_the_outline_and_keeps_its_linked_fill_in_sync() {
    use crate::object_document::pressure_geometry::{profiled_outline, SegOut};
    let mut owner = ribbon_doc();
    let before = state(&owner);
    // The profile peaks at 4: a size of 8 doubles every width.
    let r = send(
        &mut owner,
        "size",
        batch(vec![restyle("native-stroke:1:1", json!({"width":8}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["result"]["strokeIds"],
        json!(["native-stroke:1:1"]),
        "the fill re-sync is not a step of its own"
    );
    let (ribbon, fill) = (
        at(&owner, "native-stroke:1:1"),
        at(&owner, "native-stroke:1:0"),
    );
    let pressure = &ribbon["vectorExt"]["pressure"];
    assert_eq!(
        pressure["widthProfile"],
        json!([{"t":0,"width":4},{"t":1,"width":8}])
    );
    assert_eq!(pressure["centerSegments"][0]["width"], 4);
    assert_eq!(pressure["centerSegments"][1]["width"], 8);
    let f = |v: &Value| [v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()];
    let segs: Vec<SegOut> = pressure["centerSegments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| SegOut {
            point: f(&a["point"]),
            handle_in: f(&a["handleIn"]),
            handle_out: f(&a["handleOut"]),
        })
        .collect();
    let outline = profiled_outline(&segs, &[(0., 4.), (1., 8.)]).unwrap();
    let painted = ribbon["geometry"]["segments"].as_array().unwrap();
    assert_eq!(
        painted.len(),
        outline.len(),
        "the outline is the shared producer's, from the rescaled profile"
    );
    for (p, o) in painted.iter().zip(&outline) {
        assert_eq!(
            (f(&p["point"]), f(&p["handleIn"]), f(&p["handleOut"])),
            (o.point, o.handle_in, o.handle_out)
        );
    }
    assert_eq!(
        fill["geometry"]["segments"].as_array().unwrap().len(),
        2,
        "the linked fill follows the centreline"
    );
    assert_eq!(
        ribbon["fill"],
        before["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["target"]["strokeId"] == "native-stroke:1:1")
            .unwrap()["fill"]
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        state(&owner),
        before,
        "one undo restores the ribbon and its fill"
    );
}

#[test]
fn a_ribbon_is_inked_by_its_fill_and_takes_either_colour_member_but_not_two_that_disagree() {
    let mut owner = ribbon_doc();
    let id = "native-stroke:1:1";
    let r = send(
        &mut owner,
        "ink",
        batch(vec![restyle(id, json!({"stroke":"#ff8800"}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(paint(&at(&owner, id)["fill"]), [1.0, byte(136), 0.0, 1.0]);
    let r = send(
        &mut owner,
        "fill",
        batch(vec![restyle(id, json!({"fill":"#0000ff","opacity":0.5}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(paint(&at(&owner, id)["fill"]), [0.0, 0.0, 1.0, 0.5]);
    assert_eq!(
        at(&owner, id)["vector"]["hasRealStroke"],
        false,
        "a ribbon never gains a stroke of its own"
    );
    let both = send(
        &mut owner,
        "both",
        batch(vec![restyle(
            id,
            json!({"fill":"#0000ff","stroke":"#ff0000"}),
        )]),
    );
    assert_eq!(both["error"]["code"], "invalid_request", "{both}");
}

pub(super) fn refused(doc: &Value, ops: Vec<Value>, code: &str) {
    let mut owner = owner_of(doc);
    let before = state(&owner);
    let r = send(&mut owner, "refused", batch(ops.clone()));
    assert_eq!(r["ok"], false, "{ops:?}");
    assert_eq!(r["error"]["code"], code, "{ops:?}: {r}");
    assert_eq!(
        (state(&owner), owner.history_depths()),
        (before, (0, 0)),
        "nothing was written"
    );
}

#[test]
fn malformed_styles_and_steps_are_refused_with_nothing_written() {
    let doc = mixed();
    for style in [
        json!({}),
        json!({"fill":"red"}),
        json!({"fill":"#12"}),
        json!({"stroke":"#12345g"}),
        json!({"opacity":1.5}),
        json!({"opacity":-0.1}),
        json!({"width":0}),
        json!({"width":-2}),
        json!({"width":1e9}),
        json!({"colour":"#000000"}),
        json!({"fill":null}),
    ] {
        refused(&doc, vec![restyle("s-b", style.clone())], "invalid_request");
    }
    let step = restyle("s-b", json!({"width":3}));
    refused(&doc, vec![step.clone(), step.clone()], "invalid_request");
    refused(
        &doc,
        vec![restyle("missing", json!({"width":3}))],
        "not_found",
    );
    let mut with_matrix = step.clone();
    with_matrix["matrix"] = json!([1, 0, 0, 1, 0, 0]);
    refused(&doc, vec![with_matrix], "invalid_request");
    let mut with_frame = step.clone();
    with_frame["frame"] = json!(-1);
    refused(&doc, vec![with_frame], "invalid_request");
    let mut no_style = step.clone();
    no_style.as_object_mut().unwrap().remove("style");
    refused(&doc, vec![no_style], "invalid_request");
    let mut on_create =
        json!({"op":"create","record":record_at("B", "L1", 0, ""),"style":{"width":3}});
    refused(&doc, vec![on_create.take()], "invalid_request");
    // One refused step refuses the whole batch, however many steps before it would have applied.
    refused(
        &doc,
        vec![step, restyle("s-a", json!({"fill":"nope"}))],
        "invalid_request",
    );
}

#[test]
fn brush_families_the_engine_cannot_restyle_are_refused_by_type_and_a_plain_anchor_width_is_not() {
    let anchor_ext = json!({"brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1,"strokeSeed":1}});
    let copy_ext = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
    let with = |id: &str, ext: &Value| {
        let mut r = record_at("A", "L1", 0, id);
        r["vectorExt"] = ext.clone();
        r
    };
    let anchor_only = document(1, vec![with("anchor", &anchor_ext)], None);
    let with_copy = document(
        1,
        vec![with("anchor", &anchor_ext), with("copy", &copy_ext)],
        None,
    );
    refused(
        &with_copy,
        vec![restyle("copy", json!({"width":3}))],
        "unavailable",
    );
    refused(
        &anchor_only,
        vec![restyle("anchor", json!({"stroke":"#ff0000"}))],
        "unavailable",
    );
    refused(
        &with_copy,
        vec![restyle("anchor", json!({"width":3}))],
        "unavailable",
    );
    refused(
        &anchor_only,
        vec![restyle("anchor", json!({"opacity":0.5}))],
        "unavailable",
    );
    let mut owner = owner_of(&anchor_only);
    let r = send(
        &mut owner,
        "anchor",
        batch(vec![restyle("anchor", json!({"width":9}))]),
    );
    assert_eq!(
        r["ok"], true,
        "an anchor whose copies the engine paints follows its width: {r}"
    );
    assert_eq!(at(&owner, "anchor")["vector"]["strokeWidth"], 9);
}

#[test]
fn a_restyle_on_a_held_frame_keys_that_cell_and_leaves_the_key_untouched() {
    let mut doc = mixed();
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    let held = |id: &str| json!({"op":"restyle","stableTarget":target(0, id),"style":{"fill":"#00ff00","opacity":0.5},"frame":4});
    let mut payload = batch(vec![held("s-b"), held("txt")]);
    let refused = send(&mut owner, "no-copy-key", payload.clone());
    assert_eq!(
        refused["ok"], false,
        "without the copy key a held cell has no record to edit: {refused}"
    );
    payload["keyframe"] = json!("copy");
    let r = send(&mut owner, "held", payload);
    assert_eq!(r["ok"], true, "{r}");
    let now = state(&owner);
    let in_cell = |frame: u64, id: &str| {
        objects(&now)
            .iter()
            .find(|r| {
                r["target"]["frameScope"]["frame"].as_u64() == Some(frame)
                    && r["target"]["strokeId"] == id
            })
            .cloned()
            .unwrap()
    };
    assert_eq!(
        in_cell(0, "s-b"),
        objects(&before)[0],
        "the held key keeps its style"
    );
    assert_eq!(
        paint(&in_cell(4, "s-b")["fill"]),
        [0.0, 1.0, 0.0, 0.5],
        "the cell's copy takes the new one"
    );
    let texts = now["items"]["text"].as_array().unwrap();
    let colours: Vec<_> = texts
        .iter()
        .map(|t| {
            (
                t["target"]["frameScope"]["frame"].as_u64().unwrap(),
                t["block"]["color"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        colours.contains(&(0, "#112233".into())) && colours.contains(&(4, "#00ff0080".into())),
        "{colours:?}"
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        state(&owner),
        before,
        "one undo removes the key and the restyle"
    );
}
