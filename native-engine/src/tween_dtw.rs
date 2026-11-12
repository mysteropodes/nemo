//! Monotone full-curve correspondence; production default position/tangent cost.
use crate::tween_kernels::pair::*;
pub(super) fn dtw(a: &PathSampler, b: &PathSampler, n: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    if a.total <= 0. || b.total <= 0. {
        return None;
    }
    let k = n.clamp(16, 90);
    let pa = probes(a, k, false);
    let pb = probes(b, k, false);
    let ta: Vec<_> = (0..k)
        .map(|i| {
            let (x, y) = a.tangent_at(a.total * i as f64 / (k - 1) as f64);
            y.atan2(x)
        })
        .collect();
    let tb: Vec<_> = (0..k)
        .map(|i| {
            let (x, y) = b.tangent_at(b.total * i as f64 / (k - 1) as f64);
            y.atan2(x)
        })
        .collect();
    let transform = fit_similarity_transform(&pa, &pb)?;
    let pred: Vec<_> = pa
        .iter()
        .map(|&(x, y)| apply_similarity_transform(&transform, x, y))
        .collect();
    let scale = ((a.total + b.total) / 2. * 0.25).max(1.);
    // TW_DTW_SYMMETRIC and TW_CURVATURE_DTW are both false in the agreed baseline.
    let cost = |i: usize, j: usize| {
        (pred[i].0 - pb[j].0).hypot(pred[i].1 - pb[j].1) / scale
            + 0.35 * pair_wrap_pi(ta[i] - tb[j]).abs() / std::f64::consts::PI
    };
    let mut d = vec![0.; k * k];
    let mut bp = vec![0u8; k * k];
    d[0] = cost(0, 0);
    for i in 1..k {
        d[i * k] = d[(i - 1) * k] + cost(i, 0);
        bp[i * k] = 1;
    }
    for j in 1..k {
        d[j] = d[j - 1] + cost(0, j);
        bp[j] = 2;
    }
    for i in 1..k {
        for j in 1..k {
            let (dd, du, dl) = (d[(i - 1) * k + j - 1], d[(i - 1) * k + j], d[i * k + j - 1]);
            let (prev, mv) = if dd <= du && dd <= dl {
                (dd, 0)
            } else if du <= dl {
                (du, 1)
            } else {
                (dl, 2)
            };
            d[i * k + j] = prev + cost(i, j);
            bp[i * k + j] = mv;
        }
    }
    let (mut i, mut j) = (k - 1, k - 1);
    let mut path = vec![(i, j)];
    while i > 0 || j > 0 {
        let mv = if i == 0 {
            2
        } else if j == 0 {
            1
        } else {
            bp[i * k + j]
        };
        if mv == 0 {
            i -= 1;
            j -= 1;
        } else if mv == 1 {
            i -= 1;
        } else {
            j -= 1;
        }
        path.push((i, j));
    }
    path.reverse();
    let den = (k - 1) as f64;
    let prog: Vec<_> = path
        .iter()
        .map(|&(i, j)| (i as f64 / den + j as f64 / den) / 2.)
        .collect();
    let mut fa = vec![0.];
    let mut fb = vec![0.];
    let mut pk = 0;
    for i in 1..n - 1 {
        let target = i as f64 / (n - 1) as f64;
        while pk < prog.len() - 2 && prog[pk + 1] < target {
            pk += 1;
        }
        let lt = if prog[pk + 1] > prog[pk] {
            (target - prog[pk]) / (prog[pk + 1] - prog[pk])
        } else {
            0.
        };
        let a0 = path[pk].0 as f64 / den;
        let a1 = path[pk + 1].0 as f64 / den;
        let b0 = path[pk].1 as f64 / den;
        let b1 = path[pk + 1].1 as f64 / den;
        fa.push(a0 + (a1 - a0) * lt);
        fb.push(b0 + (b1 - b0) * lt);
    }
    fa.push(1.);
    fb.push(1.);
    for i in 1..n {
        fa[i] = fa[i].max(fa[i - 1]);
        fb[i] = fb[i].max(fb[i - 1]);
    }
    Some((regularise(fa), regularise(fb)))
}
