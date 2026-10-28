//! Paper.js 0.12.18 CurveLocation.isCrossing endpoint-side classification.
//! Copyright (c) 2011–2020 Jürg Lehni & Jonathan Puckey. MIT (tween_paper_license.txt).
use crate::object_document::property_follow_path_sample::{
    evaluate, point_tangent_at, range_length,
};
use kurbo::CubicBez;
fn values(c: CubicBez) -> [f64; 8] {
    [
        c.p0.x, c.p0.y, c.p1.x, c.p1.y, c.p2.x, c.p2.y, c.p3.x, c.p3.y,
    ]
}
fn roots(v: &[f64; 8]) -> Vec<f64> {
    let [x0, y0, x1, y1, x2, y2, x3, y3] = *v;
    let a2 = x1 * (y0 - y3) + y1 * (x3 - x0) + x0 * y3 - y0 * x3;
    let a3 = 3. * (x2 * (y1 - y0) + y2 * (x0 - x1) + x1 * y0 - y1 * x0);
    let mut d2 = a3 - a2;
    let mut d1 = d2 - a2 + x0 * (y3 - y2) + y0 * (x2 - x3) + x3 * y2 - y3 * x2;
    let mut d3 = a3;
    let length = (d1 * d1 + d2 * d2 + d3 * d3).sqrt();
    let scale = if length != 0. { 1. / length } else { 0. };
    d1 *= scale;
    d2 *= scale;
    d3 *= scale;
    let zero = |x: f64| x.abs() <= 1e-12;
    let (looped, candidates) = if zero(d1) {
        (
            false,
            if zero(d2) {
                vec![]
            } else {
                vec![d3 / (3. * d2)]
            },
        )
    } else {
        let disc = 3. * d2 * d2 - 4. * d1 * d3;
        if zero(disc) {
            (false, vec![d2 / (2. * d1)])
        } else {
            let root = if disc > 0. {
                (disc / 3.).sqrt()
            } else {
                (-disc).sqrt()
            };
            (
                disc < 0.,
                vec![(d2 + root) / (2. * d1), (d2 - root) / (2. * d1)],
            )
        }
    };
    let mut result: Vec<_> = candidates
        .into_iter()
        .filter(|&t| t > 0. && t < 1.)
        .collect();
    if looped && result.len() != 2 {
        result.clear();
    }
    result.sort_by(f64::total_cmp);
    if !result.is_empty() {
        return result;
    }
    // Curve.getPeaks: stationary points of squared speed.
    let ax = 3. * x1 - x0 - 3. * x2 + x3;
    let bx = 3. * x0 - 6. * x1 + 3. * x2;
    let cx = -3. * x0 + 3. * x1;
    let ay = 3. * y1 - y0 - 3. * y2 + y3;
    let by = 3. * y0 - 6. * y1 + 3. * y2;
    let cy = -3. * y0 + 3. * y1;
    let mut result: Vec<_> = kurbo::common::solve_cubic(
        cx * bx + by * cy,
        2. * (bx * bx + by * by) + 3. * (cx * ax + cy * ay),
        9. * (ax * bx + by * ay),
        9. * (ax * ax + ay * ay),
    )
    .into_iter()
    .filter(|&t| (1e-8..=1. - 1e-8).contains(&t))
    .collect();
    result.sort_by(f64::total_cmp);
    result
}
fn distance(c: CubicBez, end: bool) -> f64 {
    let v = values(c);
    let r = roots(&v);
    let length = range_length(
        &v,
        if end {
            r.last().copied().unwrap_or(0.)
        } else {
            0.
        },
        if end {
            1.
        } else {
            r.first().copied().unwrap_or(1.)
        },
    );
    if r.is_empty() {
        length / 32.
    } else {
        length
    }
}
fn direction(c: CubicBez, offset: f64, end: bool, origin: [f64; 2]) -> [f64; 2] {
    let v = values(c);
    // Paper getPointAt(-0) still starts at time zero; only a negative offset walks from the end.
    let at = if end && offset != 0. {
        range_length(&v, 0., 1.) - offset
    } else {
        offset
    };
    let p = point_tangent_at(&[v], false, at)
        .map(|p| p.point)
        .unwrap_or(origin);
    [p[0] - origin[0], p[1] - origin[1]]
}
fn tangent(c: CubicBez, t: f64) -> [f64; 2] {
    evaluate(&values(c), t, true).unwrap_or([0.; 2])
}
fn angle(v: [f64; 2]) -> f64 {
    v[1].atan2(v[0])
}
fn between(t: f64, a: f64, b: f64) -> bool {
    if a < b {
        a < t && t < b
    } else {
        a < t || t < b
    }
}
pub(super) fn is_crossing(curves: &[CubicBez], i: usize, t: f64, j: usize, u: f64) -> bool {
    is_crossing_between(curves, i, t, curves, j, u)
}
pub(crate) fn is_crossing_between(
    a_curves: &[CubicBez],
    i: usize,
    t: f64,
    b_curves: &[CubicBez],
    j: usize,
    u: f64,
) -> bool {
    let inside = |t: f64| (1e-8..=1. - 1e-8).contains(&t);
    let (a, b) = (inside(t), inside(u));
    if a && b {
        let x = tangent(a_curves[i], t);
        let y = tangent(b_curves[j], u);
        return (x[0] * y[1] - x[1] * y[0]).abs()
            > 1e-8 * ((x[0] * x[0] + x[1] * x[1]) * (y[0] * y[0] + y[1] * y[1])).sqrt();
    }

    let prev = |i, t, n| if t < 1e-8 { (i + n - 1) % n } else { i };
    let next = |i, t, n| if t > 1. - 1e-8 { (i + 1) % n } else { i };
    let (ap, an, bp, bn) = (
        prev(i, t, a_curves.len()),
        next(i, t, a_curves.len()),
        prev(j, u, b_curves.len()),
        next(j, u, b_curves.len()),
    );
    let mut d = f64::INFINITY;
    if !a {
        d = d
            .min(distance(a_curves[ap], true))
            .min(distance(a_curves[an], false));
    }
    if !b {
        d = d
            .min(distance(b_curves[bp], true))
            .min(distance(b_curves[bn], false));
    }
    let origin = evaluate(&values(a_curves[i]), t, false).unwrap();
    let af = if a {
        tangent(a_curves[i], t)
    } else {
        direction(a_curves[an], d, false, origin)
    };
    let ab = if a {
        [-af[0], -af[1]]
    } else {
        direction(a_curves[ap], d, true, origin)
    };
    let bf = if b {
        tangent(b_curves[j], u)
    } else {
        direction(b_curves[bn], d, false, origin)
    };
    let bb = if b {
        [-bf[0], -bf[1]]
    } else {
        direction(b_curves[bp], d, true, origin)
    };
    let (af, ab, bf, bb) = (angle(af), angle(ab), angle(bf), angle(bb));
    if a {
        (between(ab, bb, bf) ^ between(af, bb, bf)) && (between(ab, bf, bb) ^ between(af, bf, bb))
    } else {
        (between(bb, ab, af) ^ between(bf, ab, af)) && (between(bb, af, ab) ^ between(bf, af, ab))
    }
}
