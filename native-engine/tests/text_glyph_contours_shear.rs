//! Faux-italic shear, typed refusals and scene-join flags of the glyph contour
//! kernel (G38b / L06-B). Literal expected values come from rows of the real
//! baseline run in `fixtures/text-glyph-contours/oracle.txt`; the two shear
//! mutation controls rebuild plausible simplifications and prove the oracle
//! (real Paper.js `Segment#_transformCoordinates`) rejects them.

use super::oracle::{assert_segments, case, parse_oracle};
use super::{
    glyph_paths, outline_to_contours, path_flags, shear_segments, GlyphContourError, GlyphSegment,
    ItalicShear, OutlineCommand, PathFlag,
};

fn seg(point: [f64; 2]) -> GlyphSegment {
    GlyphSegment {
        point,
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

/// Mutation: transform handles as plain vectors (linear part only).
fn linear_shear(segments: &[GlyphSegment], shear: &ItalicShear) -> Vec<GlyphSegment> {
    let m = shear.matrix();
    let at = |x: f64, y: f64| [x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5]];
    let vector = |h: [f64; 2]| [h[0] * m[0] + h[1] * m[2], h[0] * m[1] + h[1] * m[3]];
    segments
        .iter()
        .map(|s| GlyphSegment {
            point: at(s.point[0], s.point[1]),
            handle_in: vector(s.handle_in),
            handle_out: vector(s.handle_out),
        })
        .collect()
}

/// Mutation: transform every handle, including Paper's "zero" ones.
fn shear_every_handle(segments: &[GlyphSegment], shear: &ItalicShear) -> Vec<GlyphSegment> {
    let m = shear.matrix();
    let at = |x: f64, y: f64| [x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5]];
    segments
        .iter()
        .map(|s| {
            let p = at(s.point[0], s.point[1]);
            let handle = |h: [f64; 2]| {
                let q = at(h[0] + s.point[0], h[1] + s.point[1]);
                [q[0] - p[0], q[1] - p[1]]
            };
            GlyphSegment {
                point: p,
                handle_in: handle(s.handle_in),
                handle_out: handle(s.handle_out),
            }
        })
        .collect()
}

#[test]
fn shear_pins_paper_handle_semantics_and_two_simplifications_fail() {
    let oracle = parse_oracle();
    let c = case(&oracle, "italic_curve_handles_not_linear");
    let (skew, base) = c.italic.expect("italic case");
    let shear = ItalicShear::new(skew, base).expect("finite");
    assert_eq!(shear.matrix(), [1.0, 0.0, -0.2, 1.0, 0.2 * 100.0, 0.0]);
    let got = shear_segments(&c.merged[0], &shear);
    assert_eq!(got[0].point, [20.6, 2.0]);
    assert_eq!(got[0].handle_out, [0.4499999999999993, 0.25]);
    assert_eq!(got[1].point, [26.0, 20.0]);
    assert_eq!(got[1].handle_in, [1.1999999999999993, -11.0]);
    assert_segments("paper", &got, &c.sheared[0]);
    let linear = linear_shear(&c.merged[0], &shear);
    assert_ne!(
        linear[0].handle_out, got[0].handle_out,
        "0.45 vs 0.4499999999999993"
    );
    assert_ne!(linear, c.sheared[0]);
    for name in [
        "italic_tiny_diagonal_handle_skipped",
        "italic_handle_exactly_epsilon_skipped",
    ] {
        let t = case(&oracle, name);
        let (skew, base) = t.italic.expect("italic case");
        let shear = ItalicShear::new(skew, base).expect("finite");
        assert_segments(name, &shear_segments(&t.merged[0], &shear), &t.sheared[0]);
        assert_eq!(
            t.sheared[0][0].handle_out, t.merged[0][0].handle_out,
            "{name}: kept verbatim"
        );
        assert_ne!(
            shear_every_handle(&t.merged[0], &shear)[0].handle_out,
            t.sheared[0][0].handle_out,
            "{name}: transforming a zero handle would perturb it"
        );
    }
}

#[test]
fn shear_zero_test_is_inclusive_at_1e_12_on_both_axes() {
    let shear = ItalicShear::new(0.25, 40.0).expect("finite");
    let at = |h: [f64; 2]| {
        let s = GlyphSegment {
            point: [10.0, 10.0],
            handle_in: h,
            handle_out: [0.0, 0.0],
        };
        shear_segments(&[s], &shear)[0].handle_in
    };
    assert_eq!(at([1e-12, -1e-12]), [1e-12, -1e-12]);
    assert_ne!(
        at([1e-12, 2e-12]),
        [1e-12, 2e-12],
        "one axis over the epsilon transforms the handle"
    );
    assert_ne!(at([2e-12, 0.0]), [2e-12, 0.0]);
}

#[test]
fn zero_skew_is_not_applied_and_a_hollow_glyph_stays_hollow() {
    let oracle = parse_oracle();
    let c = case(&oracle, "hole_o_curves");
    let flat = glyph_paths(&c.commands, None).expect("plain");
    let zero = ItalicShear::new(0.0, 50.0).expect("finite");
    assert_eq!(
        glyph_paths(&c.commands, Some(&zero)).expect("zero skew"),
        flat
    );
    let vanished = case(&oracle, "identical_bbox_pair_both_holes");
    assert!(glyph_paths(&vanished.commands, None)
        .expect("plain")
        .is_empty());
    assert!(glyph_paths(&[], None).expect("empty outline").is_empty());
}

#[test]
fn malformed_input_is_refused_with_a_typed_error_and_no_output() {
    let nan = f64::NAN;
    let bad = [
        OutlineCommand::Move { x: 0.0, y: 0.0 },
        OutlineCommand::Line { x: 1.0, y: 1.0 },
        OutlineCommand::Quad {
            x1: 1.0,
            y1: nan,
            x: 2.0,
            y: 2.0,
        },
    ];
    assert_eq!(
        outline_to_contours(&bad),
        Err(GlyphContourError::NonFiniteCoordinate { command: 2 })
    );
    assert_eq!(
        glyph_paths(&bad, None),
        Err(GlyphContourError::NonFiniteCoordinate { command: 2 })
    );
    let inf = [OutlineCommand::Move {
        x: f64::INFINITY,
        y: 0.0,
    }];
    assert_eq!(
        outline_to_contours(&inf),
        Err(GlyphContourError::NonFiniteCoordinate { command: 0 })
    );
    let cubic = [
        OutlineCommand::Move { x: 0.0, y: 0.0 },
        OutlineCommand::Cubic {
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: f64::NEG_INFINITY,
            x: 1.0,
            y: 1.0,
        },
    ];
    assert!(outline_to_contours(&cubic).is_err());
    let late = [
        OutlineCommand::Close,
        OutlineCommand::Cubic {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            x: 2.0,
            y: 2.0,
        },
    ];
    assert_eq!(
        outline_to_contours(&late),
        Err(GlyphContourError::MissingMove { command: 1 })
    );
    assert_eq!(
        ItalicShear::new(nan, 1.0),
        Err(GlyphContourError::NonFiniteShear)
    );
    assert_eq!(
        ItalicShear::new(0.2, f64::INFINITY),
        Err(GlyphContourError::NonFiniteShear)
    );
}

#[test]
fn a_shear_that_overflows_is_refused_not_published() {
    let oracle = parse_oracle();
    let square = &case(&oracle, "square_dup_start_dropped").commands;
    let huge = ItalicShear::new(1e308, 1e308).expect("each value is finite");
    assert_eq!(
        glyph_paths(square, Some(&huge)),
        Err(GlyphContourError::NonFiniteGeometry)
    );
}

fn ring(count: usize) -> Vec<GlyphSegment> {
    (0..count)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / count as f64;
            seg([100.0 * a.cos(), 100.0 * a.sin()])
        })
        .collect()
}

#[test]
fn flags_report_what_closed_cubic_path_will_refuse() {
    assert!(path_flags(&ring(2)).is_empty());
    assert!(path_flags(&ring(256)).is_empty());
    assert_eq!(path_flags(&ring(257)), [PathFlag::SegmentCount(257)]);
    assert_eq!(path_flags(&ring(300)), [PathFlag::SegmentCount(300)]);
    assert_eq!(
        path_flags(&[seg([5.0, 5.0])]),
        [PathFlag::SegmentCount(1), PathFlag::DegenerateCurve(0)]
    );
    let doubled = [seg([0.0, 0.0]), seg([0.0, 0.0]), seg([10.0, 0.0])];
    assert_eq!(path_flags(&doubled), [PathFlag::DegenerateCurve(0)]);
    let tiny = [seg([0.0, 0.0]), seg([1e-13, 0.0])];
    assert_eq!(
        path_flags(&tiny),
        [PathFlag::DegenerateCurve(0), PathFlag::DegenerateCurve(1)]
    );
    let visible = [seg([0.0, 0.0]), seg([1e-9, 0.0])];
    assert!(
        path_flags(&visible).is_empty(),
        "1e-9 survives the f32 test"
    );
    // At 1e8 an f32 anchor cannot represent +1: the curve is degenerate there.
    let coarse = [seg([1e8, 0.0]), seg([1e8 + 1.0, 0.0]), seg([1e8, 50.0])];
    assert_eq!(path_flags(&coarse), [PathFlag::DegenerateCurve(0)]);
}

#[test]
fn glyph_paths_carry_the_flags_of_the_finished_path() {
    let mut commands = vec![OutlineCommand::Move { x: 100.0, y: 0.0 }];
    for i in 1..300 {
        let a = i as f64 * std::f64::consts::TAU / 300.0;
        commands.push(OutlineCommand::Line {
            x: 100.0 * a.cos(),
            y: 100.0 * a.sin(),
        });
    }
    commands.push(OutlineCommand::Close);
    let paths = glyph_paths(&commands, None).expect("paths");
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].segments.len(), 300);
    assert_eq!(paths[0].flags, [PathFlag::SegmentCount(300)]);
    let small = glyph_paths(&[OutlineCommand::Move { x: 1.0, y: 1.0 }], None).expect("paths");
    assert_eq!(
        small[0].flags,
        [PathFlag::SegmentCount(1), PathFlag::DegenerateCurve(0)]
    );
}
