//! Literal pins for the parametric-shape kernel. Every expected number comes from the baseline (see
//! `object_parametric_geometry_pin_data.rs`); the kernel-only behaviours (typed refusals) are marked as such.

use super::super::boxed::ParamShape;
use super::super::{
    build_arc_ellipse, build_round_rect, build_star_polygon, js_max, js_min, shape_geom,
    GeomSegment, GeomSource, ParamSegment, ShapeTool, StarState,
};
use super::same;

#[path = "object_parametric_geometry_pin_data.rs"]
mod data;
#[path = "object_parametric_geometry_pins_box.rs"]
mod pins_box;

const TRIG: f64 = 1e-11;

fn flat(s: &ParamSegment) -> [f64; 6] {
    [
        s.point[0],
        s.point[1],
        s.handle_in[0],
        s.handle_in[1],
        s.handle_out[0],
        s.handle_out[1],
    ]
}

/// Bit-exact (`-0` included) when `tol` is 0, else within `tol` per value.
fn matches(got: &[ParamSegment], want: &[[f64; 6]], tol: f64) {
    assert_eq!(got.len(), want.len(), "segment count");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        for (k, (a, b)) in flat(g).iter().zip(w).enumerate() {
            let ok = same(*a, *b) || (tol > 0.0 && a != b && (a - b).abs() <= tol);
            assert!(ok, "segment {i} value {k}: {a:e} vs {b:e}");
        }
    }
}

fn geom_rows(a: &[[Option<f64>; 6]]) -> Vec<GeomSegment> {
    let pair = |x: Option<f64>, y: Option<f64>| x.zip(y).map(|(x, y)| [x, y]);
    a.iter()
        .map(|r| GeomSegment {
            point: [r[0].unwrap(), r[1].unwrap()],
            handle_in: pair(r[2], r[3]),
            handle_out: pair(r[4], r[5]),
        })
        .collect()
}

fn rect(tl: f64, tr: f64, br: f64, bl: f64) -> ParamShape {
    ParamShape::Rect {
        tl: Some(tl),
        tr: Some(tr),
        br: Some(br),
        bl: Some(bl),
    }
}

fn frame_70() -> Vec<ParamSegment> {
    let c = |x: f64, y: f64| ParamSegment {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    };
    vec![c(0.0, 0.0), c(120.0, 0.0), c(120.0, 70.0), c(0.0, 70.0)]
}

#[test]
fn round_rect_corners_follow_the_baseline_anchor_order() {
    let one = build_round_rect(0.0, 0.0, 100.0, 60.0, [10.0, 0.0, 0.0, 0.0]).unwrap();
    matches(&one, &data::RR_TL_ONLY, 0.0);
    let all = build_round_rect(10.0, 20.0, 110.0, 80.0, [4.0, 8.0, 12.0, 16.0]).unwrap();
    matches(&all, &data::RR_ALL_CORNERS, 0.0);
    let sharp = build_round_rect(0.0, 0.0, 100.0, 60.0, [0.0; 4]).unwrap();
    assert_eq!(sharp.len(), 4, "a zero radius is one anchor per corner");
}

#[test]
fn round_rect_radii_clamp_to_half_the_shorter_side_and_floor_at_zero() {
    let clamped = build_round_rect(0.0, 0.0, 100.0, 60.0, [100.0, -5.0, 0.0, 40.0]).unwrap();
    matches(&clamped, &data::RR_CLAMPED, 0.0);
    let inverted = build_round_rect(100.0, 60.0, 0.0, 0.0, [10.0; 4]).unwrap();
    matches(&inverted, &data::RR_MIRRORED, 0.0);
}

#[test]
fn js_min_max_keep_the_signed_zero_rules_of_math_min_and_max() {
    assert!(js_min(0.0, -0.0).is_sign_negative());
    assert!(js_min(-0.0, 0.0).is_sign_negative());
    assert!(js_max(-0.0, 0.0).is_sign_positive());
    assert!(js_max(0.0, -0.0).is_sign_positive());
    assert!(js_min(f64::NAN, 1.0).is_nan() && js_max(1.0, f64::NAN).is_nan());
    // max(0, min(-0, half)) is +0, so a -0 radius is a square corner.
    let sq = build_round_rect(0.0, 0.0, 100.0, 60.0, [-0.0; 4]).unwrap();
    assert_eq!(sq.len(), 4);
}

#[test]
fn arc_ellipse_step_counts_and_pie_or_donut_closure() {
    let n = |sweep: f64, inner: f64| {
        build_arc_ellipse(0.0, 0.0, 50.0, 50.0, 0.0, sweep, Some(inner))
            .unwrap()
            .len()
    };
    let got = [
        n(0.1, 0.0),
        n(0.1, 0.5),
        n(359.9, 0.0),
        n(359.9, 0.5),
        n(32.0, 0.0),
        n(32.1, 0.0),
    ];
    assert_eq!(got, data::ARC_COUNTS);
    let pie = build_arc_ellipse(0.0, 0.0, 10.0, 10.0, 0.0, 90.0, None).unwrap();
    let edges: Vec<ParamSegment> = pie[..3]
        .iter()
        .chain(&pie[pie.len() - 3..])
        .copied()
        .collect();
    matches(&edges, &data::ARC_PIE_90_EDGES, TRIG);
    assert_eq!(
        pie[pie.len() - 1].point,
        [0.0, 0.0],
        "a pie closes through the centre"
    );
}

#[test]
fn arc_ellipse_clamps_sweep_and_inner_ratio() {
    let b = |sweep: f64, inner: Option<f64>| {
        build_arc_ellipse(3.0, 4.0, 20.0, 10.0, 15.0, sweep, inner).unwrap()
    };
    assert_eq!(b(400.0, Some(0.3)), b(359.9, Some(0.3)));
    assert_eq!(b(-5.0, Some(0.3)), b(0.1, Some(0.3)));
    assert_eq!(b(90.0, Some(1.2)), b(90.0, Some(0.95)));
    assert_eq!(b(90.0, Some(-0.2)), b(90.0, None));
    assert_eq!(b(90.0, Some(0.0)), b(90.0, None));
}

#[test]
fn star_counts_polygon_threshold_and_corner_epsilon() {
    let n = |pc: f64, ir: f64, cr: f64| {
        build_star_polygon(0.0, 0.0, 50.0, Some(pc), Some(ir), Some(cr))
            .unwrap()
            .len()
    };
    let got = [
        n(5.0, 0.5, 0.0),
        n(5.0, 0.999, 0.0),
        n(5.0, 0.998, 0.0),
        n(5.0, 0.5, 0.01),
        n(5.0, 0.5, 0.0100001),
        n(5.0, 1.0, 2.0),
        n(2.4, 0.5, 0.0),
        n(0.0, 0.5, 0.0),
        n(5.5, 0.5, 0.0),
    ];
    assert_eq!(got, data::STAR_COUNTS);
    let absent = build_star_polygon(0.0, 0.0, 50.0, None, None, None).unwrap();
    assert_eq!(
        absent,
        build_star_polygon(0.0, 0.0, 50.0, Some(5.0), Some(0.5), Some(0.0)).unwrap()
    );
}

#[test]
fn star_starts_pointing_up_and_corner_handles_are_kappa_times_the_gap() {
    let five = build_star_polygon(100.0, 100.0, 60.0, Some(5.0), Some(0.5), Some(0.0)).unwrap();
    matches(&five, &data::STAR_FIVE_POINTS, TRIG);
    let tri = build_star_polygon(0.0, 0.0, 50.0, Some(3.0), Some(1.0), Some(10.0)).unwrap();
    matches(&tri, &data::STAR_CORNERS_TRIANGLE, TRIG);
}

#[test]
fn shape_geom_wasm_and_javascript_fallback_really_differ() {
    let g = |tool, x0, y0, x1, y1, src| {
        shape_geom(tool, x0, y0, x1, y1, StarState::default(), src).unwrap()
    };
    let w = g(ShapeTool::Rect, 100.0, 60.0, 0.0, 0.0, GeomSource::Wasm);
    assert!(w.closed);
    assert_eq!(w.segments, geom_rows(&data::SG_RECT_REVERSED_WASM));
    let j = g(
        ShapeTool::Rect,
        100.0,
        60.0,
        0.0,
        0.0,
        GeomSource::JsFallback,
    );
    assert_eq!(j.segments, geom_rows(&data::SG_RECT_REVERSED_JS));
    let lw = g(ShapeTool::Line, 1.0, 2.0, 3.0, 4.0, GeomSource::Wasm);
    assert!(!lw.closed);
    assert_eq!(lw.segments, geom_rows(&data::SG_LINE_WASM));
    let lj = g(ShapeTool::Line, 1.0, 2.0, 3.0, 4.0, GeomSource::JsFallback);
    assert!(!lj.closed);
    assert_eq!(lj.segments, geom_rows(&data::SG_LINE_JS));
    for src in [GeomSource::Wasm, GeomSource::JsFallback] {
        let e = g(ShapeTool::Ellipse, 0.0, 0.0, 10.0, 20.0, src);
        assert_eq!(e.segments, geom_rows(&data::SG_ELLIPSE));
        assert_eq!(e.segments, geom_rows(&data::SG_ELLIPSE_JS));
    }
}

#[test]
fn shape_geom_star_radius_and_speech_bubble_radius_floor() {
    let s = shape_geom(
        ShapeTool::Star,
        0.0,
        0.0,
        120.0,
        80.0,
        StarState::default(),
        GeomSource::Wasm,
    )
    .unwrap();
    let want = geom_rows(&data::SG_STAR_FIRST);
    for (g, w) in s.segments.iter().zip(&want) {
        for k in 0..2 {
            assert!(
                (g.point[k] - w.point[k]).abs() <= TRIG,
                "star outer radius is half the shorter side"
            );
        }
    }
    let big = shape_geom(
        ShapeTool::SpeechBubble,
        0.0,
        0.0,
        100.0,
        60.0,
        StarState::default(),
        GeomSource::Wasm,
    )
    .unwrap();
    assert_eq!(
        big.segments[..2],
        geom_rows(&data::SG_BUBBLE_100X60_FIRST)[..]
    );
    let small = shape_geom(
        ShapeTool::SpeechBubble,
        0.0,
        0.0,
        10.0,
        10.0,
        StarState::default(),
        GeomSource::Wasm,
    )
    .unwrap();
    assert_eq!(
        small.segments[..2],
        geom_rows(&data::SG_BUBBLE_10X10_FIRST)[..]
    );
    assert_eq!(small.segments[0].point[0], 4.0, "radius floor is 4");
}
