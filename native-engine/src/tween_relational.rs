//! Relational matching refinement from baseline tweens.js (enabled by default).
use crate::tween_kernels::*;

pub(super) fn relational_refine(
    matches: Vec<MatchOut>,
    fa: &[Feat],
    fb: &[Feat],
    cost: &[Vec<f64>],
    transforms: &[SimT],
) -> Vec<MatchOut> {
    let (n, m) = (fa.len(), fb.len());
    if n < 3 || m < 2 {
        return matches;
    }
    let neighbours: Vec<Vec<(usize, f64, f64, f64)>> = fa
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let mut list: Vec<_> = fa
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(j, g)| {
                    let dx = g.cx - f.cx;
                    let dy = g.cy - f.cy;
                    (j, dx, dy, dx * dx + dy * dy)
                })
                .collect();
            list.sort_by(|a, b| a.3.total_cmp(&b.3));
            list.truncate(6.min(n - 1));
            list
        })
        .collect();
    let mut sigma = vec![-1i64; n];
    for matched in matches {
        sigma[matched.a] = matched.b as i64;
    }
    for _ in 0..6 {
        let mut augmented = cost.to_vec();
        for i in 0..n {
            let size = (fa[i].bounds.width().powi(2) + fa[i].bounds.height().powi(2)).sqrt() + 1.0;
            for a in 0..m {
                let (mut acc, mut count) = (0.0, 0);
                for &(j, dx, dy, _) in &neighbours[i] {
                    let b = sigma[j];
                    if b < 0 || b as usize == a {
                        continue;
                    }
                    let tf = &transforms[i];
                    let p = (tf.w_re * dx - tf.w_im * dy, tf.w_im * dx + tf.w_re * dy);
                    let q = (fb[b as usize].cx - fb[a].cx, fb[b as usize].cy - fb[a].cy);
                    let norm =
                        (p.0 * p.0 + p.1 * p.1).sqrt() + (q.0 * q.0 + q.1 * q.1).sqrt() + size;
                    let error = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                    acc += (error / norm * 2.0).min(1.0);
                    count += 1;
                }
                if count != 0 {
                    augmented[i][a] += 0.45 * acc / count as f64;
                }
            }
        }
        let assignment = hungarian(&augmented);
        let next: Vec<_> = assignment[..n]
            .iter()
            .map(|&b| if b >= 0 && (b as usize) < m { b } else { -1 })
            .collect();
        let stable = next == sigma;
        sigma = next;
        if stable {
            break;
        }
    }
    sigma
        .into_iter()
        .enumerate()
        .filter_map(|(a, b)| {
            (b >= 0).then(|| MatchOut {
                a,
                b: b as usize,
                score: cost[a][b as usize],
            })
        })
        .collect()
}
