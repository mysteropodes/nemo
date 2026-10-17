//! L07-I (F16, G03a): the scene style assembled from real decoded records. Records go through the real native
//! codec (`decode_project`); every expected value is a literal worked out from the baseline rules (engine-bridge.js
//! stroke block, engine.rs `stroke_from`/`dash_pattern_with_offset`), not read back from the code under test.
use super::{style_of_record, ItemStroke, SceneItemStyle, LEGACY_FAMILY};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, PaintOrder, SceneSolid,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    SceneCap, SceneJoin, SceneStroke,
};
use crate::object_codec::{decode_project, ObjectDocument};
use crate::object_document::{ObjectRecord, SolidFill};
use serde_json::{json, Number, Value};

pub(super) fn decode(records: Vec<Value>) -> ObjectDocument {
    let value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L1"}],"objects":records});
    decode_project(&serde_json::to_vec(&value).unwrap())
        .unwrap_or_else(|error| panic!("record refused: {error}"))
}

fn segments() -> Value {
    let seg = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}});
    json!([seg(0.0, 0.0), seg(10.0, 0.0), seg(10.0, 10.0)])
}

fn target(id: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":id})
}

pub(super) fn legacy_record() -> ObjectRecord {
    let record = json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":target("legacy"),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":segments()},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}});
    decode(vec![record]).objects()[0].clone()
}

/// An open-or-closed vector record; `vector` members beyond the required ones come from `extra`.
pub(super) fn vector_record(fill: Value, stroke: Value, extra: Value) -> ObjectRecord {
    let real = !stroke.is_null();
    let mut vector = json!({"stroke":stroke,"hasRealStroke":real,"strokeWidth":2.0,
        "strokeCap":"round","strokeJoin":"round","opacity":1.0});
    for (key, value) in extra.as_object().expect("extra members") {
        vector[key] = value.clone();
    }
    let record = json!({"schemaVersion":1,"family":"editable-vector-path","target":target("vec"),
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":segments()},
        "fill":fill,"vector":vector});
    decode(vec![record]).objects()[0].clone()
}

pub(super) fn solid(r: f64, g: f64, b: f64, a: f64) -> Value {
    json!({"kind":"solid","r":r,"g":g,"b":b,"a":a})
}

fn scene_solid(r: f64, g: f64, b: f64, a: f64) -> SceneSolid {
    let number = |value: f64| Number::from_f64(value).unwrap();
    SceneSolid::from_fill(&SolidFill {
        kind: "solid".to_string(),
        r: number(r),
        g: number(g),
        b: number(b),
        a: number(a),
    })
    .unwrap()
}

#[test]
fn the_legacy_family_is_a_solid_fill_without_stroke_and_fill_first() {
    let record = legacy_record();
    assert_eq!(record.family, LEGACY_FAMILY);
    let style = style_of_record(&record).unwrap();
    assert_eq!(style.stroke, None);
    assert_eq!(style.order, PaintOrder::FillFirst);
    let solid = style.fill.solid().expect("solid fill");
    assert_eq!(
        solid.rgba_f32(),
        Ok([0.2f64 as f32, 0.4f64 as f32, 0.6f64 as f32, 0.75f32])
    );
    assert_eq!(style, SceneItemStyle::solid_fill(solid.clone()));
    // Pure: asking twice gives an equal value and the immutable record is untouched.
    let before = record.clone();
    assert_eq!(style_of_record(&record).unwrap(), style);
    assert_eq!(record, before);
}

#[test]
fn a_vector_record_carries_every_stored_style_member() {
    // dash [5,3] at offset 1: remaining 1 < 5 stays in the first dash -> first_seg_left 4, then the gap 3 -> [4, 3].
    let record = vector_record(
        solid(1.0, 0.0, 0.0, 1.0),
        solid(0.0, 0.0, 1.0, 0.5),
        json!({"strokeWidth":3.5,"strokeCap":"square","strokeJoin":"bevel","miterLimit":2.5,
            "dashArray":[5.0, 3.0],"dashOffset":1.0,"paintOrder":"strokeFirst"}),
    );
    let style = style_of_record(&record).unwrap();
    assert_eq!(
        style.fill,
        FillResolution::Solid(scene_solid(1.0, 0.0, 0.0, 1.0))
    );
    assert_eq!(style.order, PaintOrder::StrokeFirst);
    assert_eq!(
        style.stroke,
        Some(ItemStroke {
            paint: scene_solid(0.0, 0.0, 1.0, 0.5),
            stroke: SceneStroke {
                width: 3.5,
                cap: SceneCap::Square,
                join: SceneJoin::Bevel,
                miter_limit: Some(2.5),
                dash: Some(vec![4.0, 3.0]),
            },
        })
    );
}

#[test]
fn an_explicit_null_fill_and_stroke_are_no_paint_and_absent_members_are_distinct_from_zero() {
    let none = style_of_record(&vector_record(
        Value::Null,
        Value::Null,
        json!({"strokeWidth":7.0}),
    ))
    .unwrap();
    assert_eq!(none.fill, FillResolution::Empty);
    assert_eq!(
        none.stroke, None,
        "no authored stroke paint means no stroke, whatever the width says"
    );
    let absent = style_of_record(&vector_record(
        Value::Null,
        solid(0.0, 0.0, 0.0, 1.0),
        json!({}),
    ))
    .unwrap();
    let explicit_zero = style_of_record(&vector_record(
        Value::Null,
        solid(0.0, 0.0, 0.0, 1.0),
        json!({"miterLimit":0.0}),
    ))
    .unwrap();
    assert_eq!(absent.stroke.unwrap().stroke.miter_limit, None);
    assert_eq!(explicit_zero.stroke.unwrap().stroke.miter_limit, Some(0.0));
}

#[test]
fn paint_order_is_stroke_first_only_for_that_exact_member() {
    let order = |extra: Value| {
        style_of_record(&vector_record(Value::Null, Value::Null, extra))
            .unwrap()
            .order
    };
    assert_eq!(order(json!({})), PaintOrder::FillFirst);
    assert_eq!(
        order(json!({"paintOrder":"fillFirst"})),
        PaintOrder::FillFirst
    );
    assert_eq!(
        order(json!({"paintOrder":"strokeFirst"})),
        PaintOrder::StrokeFirst
    );
}

#[test]
fn item_opacity_is_folded_into_both_alphas_and_leaves_the_channels_alone() {
    let record = vector_record(
        solid(0.2, 0.4, 0.6, 0.75),
        solid(0.25, 0.5, 0.75, 0.5),
        json!({"opacity":0.5}),
    );
    let style = style_of_record(&record).unwrap();
    let fill = style.fill.solid().unwrap().rgba_f32().unwrap();
    assert_eq!(
        fill,
        [0.2f64 as f32, 0.4f64 as f32, 0.6f64 as f32, 0.375f32]
    );
    assert_eq!(
        style.stroke.unwrap().paint.rgba_f32().unwrap(),
        [0.25f32, 0.5f32, 0.75f32, 0.25f32]
    );
    // Fully transparent items keep their channels and lose all alpha.
    let clear = style_of_record(&vector_record(
        solid(0.2, 0.4, 0.6, 0.75),
        Value::Null,
        json!({"opacity":0.0}),
    ))
    .unwrap();
    assert_eq!(clear.fill.solid().unwrap().rgba_f32().unwrap()[3], 0.0f32);
}
