//! Geometric primitives used by baseline tween interpolation.
use crate::tween_kernels::interpolation::*;
pub(super) fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
pub(super) fn mix(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t)]
}
pub(super) fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
pub(super) fn norm(p: [f64; 2]) -> f64 {
    (p[0] * p[0] + p[1] * p[1]).sqrt()
}
pub(super) fn rotate(p: [f64; 2], angle: f64, scale: f64) -> [f64; 2] {
    let (c, s) = (angle.cos(), angle.sin());
    [scale * (p[0] * c - p[1] * s), scale * (p[0] * s + p[1] * c)]
}
pub(super) fn length(s: &[SegOut]) -> f64 {
    s.windows(2)
        .map(|w| norm(sub(w[1].point, w[0].point)))
        .sum()
}
pub(super) fn centroid(s: &[SegOut]) -> [f64; 2] {
    let mut c = [0.; 2];
    for p in s {
        c[0] += p.point[0];
        c[1] += p.point[1];
    }
    [c[0] / s.len() as f64, c[1] / s.len() as f64]
}
pub(super) fn turn(s: &[SegOut], i: usize) -> f64 {
    let a = sub(s[i].point, s[i - 1].point);
    let b = sub(s[i + 1].point, s[i].point);
    wrap_pi(b[1].atan2(b[0]) - a[1].atan2(a[0]))
}
pub(super) fn crossings(s: &[SegOut]) -> usize {
    let mut count = 0;
    for i in 0..s.len().saturating_sub(1) {
        for j in i + 2..s.len().saturating_sub(1) {
            let (a, b, c, d) = (s[i].point, s[i + 1].point, s[j].point, s[j + 1].point);
            let near = |p, q| {
                let r = sub(p, q);
                r[0] * r[0] + r[1] * r[1] < 1e-9
            };
            if near(a, c) || near(a, d) || near(b, c) || near(b, d) {
                continue;
            }
            if segs_intersect((a[0], a[1]), (b[0], b[1]), (c[0], c[1]), (d[0], d[1])) {
                count += 1;
            }
        }
    }
    count
}
/// `crossings` with a tolerance: a straight run that was rotated by a tiny angle
/// has near-zero turns whose sign is rounding noise, and the exact test reads
/// two collinear segments of it as crossing.
pub(super) fn true_crossings(s: &[SegOut]) -> usize {
    let side = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        let (u, v) = (sub(b, a), sub(c, a));
        let cross = u[0] * v[1] - u[1] * v[0];
        let scale = norm(u) * norm(v);
        if cross.abs() <= 1e-9 * scale {
            0.
        } else {
            cross.signum()
        }
    };
    let mut count = 0;
    for i in 0..s.len().saturating_sub(1) {
        for j in i + 2..s.len().saturating_sub(1) {
            let (a, b, c, d) = (s[i].point, s[i + 1].point, s[j].point, s[j + 1].point);
            let (s1, s2) = (side(c, d, a), side(c, d, b));
            let (s3, s4) = (side(a, b, c), side(a, b, d));
            if s1 * s2 < 0. && s3 * s4 < 0. {
                count += 1;
            }
        }
    }
    count
}
pub(super) fn blend_segments(a: &[SegOut], b: &[SegOut], weights: &[f64]) -> Vec<SegOut> {
    a.iter()
        .zip(b)
        .zip(weights)
        .map(|((a, b), &t)| SegOut {
            point: mix(a.point, b.point, t),
            handle_in: mix(a.handle_in, b.handle_in, t),
            handle_out: mix(a.handle_out, b.handle_out, t),
        })
        .collect()
}
pub(super) fn linear(
    a: &[SegOut],
    b: &[SegOut],
    ca: [f64; 2],
    cb: [f64; 2],
    theta: f64,
    scale: f64,
    t: f64,
    center: [f64; 2],
) -> Vec<SegOut> {
    let at = theta * t;
    let bt = at - theta;
    let sa = lerp(1., scale, t);
    let sb = lerp(if scale > 1e-6 { 1. / scale } else { 1. }, 1., t);
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            let p = mix(
                rotate(sub(a.point, ca), at, sa),
                rotate(sub(b.point, cb), bt, sb),
                t,
            );
            SegOut {
                point: [center[0] + p[0], center[1] + p[1]],
                handle_in: mix(rotate(a.handle_in, at, sa), rotate(b.handle_in, bt, sb), t),
                handle_out: mix(
                    rotate(a.handle_out, at, sa),
                    rotate(b.handle_out, bt, sb),
                    t,
                ),
            }
        })
        .collect()
}
pub(super) fn rigid_fit(a: &[SegOut], b: &[SegOut], ca: [f64; 2], cb: [f64; 2]) -> (f64, f64) {
    let aa: Vec<_> = a
        .iter()
        .map(|s| (s.point[0] - ca[0], s.point[1] - ca[1]))
        .collect();
    let bb: Vec<_> = b
        .iter()
        .map(|s| (s.point[0] - cb[0], s.point[1] - cb[1]))
        .collect();
    let Some(tf) = fit_similarity_transform(&aa, &bb) else {
        return (0., 1.);
    };
    let mut mag = (tf.w_re * tf.w_re + tf.w_im * tf.w_im).sqrt();
    if !(mag > 0.15 && mag < 8.) {
        return (0., 1.);
    }
    let mut theta = tf.w_im.atan2(tf.w_re);
    if theta.abs() > std::f64::consts::FRAC_PI_2 {
        let dot: f64 = aa.iter().zip(&bb).map(|(a, b)| a.0 * b.0 + a.1 * b.1).sum();
        let square: f64 = aa.iter().map(|a| a.0 * a.0 + a.1 * a.1).sum();
        let s0 = if square > 1e-9 { dot / square } else { mag };
        let (mut fitted, mut plain) = (0., 0.);
        for (a, b) in aa.iter().zip(&bb) {
            let r = rotate([a.0, a.1], theta, mag);
            fitted += (r[0] - b.0).powi(2) + (r[1] - b.1).powi(2);
            plain += (s0 * a.0 - b.0).powi(2) + (s0 * a.1 - b.1).powi(2);
        }
        if fitted * 20. >= plain {
            theta = 0.;
            mag = s0.clamp(0.33, 3.);
        }
    }
    if theta.abs() < 0.06 {
        theta = 0.;
    }
    let scale = mag.clamp(0.33, 3.);
    let radius: f64 = aa
        .iter()
        .zip(&bb)
        .map(|(a, b)| (a.0.hypot(a.1) + b.0.hypot(b.1)) / 2.)
        .sum::<f64>()
        / a.len() as f64;
    theta *= (1. - (norm(sub(cb, ca)) / radius.max(1.) - 0.6) / 1.4).clamp(0., 1.);
    if theta != 0. && a.len() >= 5 {
        let base = crossings(a).max(crossings(b));
        let (mut with, mut without) = (0, 0);
        for t in [0.15, 0.35, 0.5, 0.65, 0.85] {
            let c = mix(ca, cb, t);
            with += crossings(&linear(a, b, ca, cb, theta, scale, t, c)).saturating_sub(base);
            without += crossings(&linear(a, b, ca, cb, 0., scale, t, c)).saturating_sub(base);
        }
        if with > without {
            theta = 0.;
        }
    }
    (theta, scale)
}
/// Undamped similarity fit of the centred pair: angle, scale and how well it
/// explains the pair (1 = rigid, 0 = a quarter of the shape's size unexplained).
pub(super) fn similarity(
    a: &[SegOut],
    b: &[SegOut],
    ca: [f64; 2],
    cb: [f64; 2],
) -> Option<(f64, f64, f64)> {
    let centred = |s: &[SegOut], c: [f64; 2]| -> Vec<(f64, f64)> {
        s.iter()
            .map(|s| (s.point[0] - c[0], s.point[1] - c[1]))
            .collect()
    };
    let (aa, bb) = (centred(a, ca), centred(b, cb));
    let fit = fit_similarity_transform(&aa, &bb)?;
    let scale = fit.w_re.hypot(fit.w_im);
    if !(scale > 0.33 && scale < 3.) {
        return None;
    }
    // Residual with the fitted rotation, and with the best scale alone.
    let (dot, square) = aa.iter().zip(&bb).fold((0., 0.), |(d, q), (a, b)| {
        (d + a.0 * b.0 + a.1 * b.1, q + a.0 * a.0 + a.1 * a.1)
    });
    let plain_scale = if square > 1e-9 { dot / square } else { 1. };
    let (mut residual, mut plain, mut size) = (0., 0., 0.);
    for (a, b) in aa.iter().zip(&bb) {
        let r = [
            fit.w_re * a.0 - fit.w_im * a.1 - b.0,
            fit.w_im * a.0 + fit.w_re * a.1 - b.1,
        ];
        residual += r[0] * r[0] + r[1] * r[1];
        plain += (plain_scale * a.0 - b.0).powi(2) + (plain_scale * a.1 - b.1).powi(2);
        size += b.0 * b.0 + b.1 * b.1;
    }
    if !(size > 1e-9) {
        return None;
    }
    let (relative, unturned) = ((residual / size).sqrt(), (plain / size).sqrt());
    // The turn has to be what explains the pair. A round shape fits any angle,
    // so its fitted rotation says nothing and must not bend a straight move.
    if unturned < 0.2_f64.max(2. * relative) {
        return None;
    }
    Some((
        fit.w_im.atan2(fit.w_re),
        scale,
        ((0.25 - relative) / 0.15).clamp(0., 1.),
    ))
}
/// A limb that bends: its length is the same in both keys (within a tenth) and
/// part of it (a sixth of its points at least) stays where it is. Only such a
/// pair is offered the `registered` reconstruction; a stroke that changes its
/// drawing, or that moves as a whole, keeps the baseline candidates.
pub(super) fn articulated(a: &[SegOut], b: &[SegOut]) -> bool {
    let len = |s: &[SegOut]| -> f64 {
        s.windows(2)
            .map(|w| norm(sub(w[1].point, w[0].point)))
            .sum()
    };
    let (la, lb) = (len(a), len(b));
    let size = la.max(lb);
    if size < 1e-6 || (la - lb).abs() > 0.1 * size {
        return false;
    }
    let still = a
        .iter()
        .zip(b)
        .filter(|(a, b)| norm(sub(b.point, a.point)) < 0.03 * size)
        .count();
    still * 6 >= a.len() && still < a.len()
}
/// Length-true in-between for an articulated stroke: edge lengths and turning
/// angles are interpolated (so no edge shortens), then the result is laid onto
/// the point-by-point blend of the keys by a rotation and a translation only. Points that hardly move
/// between the keys weigh most, so the still part of a limb (the upper arm of a
/// bending elbow) stays where it is and the moving part swings from it.
pub(super) fn registered(a: &[SegOut], b: &[SegOut], t: f64, closed: bool) -> Vec<SegOut> {
    let mut shape = intrinsic(a, b, t, [0.; 2], closed, None);
    let target: Vec<[f64; 2]> = a
        .iter()
        .zip(b)
        .map(|(a, b)| mix(a.point, b.point, t))
        .collect();
    let moved: Vec<f64> = a
        .iter()
        .zip(b)
        .map(|(a, b)| norm(sub(b.point, a.point)))
        .collect();
    let reach = (moved.iter().sum::<f64>() / moved.len() as f64).max(1e-6);
    let weights: Vec<f64> = moved
        .iter()
        .map(|d| 1e-3 + (-(4. * d / reach).powi(2)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let mean = |s: &[[f64; 2]]| {
        s.iter().zip(&weights).fold([0.; 2], |c, (s, w)| {
            [c[0] + s[0] * w / total, c[1] + s[1] * w / total]
        })
    };
    let points: Vec<[f64; 2]> = shape.iter().map(|s| s.point).collect();
    let (cs, ct) = (mean(&points), mean(&target));
    let (mut dot, mut cross) = (0., 0.);
    for ((s, g), w) in shape.iter().zip(&target).zip(&weights) {
        let (p, q) = (sub(s.point, cs), sub(*g, ct));
        dot += w * (p[0] * q[0] + p[1] * q[1]);
        cross += w * (p[0] * q[1] - p[1] * q[0]);
    }
    let angle = cross.atan2(dot);
    for s in &mut shape {
        let p = rotate(sub(s.point, cs), angle, 1.);
        s.point = [ct[0] + p[0], ct[1] + p[1]];
        s.handle_in = rotate(s.handle_in, angle, 1.);
        s.handle_out = rotate(s.handle_out, angle, 1.);
    }
    shape
}
pub(super) fn intrinsic(
    a: &[SegOut],
    b: &[SegOut],
    t: f64,
    center: [f64; 2],
    closed: bool,
    anchor: Option<&[SegOut]>,
) -> Vec<SegOut> {
    let n = a.len();
    let (mut lens, mut directions) = (Vec::new(), Vec::new());
    let (mut pa, mut pb, mut direction) = (0., 0., 0.);
    for i in 1..n {
        let av = sub(a[i].point, a[i - 1].point);
        let bv = sub(b[i].point, b[i - 1].point);
        let (aa, ab) = (av[1].atan2(av[0]), bv[1].atan2(bv[0]));
        direction = if i == 1 {
            aa + wrap_pi(ab - aa) * t
        } else {
            direction + lerp(wrap_pi(aa - pa), wrap_pi(ab - pb), t)
        };
        pa = aa;
        pb = ab;
        lens.push(lerp(norm(av), norm(bv), t));
        directions.push(direction);
    }
    let mut pts = vec![[0.; 2]];
    for (i, (&l, &d)) in lens.iter().zip(&directions).enumerate() {
        pts.push([pts[i][0] + l * d.cos(), pts[i][1] + l * d.sin()]);
    }
    let total: f64 = lens.iter().sum();
    let correction = if closed {
        Some([-pts[n - 1][0], -pts[n - 1][1]])
    } else {
        anchor.map(|s| sub(sub(s[n - 1].point, s[0].point), pts[n - 1]))
    };
    if let Some(c) = correction {
        if total > 1e-9 {
            let mut acc = 0.;
            for i in 1..n {
                acc += lens[i - 1];
                for axis in 0..2 {
                    pts[i][axis] += c[axis] * acc / total;
                }
            }
        }
    }
    let mean = [
        pts.iter().map(|p| p[0]).sum::<f64>() / n as f64,
        pts.iter().map(|p| p[1]).sum::<f64>() / n as f64,
    ];
    (0..n)
        .map(|i| SegOut {
            point: [
                pts[i][0] - mean[0] + center[0],
                pts[i][1] - mean[1] + center[1],
            ],
            handle_in: if i == 0 {
                [0.; 2]
            } else {
                let d = sub(pts[i], pts[i - 1]);
                [-d[0] / 3., -d[1] / 3.]
            },
            handle_out: if i == n - 1 {
                [0.; 2]
            } else {
                let d = sub(pts[i + 1], pts[i]);
                [d[0] / 3., d[1] / 3.]
            },
        })
        .collect()
}
