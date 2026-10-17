//! Literal tests of the stroke-aware scene metadata (L01-I): the retained primary solid and the stroke reach. Runs under the
//! light `test-evaluation` build (no wgpu/vello): records are real codec-admitted storage-v1 documents and the style is
//! L07-I's `style_of_record`, so nothing here is a stand-in for the owner.
use super::{primary_solid, stroke_reach, ENVELOPE_MARGIN};
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record, SceneItemStyle,
};
use crate::object_codec::decode_project;
use serde_json::{json, Value};

const SQUARE: [[f64; 2]; 4] = [[16., 16.], [48., 16.], [48., 48.], [16., 48.]];

fn red() -> Value {
    json!({"kind":"solid","r":1,"g":0,"b":0,"a":1})
}
fn blue() -> Value {
    json!({"kind":"solid","r":0,"g":0,"b":1,"a":1})
}

/// The style of one admitted vector record: `fill` is the record fill, `over` replaces members of the vector body.
fn style(fill: Value, over: Value) -> SceneItemStyle {
    let mut vector = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1});
    for (key, value) in over.as_object().unwrap() {
        vector[key.as_str()] = value.clone();
    }
    let segments: Vec<Value> = SQUARE
        .iter()
        .map(|p| json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}))
        .collect();
    let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"L1"}],
        "objects":[{"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
            "fill":fill,"vector":vector}]});
    let document =
        decode_project(doc.to_string().as_bytes()).expect("record admitted by the real codec");
    style_of_record(&document.objects()[0]).expect("style")
}

fn stroked(width: f64, cap: &str, join: &str, extra: Value) -> SceneItemStyle {
    let mut over = json!({"stroke":blue(),"hasRealStroke":true,"strokeWidth":width,"strokeCap":cap,"strokeJoin":join});
    for (key, value) in extra.as_object().unwrap() {
        over[key.as_str()] = value.clone();
    }
    style(Value::Null, over)
}

fn rgba(style: &SceneItemStyle) -> [f32; 4] {
    primary_solid(style)
        .expect("solid")
        .rgba_f32()
        .expect("rgba")
}

#[test]
fn the_primary_solid_is_the_fill_then_the_stroke_then_transparent() {
    // fill and stroke: the fill wins, exactly what the legacy family retained
    assert_eq!(
        rgba(&style(red(), json!({"stroke":blue(),"hasRealStroke":true}))),
        [1.0, 0.0, 0.0, 1.0]
    );
    // no fill: the stroke paint
    assert_eq!(
        rgba(&style(
            Value::Null,
            json!({"stroke":blue(),"hasRealStroke":true})
        )),
        [0.0, 0.0, 1.0, 1.0]
    );
    // neither: transparent, not an opaque default and not an error
    assert_eq!(rgba(&style(Value::Null, json!({}))), [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn the_primary_solid_carries_the_item_opacity_folded_into_its_alpha() {
    assert_eq!(
        rgba(&style(red(), json!({"opacity":0.5}))),
        [1.0, 0.0, 0.0, 0.5]
    );
    assert_eq!(
        rgba(&style(
            Value::Null,
            json!({"stroke":blue(),"hasRealStroke":true,"opacity":0.25})
        )),
        [0.0, 0.0, 1.0, 0.25]
    );
    // opacity does not touch an item that has no paint at all
    assert_eq!(
        rgba(&style(Value::Null, json!({"opacity":0.5}))),
        [0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn no_stroke_has_no_reach_and_a_filled_item_is_not_a_stroked_one() {
    assert_eq!(stroke_reach(&style(red(), json!({}))), None);
    assert_eq!(stroke_reach(&style(Value::Null, json!({}))), None);
    assert_eq!(ENVELOPE_MARGIN, 1.0);
}

#[test]
fn stroke_reach_is_half_the_width_times_the_widest_cap_or_join_plus_the_margin() {
    let reach = |style: SceneItemStyle| stroke_reach(&style).expect("reach");
    // round cap, round join: half the width
    assert_eq!(reach(stroked(12.0, "round", "round", json!({}))), 7.0);
    // butt cap, bevel join: half the width
    assert_eq!(reach(stroked(8.0, "butt", "bevel", json!({}))), 5.0);
    // a miter join without an authored limit reaches kurbo's default limit of 4 half-widths
    assert_eq!(reach(stroked(8.0, "butt", "miter", json!({}))), 17.0);
    // an authored limit replaces the default, an explicit 0 included (never below one half-width)
    assert_eq!(
        reach(stroked(8.0, "butt", "miter", json!({"miterLimit":1.5}))),
        7.0
    );
    assert_eq!(
        reach(stroked(8.0, "butt", "miter", json!({"miterLimit":0}))),
        5.0
    );
    // a square cap reaches the corner of its square: half the width times sqrt(2)
    let square = reach(stroked(12.0, "square", "round", json!({})));
    assert!((square - 9.485281374238571).abs() < 1e-12, "{square}");
    // the widest of the two wins
    assert_eq!(reach(stroked(8.0, "square", "miter", json!({}))), 17.0);
}

#[test]
fn a_dash_never_extends_a_stroke_and_an_authored_zero_width_draws_at_width_one() {
    let plain = stroke_reach(&stroked(8.0, "butt", "round", json!({})));
    let dashed = stroke_reach(&stroked(
        8.0,
        "butt",
        "round",
        json!({"dashArray":[8,8],"dashOffset":4}),
    ));
    assert_eq!(plain, Some(5.0));
    assert_eq!(dashed, plain);
    // `strokeWidth || 1`: the stroke that is drawn has width 1, so its reach is 0.5 + the margin
    assert_eq!(
        stroke_reach(&stroked(0.0, "butt", "round", json!({}))),
        Some(1.5)
    );
}
