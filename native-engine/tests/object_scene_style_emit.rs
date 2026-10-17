//! L07-I (F16, G03a): the typed scene style becomes the exact Vello values the baseline engine builds, and every
//! style is drawn into a real Vello scene. CPU only: pixel rows live in object_scene_style_emit_gpu.rs.
//! Oracle: tests/fixtures/object-scene-style-emit/oracle.txt (generator tools/gen_oracle.js, baseline fbe4b8a3).
use super::rows::{document, expected_stroke, gradient_fill, rows, scene_items, ORACLE};
use super::{prepare_item, EmitError, PreparedFill, PreparedItem};
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record, SceneItemStyle,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, GradientInput, GradientRejection, PaintOrder, PaintRules, PaintStep,
    SceneSolid, StopInput,
};
use crate::object_document::SolidFill;
use serde_json::{json, Number, Value};
use vello::kurbo::{Affine, BezPath, Point};
use vello::peniko::color::DynamicColor;
use vello::peniko::{Color, Extend, Gradient, GradientKind};
use vello::Scene;

const PINNED: [(&str, &str); 8] = [
    (
        "dash_pattern_with_offset",
        "c19710f801510a0d7376b1f4db4fc335052fb52a4bc151e740bfdd58c8a6069b",
    ),
    (
        "stroke_from",
        "20f612107bc4059ea042ec181db6680924b4af4b73f7f28069482ef50cb8f75f",
    ),
    (
        "cap_from",
        "b03ca133c3019f7366374608f7d952988a6bb4dd245ac65336aede8da5f8d25f",
    ),
    (
        "join_from",
        "f35f26092d234234992ba70e194ec8a692e30b0cb204b1e6a58dd81145e9439f",
    ),
    (
        "gradient_brush",
        "7f5aa58109735e2c37d5ba04aa4418fd807f5bdfd99ceeae042682541e25d2e4",
    ),
    (
        "paint_order_block",
        "22e321053481a73e07fe59cecdab6e48c7b8efd296cc9565e6107670aa999e0c",
    ),
    (
        "stroke_block",
        "ba038124fe85295d745fe825e99b424ffa0606d9dda029aeb94b92079f98b0f6",
    ),
    (
        "cssColorToRgba",
        "0855d2b41224bde044b41cccf68148dedadd17c6638fd9f08cffe5fc8786f2a9",
    ),
];

fn scene_row(id: &str) -> Value {
    rows("scene")
        .into_iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no scene row {id}"))
}

fn prepared_of(id: &str) -> PreparedItem {
    prepare_item(&scene_items(&scene_row(id))[0].style).expect("scene style prepares")
}

fn single(record: &Value) -> PreparedItem {
    let doc = document(vec![record.clone()]);
    let style = style_of_record(&doc.objects()[0]).expect("style");
    prepare_item(&style).expect("prepared")
}

fn square() -> BezPath {
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((10.0, 0.0));
    path.line_to((10.0, 10.0));
    path.line_to((0.0, 10.0));
    path.close_path();
    path
}

fn drawn_paths(item: &PreparedItem) -> u32 {
    let mut scene = Scene::new();
    item.draw(&mut scene, Affine::IDENTITY, &square());
    scene.encoding().n_paths
}

fn vector_record(id: &str, fill: Value, stroke: Value, opacity: f64) -> Value {
    let real = !stroke.is_null();
    let seg = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[seg(0.0, 0.0), seg(10.0, 0.0), seg(10.0, 10.0)]},
        "fill":fill,
        "vector":{"stroke":stroke,"hasRealStroke":real,"strokeWidth":2.0,"strokeCap":"butt","strokeJoin":"miter","opacity":opacity}})
}

fn gradient_input_style(fill: FillResolution) -> SceneItemStyle {
    SceneItemStyle {
        fill,
        stroke: None,
        order: PaintOrder::FillFirst,
    }
}

#[test]
fn oracle_header_pins_the_baseline_slices_and_counts() {
    let header: Vec<&str> = ORACLE
        .lines()
        .filter(|line| line.starts_with('#'))
        .collect();
    assert_eq!(header.len(), 4);
    assert!(header[0].ends_with("(baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc)"));
    for (name, sha) in PINNED {
        let pair = format!("\"{name}\":\"{sha}\"");
        assert!(
            header[1..3].iter().any(|line| line.contains(&pair)),
            "{name}"
        );
    }
    assert_eq!(
        header[3],
        "# counts {\"stroke\":14,\"scene\":20,\"probes\":192}"
    );
    assert_eq!(rows("stroke").len(), 14);
    assert_eq!(rows("scene").len(), 20);
    let probes: usize = rows("scene")
        .iter()
        .map(|r| r["probes"].as_array().unwrap().len())
        .sum();
    assert_eq!(probes, 192);
}

#[test]
fn every_stroke_row_prepares_the_exact_baseline_stroke() {
    for row in rows("stroke") {
        let prepared = single(&row["record"]);
        let (stroke, color) = prepared.stroke.as_ref().expect("row has a stroke");
        assert_eq!(*stroke, expected_stroke(&row["expect"]), "{}", row["id"]);
        assert_eq!(*color, Color::new([0.0, 0.0, 1.0, 1.0]), "{}", row["id"]);
        assert!(prepared.fill.is_none(), "{}", row["id"]);
    }
}

#[test]
fn hand_computed_stroke_literals_agree_with_the_oracle_and_the_prepared_stroke() {
    // (id, width, miter, dash). Dash arrays are worked out by hand from engine.rs
    // `dash_pattern_with_offset`, independent of the generator's transcription of it:
    // [4,4]@2 -> remaining 2 inside the first dash -> [2,4]; [4,4]@5 -> lands 1 into the gap -> [0,3,4];
    // [4,4]@-2 -> rem_euclid 6 -> 2 into the gap -> [0,2,4]; offset 0 bakes nothing.
    let cases: [(&str, f64, f64, Option<Vec<f64>>); 6] = [
        ("dash-offset-inside-dash", 2.0, 4.0, Some(vec![2.0, 4.0])),
        (
            "dash-offset-inside-gap",
            2.0,
            4.0,
            Some(vec![0.0, 3.0, 4.0]),
        ),
        ("dash-negative-offset", 2.0, 4.0, Some(vec![0.0, 2.0, 4.0])),
        ("dash-explicit-zero-offset", 2.0, 4.0, Some(vec![6.0, 2.0])),
        ("zero-width-uses-one", 1.0, 4.0, None),
        ("explicit-zero-miter-limit", 2.0, 0.0, None),
    ];
    let all = rows("stroke");
    for (id, width, miter, dash) in cases {
        let row = all.iter().find(|row| row["id"] == id).expect("row");
        let prepared = single(&row["record"]);
        let (stroke, _) = prepared.stroke.as_ref().unwrap();
        assert_eq!(stroke.width, width, "{id}");
        assert_eq!(stroke.miter_limit, miter, "{id}");
        assert_eq!(stroke.dash_offset, 0.0, "{id}");
        assert_eq!(
            stroke.dash_pattern.as_slice(),
            dash.as_deref().unwrap_or(&[]),
            "{id}"
        );
        let oracle_dash = row["expect"]["dash"].as_array().map(|entries| {
            entries
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<f64>>()
        });
        assert_eq!(oracle_dash, dash, "{id}");
    }
}

#[test]
fn colors_are_straight_floats_with_the_item_opacity_folded_into_alpha() {
    let fill = json!({"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75});
    let stroke = json!({"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":0.5});
    let folded = single(&vector_record("c1", fill.clone(), stroke.clone(), 0.5));
    let channel = |value: f64| value as f32;
    assert_eq!(
        folded.fill,
        Some(PreparedFill::Solid(Color::new([
            channel(0.2),
            channel(0.4),
            channel(0.6),
            0.375
        ])))
    );
    assert_eq!(
        folded.stroke.as_ref().unwrap().1,
        Color::new([0.25, 0.5, 0.75, 0.25])
    );
    // Opacity 1 leaves the authored alpha untouched.
    let plain = single(&vector_record("c2", fill, stroke, 1.0));
    assert_eq!(
        plain.fill,
        Some(PreparedFill::Solid(Color::new([
            channel(0.2),
            channel(0.4),
            channel(0.6),
            0.75
        ])))
    );
    assert_eq!(
        plain.stroke.as_ref().unwrap().1,
        Color::new([0.25, 0.5, 0.75, 0.5])
    );
}

#[test]
fn paint_order_steps_follow_the_recorded_order() {
    for (id, steps) in [
        ("rect-fill-first", [PaintStep::Fill, PaintStep::Stroke]),
        ("rect-stroke-first", [PaintStep::Stroke, PaintStep::Fill]),
        (
            "rect-absent-paint-order-is-fill-first",
            [PaintStep::Fill, PaintStep::Stroke],
        ),
    ] {
        assert_eq!(prepared_of(id).order, steps, "{id}");
    }
}

#[test]
fn gradient_rows_prepare_peniko_gradients_with_literal_geometry_and_stops() {
    let gradient_of = |id: &str| -> Gradient {
        match prepared_of(id).fill {
            Some(PreparedFill::Gradient(gradient)) => gradient,
            other => panic!("{id}: {other:?}"),
        }
    };
    let rgba =
        |r: u8, g: u8, b: u8, a: u8| DynamicColor::from_alpha_color(Color::from_rgba8(r, g, b, a));
    let linear = gradient_of("gradient-linear");
    assert_eq!(linear.extend, Extend::Pad);
    match linear.kind {
        GradientKind::Linear(position) => {
            assert_eq!(position.start, Point::new(0.0, 0.0));
            assert_eq!(position.end, Point::new(32.0, 0.0));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(linear.stops.len(), 2);
    assert_eq!((linear.stops[0].offset, linear.stops[1].offset), (0.0, 1.0));
    assert_eq!(linear.stops[0].color, rgba(0, 0, 0, 255));
    assert_eq!(linear.stops[1].color, rgba(255, 255, 255, 255));

    let radial = gradient_of("gradient-radial");
    match radial.kind {
        GradientKind::Radial(position) => {
            assert_eq!(position.start_center, Point::new(16.0, 16.0));
            assert_eq!(position.end_center, Point::new(16.0, 16.0));
            assert_eq!((position.start_radius, position.end_radius), (0.0, 16.0));
        }
        other => panic!("{other:?}"),
    }
    // Opacity 0.5 reaches the stop alpha through cssColorToRgba: round(255 * 0.5) = 128.
    let faded = gradient_of("gradient-linear-with-opacity");
    assert_eq!(faded.stops[0].color, rgba(255, 0, 0, 128));
    assert_eq!(faded.stops[1].color, rgba(0, 0, 255, 128));
}

#[test]
fn a_degenerate_radial_gradient_keeps_the_baseline_radius_floor() {
    let spec = json!({"kind":"radial","from":[5.0,5.0],"to":[5.0,5.0],"opacity":1.0,
        "stops":[{"offset":0.0,"color":"#000000"},{"offset":1.0,"color":"#ffffff"}]});
    let item = prepare_item(&gradient_input_style(gradient_fill(&spec))).unwrap();
    match item.fill {
        Some(PreparedFill::Gradient(Gradient {
            kind: GradientKind::Radial(position),
            ..
        })) => {
            assert_eq!(position.end_radius, 0.01);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unbuildable_gradient_is_a_typed_error_never_a_flat_or_missing_fill() {
    let flat = SceneSolid::from_fill(&SolidFill {
        kind: "solid".to_string(),
        r: Number::from_f64(1.0).unwrap(),
        g: Number::from_f64(0.0).unwrap(),
        b: Number::from_f64(0.0).unwrap(),
        a: Number::from_f64(1.0).unwrap(),
    })
    .unwrap();
    let origin = [0.0, 0.0];
    let unit_x = [1.0, 0.0];
    let one = [StopInput {
        offset: 0.0,
        color: Some("#ff0000"),
    }];
    let few = GradientInput {
        kind: "linear",
        from: Some(&origin[..]),
        to: Some(&unit_x[..]),
        stops: Some(&one[..]),
    };
    let few_style = gradient_input_style(
        PaintRules::BASELINE
            .resolve_fill(Some(&few), Some(flat.clone()), None)
            .unwrap(),
    );
    // Baseline engine.rs would paint the flat fill here; the lane acceptance forbids a silent flat fill.
    assert!(few_style.fill.fallback().is_some() && few_style.fill.solid().is_some());
    let error = prepare_item(&few_style).unwrap_err();
    assert_eq!(
        error,
        EmitError::GradientRejected(GradientRejection::TooFewStops)
    );
    assert_eq!(error.message(), "gradient needs at least two stops");

    let none = GradientInput {
        kind: "linear",
        from: None,
        to: Some(&unit_x[..]),
        stops: Some(&one[..]),
    };
    let none_style = gradient_input_style(
        PaintRules::BASELINE
            .resolve_fill(Some(&none), None, None)
            .unwrap(),
    );
    let error = prepare_item(&none_style).unwrap_err();
    assert_eq!(
        error,
        EmitError::GradientRejected(GradientRejection::MissingGeometry)
    );
    assert_eq!(error.message(), "gradient geometry is missing");
}

#[test]
fn an_unrepresentable_paint_keeps_the_compositors_own_message() {
    let error = SceneSolid::from_fill(&SolidFill {
        kind: "solid".to_string(),
        r: Number::from_f64(1.5).unwrap(),
        g: Number::from_f64(0.0).unwrap(),
        b: Number::from_f64(0.0).unwrap(),
        a: Number::from_f64(1.0).unwrap(),
    })
    .unwrap_err();
    assert_eq!(
        EmitError::Paint(error).message(),
        "unrepresentable object straight RGBA"
    );
}

#[test]
fn drawing_paints_one_path_per_present_paint_and_nothing_for_none() {
    assert_eq!(drawn_paths(&prepared_of("rect-fill-first")), 2);
    assert_eq!(drawn_paths(&prepared_of("rect-stroke-first")), 2);
    assert_eq!(drawn_paths(&prepared_of("rect-stroke-only")), 1);
    assert_eq!(drawn_paths(&prepared_of("open-path-fill")), 1);
    assert_eq!(drawn_paths(&prepared_of("gradient-linear")), 1);
    let nothing = single(&vector_record("n1", Value::Null, Value::Null, 1.0));
    assert!(nothing.fill.is_none() && nothing.stroke.is_none());
    assert_eq!(drawn_paths(&nothing), 0);
}
