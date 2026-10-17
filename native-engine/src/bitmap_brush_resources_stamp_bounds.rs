//! Paper.js 0.12.18 path bounds (L07-E / G06): the rectangle `bakeToCanvas`
//! reads as `basePath.bounds`.
//!
//! Mirrors the shipped `src/paper-full.min.js`: `Path.getBounds` with no matrix and
//! no stroke padding (the first segment seeds the box, each following segment and,
//! for a closed path, the first one again adds its curve), `Curve._addBounds`
//! (a curve whose control points all sit inside the box adds nothing; a monotone
//! curve adds its end points; otherwise the roots of the derivative inside
//! `[1e-8, 1 - 1e-8]` are evaluated) and `Numerical.solveQuadratic` with its
//! Dekker-split discriminant and power-of-two renormalisation. Only `+ - * /`,
//! `sqrt`, `log2` and `powf` of a power of two are used, so the result is
//! bit-exact with Paper; `Math.round` is reproduced as `floor(x + 0.5)` without the
//! rounding hazard (`js_round`). The tangent-root count of Paper's `-1` ("every
//! t is a root") is kept: it adds nothing, like a `for` loop that never runs.

use crate::evaluation::property_follow_path_sample::SegmentIn;

/// `new Rectangle(x, y, width, height)` of `path.bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `Math.round`: halves go toward positive infinity.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// Veltkamp split used by `Numerical.getDiscriminant`.
fn split(v: f64) -> [f64; 2] {
    let x = 134217729.0 * v;
    let y = v - x + x;
    [y, v - y]
}

/// `getDiscriminant(a, b, c)` = `b * b - a * c`, refined when the two products nearly cancel.
fn discriminant(a: f64, b: f64, c: f64) -> f64 {
    let mut d = b * b - a * c;
    let e = b * b + a * c;
    if 3.0 * d.abs() < e {
        let (sa, sb, sc) = (split(a), split(b), split(c));
        let bb = b * b;
        let ac = a * c;
        d = bb - ac
            + (sb[0] * sb[0] - bb + 2.0 * sb[0] * sb[1] + sb[1] * sb[1]
                - (sa[0] * sc[0] - ac + sa[0] * sc[1] + sa[1] * sc[0] + sa[1] * sc[1]));
    }
    d
}

/// `getNormalizationFactor(a, b, c)`: a power of two that brings the largest
/// magnitude into `[1e-8, 1e8]`, or 0 when it already is (or is zero / NaN).
fn normalization_factor(a: f64, b: f64, c: f64) -> f64 {
    if a.is_nan() || b.is_nan() || c.is_nan() {
        return 0.0;
    }
    let m = a.max(b).max(c);
    if m != 0.0 && (m < 1e-8 || m > 1e8) {
        2f64.powf(-js_round(m.log2()))
    } else {
        0.0
    }
}

/// `Numerical.solveQuadratic(a, b, c, roots)` without a range: the number of
/// roots (`-1` when every value is a root) and the roots in Paper's order.
pub(crate) fn solve_quadratic(a: f64, b: f64, c: f64) -> (i32, [f64; 2]) {
    let epsilon = 1e-12;
    let machine_epsilon = 112e-18;
    let (mut a, mut c) = (a, c);
    let mut x1 = f64::NAN;
    let mut x2 = f64::INFINITY;
    if a.abs() < epsilon {
        if b.abs() < epsilon {
            return (if c.abs() < epsilon { -1 } else { 0 }, [0.0, 0.0]);
        }
        x1 = -c / b;
    } else {
        let mut b = b * -0.5;
        let mut d = discriminant(a, b, c);
        if d != 0.0 && d.abs() < machine_epsilon {
            let factor = normalization_factor(a.abs(), b.abs(), c.abs());
            if factor != 0.0 {
                a *= factor;
                b *= factor;
                c *= factor;
                d = discriminant(a, b, c);
            }
        }
        if d >= -machine_epsilon {
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
    let mut roots = [0.0, 0.0];
    if x1.is_finite() {
        roots[count] = x1;
        count += 1;
    }
    if x2 != x1 && x2.is_finite() {
        roots[count] = x2;
        count += 1;
    }
    (count as i32, roots)
}

/// `u(value, padding)` of `_addBounds`: widen `[min, max]` by `value -/+ padding`.
fn grow(min: &mut f64, max: &mut f64, value: f64, padding: f64) {
    let low = value - padding;
    let high = value + padding;
    if low < *min {
        *min = low;
    }
    if high > *max {
        *max = high;
    }
}

/// `Curve._addBounds(v0, v1, v2, v3, coord, 0, min, max, roots)` for one axis:
/// the cubic runs `p0 -> p1 -> p2 -> p3`; the stroke padding is 0.
fn add_bounds(p0: f64, p1: f64, p2: f64, p3: f64, min: &mut f64, max: &mut f64) {
    let padding = 0.0;
    let (low, high) = (*min, *max);
    let outside = p0 < low
        || p1 < low
        || p2 < low
        || p3 < low
        || high < p0
        || high < p1
        || high < p2
        || high < p3;
    if !outside {
        return;
    }
    if (p1 < p0) != (p1 < p3) && (p2 < p0) != (p2 < p3) {
        grow(min, max, p0, padding);
        grow(min, max, p3, padding);
        return;
    }
    let (count, roots) = solve_quadratic(
        3.0 * (p1 - p2) - p0 + p3,
        2.0 * (p0 + p2) - 4.0 * p1,
        p1 - p0,
    );
    grow(min, max, p3, padding);
    let roots_used = if count > 0 { count as usize } else { 0 };
    for root in roots.iter().take(roots_used) {
        let t = *root;
        let g = 1.0 - t;
        if (1e-8..=1.0 - 1e-8).contains(&t) {
            let value =
                g * g * g * p0 + 3.0 * g * g * t * p1 + 3.0 * g * t * t * p2 + t * t * t * p3;
            grow(min, max, value, padding);
        }
    }
}

/// `[x, y, handleIn.x + x, handleIn.y + y, handleOut.x + x, handleOut.y + y]`
/// (`Segment._transformCoordinates` without a matrix; a missing handle is `[0, 0]`).
fn coordinates(segment: &SegmentIn) -> [f64; 6] {
    let handle_in = segment.handle_in.unwrap_or([0.0, 0.0]);
    let handle_out = segment.handle_out.unwrap_or([0.0, 0.0]);
    let [x, y] = segment.point;
    [
        x,
        y,
        handle_in[0] + x,
        handle_in[1] + y,
        handle_out[0] + x,
        handle_out[1] + y,
    ]
}

/// `path.bounds` of a Paper `Path` built from `segments` and `closed`; an empty
/// path is `new Rectangle()` (all zero).
pub(crate) fn path_bounds(segments: &[SegmentIn], closed: bool) -> Bounds {
    let first = match segments.first() {
        Some(first) => first,
        None => {
            return Bounds {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }
        }
    };
    let mut previous = coordinates(first);
    let mut min = [previous[0], previous[1]];
    let mut max = min;
    let closing = if closed { Some(first) } else { None };
    for segment in segments.iter().skip(1).chain(closing) {
        let current = coordinates(segment);
        for axis in 0..2 {
            add_bounds(
                previous[axis],
                previous[axis + 4],
                current[axis + 2],
                current[axis],
                &mut min[axis],
                &mut max[axis],
            );
        }
        previous = current;
    }
    Bounds {
        x: min[0],
        y: min[1],
        width: max[0] - min[0],
        height: max[1] - min[1],
    }
}
