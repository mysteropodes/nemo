//! Paper.js 0.12.18 `Curve.getTimeOf` and the root finders behind it (I12): `Numerical.solveQuadratic`,
//! `Numerical.solveCubic` (Kahan style, with the Dekker-split discriminant and the power-of-two
//! normalisation), `Curve.solveCubic` and the 1e-12 / 1e-7 closeness tests of `getTimeOf`.
//!
//! `getNearestTime` reaches this file only for a STRAIGHT curve: it projects the query onto the chord and
//! asks `getTimeOf` for the curve time of the projected point. A straight curve with zero handles is the
//! common bone, and its time is the inverse of the smoothstep `3t^2 - 2t^3`, so a cubic really is solved.
//!
//! Kept on purpose: `getTimeOf` shares ONE `roots` array between the x and the y solve, and
//! `Numerical.solveCubic` reads `roots[0]` and `roots[1]` to avoid a duplicate root even when fewer roots
//! were written, so a stale entry of the x solve can hide the y root; the array therefore lives in
//! [`time_of`] and is passed by reference (`NaN` stands for JavaScript's `undefined`). A `-1` count (all
//! coefficients zero) is returned as is and never iterated. A root outside `[min - 1e-12, max + 1e-12]` or
//! not finite is dropped; the others are clamped.
//!
//! Float semantics: only `+ - * /`, `sqrt` and `Math.pow(|t|, 1/3)` (`powf`, platform libm) occur on the
//! main path, plus `log2` and `Math.round` for inputs below 1e-8 or above 1e8. The cube root only picks
//! the Newton start, so the root can differ from V8's by a few ulps; the oracle compares straight-curve
//! rows with a tolerance (see `tests/rig_pose_nearest_rows.rs`).

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::js_round;
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::{point_at, Values};

/// `Numerical.EPSILON`.
const EPSILON: f64 = 1e-12;
/// `Numerical.MACHINE_EPSILON`.
const MACHINE_EPSILON: f64 = 112e-18;

/// `Numerical.clamp(x, min, max)`.
fn clamp(x: f64, min: f64, max: f64) -> f64 {
    if x < min {
        min
    } else if max < x {
        max
    } else {
        x
    }
}

/// `Math.max(a, b)` for NaN-aware maxima of non-negative values.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `getNormalizationFactor(...)`: `2^-round(log2(max))` for a maximum below 1e-8 or above 1e8, else 0.
fn normalization_factor(values: &[f64]) -> f64 {
    let t = values.iter().fold(f64::NEG_INFINITY, |m, &x| js_max(m, x));
    if t != 0.0 && !t.is_nan() && (t < 1e-8 || 1e8 < t) {
        2f64.powf(-js_round(t.log2()))
    } else {
        0.0
    }
}

/// Dekker split of a double into a high and a low half.
fn split(p: f64) -> [f64; 2] {
    let x = p * 134217729.0;
    let y = p - x;
    let hi = y + x;
    [hi, p - hi]
}

/// `getDiscriminant(a, b, c)`: `b*b - a*c`, recomputed with the split halves when it cancels.
fn discriminant(a: f64, b: f64, c: f64) -> f64 {
    let mut d = b * b - a * c;
    let e = b * b + a * c;
    if d.abs() * 3.0 < e {
        let (ad, bd, cd) = (split(a), split(b), split(c));
        let p = b * b;
        let dp = (bd[0] * bd[0] - p + 2.0 * bd[0] * bd[1]) + bd[1] * bd[1];
        let q = a * c;
        let dq = (ad[0] * cd[0] - q + ad[0] * cd[1] + ad[1] * cd[0]) + ad[1] * cd[1];
        d = (p - q) + (dp - dq);
    }
    d
}

/// `Numerical.solveQuadratic(a, b, c, roots, min, max)` with finite bounds: the count (`-1` when every
/// coefficient is zero); the roots are written to the front of `roots`.
pub(crate) fn solve_quadratic(
    a: f64,
    b: f64,
    c: f64,
    roots: &mut [f64; 3],
    min: f64,
    max: f64,
) -> i32 {
    let (mut a, mut b, mut c) = (a, b, c);
    let mut x1 = f64::NAN;
    let mut x2 = f64::INFINITY;
    if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            return if c.abs() < EPSILON { -1 } else { 0 };
        }
        x1 = -c / b;
    } else {
        b *= -0.5;
        let mut d = discriminant(a, b, c);
        if d != 0.0 && d.abs() < MACHINE_EPSILON {
            let f = normalization_factor(&[a.abs(), b.abs(), c.abs()]);
            if f != 0.0 {
                a *= f;
                b *= f;
                c *= f;
                d = discriminant(a, b, c);
            }
        }
        if d >= -MACHINE_EPSILON {
            let q = if d < 0.0 { 0.0 } else { d.sqrt() };
            let r = b + if b < 0.0 { -q } else { q };
            if r == 0.0 {
                x1 = c / a;
                x2 = -x1;
            } else {
                x1 = r / a;
                x2 = c / r;
            }
        }
    }
    let mut count = 0;
    let (low, high) = (min - EPSILON, max + EPSILON);
    if x1.is_finite() && low < x1 && x1 < high {
        roots[count] = clamp(x1, min, max);
        count += 1;
    }
    if x2 != x1 && x2.is_finite() && low < x2 && x2 < high {
        roots[count] = clamp(x2, min, max);
        count += 1;
    }
    count as i32
}

/// The Horner evaluation of `evaluate(x)` inside `Numerical.solveCubic`: `(b1, c2, qd, q)`.
fn horner(a: f64, b: f64, c: f64, d: f64, x: f64) -> (f64, f64, f64, f64) {
    let tmp = a * x;
    let b1 = tmp + b;
    let c2 = b1 * x + c;
    let qd = (tmp + b1) * x + c2;
    let q = c2 * x + d;
    (b1, c2, qd, q)
}

/// `Numerical.solveCubic(a, b, c, d, roots, min, max)` with finite bounds.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub(crate) fn solve_cubic(
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    roots: &mut [f64; 3],
    min: f64,
    max: f64,
) -> i32 {
    let (mut a, mut b, mut c, mut d) = (a, b, c, d);
    let f = normalization_factor(&[a.abs(), b.abs(), c.abs(), d.abs()]);
    if f != 0.0 {
        a *= f;
        b *= f;
        c *= f;
        d *= f;
    }
    let x: f64;
    let b1: f64;
    let c2: f64;
    if a.abs() < EPSILON {
        a = b;
        b1 = c;
        c2 = d;
        x = f64::INFINITY;
    } else if d.abs() < EPSILON {
        b1 = b;
        c2 = c;
        x = 0.0;
    } else {
        let mut cur = -b / a / 3.0;
        let (mut hb, mut hc, mut qd, mut q) = horner(a, b, c, d, cur);
        let t = q / a;
        let r = t.abs().powf(1.0 / 3.0);
        let s = if t < 0.0 { -1.0 } else { 1.0 };
        let td = -qd / a;
        let rd = if 0.0 < td {
            1.324717957244746 * js_max(r, td.sqrt())
        } else {
            r
        };
        let mut p = cur - s * rd;
        if p != cur {
            loop {
                cur = p;
                (hb, hc, qd, q) = horner(a, b, c, d, cur);
                p = if qd == 0.0 {
                    cur
                } else {
                    cur - q / qd / (1.0 + MACHINE_EPSILON)
                };
                if !(s * cur < s * p) {
                    break;
                }
            }
            if a.abs() * cur * cur > (d / cur).abs() {
                hc = -d / cur;
                hb = (hc - c) / cur;
            }
        }
        b1 = hb;
        c2 = hc;
        x = cur;
    }
    let mut count = solve_quadratic(a, b1, c2, roots, min, max);
    let fresh = count == 0 || (0 < count && x != roots[0] && x != roots[1]);
    if x.is_finite() && fresh && min - EPSILON < x && x < max + EPSILON {
        roots[count as usize] = clamp(x, min, max);
        count += 1;
    }
    count
}

/// `Curve.solveCubic(values, coord, val, roots, min, max)`: the times at which one coordinate of the
/// curve equals `val`; nothing when all four control values lie on one side of it.
pub(crate) fn curve_solve_cubic(
    v: &Values,
    coord: usize,
    val: f64,
    roots: &mut [f64; 3],
    min: f64,
    max: f64,
) -> i32 {
    let (v0, v1, v2, v3) = (v[coord], v[coord + 2], v[coord + 4], v[coord + 6]);
    if (v0 < val && v3 < val && v1 < val && v2 < val)
        || (val < v0 && val < v3 && val < v1 && val < v2)
    {
        return 0;
    }
    let c = 3.0 * (v1 - v0);
    let b = 3.0 * (v2 - v1) - c;
    let a = v3 - v0 - c - b;
    solve_cubic(a, b, c, v0 - val, roots, min, max)
}

/// `point.isClose(other, tolerance)`: `sqrt(dx*dx + dy*dy) <= tolerance`.
fn is_close(p: [f64; 2], q: [f64; 2], tolerance: f64) -> bool {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    (dx * dx + dy * dy).sqrt() <= tolerance
}

/// `Curve.getTimeOf(values, point)`: the time of a point lying on the curve, or `None` (Paper's null).
pub(crate) fn time_of(v: &Values, p: [f64; 2]) -> Option<f64> {
    let (start, end) = ([v[0], v[1]], [v[6], v[7]]);
    let exact = is_close(p, start, 1e-12) || is_close(p, end, 1e-12);
    if !exact {
        let mut roots = [f64::NAN; 3];
        for coord in 0..2 {
            let count = curve_solve_cubic(v, coord, p[coord], &mut roots, 0.0, 1.0);
            for h in 0..count.max(0) as usize {
                let u = roots[h];
                if point_at(v, u).is_some_and(|q| is_close(p, q, 1e-7)) {
                    return Some(u);
                }
            }
        }
    }
    if is_close(p, start, 1e-7) {
        Some(0.0)
    } else if is_close(p, end, 1e-7) {
        Some(1.0)
    } else {
        None
    }
}
