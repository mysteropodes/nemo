//! Literal pins for the box, rotation, animation and refusal behaviour of the parametric-shape kernel
//! (second half of `object_parametric_geometry_pins.rs`; same baseline-printed literals).

use super::super::super::boxed::{
    apply_param_shape, apply_param_shape_animated, has_param_shape_motion, rotate_segments,
    sync_box_on_translate, sync_on_scale, AnimatedParams, ParamBox, ParamShape, ParamShapeKind,
    PathBounds,
};
use super::super::super::{
    build_arc_ellipse, build_round_rect, build_star_polygon, shape_geom, GeomSource,
    ParamGeomError, ParamSegment, ShapeTool, StarState, MAX_STAR_POINTS,
};
use super::{data, frame_70, matches, rect, TRIG};

#[test]
fn stamp_translate_and_scale_sync_literals() {
    let b = ParamBox::stamp(PathBounds {
        x: 0.1,
        y: 0.2,
        width: 0.7,
        height: 0.9,
    });
    assert_eq!(
        b.x2.to_bits(),
        (0.1_f64 + 0.7).to_bits(),
        "right is x + width, not the maximum"
    );
    assert_ne!(b.x2, 0.8);
    let base = ParamBox {
        x1: 0.0,
        y1: 0.0,
        x2: 100.0,
        y2: 60.0,
    };
    let moved = sync_box_on_translate(Some(base), 5.0, -7.5)
        .unwrap()
        .unwrap();
    assert_eq!(
        (moved.x1, moved.y1, moved.x2, moved.y2),
        (5.0, -7.5, 105.0, 52.5)
    );
    assert_eq!(sync_box_on_translate(None, 5.0, 5.0), Ok(None));
    let shape = rect(10.0, 10.0, 10.0, 10.0);
    let s = sync_on_scale(&shape, Some(base), 0.0, 2.0, 3.0, [0.0, 0.0])
        .unwrap()
        .unwrap();
    assert_eq!(
        [s.box_.x1, s.box_.y1, s.box_.x2, s.box_.y2],
        data::SCALE_RECT_BOX
    );
    matches(&s.rebuilt.unwrap(), &data::SCALE_RECT_SEGS, 0.0);
    let n = sync_on_scale(&shape, Some(base), 0.0, -2.0, 3.0, [0.0, 0.0])
        .unwrap()
        .unwrap();
    assert_eq!(
        [n.box_.x1, n.box_.y1, n.box_.x2, n.box_.y2],
        data::SCALE_NEG_BOX
    );
    let ellipse = ParamShape::Ellipse {
        start_angle: None,
        sweep: None,
        inner_radius: None,
    };
    let e = sync_on_scale(&ellipse, Some(base), 0.0, 2.0, 2.0, [0.0, 0.0])
        .unwrap()
        .unwrap();
    assert_eq!(e.rebuilt, None, "only a rect is rebuilt by a scale");
    assert_eq!(
        sync_on_scale(&shape, None, 0.0, 2.0, 2.0, [0.0, 0.0]),
        Ok(None)
    );
    let angled = ParamBox {
        x1: -20.0,
        y1: 10.5,
        x2: 80.25,
        y2: 90.0,
    };
    let a = sync_on_scale(
        &rect(8.0, 0.0, 8.0, 0.0),
        Some(angled),
        30.0,
        0.5,
        0.5,
        [50.0, 30.0],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        [a.box_.x1, a.box_.y1, a.box_.x2, a.box_.y2],
        data::SCALE_ANGLE_BOX
    );
    matches(&a.rebuilt.unwrap(), &data::SCALE_ANGLE_SEGS, TRIG);
}

#[test]
fn apply_self_heals_a_missing_box_and_the_ellipse_ignores_the_box_angle() {
    let bounds = PathBounds {
        x: 3.0,
        y: 5.0,
        width: 50.0,
        height: 40.0,
    };
    let healed = apply_param_shape(&rect(10.0, 0.0, 0.0, 0.0), None, Some(bounds), 0.0).unwrap();
    assert_eq!(
        [
            healed.box_.x1,
            healed.box_.y1,
            healed.box_.x2,
            healed.box_.y2
        ],
        data::HEALED_BOX
    );
    matches(&healed.segments, &data::HEALED_SEGS, 0.0);
    let boxed = ParamBox {
        x1: 0.0,
        y1: 0.0,
        x2: 100.0,
        y2: 60.0,
    };
    let star = ParamShape::Star {
        point_count: Some(5.0),
        inner_ratio: Some(0.5),
        corner_radius: Some(0.0),
    };
    let rotated = apply_param_shape(&star, Some(boxed), None, 30.0).unwrap();
    matches(&rotated.segments, &data::STAR_ROTATED_30, TRIG);
    let ellipse = ParamShape::Ellipse {
        start_angle: Some(0.0),
        sweep: Some(359.9),
        inner_radius: Some(0.0),
    };
    let flat0 = apply_param_shape(&ellipse, Some(boxed), None, 0.0).unwrap();
    let flat30 = apply_param_shape(&ellipse, Some(boxed), None, 30.0).unwrap();
    assert_eq!(
        flat0, flat30,
        "applyParamShapeEllipse never re-applies boxAngle"
    );
    assert_eq!(
        apply_param_shape(&star, None, None, 0.0),
        Err(ParamGeomError::MissingBox)
    );
}

#[test]
fn rotation_matches_paper_and_leaves_zero_handles_alone() {
    let r = build_round_rect(0.0, 0.0, 100.0, 60.0, [0.0; 4]).unwrap();
    let turned = rotate_segments(&r, 90.0, [50.0, 30.0]).unwrap();
    matches(&turned, &data::ROT_RECT_90, TRIG);
    let tiny = vec![
        ParamSegment {
            point: [0.0, 0.0],
            handle_in: [1e-13, -1e-13],
            handle_out: [0.0, 0.0],
        },
        ParamSegment {
            point: [10.0, 10.0],
            handle_in: [5e-13, 0.0],
            handle_out: [2.0, 3.0],
        },
    ];
    let got = rotate_segments(&tiny, 45.0, [5.0, 5.0]).unwrap();
    matches(&got, &data::ROT_TINY_HANDLES_45, TRIG);
    assert_eq!(
        got[0].handle_in,
        [1e-13, -1e-13],
        "handles of at most 1e-12 are not transformed"
    );
    assert_eq!(got[1].handle_in, [5e-13, 0.0]);
    let same_back = rotate_segments(&tiny, 0.0, [5.0, 5.0]).unwrap();
    assert_eq!(
        same_back, tiny,
        "an identity matrix leaves the path as it is"
    );
}

#[test]
fn animated_tracks_replace_stored_parameters_and_percent_fallbacks_round() {
    let frame = frame_70();
    let stored = rect(10.0, 10.0, 10.0, 10.0);
    let a25 = AnimatedParams {
        corner_tl: Some(25.0),
        ..Default::default()
    };
    matches(
        &apply_param_shape_animated(&stored, &frame, &a25).unwrap(),
        &data::ANIM_RECT_TL25,
        0.0,
    );
    let a0 = AnimatedParams {
        corner_tl: Some(0.0),
        ..Default::default()
    };
    matches(
        &apply_param_shape_animated(&stored, &frame, &a0).unwrap(),
        &data::ANIM_RECT_TL0,
        0.0,
    );
    let ellipse = ParamShape::Ellipse {
        start_angle: None,
        sweep: None,
        inner_radius: Some(0.335),
    };
    let start = AnimatedParams {
        arc_start: Some(15.0),
        ..Default::default()
    };
    let got = apply_param_shape_animated(&ellipse, &frame, &start).unwrap();
    assert_eq!(got.len(), data::ANIM_ELLIPSE_LEN[0]);
    let edges: Vec<ParamSegment> = got[..3]
        .iter()
        .chain(&got[got.len() - 3..])
        .copied()
        .collect();
    matches(&edges, &data::ANIM_ELLIPSE_STORED_0335_EDGES, TRIG);
    let want = build_arc_ellipse(60.0, 35.0, 60.0, 35.0, 15.0, 359.9, Some(0.34)).unwrap();
    assert_eq!(
        got, want,
        "a stored 0.335 falls back through Math.round(33.5) = 34, i.e. 0.34"
    );
    let pct = AnimatedParams {
        arc_inner: Some(50.0),
        ..Default::default()
    };
    let g50 = apply_param_shape_animated(&ellipse, &frame, &pct).unwrap();
    assert_eq!(g50.len(), data::ANIM_ELLIPSE_LEN[1]);
    let edges50: Vec<ParamSegment> = g50[..3]
        .iter()
        .chain(&g50[g50.len() - 3..])
        .copied()
        .collect();
    matches(&edges50, &data::ANIM_ELLIPSE_INNER_PCT_50_EDGES, TRIG);
}

#[test]
fn motion_flags_follow_the_holder_kind_only() {
    let corner = AnimatedParams {
        corner_bl: Some(1.0),
        ..Default::default()
    };
    assert!(has_param_shape_motion(Some(ParamShapeKind::Rect), &corner));
    assert!(!has_param_shape_motion(
        Some(ParamShapeKind::Ellipse),
        &corner
    ));
    assert!(!has_param_shape_motion(Some(ParamShapeKind::Star), &corner));
    assert!(!has_param_shape_motion(None, &corner));
    let star = AnimatedParams {
        star_corner: Some(2.0),
        ..Default::default()
    };
    assert!(has_param_shape_motion(Some(ParamShapeKind::Star), &star));
    assert!(!has_param_shape_motion(Some(ParamShapeKind::Rect), &star));
}

/// Kernel-only behaviour (the baseline accepts these inputs): typed refusal, nothing built.
#[test]
fn non_finite_or_oversized_input_is_refused_with_a_typed_error() {
    let nan = f64::NAN;
    assert_eq!(
        build_round_rect(nan, 0.0, 1.0, 1.0, [0.0; 4]),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_round_rect(0.0, 0.0, 1.0, 1.0, [f64::INFINITY, 0.0, 0.0, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_arc_ellipse(0.0, 0.0, 1.0, 1.0, nan, 90.0, None),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_arc_ellipse(0.0, 0.0, 1.0, 1.0, 0.0, 90.0, Some(nan)),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_star_polygon(0.0, 0.0, f64::NEG_INFINITY, None, None, None),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_star_polygon(0.0, 0.0, 1.0, Some(nan), None, None),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        build_star_polygon(0.0, 0.0, 1.0, Some(MAX_STAR_POINTS + 1.0), None, None),
        Err(ParamGeomError::StarPointBudget)
    );
    assert_eq!(
        build_star_polygon(0.0, 0.0, 1.0, Some(1e300), None, None),
        Err(ParamGeomError::StarPointBudget)
    );
    assert_eq!(
        shape_geom(
            ShapeTool::Rect,
            0.0,
            nan,
            1.0,
            1.0,
            StarState::default(),
            GeomSource::Wasm
        ),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        rotate_segments(&[], nan, [0.0, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        sync_on_scale(&rect(0.0, 0.0, 0.0, 0.0), None, 0.0, nan, 1.0, [0.0, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        apply_param_shape_animated(&rect(0.0, 0.0, 0.0, 0.0), &[], &AnimatedParams::default()),
        Err(ParamGeomError::EmptyGeometry)
    );
}
