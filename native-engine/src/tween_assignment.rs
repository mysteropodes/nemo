//! #2358: tween assignment; adapted from geometry-wasm at 46442ea9.
use super::*;

// ---- "force line" similarity transform (fitSimilarityTransform) ----
#[derive(Clone)]
pub(super) struct SimT {
    pub(super) w_re: f64,
    pub(super) w_im: f64,
    pub(super) ca: (f64, f64),
    pub(super) cb: (f64, f64),
}
pub(super) fn fit_similarity_transform(pts_a: &[(f64, f64)], pts_b: &[(f64, f64)]) -> Option<SimT> {
    let n = pts_a.len();
    if n < 2 {
        return None;
    }
    let mut ca = (0.0, 0.0);
    let mut cb = (0.0, 0.0);
    for i in 0..n {
        ca.0 += pts_a[i].0;
        ca.1 += pts_a[i].1;
        cb.0 += pts_b[i].0;
        cb.1 += pts_b[i].1;
    }
    ca.0 /= n as f64;
    ca.1 /= n as f64;
    cb.0 /= n as f64;
    cb.1 /= n as f64;
    let (mut num_re, mut num_im, mut den) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (ax, ay) = (pts_a[i].0 - ca.0, pts_a[i].1 - ca.1);
        let (bx, by) = (pts_b[i].0 - cb.0, pts_b[i].1 - cb.1);
        num_re += ax * bx + ay * by;
        num_im += ax * by - ay * bx;
        den += ax * ax + ay * ay;
    }
    if den < 1e-6 {
        return None;
    }
    Some(SimT {
        w_re: num_re / den,
        w_im: num_im / den,
        ca,
        cb,
    })
}
pub(super) fn apply_similarity_transform(t: &SimT, x: f64, y: f64) -> (f64, f64) {
    let (dx, dy) = (x - t.ca.0, y - t.ca.1);
    let (rx, ry) = (t.w_re * dx - t.w_im * dy, t.w_im * dx + t.w_re * dy);
    (rx + t.cb.0, ry + t.cb.1)
}

// ---- Hungarian (Kuhn-Munkres), direct port ----
pub(super) fn hungarian(cost: &[Vec<f64>]) -> Vec<i64> {
    let n = cost.len();
    const INF: f64 = 1e9;
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![INF; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = INF;
            let mut j1 = 0usize;
            for j in 1..=n {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j2 in 0..=n {
                if used[j2] {
                    u[p[j2]] += delta;
                    v[j2] -= delta;
                } else {
                    minv[j2] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1b = way[j0];
            p[j0] = p[j1b];
            j0 = j1b;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut assign = vec![-1i64; n];
    for j in 1..=n {
        if p[j] > 0 {
            assign[p[j] - 1] = (j - 1) as i64;
        }
    }
    assign
}
