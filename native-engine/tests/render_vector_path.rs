//! CPU tests of the vector scene join (L01-I): the drawn path, the clipping contour and envelope, the scene entry, and the
//! paint-plan accessors. Every expected number is a literal derived by hand from the geometry or comes from the baseline
//! oracle (`fixtures/render-vector-stroke/oracle.txt`); none is computed with the code under test.
use super::{build_path, VectorShape, MAX_ANCHORS};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::PaintOrder;
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    SceneCap, SceneJoin,
};
use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_vector;
use crate::evaluation::object_vector_kernels::render_vector_eval::tests::{
    rows as eval_rows, snapshot as eval_snapshot,
};
use crate::object_render_scene::ObjectSceneError;
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use serde_json::json;
use vello::kurbo::{PathEl, Point};

#[path = "render_vector_controls.rs"]
mod controls;
#[path = "render_vector_support.rs"]
pub(crate) mod support;
use support::*;

fn seg(x: f64, y: f64) -> CubicSegment {
    CubicSegment {
        point: [x, y],
        handle_in: [0., 0.],
        handle_out: [0., 0.],
    }
}
fn segs(points: &[[f64; 2]]) -> Vec<CubicSegment> {
    points.iter().map(|p| seg(p[0], p[1])).collect()
}
fn line() -> Vec<CubicSegment> {
    segs(&[[16., 32.], [48., 32.]])
}

#[test]
fn oracle_paths_match_the_baseline_builder_bit_for_bit() {
    let rows = path_rows();
    assert_eq!(rows.len(), 7);
    for row in &rows {
        let built = build_path(&row.segments, row.closed);
        assert_eq!(built.elements(), row.elements.as_slice(), "{}", row.id);
    }
}

#[test]
fn a_closed_vector_path_is_the_contour_path_the_closed_family_already_drew() {
    let mut admitted = 0;
    for row in path_rows().iter().filter(|row| row.closed) {
        if let Ok(contour) = ClosedCubicPath::new(row.segments.clone()) {
            admitted += 1;
            assert_eq!(
                build_path(&row.segments, true).elements(),
                contour.commands(),
                "{}",
                row.id
            );
        }
    }
    assert!(
        admitted >= 2,
        "the oracle must keep exercising the closed contour"
    );
}

#[test]
fn an_open_path_has_one_curve_per_gap_and_never_closes() {
    let open = build_path(&line(), false);
    assert_eq!(
        open.elements(),
        [
            PathEl::MoveTo(Point::new(16., 32.)),
            PathEl::CurveTo(
                Point::new(16., 32.),
                Point::new(48., 32.),
                Point::new(48., 32.)
            ),
        ]
    );
    let closed = build_path(&line(), true);
    assert_eq!(closed.elements().len(), 4);
    assert_eq!(closed.elements()[3], PathEl::ClosePath);
}

#[test]
fn an_open_line_carries_the_padded_bounds_rectangle_as_its_clipping_contour() {
    let shape = VectorShape::new(&line(), false, Some(7.0)).expect("shape");
    assert_eq!(shape.contour.segments().len(), 4);
    assert_eq!(shape.contour.control_bounds(), [9., 25., 55., 39.]);
    assert_eq!(shape.envelope, [9., 25., 55., 39.]);
    assert_eq!(shape.path.elements().len(), 2);
}

#[test]
fn a_closed_path_keeps_its_real_contour_and_a_stroke_only_pads_the_envelope() {
    let square = segs(&SQUARE);
    let plain = VectorShape::new(&square, true, None).expect("shape");
    assert_eq!(plain.contour.segments(), square.as_slice());
    assert_eq!(plain.contour.control_bounds(), [16., 16., 48., 48.]);
    assert_eq!(plain.envelope, [16., 16., 48., 48.]);
    let stroked = VectorShape::new(&square, true, Some(9.0)).expect("shape");
    assert_eq!(stroked.contour.segments(), square.as_slice());
    assert_eq!(stroked.envelope, [7., 7., 57., 57.]);
    assert_eq!(stroked.path.elements(), plain.path.elements());
}

#[test]
fn a_closed_path_without_a_hull_falls_back_to_the_rectangle_and_still_draws_closed() {
    let shape = VectorShape::new(&line(), true, Some(7.0)).expect("shape");
    assert!(ClosedCubicPath::new(line()).is_err());
    assert_eq!(shape.contour.control_bounds(), [9., 25., 55., 39.]);
    assert_eq!(shape.path.elements().len(), 4);
    assert_eq!(shape.path.elements()[3], PathEl::ClosePath);
}

#[test]
fn a_fill_only_open_path_is_padded_by_the_antialiasing_margin_only() {
    let shape = VectorShape::new(&line(), false, None).expect("shape");
    assert_eq!(shape.contour.control_bounds(), [15., 31., 49., 33.]);
    assert_eq!(shape.envelope, [15., 31., 49., 33.]);
}

#[test]
fn handles_extend_the_bounds_of_the_clipping_contour() {
    let curve = vec![
        CubicSegment {
            point: [10., 10.],
            handle_in: [0., 0.],
            handle_out: [0., -20.],
        },
        CubicSegment {
            point: [30., 10.],
            handle_in: [0., -20.],
            handle_out: [0., 0.],
        },
    ];
    let shape = VectorShape::new(&curve, false, Some(2.0)).expect("shape");
    assert_eq!(shape.contour.control_bounds(), [8., -12., 32., 12.]);
    assert_eq!(
        shape.path.elements()[1],
        PathEl::CurveTo(
            Point::new(10., -10.),
            Point::new(30., -10.),
            Point::new(30., 10.)
        )
    );
}

#[test]
fn unusable_geometry_is_a_typed_refusal_never_a_default() {
    let refused =
        |segments: &[CubicSegment], closed, reach| VectorShape::new(segments, closed, reach).err();
    assert_eq!(
        refused(&segs(&[[5., 5.], [5., 5.]]), false, Some(3.0)),
        Some("vector path has no extent")
    );
    assert_eq!(
        refused(&segs(&[[5., 5.]]), false, None),
        Some("vector path requires 2..=256 anchors in the scene")
    );
    let many: Vec<[f64; 2]> = (0..=MAX_ANCHORS).map(|i| [i as f64, 0.]).collect();
    assert_eq!(
        refused(&segs(&many), false, None),
        Some("vector path requires 2..=256 anchors in the scene")
    );
    assert_eq!(
        refused(&segs(&[[f64::NAN, 0.], [1., 1.]]), false, None),
        Some("vector anchors and handles must be finite")
    );
    // finite as f64 but beyond what the GPU encoding can hold: refused through the padded rectangle
    assert!(refused(&segs(&[[1e39, 0.], [0., 0.]]), false, Some(7.0)).is_some());
    // the bound itself is admitted
    let zigzag: Vec<[f64; 2]> = (0..MAX_ANCHORS)
        .map(|i| [i as f64, if i % 2 == 0 { 0. } else { 10. }])
        .collect();
    assert!(refused(&segs(&zigzag), false, Some(3.0)).is_none());
}

fn stroke_body(width: f64, cap: &str) -> serde_json::Value {
    body(
        json!({"stroke":solid(1., 0., 0., 1.),"hasRealStroke":true,"strokeWidth":width,"strokeCap":cap}),
    )
}

#[test]
fn an_open_stroked_line_is_admitted_with_its_open_path_and_stroke_paint() {
    let spec = parts(
        false,
        &[[16., 32.], [48., 32.]],
        serde_json::Value::Null,
        stroke_body(12., "round"),
    );
    let scene = scene_of(&snapshot(&v1_doc(&spec))).expect("scene");
    assert_eq!(scene.entries().len(), 1);
    let entry = scene.entry(0).expect("entry");
    assert_eq!(
        entry.path().elements(),
        [
            PathEl::MoveTo(Point::new(16., 32.)),
            PathEl::CurveTo(
                Point::new(16., 32.),
                Point::new(48., 32.),
                Point::new(48., 32.)
            ),
        ]
    );
    assert_eq!(entry.contour().control_bounds(), [9., 25., 55., 39.]);
    assert_eq!(entry.envelope(), [9., 25., 55., 39.]);
    assert_eq!(entry.solid().rgba_f32(), Ok([1.0, 0.0, 0.0, 1.0]));
}

#[test]
fn an_invisible_vector_is_an_item_that_draws_nothing_not_a_refusal() {
    let spec = parts(true, &SQUARE, serde_json::Value::Null, body(json!({})));
    let scene = scene_of(&snapshot(&v1_doc(&spec))).expect("an item without paint is valid");
    let entry = scene.entry(0).expect("entry");
    assert_eq!(entry.solid().rgba_f32(), Ok([0.0, 0.0, 0.0, 0.0]));
    assert_eq!(entry.path().elements().len(), 6);
}

#[test]
fn a_closed_fill_only_vector_keeps_the_contour_of_the_earlier_join_and_the_legacy_record_its_own_path(
) {
    let spec = parts(true, &SQUARE, solid(0., 0.5, 1., 1.), body(json!({})));
    let scene = scene_of(&snapshot(&v1_doc(&spec))).expect("scene");
    let entry = scene.entry(0).expect("entry");
    assert_eq!(entry.contour().segments(), segs(&SQUARE).as_slice());
    assert_eq!(entry.path().elements(), entry.contour().commands());
    assert_eq!(entry.envelope(), [16., 16., 48., 48.]);

    let legacy = scene_of(&snapshot(&legacy_doc(&SQUARE, solid(0., 0.5, 1., 1.)))).expect("scene");
    let drawn = legacy.entry(0).expect("entry");
    // a record that is not a vector draws the very path of its contour, not a copy
    assert!(std::ptr::eq(drawn.path(), drawn.contour().path()));
    assert_eq!(drawn.path().elements(), entry.path().elements());
}

#[test]
fn what_cannot_be_drawn_stays_a_typed_unsupported_geometry() {
    let refusal = |points: &[[f64; 2]]| {
        let spec = parts(false, points, solid(0., 0., 0., 1.), body(json!({})));
        scene_of(&snapshot(&v1_doc(&spec))).err()
    };
    assert_eq!(
        refusal(&[[5., 5.], [5., 5.]]),
        Some(ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            reason: "vector path has no extent"
        })
    );
    assert!(matches!(
        refusal(&[[1e39, 0.], [0., 0.]]),
        Some(ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            ..
        })
    ));
}

fn three_records() -> serde_json::Value {
    let line_a = parts(
        false,
        &[[8., 32.], [56., 32.]],
        serde_json::Value::Null,
        stroke_body(12., "round"),
    );
    let invisible = parts(true, &SQUARE, serde_json::Value::Null, body(json!({})));
    let filled = parts(
        true,
        &SQUARE,
        solid(0., 1., 0., 1.),
        body(json!({"opacity":0.5,"paintOrder":"strokeFirst"})),
    );
    scene_doc(&[line_a, invisible, filled])
}

#[test]
fn the_paint_plan_keeps_natural_order_and_paint_is_total_for_every_vector() {
    let snap = snapshot(&three_records());
    let plan = plan_of(&snap);
    assert_eq!(plan.commands().len(), 3);
    let paints: Vec<[f64; 4]> = plan
        .commands()
        .map(|c| c.paint().map(|n| n.as_f64().expect("number")))
        .collect();
    assert_eq!(
        paints,
        [[1., 0., 0., 1.], [0., 0., 0., 0.], [0., 1., 0., 1.]]
    );
    // the filled record still borrows its fill from the immutable source, as the legacy family always did
    let filled = plan.commands().nth(2).expect("command");
    let source = filled.source().record().solid_fill().expect("fill");
    assert!(std::ptr::eq(filled.paint()[3], &source.a));
    // the two unfilled records borrow the retained primary solid instead (no panic, no default)
    let first = plan.commands().next().expect("command");
    assert!(first.source().record().solid_fill().is_none());
}

#[test]
fn the_paint_plan_exposes_stroke_paint_order_and_opacity_of_the_vector_family() {
    let snap = snapshot(&three_records());
    let plan = plan_of(&snap);
    let styles: Vec<_> = plan.commands().map(|c| c.style().expect("style")).collect();
    let stroke = styles[0].stroke.as_ref().expect("stroke");
    assert_eq!(stroke.stroke.width, 12.0);
    assert_eq!(stroke.stroke.cap, SceneCap::Round);
    assert_eq!(stroke.stroke.join, SceneJoin::Round);
    assert_eq!(styles[0].order, PaintOrder::FillFirst);
    assert!(styles[1].stroke.is_none() && styles[1].fill.solid().is_none());
    assert_eq!(styles[2].order, PaintOrder::StrokeFirst);
    assert_eq!(
        styles[2].fill.solid().expect("fill").rgba_f32(),
        Ok([0.0, 1.0, 0.0, 0.5])
    );
    let first = plan.commands().next().expect("command");
    assert_eq!(
        first.path().elements(),
        [
            PathEl::MoveTo(Point::new(8., 32.)),
            PathEl::CurveTo(
                Point::new(8., 32.),
                Point::new(56., 32.),
                Point::new(56., 32.)
            ),
        ]
    );
}

#[test]
fn the_scene_draws_exactly_the_evaluated_vector_for_every_l01_h_oracle_row() {
    // 57 baseline rows: plain, zigzag, roughen, rebuilt shapes, open and closed, at their pinned frames. The scene must draw
    // the anchors the evaluation join returns, through the independently recomposed baseline builder, bit for bit.
    let (_, all) = eval_rows();
    assert_eq!(all.len(), 57);
    // Three rows carry property tracks the evaluation join does not read (a foreign kind on a rect or a star, any
    // track on a plain vector): L01-H's packet guard keeps their packet `Unavailable` (pinned there as
    // `a_packet_over_consumed_shape_tracks_evaluates_to_the_oracle`), so no scene can exist for them either.
    const PACKET_UNAVAILABLE: [&str; 3] = [
        "rect-foreign-kind-tracks-are-ignored@5",
        "plain-vector-with-foreign-tracks-is-not-rebuilt@5",
        "star-foreign-kind-tracks-are-ignored@5",
    ];
    let (mut open, mut refused) = (0, 0);
    for row in &all {
        let mut value: serde_json::Value = serde_json::from_str(&row.doc).expect("oracle document");
        value["objects"][0]["target"]["frameScope"]["frame"] = json!(row.frame);
        let snap = eval_snapshot(&value.to_string());
        if PACKET_UNAVAILABLE.contains(&row.id.as_str()) {
            assert_eq!(
                crate::object_frame_packet::prepare_object_frame(
                    &snap,
                    &selector(&snap, row.frame)
                )
                .err(),
                Some(crate::object_frame_packet::ObjectFrameError::Unavailable),
                "{}",
                row.id
            );
            refused += 1;
            continue;
        }
        let evaluated = evaluate_vector(&snap, 0, row.frame).expect("join");
        let scene = scene_at(&snap, row.frame).unwrap_or_else(|e| panic!("{}: {e:?}", row.id));
        let anchors: Vec<CubicSegment> = evaluated
            .segments
            .iter()
            .map(|s| CubicSegment {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect();
        let want = controls::mutated(controls::Mistake::NoMistake, &anchors, evaluated.closed);
        let entry = scene.entry(0).expect("entry");
        assert_eq!(entry.path().elements(), want.elements(), "{}", row.id);
        open += usize::from(!evaluated.closed);
    }
    assert_eq!((open, refused), (5, 3));
}
