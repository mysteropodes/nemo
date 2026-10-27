//! Paper-compatible path smoothing and bounds used by the original vector dab/ribbon producer.
use crate::tween_kernels::*;
pub(super) fn path_of(s: &[SegOut], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    add_segs(
        &mut path,
        s.iter().map(|s| (s.point, s.handle_in, s.handle_out)),
    );
    if closed && s.len() > 1 {
        let a = s.last().unwrap();
        let b = &s[0];
        path.curve_to(
            (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]),
            (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]),
            (b.point[0], b.point[1]),
        );
        path.close_path();
    }
    path
}
pub(super) fn smooth_continuous(segs: &mut [SegOut], closed: bool) {
    let n = segs.len();
    if n < 2 {
        return;
    }
    let pad = if closed { n.min(4) } else { 0 };
    let d = n - 1 + 2 * pad;
    if d <= 1 {
        return;
    }
    let points: Vec<_> = (0..=d).map(|i| segs[(i + n - pad) % n].point).collect();
    let (mut rx, mut ry, mut div) = (vec![0.; d], vec![0.; d], vec![0.; d]);
    let (mut ax, mut ay, mut p) = (
        points[0][0] + 2. * points[1][0],
        points[0][1] + 2. * points[1][1],
        2.,
    );
    rx[0] = ax;
    ry[0] = ay;
    div[0] = p;
    for i in 1..d {
        let inner = i < d - 1;
        let s = if inner { 4. } else { 8. };
        let k = if inner { 2. } else { 1. };
        let factor = if inner { 1. } else { 2. } / p;
        p = if inner { 4. } else { 7. } - factor;
        ax = s * points[i][0] + k * points[i + 1][0] - factor * ax;
        ay = s * points[i][1] + k * points[i + 1][1] - factor * ay;
        div[i] = p;
        rx[i] = ax;
        ry[i] = ay;
    }
    let (mut x, mut y) = (vec![0.; d + 1], vec![0.; d + 1]);
    x[d - 1] = rx[d - 1] / div[d - 1];
    y[d - 1] = ry[d - 1] / div[d - 1];
    for i in (0..d - 1).rev() {
        x[i] = (rx[i] - x[i + 1]) / div[i];
        y[i] = (ry[i] - y[i + 1]) / div[i];
    }
    x[d] = (3. * points[d][0] - x[d - 1]) / 2.;
    y[d] = (3. * points[d][1] - y[d - 1]) / 2.;
    for i in 0..n {
        let g = i + pad;
        let h = [x[g] - segs[i].point[0], y[g] - segs[i].point[1]];
        if closed || i < n - 1 {
            segs[i].handle_out = h;
        }
        if closed || i > 0 {
            segs[i].handle_in = [-h[0], -h[1]];
        }
    }
}
pub(super) fn smooth_catmull(segs: &mut [SegOut]) {
    let n = segs.len();
    if n < 2 {
        return;
    }
    for i in 0..n {
        let a = segs[(i + n - 1) % n].point;
        let o = segs[i].point;
        let h = segs[(i + 1) % n].point;
        let u = (a[0] - o[0]).hypot(a[1] - o[1]);
        let l = (o[0] - h[0]).hypot(o[1] - h[1]);
        let f = u.sqrt();
        let t = l.sqrt();
        let ff = f * f;
        let tt = t * t;
        let ci = 2. * tt + 3. * t * f + ff;
        let di = 3. * t * (t + f);
        let co = 2. * ff + 3. * f * t + tt;
        let dout = 3. * f * (f + t);
        for axis in 0..2 {
            segs[i].handle_in[axis] = if di != 0. {
                (tt * a[axis] + ci * o[axis] - ff * h[axis]) / di - o[axis]
            } else {
                0.
            };
            segs[i].handle_out[axis] = if dout != 0. {
                (ff * h[axis] + co * o[axis] - tt * a[axis]) / dout - o[axis]
            } else {
                0.
            };
        }
    }
}
pub(super) fn position_shape(segs: &mut [SegOut], closed: bool, angle: f64, center: [f64; 2]) {
    let r = angle * std::f64::consts::PI / 180.;
    let (c, s) = (r.cos(), r.sin());
    let rot = |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    for p in segs.iter_mut() {
        p.point = rot(p.point);
        p.handle_in = rot(p.handle_in);
        p.handle_out = rot(p.handle_out);
    }
    let b = path_of(segs, closed).bounding_box();
    let offset = [
        center[0] - (b.x0 + b.x1) / 2.,
        center[1] - (b.y0 + b.y1) / 2.,
    ];
    for p in segs {
        p.point[0] += offset[0];
        p.point[1] += offset[1];
    }
}
pub(super) fn zero_segment(point: [f64; 2]) -> SegOut {
    SegOut {
        point,
        handle_in: [0.; 2],
        handle_out: [0.; 2],
    }
}
