//! Feature detection and sample spacing from the pinned production tween baseline.
use super::*;
pub(super) fn pair_wrap_pi(mut d: f64) -> f64 {
    while d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    while d < -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    d
}
pub(super) fn feature_fractions(p: &PathSampler, max_features: usize) -> Vec<f64> {
    if p.total <= 0. {
        return vec![];
    }
    let angles: Vec<_> = (0..=96)
        .map(|i| {
            let (x, y) = p.tangent_at(p.total * i as f64 / 96.);
            y.atan2(x)
        })
        .collect();
    let turn: Vec<_> = angles
        .windows(2)
        .map(|a| pair_wrap_pi(a[1] - a[0]).abs())
        .collect();
    let mut features: Vec<_> = (0..turn.len())
        .filter(|&i| {
            turn[i] >= 0.28
                && turn[i] >= if i > 0 { turn[i - 1] } else { -1. }
                && turn[i] >= turn.get(i + 1).copied().unwrap_or(-1.)
        })
        .collect();
    features.sort_by(|&a, &b| turn[b].total_cmp(&turn[a]));
    let mut picked: Vec<f64> = vec![];
    for i in features {
        let t = (i + 1) as f64 / 96.;
        if picked
            .iter()
            .all(|&p| (p - t).abs() >= 0.5 / max_features.max(2) as f64)
        {
            picked.push(t);
        }
        if picked.len() >= max_features {
            break;
        }
    }
    picked.sort_by(f64::total_cmp);
    picked
}
pub(super) fn shared_fractions(a: &PathSampler, b: &PathSampler, n: usize) -> Vec<f64> {
    let mut grid: Vec<_> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    if n < 4 {
        return grid;
    }
    let max = (n / 4).clamp(2, 10);
    let mut used = vec![false; n];
    for f in feature_fractions(a, max)
        .into_iter()
        .chain(feature_fractions(b, max))
    {
        if f <= 0. || f >= 1. {
            continue;
        }
        let i = (1..n - 1)
            .filter(|&i| !used[i])
            .min_by(|&a, &b| (grid[a] - f).abs().total_cmp(&(grid[b] - f).abs()));
        if let Some(i) = i {
            if (grid[i] - f).abs() < 2. / n as f64 {
                grid[i] = f;
                used[i] = true;
            }
        }
    }
    grid.sort_by(f64::total_cmp);
    grid[0] = 0.;
    grid[n - 1] = 1.;
    for i in 1..n {
        if grid[i] - grid[i - 1] < 1. / (n * 2) as f64 {
            grid[i] = grid[i - 1] + 1. / (n * 2) as f64;
        }
    }
    if grid[n - 1] > 1. {
        let over = grid[n - 1] - 1.;
        for (i, f) in grid.iter_mut().enumerate() {
            *f = (*f - over * (i as f64 / (n - 1) as f64)).max(0.);
        }
    }
    grid
}
pub(super) fn sample_fractions(p: &PathSampler, fr: &[f64]) -> Vec<SegOut> {
    fr.iter()
        .enumerate()
        .map(|(i, &f)| {
            let off = f * p.total;
            let pt = p.point_at(off);
            let (tx, ty) = p.tangent_at(off);
            let prev = if i > 0 { fr[i - 1] * p.total } else { off };
            let next = fr.get(i + 1).map_or(off, |f| f * p.total);
            let h = (next - prev).max(0.01) / 6.;
            SegOut {
                point: [pt.x, pt.y],
                handle_in: if i == 0 { [0.; 2] } else { [-tx * h, -ty * h] },
                handle_out: if i == fr.len() - 1 {
                    [0.; 2]
                } else {
                    [tx * h, ty * h]
                },
            }
        })
        .collect()
}
pub(super) fn regularise(fr: Vec<f64>) -> Vec<f64> {
    let n = fr.len();
    if n < 4 {
        return fr;
    }
    let mut d: Vec<_> = fr.windows(2).map(|p| (p[1] - p[0]).max(0.)).collect();
    for _ in 0..4 {
        let mut next = d.clone();
        for i in 1..d.len() - 1 {
            next[i] = d[i] * 0.5 + (d[i - 1] + d[i + 1]) * 0.25;
        }
        d = next;
    }
    let mut sum = 0.;
    for f in &mut d {
        *f = f.max(0.4 / (n - 1) as f64);
        sum += *f;
    }
    if sum <= 0. {
        return fr;
    }
    let mut out = vec![0.];
    for x in d {
        out.push(out.last().unwrap() + x / sum);
    }
    out[n - 1] = 1.;
    out
}
pub(super) fn probes(p: &PathSampler, n: usize, closed: bool) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let p = p.point_at(p.total * i as f64 / (n - usize::from(!closed)) as f64);
            (p.x, p.y)
        })
        .collect()
}
