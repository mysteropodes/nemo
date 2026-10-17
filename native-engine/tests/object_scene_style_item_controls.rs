//! L07-I negative and mutation controls for the typed item style: every refusal is typed, every stored member
//! reaches the style, and the opacity fold is a product (a replacement or an ignored opacity is noticed).
use super::tests::{legacy_record, solid, vector_record};
use super::{style_of_record, StyleUnavailable};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::ScenePaintError;
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::StrokeError;
use crate::object_document::{ObjectRecord, Presence, SolidFill, VectorBody};
use serde_json::{json, Number, Value};

const ITEM_SOURCE: &str = include_str!("../src/object_scene_style_item.rs");
const PAINT_PLAN: &str = include_str!("../src/object_paint_plan.rs");

fn number(value: f64) -> Number {
    Number::from_f64(value).unwrap()
}

fn with_body(record: &ObjectRecord, change: impl FnOnce(&mut VectorBody)) -> ObjectRecord {
    let mut out = record.clone();
    let mut body = out.vector.value().expect("vector body").clone();
    change(&mut body);
    out.vector = Presence::Present(body);
    out
}

fn with_fill(record: &ObjectRecord, change: impl FnOnce(&mut SolidFill)) -> ObjectRecord {
    let mut out = record.clone();
    let mut paint = out.solid_fill().expect("solid fill").clone();
    change(&mut paint);
    out.fill = Presence::Present(Some(paint));
    out
}

#[test]
fn every_record_that_cannot_be_drawn_is_a_typed_unavailable() {
    let drawable = vector_record(
        solid(1.0, 0.0, 0.0, 1.0),
        solid(0.0, 0.0, 1.0, 1.0),
        json!({}),
    );
    assert!(style_of_record(&drawable).is_ok());
    let strokeless = vector_record(Value::Null, Value::Null, json!({}));
    let gradient_kind = |paint: &mut SolidFill| paint.kind = "gradient".to_string();

    let mut unknown_family = drawable.clone();
    unknown_family.family = "image-mesh".to_string();
    let mut no_body = drawable.clone();
    no_body.vector = Presence::Missing;
    let mut no_fill_slot = drawable.clone();
    no_fill_slot.fill = Presence::Missing;
    let mut legacy_no_fill = legacy_record();
    legacy_no_fill.fill = Presence::Missing;
    let mut stroke_kind = drawable.clone();
    let mut paint = stroke_kind
        .vector()
        .unwrap()
        .solid_stroke()
        .unwrap()
        .clone();
    gradient_kind(&mut paint);
    stroke_kind = with_body(&stroke_kind, |body| {
        body.stroke = Presence::Present(Some(paint))
    });

    let cases: Vec<(&str, ObjectRecord, StyleUnavailable)> = vec![
        (
            "unknown family",
            unknown_family,
            StyleUnavailable::UnknownFamily,
        ),
        (
            "vector family without a body",
            no_body,
            StyleUnavailable::MissingVectorBody,
        ),
        (
            "vector fill slot absent",
            no_fill_slot,
            StyleUnavailable::MissingFill,
        ),
        (
            "legacy fill absent",
            legacy_no_fill,
            StyleUnavailable::MissingFill,
        ),
        (
            "stroke slot absent",
            with_body(&drawable, |body| body.stroke = Presence::Missing),
            StyleUnavailable::MissingStroke,
        ),
        (
            "hasRealStroke without a stroke paint",
            with_body(&strokeless, |body| body.has_real_stroke = true),
            StyleUnavailable::InconsistentStroke,
        ),
        (
            "stroke paint without hasRealStroke",
            with_body(&drawable, |body| body.has_real_stroke = false),
            StyleUnavailable::InconsistentStroke,
        ),
        (
            "gradient-kind fill",
            with_fill(&drawable, gradient_kind),
            StyleUnavailable::NonSolidPaint,
        ),
        (
            "gradient-kind legacy fill",
            with_fill(&legacy_record(), gradient_kind),
            StyleUnavailable::NonSolidPaint,
        ),
        (
            "gradient-kind stroke",
            stroke_kind,
            StyleUnavailable::NonSolidPaint,
        ),
        (
            "opacity above one",
            with_body(&drawable, |body| body.opacity = number(1.5)),
            StyleUnavailable::InvalidOpacity,
        ),
        (
            "negative opacity",
            with_body(&drawable, |body| body.opacity = number(-0.1)),
            StyleUnavailable::InvalidOpacity,
        ),
        (
            "channel above one",
            with_fill(&drawable, |paint| paint.r = number(1.5)),
            StyleUnavailable::Paint(ScenePaintError::UnrepresentableStraightRgba),
        ),
        (
            "negative dash entry",
            with_body(&drawable, |body| {
                body.dash_array = Presence::Present(vec![number(4.0), number(-1.0)])
            }),
            StyleUnavailable::Stroke(StrokeError::NegativeDashEntry),
        ),
    ];
    for (label, record, expected) in cases {
        assert_eq!(style_of_record(&record), Err(expected), "{label}");
    }
}

#[test]
fn a_negative_dash_is_only_refused_when_the_stroke_exists() {
    // The baseline reads the stroke fields only inside `if (sc)`; a strokeless item never builds a stroke.
    let strokeless = vector_record(Value::Null, Value::Null, json!({}));
    let with_dash = with_body(&strokeless, |body| {
        body.dash_array = Presence::Present(vec![number(4.0), number(-1.0)])
    });
    assert_eq!(style_of_record(&with_dash).unwrap().stroke, None);
}

#[test]
fn every_unavailable_variant_has_its_own_message() {
    let all = [
        StyleUnavailable::UnknownFamily,
        StyleUnavailable::MissingVectorBody,
        StyleUnavailable::MissingFill,
        StyleUnavailable::MissingStroke,
        StyleUnavailable::NonSolidPaint,
        StyleUnavailable::InconsistentStroke,
        StyleUnavailable::InvalidOpacity,
        StyleUnavailable::Paint(ScenePaintError::UnrepresentableStraightRgba),
        StyleUnavailable::Stroke(StrokeError::NegativeDashEntry),
    ];
    let messages: Vec<&str> = all.iter().map(|variant| variant.message()).collect();
    for (index, message) in messages.iter().enumerate() {
        assert!(!message.is_empty());
        assert!(
            !messages[..index].contains(message),
            "duplicate message {message}"
        );
    }
    assert_eq!(
        StyleUnavailable::Paint(ScenePaintError::UnrepresentableStraightRgba).message(),
        "unrepresentable object straight RGBA"
    );
    assert_eq!(
        StyleUnavailable::Stroke(StrokeError::NegativeDashEntry).message(),
        "dash pattern entries must not be negative"
    );
}

#[test]
fn every_stored_style_member_changes_the_style_and_identity_does_not() {
    let base_extra = json!({"strokeWidth":3.0,"strokeCap":"butt","strokeJoin":"miter","miterLimit":2.0,
        "dashArray":[4.0, 4.0],"dashOffset":1.0,"paintOrder":"fillFirst","opacity":0.8});
    let build = |fill: Value, stroke: Value, change: Value| {
        let mut extra = base_extra.clone();
        for (key, value) in change.as_object().unwrap() {
            extra[key] = value.clone();
        }
        vector_record(fill, stroke, extra)
    };
    let red = solid(0.2, 0.4, 0.6, 0.75);
    let blue = solid(0.25, 0.5, 0.75, 0.5);
    let base = style_of_record(&build(red.clone(), blue.clone(), json!({}))).unwrap();
    let changes: Vec<(&str, ObjectRecord)> = vec![
        (
            "strokeWidth",
            build(red.clone(), blue.clone(), json!({"strokeWidth":4.0})),
        ),
        (
            "strokeCap",
            build(red.clone(), blue.clone(), json!({"strokeCap":"round"})),
        ),
        (
            "strokeJoin",
            build(red.clone(), blue.clone(), json!({"strokeJoin":"bevel"})),
        ),
        (
            "miterLimit",
            build(red.clone(), blue.clone(), json!({"miterLimit":3.0})),
        ),
        (
            "dashArray",
            build(red.clone(), blue.clone(), json!({"dashArray":[4.0, 5.0]})),
        ),
        (
            "dashOffset",
            build(red.clone(), blue.clone(), json!({"dashOffset":2.0})),
        ),
        (
            "paintOrder",
            build(
                red.clone(),
                blue.clone(),
                json!({"paintOrder":"strokeFirst"}),
            ),
        ),
        (
            "opacity",
            build(red.clone(), blue.clone(), json!({"opacity":0.9})),
        ),
        (
            "fill colour",
            build(solid(0.2, 0.4, 0.7, 0.75), blue.clone(), json!({})),
        ),
        (
            "stroke alpha",
            build(red.clone(), solid(0.25, 0.5, 0.75, 0.6), json!({})),
        ),
        ("no stroke", build(red.clone(), Value::Null, json!({}))),
        ("no fill", build(Value::Null, blue.clone(), json!({}))),
    ];
    for (member, record) in &changes {
        assert_ne!(
            style_of_record(record).unwrap(),
            base,
            "{member} must reach the style"
        );
    }
    // The target identity is not a style member: the same style with another record id is equal.
    let mut renamed = build(red, blue, json!({}));
    renamed.target.stroke_id = "another".to_string();
    assert_eq!(style_of_record(&renamed).unwrap(), base);
}

#[test]
fn the_opacity_fold_is_a_product_and_never_a_replacement_or_a_no_op() {
    let record = vector_record(
        solid(0.2, 0.4, 0.6, 0.75),
        Value::Null,
        json!({"opacity":0.5}),
    );
    let alpha = style_of_record(&record)
        .unwrap()
        .fill
        .solid()
        .unwrap()
        .rgba_f32()
        .unwrap()[3];
    assert_eq!(alpha, 0.375f32);
    assert_ne!(alpha, 0.5f32, "opacity must not replace the authored alpha");
    assert_ne!(alpha, 0.75f32, "opacity must not be ignored");
}

#[test]
fn source_guards_pin_the_layering_and_the_retained_legacy_style() {
    assert!(!ITEM_SOURCE.contains("super::"));
    assert!(ITEM_SOURCE.contains(
        "use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{"
    ));
    assert!(ITEM_SOURCE.contains(
        "use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{"
    ));
    // The paint plan keeps the L07-F retained typed solid for the legacy family and routes every other family
    // through the one style function.
    assert!(PAINT_PLAN.contains("SceneItemStyle::solid_fill(self.solid().clone())"));
    assert!(PAINT_PLAN.contains("style_of_record("));
}
