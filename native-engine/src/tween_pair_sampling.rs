//! Resolve drawing direction before assigning corresponding fractions and pressure.
use super::*;
pub(super) fn source_segments(s: &StrokeIn, vb: bool) -> Vec<CenterSegIn> {
    if vb && s.center_segments.as_ref().is_some_and(|c| c.len() > 1) {
        return s.center_segments.clone().unwrap();
    }
    s.segments
        .iter()
        .map(|s| CenterSegIn {
            point: s.point,
            handle_in: s.handle_in,
            handle_out: s.handle_out,
            width: 1.,
        })
        .collect()
}
pub(super) fn segment_sampler(s: &[CenterSegIn], closed: bool) -> PathSampler {
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
    PathSampler::new(&path)
}
fn sparse_width(s: &[CenterSegIn], f: f64) -> f64 {
    let mut lens = vec![0.];
    for p in s.windows(2) {
        lens.push(
            lens.last().unwrap()
                + (p[1].point[0] - p[0].point[0]).hypot(p[1].point[1] - p[0].point[1]),
        );
    }
    let total = lens.last().copied().unwrap_or(0.);
    let total = if total == 0. { 1. } else { total };
    let target = f * total;
    let mut i = 0;
    while i < lens.len() - 2 && lens[i + 1] < target {
        i += 1;
    }
    let t = ((target - lens[i]) / (lens[i + 1] - lens[i]).max(0.0001)).clamp(0., 1.);
    s[i].width + (s[i + 1].width - s[i].width) * t
}
pub(super) fn pair_widths(
    s: &StrokeIn,
    src: &[CenterSegIn],
    fr: &[f64],
    reversed: bool,
) -> Vec<f64> {
    let profile = s.width_profile.as_deref().filter(|p| p.len() > 1);
    fr.iter()
        .map(|&f| {
            let Some(p) = profile else {
                return sparse_width(src, f);
            };
            let f = if reversed { 1. - f } else { f };
            if f <= p[0].t {
                return p[0].width;
            }
            if f >= p[p.len() - 1].t {
                return p[p.len() - 1].width;
            }
            let (mut lo, mut hi) = (0, p.len() - 1);
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if p[mid].t < f {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let t = (f - p[lo].t) / (p[hi].t - p[lo].t).max(0.0000001);
            p[lo].width + (p[hi].width - p[lo].width) * t
        })
        .collect()
}
fn relative_cost(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    let center = |p: &[(f64, f64)]| {
        let mut sum = (0., 0.);
        for &(x, y) in p {
            sum.0 += x;
            sum.1 += y;
        }
        (sum.0 / p.len() as f64, sum.1 / p.len() as f64)
    };
    let ca = center(a);
    let cb = center(b);
    a.iter()
        .zip(b)
        .map(|(&(x, y), &(u, v))| {
            let dx = (x - ca.0) - (u - cb.0);
            let dy = (y - ca.1) - (v - cb.1);
            dx * dx + dy * dy
        })
        .sum()
}
pub(super) fn orient_pair(
    a: &PathSampler,
    mut b: PathSampler,
    mut src: Vec<CenterSegIn>,
    closed: bool,
) -> (PathSampler, Vec<CenterSegIn>, bool) {
    let pa = probes(a, 32, closed);
    let mut reverse = false;
    if !closed {
        let pb = probes(&b, 32, false);
        let flipped: Vec<_> = pb.iter().rev().copied().collect();
        if relative_cost(&pa, &flipped) < relative_cost(&pa, &pb) {
            reverse = true;
            src.reverse();
            for s in &mut src {
                std::mem::swap(&mut s.handle_in, &mut s.handle_out);
            }
            b = segment_sampler(&src, false);
        }
    } else {
        let (mut best, mut off) = (f64::INFINITY, 0.);
        for rev in [false, true] {
            for ro in 0..12 {
                let offset = ro as f64 / 12.;
                let points: Vec<_> = (0..32)
                    .map(|i| {
                        let t = (offset
                            + if rev {
                                -(i as f64 / 32.)
                            } else {
                                i as f64 / 32.
                            })
                        .rem_euclid(1.);
                        let p = b.point_at(b.total * t);
                        (p.x, p.y)
                    })
                    .collect();
                let score = relative_cost(&pa, &points);
                if score < best {
                    best = score;
                    reverse = rev;
                    off = offset;
                }
            }
        }
        if reverse || off > 0.001 {
            let hl = b.total / 32. / 3.;
            src = (0..32)
                .map(|i| {
                    let t = (off
                        + if reverse {
                            -(i as f64 / 32.)
                        } else {
                            i as f64 / 32.
                        })
                    .rem_euclid(1.);
                    let p = b.point_at(b.total * t);
                    let (tx, ty) = b.tangent_at(b.total * t);
                    CenterSegIn {
                        point: [p.x, p.y],
                        handle_in: [-tx * hl, -ty * hl],
                        handle_out: [tx * hl, ty * hl],
                        width: sparse_width(&src, t),
                    }
                })
                .collect();
            b = segment_sampler(&src, true);
        }
    }
    (b, src, reverse)
}
