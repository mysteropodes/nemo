//! Paper.js 0.12.18 fat-line cubic clipping, retained for baseline pressure crossing parity.
//! Adapted from https://github.com/paperjs/paper.js/blob/v0.12.18/src/path/Curve.js
//! Copyright (c) 2011–2020 Jürg Lehni & Jonathan Puckey. MIT license (see tween_paper_license.txt).
use crate::tween_kernels::outline_cleanup::crossings::*;
type V = [f64; 8];
type Hull = Vec<[f64; 2]>;
fn signed(v: V, x: f64, y: f64) -> f64 {
    let (dx, dy) = (v[6] - v[0], v[7] - v[1]);
    if dx == 0. {
        if dy > 0. {
            x - v[0]
        } else {
            v[0] - x
        }
    } else if dy == 0. {
        if dx < 0. {
            y - v[1]
        } else {
            v[1] - y
        }
    } else {
        ((x - v[0]) * dy - (y - v[1]) * dx)
            / if dx < dy {
                dy * (1. + dx * dx / (dy * dy)).sqrt()
            } else {
                dx * (1. + dy * dy / (dx * dx)).sqrt()
            }
    }
}
pub(super) fn split(v: V, t: f64) -> (V, V) {
    let u = 1. - t;
    let l = [u * v[0] + t * v[2], u * v[1] + t * v[3]];
    let m = [u * v[2] + t * v[4], u * v[3] + t * v[5]];
    let r = [u * v[4] + t * v[6], u * v[5] + t * v[7]];
    let a = [u * l[0] + t * m[0], u * l[1] + t * m[1]];
    let b = [u * m[0] + t * r[0], u * m[1] + t * r[1]];
    let c = [u * a[0] + t * b[0], u * a[1] + t * b[1]];
    (
        [v[0], v[1], l[0], l[1], a[0], a[1], c[0], c[1]],
        [c[0], c[1], b[0], b[1], r[0], r[1], v[6], v[7]],
    )
}
fn part(mut v: V, from: f64, to: f64) -> V {
    if from > 0. {
        v = split(v, from).1;
    }
    if to < 1. {
        v = split(v, (to - from) / (1. - from)).0;
    }
    v
}
fn hull(d: [f64; 4]) -> (Hull, Hull) {
    let p0 = [0., d[0]];
    let p1 = [1. / 3., d[1]];
    let p2 = [2. / 3., d[2]];
    let p3 = [1., d[3]];
    let a = d[1] - (2. * d[0] + d[3]) / 3.;
    let b = d[2] - (d[0] + 2. * d[3]) / 3.;
    let (top, bottom) = if a * b < 0. {
        (vec![p0, p1, p3], vec![p0, p2, p3])
    } else {
        let ratio = a / b;
        (
            if ratio >= 2. {
                vec![p0, p1, p3]
            } else if ratio <= 0.5 {
                vec![p0, p2, p3]
            } else {
                vec![p0, p1, p2, p3]
            },
            vec![p0, p3],
        )
    };
    if (if a == 0. { b } else { a }) < 0. {
        (bottom, top)
    } else {
        (top, bottom)
    }
}
fn clip_part(p: &[[f64; 2]], top: bool, threshold: f64) -> Option<f64> {
    for s in p.windows(2) {
        let (a, b) = (s[0], s[1]);
        if if top {
            b[1] >= threshold
        } else {
            b[1] <= threshold
        } {
            return Some(if b[1] == threshold {
                b[0]
            } else {
                a[0] + (threshold - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
            });
        }
    }
    None
}
fn clip(top: &[[f64; 2]], bottom: &[[f64; 2]], min: f64, max: f64) -> Option<f64> {
    if top[0][1] < min {
        clip_part(top, true, min)
    } else if bottom[0][1] > max {
        clip_part(bottom, false, max)
    } else {
        Some(top[0][0])
    }
}
fn visit(
    mut a: V,
    b: V,
    flip: bool,
    depth: u32,
    calls: &mut u32,
    t0: f64,
    t1: f64,
    u0: f64,
    u1: f64,
    out: &mut Vec<(f64, f64)>,
) {
    *calls += 1;
    let depth = depth + 1;
    if *calls >= 4096 || depth >= 40 {
        return;
    }
    let d1 = signed(b, b[2], b[3]);
    let d2 = signed(b, b[4], b[5]);
    let factor = if d1 * d2 > 0. { 3. / 4. } else { 4. / 9. };
    let min = factor * 0f64.min(d1).min(d2);
    let max = factor * 0f64.max(d1).max(d2);
    let d = [
        signed(b, a[0], a[1]),
        signed(b, a[2], a[3]),
        signed(b, a[4], a[5]),
        signed(b, a[6], a[7]),
    ];
    if d1 == 0. && d2 == 0. && d.iter().all(|&v| v == 0.) {
        return;
    }
    let (mut top, mut bottom) = hull(d);
    let Some(lo) = clip(&top, &bottom, min, max) else {
        return;
    };
    top.reverse();
    bottom.reverse();
    let Some(hi) = clip(&top, &bottom, min, max) else {
        return;
    };
    let tn = t0 + (t1 - t0) * lo;
    let tx = t0 + (t1 - t0) * hi;
    if (u1 - u0).max(tx - tn) < 1e-9 {
        let (t, u) = ((tn + tx) / 2., (u0 + u1) / 2.);
        out.push(if flip { (u, t) } else { (t, u) });
        return;
    }
    a = part(a, lo, hi);
    let ud = u1 - u0;
    if hi - lo > 0.8 {
        if tx - tn > ud {
            let (l, r) = split(a, 0.5);
            let t = (tn + tx) / 2.;
            visit(b, l, !flip, depth, calls, u0, u1, tn, t, out);
            visit(b, r, !flip, depth, calls, u0, u1, t, tx, out);
        } else {
            let (l, r) = split(b, 0.5);
            let u = (u0 + u1) / 2.;
            visit(l, a, !flip, depth, calls, u0, u, tn, tx, out);
            visit(r, a, !flip, depth, calls, u, u1, tn, tx, out);
        }
    } else if ud == 0. || ud >= 1e-9 {
        visit(b, a, !flip, depth, calls, u0, u1, tn, tx, out)
    } else {
        visit(a, b, flip, depth, calls, tn, tx, u0, u1, out)
    }
}
pub(super) fn paper_crossings(a: kurbo::CubicBez, b: kurbo::CubicBez) -> R<Vec<(f64, f64)>> {
    let values = |c: kurbo::CubicBez| {
        [
            c.p0.x, c.p0.y, c.p1.x, c.p1.y, c.p2.x, c.p2.y, c.p3.x, c.p3.y,
        ]
    };
    let mut out = vec![];
    let mut calls = 0;
    visit(
        values(a),
        values(b),
        false,
        0,
        &mut calls,
        0.,
        1.,
        0.,
        1.,
        &mut out,
    );
    if calls >= 4096 {
        return Err(crate::request_receipts::DispatchErrorCode::Unavailable);
    }
    for (t, p) in [(0., a.p0), (1., a.p3)] {
        for (u, q) in [(0., b.p0), (1., b.p3)] {
            if p.distance(q) <= 1e-12 {
                out.push((t, u));
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-8 && (a.1 - b.1).abs() < 1e-8);
    Ok(out)
}
