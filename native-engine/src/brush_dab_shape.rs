//! The Paper.js 0.12.18 path geometry the baseline dab stamper relies on (`src/paper-full.min.js`), so a dab built here
//! has the same anchors and handles as `buildDabShape` builds over Paper: `Path.Ellipse` and `Path.Rectangle` (radius)
//! segments from the shipped `KAPPA`, `Path.smooth` for `continuous` (the tridiagonal solve, padded for a closed path)
//! and `Segment.smooth` for `catmull-rom`, `Path.getBounds` (`Curve._addBounds` over `Numerical.solveQuadratic`), and
//! `Item.rotate` / `Item.position` through `Matrix._transformCoordinates`: a non-zero handle is transformed as an
//! absolute point and made relative again, exactly as `Segment._transformCoordinates` does; an identity matrix is skipped.

/// One segment: anchor, handle-in and handle-out (relative), as Paper stores them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Seg {
    pub(crate) p: [f64; 2],
    pub(crate) h_in: [f64; 2],
    pub(crate) h_out: [f64; 2],
}

/// One dab path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DabPath {
    pub(crate) segments: Vec<Seg>,
    pub(crate) closed: bool,
}

const KAPPA: f64 = 0.5522847498307936;
const EPSILON: f64 = 1e-12;
const MACHINE_EPSILON: f64 = 1.12e-16;
const CURVETIME_EPSILON: f64 = 1e-8;

fn seg(p: [f64; 2], h_in: [f64; 2], h_out: [f64; 2]) -> Seg {
    Seg { p, h_in, h_out }
}
fn is_zero(v: [f64; 2]) -> bool {
    (-EPSILON..=EPSILON).contains(&v[0]) && (-EPSILON..=EPSILON).contains(&v[1])
}

/// `new Path.Ellipse({center, radius})`.
pub(crate) fn ellipse(center: [f64; 2], radius: [f64; 2]) -> DabPath {
    let unit = [
        ([-1.0, 0.0], [0.0, KAPPA], [0.0, -KAPPA]),
        ([0.0, -1.0], [-KAPPA, 0.0], [KAPPA, 0.0]),
        ([1.0, 0.0], [0.0, -KAPPA], [0.0, KAPPA]),
        ([0.0, 1.0], [KAPPA, 0.0], [-KAPPA, 0.0]),
    ];
    let scale = |v: [f64; 2]| [v[0] * radius[0], v[1] * radius[1]];
    let segments = unit
        .iter()
        .map(|(p, i, o)| {
            let s = scale(*p);
            seg([s[0] + center[0], s[1] + center[1]], scale(*i), scale(*o))
        })
        .collect();
    DabPath {
        segments,
        closed: true,
    }
}

/// `new Path.Rectangle({point: [x, y], size: [w, h], radius})` with a numeric radius.
pub(crate) fn rectangle(x: f64, y: f64, w: f64, h: f64, radius: f64) -> DabPath {
    let (bl, tl, tr, br) = ([x, y + h], [x, y], [x + w, y], [x + w, y + h]);
    if is_zero([radius, radius]) {
        let z = [0.0, 0.0];
        return DabPath {
            segments: vec![seg(bl, z, z), seg(tl, z, z), seg(tr, z, z), seg(br, z, z)],
            closed: true,
        };
    }
    let (rw, rh) = (radius.min(w / 2.0), radius.min(h / 2.0));
    let (hx, hy, z) = (rw * KAPPA, rh * KAPPA, [0.0, 0.0]);
    let segments = vec![
        seg([bl[0] + rw, bl[1] + 0.0], z, [-hx, 0.0]),
        seg([bl[0] - 0.0, bl[1] - rh], [0.0, hy], z),
        seg([tl[0] + 0.0, tl[1] + rh], z, [0.0, -hy]),
        seg([tl[0] + rw, tl[1] + 0.0], [-hx, 0.0], z),
        seg([tr[0] - rw, tr[1] - 0.0], z, [hx, 0.0]),
        seg([tr[0] + 0.0, tr[1] + rh], [0.0, -hy], z),
        seg([br[0] - 0.0, br[1] - rh], z, [0.0, hy]),
        seg([br[0] - rw, br[1] - 0.0], [hx, 0.0], z),
    ];
    DabPath {
        segments,
        closed: true,
    }
}

/// `path.smooth({type: 'continuous'})` over the whole path (no `from`/`to`).
pub(crate) fn smooth_continuous(path: &mut DabPath) {
    let n = &mut path.segments;
    let s = n.len();
    if s == 0 {
        return;
    }
    let looped = path.closed;
    let (h, u) = (0i64, s as i64 - 1);
    let count = (u - h + 1) as usize;
    let mut d = count - 1;
    let (mut pad_right, mut pad_left) = if looped {
        let p = count.min(4);
        (p, p)
    } else {
        (1, 1)
    };
    if !looped {
        pad_left = 1usize.min(h as usize);
        pad_right = 1usize.min(s - u as usize - 1);
    }
    d += pad_left + pad_right;
    if d <= 1 {
        return;
    }
    let wrap = |p: i64| -> usize { (if p < 0 { p + s as i64 } else { p } as usize) % s };
    let knots: Vec<[f64; 2]> = (0..=d)
        .map(|g| n[wrap(h - pad_left as i64 + g as i64)].p)
        .collect();
    let v = d - 1;
    let (mut m, mut y, mut w) = (vec![0.0; d + 1], vec![0.0; d + 1], vec![0.0; d + 1]);
    let (mut big_a, mut big_t, mut big_p) = (
        knots[0][0] + 2.0 * knots[1][0],
        knots[0][1] + 2.0 * knots[1][1],
        2.0,
    );
    m[0] = big_a;
    y[0] = big_t;
    w[0] = big_p;
    for g in 1..d {
        let c = g < v;
        let (sk, k) = if c { (4.0, 2.0) } else { (8.0, 1.0) };
        let i = (if c { 1.0 } else { 2.0 }) / big_p;
        big_p = (if c { 4.0 } else { 7.0 }) - i;
        w[g] = big_p;
        big_a = sk * knots[g][0] + k * knots[g + 1][0] - i * big_a;
        m[g] = big_a;
        big_t = sk * knots[g][1] + k * knots[g + 1][1] - i * big_t;
        y[g] = big_t;
    }
    let (mut x, mut b) = (vec![0.0; d + 1], vec![0.0; d + 1]);
    x[v] = m[v] / w[v];
    b[v] = y[v] / w[v];
    for g in (0..=d.saturating_sub(2)).rev() {
        x[g] = (m[g] - x[g + 1]) / w[g];
        b[g] = (y[g] - b[g + 1]) / w[g];
    }
    x[d] = (3.0 * knots[d][0] - x[v]) / 2.0;
    b[d] = (3.0 * knots[d][1] - b[v]) / 2.0;
    let last = d - pad_right;
    let mut p = h;
    for g in pad_left..=last {
        let segment = &mut n[if p < 0 {
            (p + s as i64) as usize
        } else {
            p as usize
        }];
        let (ex, ez) = (x[g] - segment.p[0], b[g] - segment.p[1]);
        if looped || g < last {
            segment.h_out = [ex, ez];
        }
        if looped || pad_left < g {
            segment.h_in = [-ex, -ez];
        }
        p += 1;
    }
}

/// `path.smooth({type: 'catmull-rom', factor})`: `Segment.smooth` on every segment, from the anchors only.
pub(crate) fn smooth_catmull_rom(path: &mut DabPath, factor: f64) {
    let s = path.segments.len();
    let points: Vec<[f64; 2]> = path.segments.iter().map(|segment| segment.p).collect();
    let closed = path.closed;
    for k in 0..s {
        let prev = if k > 0 {
            Some(points[k - 1])
        } else if closed && s > 0 {
            Some(points[s - 1])
        } else {
            None
        };
        let next = if k + 1 < s {
            Some(points[k + 1])
        } else if closed && s > 0 {
            Some(points[0])
        } else {
            None
        };
        let (a, o, hp) = (
            prev.unwrap_or(points[k]),
            points[k],
            next.unwrap_or(points[k]),
        );
        let dist =
            |p: [f64; 2], q: [f64; 2]| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        let (u, l) = (dist(a, o), dist(o, hp));
        let f = u.powf(factor);
        let ff = f * f;
        let t = l.powf(factor);
        let tt = t * t;
        let open = !closed;
        if !(open && k == 0) && prev.is_some() {
            let c = 2.0 * tt + 3.0 * t * f + ff;
            let d = 3.0 * t * (t + f);
            path.segments[k].h_in = if d != 0.0 {
                [
                    (tt * a[0] + c * o[0] - ff * hp[0]) / d - o[0],
                    (tt * a[1] + c * o[1] - ff * hp[1]) / d - o[1],
                ]
            } else {
                [0.0, 0.0]
            };
        }
        if !(open && k == s - 1) && next.is_some() {
            let c = 2.0 * ff + 3.0 * f * t + tt;
            let d = 3.0 * f * (f + t);
            path.segments[k].h_out = if d != 0.0 {
                [
                    (ff * hp[0] + c * o[0] - tt * a[0]) / d - o[0],
                    (ff * hp[1] + c * o[1] - tt * a[1]) / d - o[1],
                ]
            } else {
                [0.0, 0.0]
            };
        }
    }
}

fn discriminant(a: f64, b: f64, c: f64) -> f64 {
    let split = |v: f64| {
        let e = 134217729.0 * v;
        let hi = v - e + e;
        [hi, v - hi]
    };
    let (mut d, e) = (b * b - a * c, b * b + a * c);
    if 3.0 * d.abs() < e {
        let (ad, bd, cd) = (split(a), split(b), split(c));
        let p = b * b;
        let q = a * c;
        d = p - q
            + (bd[0] * bd[0] - p + 2.0 * bd[0] * bd[1] + bd[1] * bd[1]
                - (ad[0] * cd[0] - q + ad[0] * cd[1] + ad[1] * cd[0] + ad[1] * cd[1]));
    }
    d
}
fn normalization(values: [f64; 3]) -> f64 {
    let t = values[0].max(values[1]).max(values[2]);
    if t != 0.0 && !(1e-8..=1e8).contains(&t) {
        2f64.powf(-t.log2().round())
    } else {
        0.0
    }
}
/// `Numerical.solveQuadratic(a, b, c, roots)` without a range: the finite real roots.
fn solve_quadratic(mut a: f64, mut b: f64, mut c: f64) -> Vec<f64> {
    let (mut x1, mut x2) = (None, f64::INFINITY);
    if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            return Vec::new();
        }
        x1 = Some(-c / b);
    } else {
        b *= -0.5;
        let mut d = discriminant(a, b, c);
        if d != 0.0 && d.abs() < MACHINE_EPSILON {
            let f = normalization([a.abs(), b.abs(), c.abs()]);
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
                x1 = Some(c / a);
                x2 = -c / a;
            } else {
                x1 = Some(r / a);
                x2 = c / r;
            }
        }
    }
    let mut roots = Vec::new();
    if let Some(x) = x1.filter(|x| x.is_finite()) {
        roots.push(x);
    }
    if Some(x2) != x1 && x2.is_finite() {
        roots.push(x2);
    }
    roots
}
fn add_bounds(v: [f64; 4], coord: usize, min: &mut [f64; 2], max: &mut [f64; 2]) {
    let [v0, v1, v2, v3] = v;
    let (lo, hi) = (min[coord], max[coord]);
    let mut add = |value: f64| {
        if value < min[coord] {
            min[coord] = value;
        }
        if value > max[coord] {
            max[coord] = value;
        }
    };
    if v0 < lo || v1 < lo || v2 < lo || v3 < lo || v0 > hi || v1 > hi || v2 > hi || v3 > hi {
        if (v1 < v0) != (v1 < v3) && (v2 < v0) != (v2 < v3) {
            add(v0);
            add(v3);
        } else {
            let (a, b, c) = (
                3.0 * (v1 - v2) - v0 + v3,
                2.0 * (v0 + v2) - 4.0 * v1,
                v1 - v0,
            );
            let roots = solve_quadratic(a, b, c);
            add(v3);
            for t in roots {
                let u = 1.0 - t;
                if (CURVETIME_EPSILON..=1.0 - CURVETIME_EPSILON).contains(&t) {
                    add(u * u * u * v0
                        + 3.0 * u * u * t * v1
                        + 3.0 * u * t * t * v2
                        + t * t * t * v3);
                }
            }
        }
    }
}
/// `path.bounds` as `[x, y, width, height]` (`Path.getBounds`, no stroke).
pub(crate) fn bounds(path: &DabPath) -> [f64; 4] {
    let s = &path.segments;
    let Some(first) = s.first() else {
        return [0.0; 4];
    };
    let coords = |segment: &Seg| {
        let (x, y) = (segment.p[0], segment.p[1]);
        [
            x,
            y,
            segment.h_in[0] + x,
            segment.h_in[1] + y,
            segment.h_out[0] + x,
            segment.h_out[1] + y,
        ]
    };
    let mut prev = coords(first);
    let (mut min, mut max) = ([prev[0], prev[1]], [prev[0], prev[1]]);
    let mut process = |segment: &Seg, prev: &mut [f64; 6]| {
        let cur = coords(segment);
        for i in 0..2 {
            add_bounds(
                [prev[i], prev[i + 4], cur[i + 2], cur[i]],
                i,
                &mut min,
                &mut max,
            );
        }
        *prev = cur;
    };
    for segment in &s[1..] {
        process(segment, &mut prev);
    }
    if path.closed {
        process(first, &mut prev);
    }
    [min[0], min[1], max[0] - min[0], max[1] - min[1]]
}
/// `path.position` (`bounds.getCenter()`).
pub(crate) fn position(path: &DabPath) -> [f64; 2] {
    let [x, y, w, h] = bounds(path);
    [x + w / 2.0, y + h / 2.0]
}

/// `Item.transform` with a fresh matrix `[a, b, c, d, tx, ty]` on an untransformed path.
fn transform(path: &mut DabPath, m: [f64; 6]) {
    let [a, b, c, d, tx, ty] = m;
    if a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0 && tx == 0.0 && ty == 0.0 {
        return;
    }
    let apply = |p: [f64; 2]| [p[0] * a + p[1] * c + tx, p[0] * b + p[1] * d + ty];
    for segment in &mut path.segments {
        let (x, y) = (segment.p[0], segment.p[1]);
        let point = apply([x, y]);
        if !is_zero(segment.h_in) {
            let abs = apply([segment.h_in[0] + x, segment.h_in[1] + y]);
            segment.h_in = [abs[0] - point[0], abs[1] - point[1]];
        }
        if !is_zero(segment.h_out) {
            let abs = apply([segment.h_out[0] + x, segment.h_out[1] + y]);
            segment.h_out = [abs[0] - point[0], abs[1] - point[1]];
        }
        segment.p = point;
    }
}
/// `path.rotate(degrees)` around `path.position`.
pub(crate) fn rotate(path: &mut DabPath, degrees: f64) {
    let t = degrees * (std::f64::consts::PI / 180.0);
    let [x, y] = position(path);
    let (cos, sin) = (t.cos(), t.sin());
    let tx = x - x * cos + y * sin;
    let ty = y - x * sin - y * cos;
    transform(path, [cos, sin, -sin, cos, tx, ty]);
}
/// `path.position = target`.
pub(crate) fn set_position(path: &mut DabPath, target: [f64; 2]) {
    let [x, y] = position(path);
    transform(path, [1.0, 0.0, 0.0, 1.0, target[0] - x, target[1] - y]);
}
