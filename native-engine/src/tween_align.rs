//! #2358: tween align; adapted from geometry-wasm at 46442ea9.
use super::*;

// ---- alignResampledPair (reverse / cyclic-rotate search) ----
// Loosened 2026-07-17 — verbatim port of tweens.js's resampledIsClosed
// near-closed test (see the JS comment for the full rationale): a
// hand-drawn head outline is a nearly-closed loop left open by a small pen
// gap and typically drawn from a different start point in each keyframe —
// the old 5%-of-diagonal gap test kept the cyclic-rotation search off for
// it, and the resulting index offset read as a spurious ~80° whole-head
// rotation, knotting the outline mid-tween. Near-closed = endpoint gap
// under 15% of the diagonal AND polyline length over 1.5x the diagonal
// (real loops only — open arcs and straight strokes stay reversal-only).
pub(super) fn resampled_is_closed(r: &ResampledOut) -> bool {
    let s = &r.segments;
    if s.len() < 4 {
        return false;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for sg in s {
        minx = minx.min(sg.point[0]);
        miny = miny.min(sg.point[1]);
        maxx = maxx.max(sg.point[0]);
        maxy = maxy.max(sg.point[1]);
    }
    let diag2 = (maxx - minx).powi(2) + (maxy - miny).powi(2);
    let (dx, dy) = (
        s[0].point[0] - s[s.len() - 1].point[0],
        s[0].point[1] - s[s.len() - 1].point[1],
    );
    let gap2 = dx * dx + dy * dy;
    if gap2 >= diag2.max(1.0) * 0.0225 {
        return false; // 0.15^2 of the diagonal
    }
    let mut poly_len = 0.0;
    for i in 1..s.len() {
        let (ddx, ddy) = (
            s[i].point[0] - s[i - 1].point[0],
            s[i].point[1] - s[i - 1].point[1],
        );
        poly_len += (ddx * ddx + ddy * ddy).sqrt();
    }
    poly_len * poly_len > diag2 * 2.25 // length > 1.5x diagonal — real loops only
}
pub(super) fn align_cost(a: &ResampledOut, b: &ResampledOut) -> f64 {
    let n = a.segments.len().min(b.segments.len());
    let (mut cax, mut cay, mut cbx, mut cby) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        cax += a.segments[i].point[0];
        cay += a.segments[i].point[1];
        cbx += b.segments[i].point[0];
        cby += b.segments[i].point[1];
    }
    cax /= n as f64;
    cay /= n as f64;
    cbx /= n as f64;
    cby /= n as f64;
    let mut sum = 0.0;
    for i in 0..n {
        let dx = (a.segments[i].point[0] - cax) - (b.segments[i].point[0] - cbx);
        let dy = (a.segments[i].point[1] - cay) - (b.segments[i].point[1] - cby);
        sum += dx * dx + dy * dy;
    }
    sum
}
// Verbatim port of alignTurnDisagreement (tweens.js) — see that function's
// comment for the measurements and reasoning. Both cost functions above judge
// a candidate ordering only on where its points land, never on whether index i
// sits on the same FEATURE in both strokes. On a stroke whose ends come back
// near each other (an arm outline drawn as one loop), every cyclic rotation
// becomes a candidate and rotation_fit_residual will buy a small positional
// gain with a correspondence that pairs elbow against wrist — which the
// interpolation then detects itself (_twTurnTrust) and reacts to by disabling
// the intrinsic reconstruction entirely. Scaling the residual by this keeps
// the factor dimensionless, so it behaves the same on a 20px eye and a 1600px
// arm, and lets position decide freely among orderings that read as the same
// shape.
pub(super) fn wrap_pi(mut d: f64) -> f64 {
    // Mirror of _wrapPI (tweens.js): fold an angle difference into (-pi, pi].
    while d > std::f64::consts::PI {
        d -= 2.0 * std::f64::consts::PI;
    }
    while d < -std::f64::consts::PI {
        d += 2.0 * std::f64::consts::PI;
    }
    d
}
pub(super) fn align_turn_disagreement(a: &ResampledOut, b: &ResampledOut) -> f64 {
    let m = a.segments.len().min(b.segments.len());
    let (mut sum, mut cnt) = (0.0, 0usize);
    let (mut pv_a, mut pv_b) = (f64::NAN, f64::NAN);
    for i in 1..m {
        let t_a = (a.segments[i].point[1] - a.segments[i - 1].point[1])
            .atan2(a.segments[i].point[0] - a.segments[i - 1].point[0]);
        let t_b = (b.segments[i].point[1] - b.segments[i - 1].point[1])
            .atan2(b.segments[i].point[0] - b.segments[i - 1].point[0]);
        if !pv_a.is_nan() {
            sum += wrap_pi(wrap_pi(t_a - pv_a) - wrap_pi(t_b - pv_b)).abs();
            cnt += 1;
        }
        pv_a = t_a;
        pv_b = t_b;
    }
    if cnt > 0 {
        sum / cnt as f64
    } else {
        0.0
    } // mean per-vertex turning disagreement, radians
}
const ALIGN_TURN_W: f64 = 1.5;
pub(super) fn reverse_resampled(r: &ResampledOut) -> ResampledOut {
    let mut segments: Vec<SegOut> = r
        .segments
        .iter()
        .rev()
        .map(|s| SegOut {
            point: s.point,
            handle_in: s.handle_out,
            handle_out: s.handle_in,
        })
        .collect();
    segments.shrink_to_fit();
    ResampledOut {
        segments,
        widths: r.widths.as_ref().map(|w| w.iter().rev().copied().collect()),
        is_vector_brush: r.is_vector_brush,
        stroke_color: r.stroke_color.clone(),
        fill_color: r.fill_color.clone(),
    }
}
pub(super) fn rotate_resampled(r: &ResampledOut, k: usize) -> ResampledOut {
    let n = r.segments.len();
    let mut segments = Vec::with_capacity(n);
    segments.extend_from_slice(&r.segments[k..]);
    segments.extend_from_slice(&r.segments[..k]);
    let widths = r.widths.as_ref().map(|w| {
        let mut out = Vec::with_capacity(w.len());
        out.extend_from_slice(&w[k..]);
        out.extend_from_slice(&w[..k]);
        out
    });
    ResampledOut {
        segments,
        widths,
        is_vector_brush: r.is_vector_brush,
        stroke_color: r.stroke_color.clone(),
        fill_color: r.fill_color.clone(),
    }
}

// Residual after fitting the best RIGID rotation+scale for a candidate B
// ordering, instead of align_cost's raw centroid-relative point distance
// — verbatim port of tweens.js's rotationFitResidual (2026-07-17 fix).
// Found by stress-testing: a 300x40 rectangle rotated a clean 90° picked
// the mathematically-optimal-by-align_cost correspondence (confirmed by
// exhaustive brute-force search — not a search bug), yet the fitted
// rotation on that exact correspondence came back mag≈0.34/theta≈-10°
// instead of the expected mag≈1/theta≈90° — visibly non-rigid, "melting"
// instead of turning. Raw point distance has no notion of "does this
// correspondence read as one coherent rigid motion" — only "are the
// points numerically close after centering" — and a shape with some
// rotational/reflective symmetry (any non-square rectangle included) can
// have multiple orderings that tie or nearly tie on that measure while
// only one of them is the clean turn. Scoring candidates by how well they
// fit a single rigid rotation+scale (what interpStroke/interp_stroke will
// actually apply) picks the one that turns instead of the one that merely
// minimizes raw distance.
pub(super) fn rotation_fit_residual(a: &ResampledOut, b: &ResampledOut) -> f64 {
    let n = a.segments.len().min(b.segments.len());
    let pts_a: Vec<(f64, f64)> = a.segments[..n]
        .iter()
        .map(|s| (s.point[0], s.point[1]))
        .collect();
    let pts_b: Vec<(f64, f64)> = b.segments[..n]
        .iter()
        .map(|s| (s.point[0], s.point[1]))
        .collect();
    match fit_similarity_transform(&pts_a, &pts_b) {
        None => align_cost(a, b), // degenerate (coincident points) — fall back to plain distance
        Some(t) => {
            let mut sum = 0.0;
            for i in 0..n {
                let (qx, qy) = apply_similarity_transform(&t, pts_a[i].0, pts_a[i].1);
                let dx = qx - pts_b[i].0;
                let dy = qy - pts_b[i].1;
                sum += dx * dx + dy * dy;
            }
            sum
        }
    }
}

// The rotation-fit criterion is CLOSED-shapes-only — verbatim port of
// alignResampledPairJS's 2026-07-17 fix (see the JS comment): for a
// near-straight OPEN stroke, reversing the point order is geometrically
// indistinguishable from a ~180° rotation, so the rotation-fit residual
// "explained" reversals with a perfect half-spin and eyebrows twirled
// -145°..-165° for no reason on a real hand-drawn face. Open strokes use
// the plain raw-distance test; closed loops keep the rotation-fit
// criterion (the 90°-rectangle / rotated-start-star fix).
pub(super) fn align_pair(a: &ResampledOut, b: &ResampledOut) -> ResampledOut {
    let closed =
        resampled_is_closed(a) && resampled_is_closed(b) && rotation_ok(a) && rotation_ok(b);
    let base_fn = if closed {
        rotation_fit_residual
    } else {
        align_cost
    };
    let cost_fn = |x: &ResampledOut, y: &ResampledOut| {
        base_fn(x, y) * (1.0 + ALIGN_TURN_W * align_turn_disagreement(x, y))
    };
    let mut best = b.clone();
    let mut best_cost = cost_fn(&a, &b);
    let rev = reverse_resampled(&b);
    let rc = cost_fn(&a, &rev);
    if rc < best_cost {
        best_cost = rc;
        best = rev.clone();
    }
    if closed {
        for base in [&b, &rev] {
            let n = base.segments.len();
            for k in 1..n {
                let cand = rotate_resampled(base, k);
                let c = cost_fn(&a, &cand);
                if c < best_cost {
                    best_cost = c;
                    best = cand;
                }
            }
        }
    }
    best
}

// A nearly closed hairpin has real endpoints; it must not receive cyclic alignment.
fn rotation_ok(r: &ResampledOut) -> bool {
    let s = &r.segments;
    let n = s.len();
    if n < 6 {
        return false;
    }
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in s {
        for axis in 0..2 {
            min[axis] = min[axis].min(p.point[axis]);
            max[axis] = max[axis].max(p.point[axis]);
        }
    }
    let diagonal = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2)).sqrt();
    let diagonal = if diagonal == 0.0 { 1.0 } else { diagonal };
    let distance =
        |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    if distance(s[0].point, s[n - 1].point) / diagonal < 0.08 {
        return true;
    }
    let count = n.div_ceil(2);
    let sum: f64 = (0..count)
        .map(|i| distance(s[i].point, s[n - 1 - i].point))
        .sum();
    sum / count as f64 / diagonal >= 0.25
}
