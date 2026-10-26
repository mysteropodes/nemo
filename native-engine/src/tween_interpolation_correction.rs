//! Baseline fold correction, handle re-harmonisation and safe point reduction.
use crate::tween_kernels::interpolation::*;
#[derive(Clone)]
pub(super) struct Folds {
    pub(super) a: Vec<f64>,
    pub(super) delta: Vec<f64>,
    pub(super) weight: Vec<f64>,
}
impl Folds {
    pub(super) fn new(a: &[SegOut], b: &[SegOut]) -> Self {
        let n = a.len();
        let (mut aa, mut delta, mut weight) = (vec![0.; n], vec![0.; n], vec![0.; n]);
        for i in 1..n - 1 {
            aa[i] = turn(a, i);
            let bb = turn(b, i);
            delta[i] = bb - aa[i];
            weight[i] = aa[i].abs().max(bb.abs());
        }
        Self {
            a: aa,
            delta,
            weight,
        }
    }
}
pub(super) fn correct_folds(mut current: Vec<SegOut>, folds: &Folds, t: f64) -> Vec<SegOut> {
    let n = current.len();
    let base = crossings(&current);
    for _ in 0..2 {
        let mut raw = vec![0.; n];
        for i in 1..n - 1 {
            if folds.weight[i] >= 0.35 {
                raw[i] = (wrap_pi(folds.a[i] + folds.delta[i] * t - turn(&current, i)) * 0.5)
                    .clamp(-0.26, 0.26);
            }
        }
        let mut smooth = vec![0.; n];
        let mut changed = false;
        for i in 1..n - 1 {
            smooth[i] = raw[i] * 0.5
                + if i > 1 { raw[i - 1] * 0.25 } else { 0. }
                + if i < n - 2 { raw[i + 1] * 0.25 } else { 0. };
            changed |= smooth[i].abs() >= 0.01;
        }
        if !changed {
            break;
        }
        let mut next = current.clone();
        for i in 1..n - 1 {
            if smooth[i].abs() >= 0.01 {
                let mid = mix(current[i - 1].point, current[i + 1].point, 0.5);
                let delta = rotate(sub(current[i].point, mid), smooth[i], 1.);
                next[i].point = [mid[0] + delta[0], mid[1] + delta[1]];
            }
        }
        if crossings(&next) > base {
            break;
        }
        current = next;
    }
    current
}
fn ripple(s: &[SegOut], closed: bool) -> f64 {
    if s.len() < 6 {
        return 0.;
    }
    let mut path = BezPath::new();
    add_segs(
        &mut path,
        s.iter().map(|s| (s.point, s.handle_in, s.handle_out)),
    );
    if closed {
        path.close_path();
    }
    let sampler = PathSampler::new(&path);
    let length = sampler.total;
    if length <= 0. {
        return 0.;
    }
    let count = (length / 3.).round().clamp(30., 1500.) as usize;
    let points: Vec<_> = (0..=count)
        .map(|i| {
            let p = sampler.point_at(length * i as f64 / count as f64);
            [p.x, p.y]
        })
        .collect();
    let angles: Vec<_> = (1..points.len() - 1)
        .map(|i| {
            let a = sub(points[i], points[i - 1]);
            let b = sub(points[i + 1], points[i]);
            wrap_pi(b[1].atan2(b[0]) - a[1].atan2(a[0])) * 180. / std::f64::consts::PI
        })
        .collect();
    let (mut flips, mut previous) = (0, 0.);
    for i in 1..angles.len() - 1 {
        let h = angles[i] - (angles[i - 1] + angles[i + 1]) / 2.;
        if (h > 0.15 && previous < -0.15) || (h < -0.15 && previous > 0.15) {
            flips += 1;
        }
        if h.abs() > 0.15 {
            previous = h;
        }
    }
    flips as f64 / length * 100.
}
pub(super) fn harmonise(
    s: Vec<SegOut>,
    a: &[SegOut],
    b: &[SegOut],
    t: f64,
    closed: bool,
) -> Vec<SegOut> {
    let before = ripple(&s, closed);
    if before <= 0.01 {
        return s;
    }
    fn relative(h: [f64; 2], edge: [f64; 2]) -> Option<[f64; 2]> {
        let el = norm(edge);
        if el <= 1e-9 {
            return None;
        }
        let hl = norm(h);
        if hl <= 1e-9 {
            return Some([0.; 2]);
        }
        Some([hl / el, wrap_pi(h[1].atan2(h[0]) - edge[1].atan2(edge[0]))])
    }
    let mut probe = s.clone();
    for i in 0..s.len() {
        for incoming in [true, false] {
            if (incoming && i == 0) || (!incoming && i + 1 == s.len()) {
                continue;
            }
            let (from, to) = if incoming { (i - 1, i) } else { (i, i + 1) };
            let ha = if incoming {
                a[i].handle_in
            } else {
                a[i].handle_out
            };
            let hb = if incoming {
                b[i].handle_in
            } else {
                b[i].handle_out
            };
            let ra = relative(ha, sub(a[to].point, a[from].point));
            let rb = relative(hb, sub(b[to].point, b[from].point));
            let edge = sub(s[to].point, s[from].point);
            let len = norm(edge);
            if let (Some(ra), Some(rb)) = (ra, rb) {
                if len > 1e-9 {
                    let fraction = lerp(ra[0], rb[0], t);
                    let offset = ra[1] + wrap_pi(rb[1] - ra[1]) * t;
                    let angle = edge[1].atan2(edge[0]) + offset;
                    let h = [angle.cos() * len * fraction, angle.sin() * len * fraction];
                    if incoming {
                        probe[i].handle_in = h
                    } else {
                        probe[i].handle_out = h
                    }
                }
            }
        }
    }
    if ripple(&probe, closed) < before * 0.7 {
        probe
    } else {
        s
    }
}
fn roughness(s: &[SegOut]) -> f64 {
    if s.len() < 5 {
        return 0.;
    }
    let turns: Vec<_> = (1..s.len() - 1).map(|i| turn(s, i)).collect();
    (1..turns.len() - 1)
        .map(|i| (turns[i] - (turns[i - 1] + turns[i + 1]) / 2.).abs())
        .sum::<f64>()
        / (turns.len() - 2) as f64
}
pub(super) fn reduction_indices(
    a: &[SegOut],
    b: &[SegOut],
    target: usize,
    folds: Option<&Folds>,
) -> Option<Vec<usize>> {
    let n = a.len();
    if target >= n {
        return None;
    }
    let mut alive = vec![true; n];
    let mut previous: Vec<_> = (0..n).map(|i| i.saturating_sub(1)).collect();
    let mut next: Vec<_> = (0..n).map(|i| i + 1).collect();
    let area = |s: &[SegOut], p: usize, i: usize, q: usize| {
        let a = sub(s[i].point, s[p].point);
        let b = sub(s[q].point, s[p].point);
        (a[0] * b[1] - b[0] * a[1]).abs()
    };
    let mut remaining = n;
    while remaining > target {
        let (mut candidate, mut score) = (None, f64::INFINITY);
        for i in 1..n - 1 {
            if !alive[i] || folds.is_some_and(|f| f.weight[i] >= 0.35) {
                continue;
            }
            let value = area(a, previous[i], i, next[i]).max(area(b, previous[i], i, next[i]));
            if value < score {
                score = value;
                candidate = Some(i);
            }
        }
        let Some(i) = candidate else {
            break;
        };
        alive[i] = false;
        next[previous[i]] = next[i];
        previous[next[i]] = previous[i];
        remaining -= 1;
    }
    let indices: Vec<_> = (0..n).filter(|&i| alive[i]).collect();
    let allowance = roughness(a).max(roughness(b));
    for t in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
        let full = blend_segments(a, b, &vec![t; n]);
        let reduced: Vec<_> = indices.iter().map(|&i| full[i].clone()).collect();
        if crossings(&reduced) > crossings(&full)
            || roughness(&reduced)
                > allowance.max(roughness(&full)) + 0.5 * std::f64::consts::PI / 180.
        {
            return None;
        }
    }
    Some(indices)
}
pub(super) fn reduce(s: &[SegOut], indices: &[usize]) -> Vec<SegOut> {
    indices
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let p = s[i].point;
            let previous = if k > 0 { s[indices[k - 1]].point } else { p };
            let next = if k + 1 < indices.len() {
                s[indices[k + 1]].point
            } else {
                p
            };
            let d = sub(next, previous);
            let len = norm(d);
            let len = if len == 0. { 1. } else { len };
            let tangent = [d[0] / len, d[1] / len];
            let before = norm(sub(p, previous));
            let after = norm(sub(next, p));
            SegOut {
                point: p,
                handle_in: if k == 0 {
                    [0.; 2]
                } else {
                    [-tangent[0] * before / 3., -tangent[1] * before / 3.]
                },
                handle_out: if k + 1 == indices.len() {
                    [0.; 2]
                } else {
                    [tangent[0] * after / 3., tangent[1] * after / 3.]
                },
            }
        })
        .collect()
}
