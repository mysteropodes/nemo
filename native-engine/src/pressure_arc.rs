//! Paper.js 0.12.18 Path.arcTo(from, through, to), preserving its three-point round-cap construction.
//! Copyright (c) 2011–2020 Jürg Lehni & Jonathan Puckey. MIT (tween_paper_license.txt).
use crate::object_document::pressure_geometry::SegOut;
use std::f64::consts::PI;
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: [f64; 2], n: f64) -> [f64; 2] {
    [a[0] * n, a[1] * n]
}
fn rotate(a: [f64; 2], degrees: f64) -> [f64; 2] {
    if degrees == 0. {
        return a;
    }
    let radians = degrees * PI / 180.;
    let sin = radians.sin();
    let cos = radians.cos();
    [a[0] * cos - a[1] * sin, a[0] * sin + a[1] * cos]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn side(p: [f64; 2], v: [f64; 2], q: [f64; 2], infinite: bool) -> f64 {
    let dx = q[0] - p[0];
    let mut value = dx * v[1] - (q[1] - p[1]) * v[0];
    if !infinite && value.abs() <= 112e-18 {
        value = (dx * v[0] + dx * v[0]) / (v[0] * v[0] + v[1] * v[1]);
        if (0. ..=1.).contains(&value) {
            value = 0.;
        }
    }
    if value < 0. {
        -1.
    } else if value > 0. {
        1.
    } else {
        0.
    }
}
pub(super) fn cap(
    prev: &mut SegOut,
    next: &mut SegOut,
    center: [f64; 2],
    dir: [f64; 2],
    hw: f64,
) -> Vec<SegOut> {
    let from = prev.point;
    let to = next.point;
    let through = add(center, scale(dir, hw));
    let p = scale(add(from, through), 0.5);
    let v = rotate(sub(through, from), 90.);
    let q = scale(add(through, to), 0.5);
    let w = rotate(sub(to, through), 90.);
    let divisor = cross(v, w);
    let chord = sub(to, from);
    let through_side = side(from, chord, through, false);
    if divisor.abs() <= 112e-18 {
        if through_side == 0. {
            prev.handle_out = [0.; 2];
            next.handle_in = [0.; 2];
        }
        return vec![];
    }
    let delta = sub(p, q);
    let t = (w[0] * delta[1] - w[1] * delta[0]) / divisor;
    let origin = add(p, scale(v, t));
    let mut x = sub(from, origin);
    let y = sub(to, origin);
    let mut angle = 180. * cross(x, y).atan2(x[0] * y[0] + x[1] * y[1]) / PI;
    let center_side = side(from, chord, origin, true);
    if center_side == 0. {
        angle = through_side * angle.abs();
    } else if through_side == center_side {
        angle += if angle < 0. { 360. } else { -360. };
    }
    if angle == 0. {
        return vec![];
    }
    let count = if angle.abs() >= 360. {
        4
    } else {
        ((angle.abs() - 1e-5) / 90.).ceil() as usize
    };
    let step = angle / count as f64;
    let half = step * PI / 360.;
    let factor = 4. / 3. * half.sin() / (1. + half.cos());
    let mut middle = vec![];
    for i in 0..=count {
        let out = if i < count { rotate(x, 90.) } else { [0.; 2] };
        if i == 0 {
            prev.handle_out = scale(out, factor);
        } else {
            let incoming = scale(rotate(x, -90.), factor);
            if i == count {
                next.handle_in = incoming;
            } else {
                middle.push(SegOut {
                    point: add(origin, x),
                    handle_in: incoming,
                    handle_out: scale(out, factor),
                });
            }
        }
        x = rotate(x, step);
    }
    middle
}
