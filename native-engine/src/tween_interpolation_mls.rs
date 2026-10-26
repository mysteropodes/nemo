//! Baseline moving-least-squares per-vertex interpolation.
use crate::tween_kernels::interpolation::*;
#[derive(Clone)]
pub(super) struct LocalFit {
    theta: f64,
    scale: f64,
    ca: [f64; 2],
    cb: [f64; 2],
}
fn cumulative(s: &[SegOut]) -> Vec<f64> {
    let mut out = vec![0.];
    for i in 1..s.len() {
        out.push(out[i - 1] + norm(sub(s[i].point, s[i - 1].point)));
    }
    out
}
fn handles(a: &[SegOut], b: &[SegOut]) -> Vec<usize> {
    let n = a.len();
    let count = 24.min(n);
    let mut chosen: Vec<_> = (0..count)
        .map(|i| (i as f64 * (n - 1) as f64 / (count - 1).max(1) as f64).round() as usize)
        .collect();
    chosen.dedup();
    if n >= 5 {
        let distances = cumulative(a);
        let mut corners: Vec<_> = (1..n - 1)
            .filter_map(|i| {
                let m = turn(a, i).abs().max(turn(b, i).abs());
                (m > 0.5).then_some((i, m))
            })
            .collect();
        corners.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut claimed = vec![false; chosen.len()];
        for (corner, _) in corners {
            if chosen.contains(&corner) {
                continue;
            }
            let nearest = chosen
                .iter()
                .enumerate()
                .filter(|(i, _)| !claimed[*i])
                .min_by(|(_, a), (_, b)| {
                    (distances[corner] - distances[**a])
                        .abs()
                        .total_cmp(&(distances[corner] - distances[**b]).abs())
                });
            if let Some((index, _)) = nearest {
                claimed[index] = true;
                chosen[index] = corner;
            }
        }
        chosen.sort_unstable();
    }
    chosen
}
pub(super) fn fit_local(a: &[SegOut], b: &[SegOut]) -> Vec<LocalFit> {
    let indices = handles(a, b);
    let count = indices.len();
    let cum = cumulative(a);
    let total = *cum.last().unwrap();
    let total = if total == 0. { 1. } else { total };
    let sigma = (total / count as f64 * 0.75).max(1.);
    (0..a.len())
        .map(|q| {
            let weights: Vec<_> = indices
                .iter()
                .map(|&h| {
                    let d = (cum[q] - cum[h]).abs();
                    1. / ((d * d) / (sigma * sigma) + 0.01)
                })
                .collect();
            let sum: f64 = weights.iter().sum();
            let (mut ca, mut cb) = ([0.; 2], [0.; 2]);
            for (&i, &w) in indices.iter().zip(&weights) {
                for axis in 0..2 {
                    ca[axis] += a[i].point[axis] * w;
                    cb[axis] += b[i].point[axis] * w;
                }
            }
            for axis in 0..2 {
                ca[axis] /= sum;
                cb[axis] /= sum;
            }
            let (mut re, mut im, mut den) = (0., 0., 0.);
            for (&i, &w) in indices.iter().zip(&weights) {
                let x = sub(a[i].point, ca);
                let y = sub(b[i].point, cb);
                re += w * (x[0] * y[0] + x[1] * y[1]);
                im += w * (x[0] * y[1] - x[1] * y[0]);
                den += w * (x[0] * x[0] + x[1] * x[1]);
            }
            if den < 1e-6 {
                return LocalFit {
                    theta: 0.,
                    scale: 1.,
                    ca: a[q].point,
                    cb: b[q].point,
                };
            }
            re /= den;
            im /= den;
            let mag = (re * re + im * im).sqrt();
            LocalFit {
                theta: if mag > 0.1 && mag < 10. {
                    im.atan2(re)
                } else {
                    0.
                },
                scale: (if mag == 0. { 1. } else { mag }).clamp(0.33, 3.),
                ca,
                cb,
            }
        })
        .collect()
}
pub(super) fn local_segments(a: &[SegOut], b: &[SegOut], fits: &[LocalFit], t: f64) -> Vec<SegOut> {
    a.iter()
        .zip(b)
        .zip(fits)
        .map(|((a, b), f)| {
            let inverse = if f.scale > 1e-6 { 1. / f.scale } else { 1. };
            let at = f.theta * t;
            let bt = at - f.theta;
            let sa = lerp(1., f.scale, t);
            let sb = lerp(inverse, 1., t);
            let pa = rotate(sub(a.point, f.ca), at, sa);
            let pb = rotate(sub(b.point, f.cb), bt, sb);
            SegOut {
                point: mix(
                    [f.ca[0] + pa[0], f.ca[1] + pa[1]],
                    [f.cb[0] + pb[0], f.cb[1] + pb[1]],
                    t,
                ),
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
