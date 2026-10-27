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
        "outline":outline,"color":"#0000ffff","opacity":0.75,"placement":placement})
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
