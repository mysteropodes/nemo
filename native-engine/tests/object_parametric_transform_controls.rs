//! L01-G controls for the move/scale kernel: each wrong implementation below is a plausible shortcut, and each
//! control proves that the oracle rows DISTINGUISH it from Paper.js, so the bit-exact replay is not vacuous.
use super::{bits, pair, rows, scale_segments, segs, translate_segments};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamSegment;

/// A mutation: add the delta to the point only and leave the relative handles alone.
fn plain_translate(input: &[ParamSegment], dx: f64, dy: f64) -> Vec<ParamSegment> {
    input
        .iter()
        .map(|s| ParamSegment {
            point: [s.point[0] + dx, s.point[1] + dy],
            ..*s
        })
        .collect()
}

/// A mutation: scale the point about the anchor and multiply the relative handles by the factors.
fn plain_scale(input: &[ParamSegment], sx: f64, sy: f64, pivot: [f64; 2]) -> Vec<ParamSegment> {
    input
        .iter()
        .map(|s| ParamSegment {
            point: [
                pivot[0] + (s.point[0] - pivot[0]) * sx,
                pivot[1] + (s.point[1] - pivot[1]) * sy,
            ],
            handle_in: [s.handle_in[0] * sx, s.handle_in[1] * sy],
            handle_out: [s.handle_out[0] * sx, s.handle_out[1] * sy],
        })
        .collect()
}

/// A mutation: run the matrix even when it is the identity (Paper returns the path untouched).
fn through_the_formula(input: &[ParamSegment]) -> Vec<ParamSegment> {
    let apply = |p: [f64; 2]| [p[0] * 1.0 + p[1] * 0.0 + 0.0, p[0] * 0.0 + p[1] * 1.0 + 0.0];
    let zero = |v: f64| v.abs() <= 1e-12;
    input
        .iter()
        .map(|s| {
            let point = apply(s.point);
            let mut out = *s;
            out.point = point;
            if !(zero(s.handle_in[0]) && zero(s.handle_in[1])) {
                let h = apply([s.handle_in[0] + s.point[0], s.handle_in[1] + s.point[1]]);
                out.handle_in = [h[0] - point[0], h[1] - point[1]];
            }
            if !(zero(s.handle_out[0]) && zero(s.handle_out[1])) {
                let h = apply([s.handle_out[0] + s.point[0], s.handle_out[1] + s.point[1]]);
                out.handle_out = [h[0] - point[0], h[1] - point[1]];
            }
            out
        })
        .collect()
}

#[test]
fn moving_only_the_points_would_differ_from_paper_on_many_rows() {
    let mut differs = 0;
    for row in rows("tr") {
        let [dx, dy] = pair(row[4]);
        let input = segs(row[5]);
        let want = bits(&segs(row[6]));
        assert_eq!(
            bits(&translate_segments(&input, dx, dy).expect("moved")),
            want
        );
        if bits(&plain_translate(&input, dx, dy)) != want {
            differs += 1;
        }
    }
    assert!(
        differs >= 30,
        "the oracle distinguishes the shortcut on {differs} rows"
    );
}

#[test]
fn multiplying_the_handles_would_differ_from_paper_on_many_rows() {
    let mut differs = 0;
    for row in rows("sc") {
        let (factors, pivot) = (pair(row[5]), pair(row[6]));
        let input = segs(row[7]);
        let want = bits(&segs(row[8]));
        assert_eq!(
            bits(&scale_segments(&input, factors[0], factors[1], pivot).expect("scaled")),
            want
        );
        if bits(&plain_scale(&input, factors[0], factors[1], pivot)) != want {
            differs += 1;
        }
    }
    assert!(
        differs >= 20,
        "the oracle distinguishes the shortcut on {differs} rows"
    );
}

#[test]
fn running_an_identity_matrix_through_the_formula_would_perturb_curved_handles() {
    let mut perturbed = 0;
    for row in rows("tr") {
        let input = segs(row[5]);
        assert_eq!(
            bits(&translate_segments(&input, 0.0, 0.0).expect("identity")),
            bits(&input)
        );
        if bits(&through_the_formula(&input)) != bits(&input) {
            perturbed += 1;
        }
    }
    assert!(
        perturbed >= 20,
        "{perturbed} rows show the early return matters"
    );
}
