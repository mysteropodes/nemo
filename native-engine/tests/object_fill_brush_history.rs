//! Native Fill Brush (#2361) through the real native history: draw-bridge's Fill Brush commit @1065e9d8 once the
//! outline exists, `applyFillBrushPlacement` and then `fillMergeSameColor(layer, shape, true)`. One commit is one
//! history entry, and one Undo restores the bytes. Geometry facts are literal.
use crate::history::object_history::{object_boolean_ops_tests as f, NativeObjectHistory};
use serde_json::{json, Value};

const BLUE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
fn outline(x: f64, y: f64, w: f64, h: f64) -> Value {
    let s = |px: f64, py: f64| json!({"point":[px,py],"handleIn":[0,0],"handleOut":[0,0]});
    json!([s(x, y), s(x + w, y), s(x + w, y + h), s(x, y + h)])
}
fn payload(outline: Value, placement: &str) -> Value {
    json!({"command":"object.fill.brush","layerUid":"L0","contextId":"scene-root","frame":0,
        "outline":outline,"color":"#0000ffff","opacity":0.75,"placement":placement,"mask":false,"copies":[],"stamps":[]})
}
fn fill(id: &str, x: f64, y: f64, w: f64, h: f64, paint: [f64; 4]) -> Value {
    f::vector(
        "L0",
        id,
        0,
        &f::rect(x, y, w, h),
        &f::VectorSpec {
            fill: Some(paint),
            ..Default::default()
        },
    )
}
fn owner(objects: Vec<Value>) -> NativeObjectHistory {
    f::owner_of(&f::document(1, objects, None, None))
}
/// Commit one Fill Brush stroke and check it is exactly one history entry that one Undo reverts.
fn brush(owner: &mut NativeObjectHistory, p: &Value) -> Vec<Value> {
    let before = f::bytes(owner);
    let candidate = owner
        .prepare_fill_brush_json(p.to_string().as_bytes())
        .unwrap();
    owner.commit_boolean_edit(candidate).unwrap();
    assert_eq!(owner.history_depths(), (1, 0), "one undo entry");
    let records = f::frame_records(&f::state(owner), "L0", 0);
    let edited = f::bytes(owner);
    owner.undo(1).unwrap();
    assert_eq!(f::bytes(owner), before, "one Undo restores the bytes");
    owner.redo(2).unwrap();
    assert_eq!(f::bytes(owner), edited);
    records
}
fn ids(records: &[Value]) -> Vec<&str> {
    records
        .iter()
        .map(|r| r["target"]["strokeId"].as_str().unwrap())
        .collect()
}
fn extent(record: &Value) -> [f64; 4] {
    let axis = |k: &str| {
        let v: Vec<f64> = record["geometry"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["point"][k].as_f64().unwrap())
            .collect();
        (
            v.iter().cloned().fold(f64::INFINITY, f64::min),
            v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let ((x0, x1), (y0, y1)) = (axis("x"), axis("y"));
    [x0, y0, x1, y1]
}

#[test]
fn above_and_below_place_an_unstroked_fill_brush_shape_on_top_or_at_the_back() {
    let red = fill("red", 100.0, 100.0, 20.0, 20.0, [1.0, 0.0, 0.0, 1.0]);
    let records = brush(
        &mut owner(vec![red.clone()]),
        &payload(outline(0.0, 0.0, 40.0, 30.0), "above"),
    );
    assert_eq!(ids(&records), ["red", "native-stroke:1:0"], "on top");
    let shape = &records[1];
    assert_eq!(shape["vectorExt"], json!({"isFillShape": true}));
    assert_eq!(
        [
            &shape["fill"]["b"],
            &shape["vector"]["opacity"],
            &shape["vector"]["hasRealStroke"]
        ],
        [&json!(1.0), &json!(0.75), &json!(false)]
    );
    assert!(shape["vector"]["stroke"].is_null());
    assert_eq!(
        extent(shape),
        [0.0, 0.0, 40.0, 30.0],
        "the outline as drawn"
    );
    let records = brush(
        &mut owner(vec![red]),
        &payload(outline(0.0, 0.0, 40.0, 30.0), "below"),
    );
    assert_eq!(ids(&records), ["native-stroke:1:0", "red"], "at the back");
}

#[test]
fn merge_unites_with_the_topmost_overlapping_fill_in_the_new_paint_under_its_identity() {
    // Two fills of other colours; the brush overlaps both, and the topmost one (green) is the target.
    let red = fill("red", 0.0, 0.0, 30.0, 30.0, [1.0, 0.0, 0.0, 1.0]);
    let green = fill("green", 20.0, 0.0, 30.0, 30.0, [0.0, 1.0, 0.0, 1.0]);
    let records = brush(
        &mut owner(vec![red, green]),
        &payload(outline(40.0, 10.0, 30.0, 10.0), "merge"),
    );
    assert_eq!(
        ids(&records),
        ["red", "green"],
        "the target keeps its place and its id"
    );
    let united = &records[1];
    assert_eq!(
        extent(united),
        [20.0, 0.0, 70.0, 30.0],
        "green and the brush, united"
    );
    assert_eq!(
        [
            &united["fill"]["g"],
            &united["fill"]["b"],
            &united["vector"]["opacity"]
        ],
        [&json!(0.0), &json!(1.0), &json!(0.75)],
        "in the brush's paint"
    );
    assert!(
        united["vectorExt"]["isFillShape"].is_null(),
        "the target's data, which never was a Fill Brush shape"
    );
    // Nothing overlapping: merge falls back to above.
    let far = fill("far", 200.0, 200.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]);
    let records = brush(
        &mut owner(vec![far]),
        &payload(outline(0.0, 0.0, 10.0, 10.0), "merge"),
    );
    assert_eq!(ids(&records), ["far", "native-stroke:1:0"]);
}

#[test]
fn a_touching_same_colour_fill_brush_shape_is_absorbed_and_a_near_identical_one_keeps_the_existing()
{
    // fillMergeSameColor(layer, shape, true): the Fill Brush may absorb another Fill Brush shape of its colour.
    let mut earlier = fill("earlier", 0.0, 0.0, 30.0, 30.0, BLUE);
    earlier["vectorExt"] = json!({"isFillShape": true});
    earlier["vector"]["opacity"] = json!(0.75);
    let records = brush(
        &mut owner(vec![earlier.clone()]),
        &payload(outline(20.0, 10.0, 30.0, 10.0), "above"),
    );
    assert_eq!(records.len(), 1, "one merged fill: {:?}", ids(&records));
    assert_eq!(
        records[0]["target"]["strokeId"], "native-stroke:1:0",
        "the new shape's identity"
    );
    assert_eq!(extent(&records[0]), [0.0, 0.0, 50.0, 30.0]);
    assert_eq!(records[0]["vectorExt"]["isFillShape"], json!(true));
    // The same stroke drawn again exactly over it: the new shape is dropped and nothing changes.
    let mut same = owner(vec![earlier]);
    let before = f::bytes(&same);
    let candidate = same
        .prepare_fill_brush_json(
            payload(outline(0.0, 0.0, 30.0, 30.0), "above")
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    same.commit_boolean_edit(candidate).unwrap();
    assert_eq!(
        (f::bytes(&same), same.history_depths()),
        (before, (0, 0)),
        "near-identical: no edit, no undo entry"
    );
}

#[test]
fn malformed_fill_brush_requests_are_refused_before_any_edit() {
    let o = owner(vec![]);
    let good = payload(outline(0.0, 0.0, 10.0, 10.0), "above");
    for (key, value) in [
        ("placement", json!("front")),
        ("color", json!("#00f")),
        ("opacity", json!(1.5)),
        ("outline", json!([])),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(
            o.prepare_fill_brush_json(bad.to_string().as_bytes())
                .is_err(),
            "{key}"
        );
    }
    let mut extra = good.clone();
    extra["remove"] = json!(false);
    assert!(
        o.prepare_fill_brush_json(extra.to_string().as_bytes())
            .is_err(),
        "unknown member"
    );
}

#[test]
fn in_mask_mode_the_path_the_merge_hands_back_becomes_a_mask_of_the_layer() {
    // tagOwner after the Fill Brush commit tags the returned path: the new shape, or the near-identical fill it gave way to.
    let mut masked = payload(outline(0.0, 0.0, 40.0, 30.0), "above");
    masked["mask"] = json!("subtract");
    let records = brush(&mut owner(vec![]), &masked);
    assert_eq!(
        records[0]["vectorExt"],
        json!({"isFillShape": true, "isMask": true, "maskMode": "subtract"})
    );
    let mut earlier = fill("earlier", 0.0, 0.0, 30.0, 30.0, BLUE);
    earlier["vector"]["opacity"] = json!(0.75);
    masked["outline"] = outline(0.0, 0.0, 30.0, 30.0);
    let records = brush(&mut owner(vec![earlier]), &masked);
    assert_eq!(
        ids(&records),
        ["earlier"],
        "the shape gave way to the near-identical fill"
    );
    assert_eq!(
        (
            &records[0]["vectorExt"]["isMask"],
            &records[0]["vectorExt"]["maskMode"]
        ),
        (&json!(true), &json!("subtract"))
    );
    let mut bad = payload(outline(0.0, 0.0, 10.0, 10.0), "above");
    bad["mask"] = json!("");
    assert!(
        owner(vec![])
            .prepare_fill_brush_json(bad.to_string().as_bytes())
            .is_err(),
        "an empty mask mode"
    );
}

#[test]
fn symmetry_copies_of_the_returned_path_land_directly_above_it_mapped_and_reoriented() {
    // symmetry-bridge onStrokeCommitted on the Fill Brush result: a mirror about x = 160 re-orients the closed copy; radial
    // copies are rotations about the centre, stacked in the given order directly above the path.
    let mut mirrored = payload(outline(10.0, 0.0, 40.0, 30.0), "above");
    mirrored["copies"] = json!([[-1, 0, 0, 1, 320, 0]]);
    let red = fill("red", 200.0, 100.0, 5.0, 5.0, [1.0, 0.0, 0.0, 1.0]);
    let records = brush(&mut owner(vec![red]), &mirrored);
    assert_eq!(
        ids(&records),
        ["red", "native-stroke:1:0", "native-stroke:1:1"]
    );
    assert_eq!(extent(&records[2]), [270.0, 0.0, 310.0, 30.0], "320 - x");
    let xs = |r: &Value| -> Vec<f64> {
        r["geometry"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["point"]["x"].as_f64().unwrap())
            .collect()
    };
    let mut back: Vec<f64> = xs(&records[1]).iter().map(|x| 320.0 - x).collect();
    back.reverse();
    assert_eq!(
        xs(&records[2]),
        back,
        "the reflected copy runs the other way round"
    );
    assert_eq!(
        records[2]["vectorExt"],
        json!({"isFillShape": true}),
        "the copy keeps the shape's data"
    );
    let mut radial = payload(outline(0.0, 0.0, 10.0, 10.0), "below");
    radial["copies"] = json!([[0, 1, -1, 0, 0, 0], [-1, 0, 0, -1, 0, 0]]);
    let records = brush(&mut owner(vec![]), &radial);
    assert_eq!(records.len(), 3);
    assert_eq!(
        extent(&records[1]),
        [-10.0, 0.0, 0.0, 10.0],
        "a quarter turn"
    );
    assert_eq!(extent(&records[2]), [-10.0, -10.0, 0.0, 0.0], "a half turn");
}

#[test]
fn multi_frame_stamps_the_returned_path_on_each_selected_frame_keyed_with_what_it_showed_before_the_stroke() {
    // Frame 0 is the layer's only key; frames 1..3 hold it. multiframe-draw keys frame 2 with what it showed before
    // this stroke (only "s-x"), then stamps the path the commit hands back (mask tag included, symmetry copy not).
    let mut doc = f::document(1, vec![fill("s-x", 200.0, 100.0, 20.0, 20.0, [1.0, 0.0, 0.0, 1.0])], None, None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = f::owner_of(&doc);
    let mut p = payload(outline(10.0, 10.0, 40.0, 30.0), "above");
    p["mask"] = json!("add");
    p["copies"] = json!([[-1, 0, 0, 1, 320, 0]]);
    p["stamps"] = json!([2]);
    let drawn = brush(&mut owner, &p);
    assert_eq!(drawn.len(), 3, "s-x, the shape, its mirror copy");
    let stamped = f::frame_records(&f::state(&owner), "L0", 2);
    assert_eq!(stamped.len(), 2, "the key copy of what frame 2 showed, then the stamp: {stamped:#?}");
    assert_eq!(extent(&stamped[0]), extent(&drawn[0]), "frame 2 keyed with s-x, not with the new stroke");
    assert_eq!(extent(&stamped[1]), extent(&drawn[1]), "the stamp is the committed shape");
    assert_eq!(stamped[1]["vectorExt"]["isMask"], true);
    assert_eq!(stamped[1]["vectorExt"]["maskMode"], "add");
    assert_ne!(stamped[1]["target"]["strokeId"], drawn[1]["target"]["strokeId"], "a fresh identity");
    assert!(f::frame_records(&f::state(&owner), "L0", 1).is_empty(), "an unselected held frame stays held");
}

#[test]
fn stamps_naming_the_drawn_frame_a_repeat_or_a_frame_past_the_end_are_refused() {
    for stamps in [json!([0]), json!([2, 2]), json!([4]), json!([-1]), json!(null)] {
        let mut owner = owner(vec![]);
        let mut p = payload(outline(10.0, 10.0, 40.0, 30.0), "above");
        p["stamps"] = stamps.clone();
        assert!(owner.prepare_fill_brush_json(p.to_string().as_bytes()).is_err(), "{stamps}");
    }
}
