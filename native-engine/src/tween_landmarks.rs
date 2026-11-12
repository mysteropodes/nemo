//! Sparse landmark fallback when no coherent full-curve fit exists.
use crate::tween_kernels::pair::*;
pub(super) fn landmark_fractions(
    a: &PathSampler,
    b: &PathSampler,
    n: usize,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let max = (n / 8).clamp(2, 8);
    let fa = feature_fractions(a, max);
    let fb = feature_fractions(b, max);
    if fa.len() < 2 || fb.len() < 2 {
        return None;
    }
    let pa: Vec<_> = fa.iter().map(|&f| a.point_at(f * a.total)).collect();
    let pb: Vec<_> = fb.iter().map(|&f| b.point_at(f * b.total)).collect();
    let transform = fit_similarity_transform(&probes(a, 32, false), &probes(b, 32, false))?;
    let limit = (0.4 * a.total.max(b.total) / fa.len().min(fb.len()).max(1) as f64).max(24.);
    let nn = fa.len() + fb.len();
    let cost: Vec<Vec<_>> = (0..nn)
        .map(|i| {
            (0..nn)
                .map(|j| {
                    if i < fa.len() && j < fb.len() {
                        let (x, y) = apply_similarity_transform(&transform, pa[i].x, pa[i].y);
                        (x - pb[j].x).hypot(y - pb[j].y)
                    } else if i >= fa.len() && j >= fb.len() {
                        0.
                    } else {
                        limit
                    }
                })
                .collect()
        })
        .collect();
    let assignment = hungarian(&cost);
    let mut pairs = vec![];
    for i in 0..fa.len() {
        let j = assignment[i];
        if j >= 0 && (j as usize) < fb.len() && cost[i][j as usize] <= limit {
            pairs.push((fa[i], fb[j as usize]));
        }
    }
    if pairs.is_empty() || pairs.windows(2).any(|p| p[1].1 <= p[0].1) {
        return None;
    }
    pairs.retain(|p| p.0 > 0.02 && p.0 < 0.98);
    if pairs.is_empty() {
        return None;
    }
    pairs.insert(0, (0., 0.));
    pairs.push((1., 1.));
    let lengths: Vec<_> = pairs
        .windows(2)
        .map(|p| ((p[1].0 - p[0].0 + p[1].1 - p[0].1) / 2.).max(0.0001))
        .collect();
    let total: f64 = lengths.iter().sum();
    let mut counts: Vec<i64> = lengths
        .iter()
        .map(|l| (l / total * (n - 1) as f64).round().max(1.) as i64)
        .collect();
    let sum: i64 = counts.iter().sum();
    *counts.last_mut().unwrap() += (n - 1) as i64 - sum;
    if counts.last().copied().unwrap() < 1 {
        return None;
    }
    let (mut outa, mut outb) = (vec![0.], vec![0.]);
    for (i, &count) in counts.iter().enumerate() {
        for k in 1..=count {
            let t = k as f64 / count as f64;
            outa.push(pairs[i].0 + (pairs[i + 1].0 - pairs[i].0) * t);
            outb.push(pairs[i].1 + (pairs[i + 1].1 - pairs[i].1) * t);
        }
    }
    (outa.len() == n).then_some((outa, outb))
}
