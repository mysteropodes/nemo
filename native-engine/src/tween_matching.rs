//! #2358: tween matching; adapted from geometry-wasm at 46442ea9.
use super::*;

// ---- autoMatch, two-pass ----
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MatchOut {
    pub a: usize,
    pub b: usize,
    pub score: f64,
}

pub(super) fn auto_match_inner(sa: &[StrokeIn], sb: &[StrokeIn]) -> Vec<MatchOut> {
    if sa.is_empty() || sb.is_empty() {
        return Vec::new();
    }
    let mut fa: Vec<Feat> = sa.iter().map(stroke_feat).collect();
    let mut fb: Vec<Feat> = sb.iter().map(stroke_feat).collect();
    let ba = union_bounds(&fa);
    let bb = union_bounds(&fb);
    for f in fa.iter_mut() {
        f.rel_x = (f.cx - ba.0) / ba.2;
        f.rel_y = (f.cy - ba.1) / ba.3;
    }
    for f in fb.iter_mut() {
        f.rel_x = (f.cx - bb.0) / bb.2;
        f.rel_y = (f.cy - bb.1) / bb.3;
    }
    let match_norm = (((ba.0 + ba.2).max(bb.0 + bb.2) - ba.0.min(bb.0)).powi(2)
        + ((ba.1 + ba.3).max(bb.1 + bb.3) - ba.1.min(bb.1)).powi(2))
    .sqrt();

    const FADE_COST: f64 = 0.6;
    let (n, m) = (sa.len(), sb.len());
    let big_n = n + m;
    let build_cost = |pts_t: Option<&Vec<Vec<(f64, f64)>>>| -> Vec<Vec<f64>> {
        let mut c = Vec::with_capacity(big_n);
        for a in 0..big_n {
            let mut row = Vec::with_capacity(big_n);
            for b in 0..big_n {
                if a < n && b < m {
                    let ov = pts_t.map(|pt| pt[a].as_slice());
                    row.push(match_sc(&fa[a], &fb[b], a == b, ov, match_norm));
                } else if a >= n && b >= m {
                    row.push(0.0);
                } else {
                    row.push(FADE_COST);
                }
            }
            c.push(row);
        }
        c
    };

    let cost = build_cost(None);
    let assign = hungarian(&cost);
    let mut matches: Vec<MatchOut> = Vec::new();
    for a in 0..n {
        let b = assign[a];
        if b >= 0 && (b as usize) < m {
            matches.push(MatchOut {
                a,
                b: b as usize,
                score: cost[a][b as usize],
            });
        }
    }
    if matches.len() < 2 {
        return matches;
    }

    let mut seeds = matches.clone();
    // total_cmp, not partial_cmp().unwrap(): a NaN score (degenerate stroke
    // features) would panic auto_match — masked by the JS fallback as a
    // silent slow path (CLAUDE.md §3), never surfaced as a visible bug.
    seeds.sort_by(|x, y| x.score.total_cmp(&y.score));
    let seed_count = (seeds.len() as f64 * 0.5).ceil().max(2.0) as usize;
    seeds.truncate(seed_count);
    let pts_a: Vec<(f64, f64)> = seeds.iter().map(|s| (fa[s.a].cx, fa[s.a].cy)).collect();
    let pts_b: Vec<(f64, f64)> = seeds.iter().map(|s| (fb[s.b].cx, fb[s.b].cy)).collect();
    let transform = match fit_similarity_transform(&pts_a, &pts_b) {
        Some(t) => t,
        None => {
            uncross_matches(&mut matches, &fa, &fb, match_norm);
            return matches;
        }
    };

    // LOCAL motion model — verbatim port of tweens.js's autoMatchJS pass-2
    // K-nearest-seeds refinement (2026-07, "mauvaise reconnaissance de
    // trait"; see the JS comment for the full rationale): one global
    // similarity transform is dominated by the biggest/most confident
    // strokes, mispredicting every region that moves differently (a head
    // tilting while the arms swing) and chain-mismatching its small
    // features. Per-stroke transform fitted on the K=4 nearest seed pairs
    // instead, global fit kept as fallback for degenerate/implausible
    // local fits and tiny seed sets.
    const K_LOCAL: usize = 4;
    let mut transforms = Vec::with_capacity(fa.len());
    let pts_t: Vec<Vec<(f64, f64)>> = fa
        .iter()
        .enumerate()
        .map(|(ai, f)| {
            let mut tf = &transform;
            let local_tf;
            let pool: Vec<_> = seeds.iter().filter(|seed| seed.a != ai).collect();
            if pool.len() > K_LOCAL {
                let mut near = pool;
                near.sort_by(|s1, s2| {
                    let d1 = (fa[s1.a].cx - f.cx).powi(2) + (fa[s1.a].cy - f.cy).powi(2);
                    let d2 = (fa[s2.a].cx - f.cx).powi(2) + (fa[s2.a].cy - f.cy).powi(2);
                    d1.total_cmp(&d2)
                });
                near.truncate(K_LOCAL);
                let la: Vec<(f64, f64)> = near.iter().map(|s| (fa[s.a].cx, fa[s.a].cy)).collect();
                let lb: Vec<(f64, f64)> = near.iter().map(|s| (fb[s.b].cx, fb[s.b].cy)).collect();
                if let Some(lt) = fit_similarity_transform(&la, &lb) {
                    let lmag = (lt.w_re * lt.w_re + lt.w_im * lt.w_im).sqrt();
                    if lmag > 0.15 && lmag < 8.0 {
                        local_tf = lt;
                        tf = &local_tf;
                    }
                }
            }
            transforms.push(tf.clone());
            f.pts
                .iter()
                .map(|p| apply_similarity_transform(tf, p.0, p.1))
                .collect()
        })
        .collect();
    let mut cost2 = build_cost(Some(&pts_t));
    // Relational baseline: prediction cannot overrule much stronger raw geometry.
    for a in 0..n {
        for b in 0..m {
            cost2[a][b] = cost2[a][b].min(cost[a][b] + 0.03);
        }
    }
    let assign2 = hungarian(&cost2);
    let mut matches2 = Vec::new();
    for a in 0..n {
        let b = assign2[a];
        if b >= 0 && (b as usize) < m {
            matches2.push(MatchOut {
                a,
                b: b as usize,
                score: cost2[a][b as usize],
            });
        }
    }
    relational_refine(matches2, &fa, &fb, &cost2, &transforms)
}

// ---- trajectory uncrossing — verbatim port of tweens.js's uncrossMatches
// (2026-07-17, "les yeux s'inversent"; see the JS comment for the full
// rationale): two nearly-identical strokes whose straight-line centroid
// trajectories intersect are almost always a matching error (cel features
// keep their spatial arrangement) — swap the B partners when the swapped
// pairing doesn't cost meaningfully more than the crossed one. A genuine
// crossing (different-colored objects passing) survives via the
// color-clash/type penalties, far above the tolerance.
pub(super) fn segs_intersect(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    p4: (f64, f64),
) -> bool {
    fn ccw(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
        (c.1 - a.1) * (b.0 - a.0) > (b.1 - a.1) * (c.0 - a.0)
    }
    ccw(p1, p3, p4) != ccw(p2, p3, p4) && ccw(p1, p2, p3) != ccw(p1, p2, p4)
}
const UNCROSS_TOL: f64 = 0.08;
// Port of _orderReversed (tweens.js) — true when the vector joining two A
// features points more than 120 deg away from the vector joining their
// assigned B counterparts, i.e. the pair traded places.
const ORDER_ANCHOR: f64 = 0.25;
pub(super) fn order_reversed(
    a1: (f64, f64),
    a2: (f64, f64),
    b1: (f64, f64),
    b2: (f64, f64),
) -> bool {
    let (ax, ay) = (a2.0 - a1.0, a2.1 - a1.1);
    let (bx, by) = (b2.0 - b1.0, b2.1 - b1.1);
    let la = (ax * ax + ay * ay).sqrt();
    let lb = (bx * bx + by * by).sqrt();
    if la < 1e-6 || lb < 1e-6 {
        return false;
    }
    (ax * bx + ay * by) / (la * lb) < -0.5
}
pub(super) fn uncross_matches(ms: &mut [MatchOut], fa: &[Feat], fb: &[Feat], match_norm: f64) {
    if ms.len() < 2 {
        return;
    }
    for _sweep in 0..4 {
        let mut swapped = false;
        for i in 0..ms.len() {
            for j in (i + 1)..ms.len() {
                let (m1a, m1b) = (ms[i].a, ms[i].b);
                let (m2a, m2b) = (ms[j].a, ms[j].b);
                let a1 = (fa[m1a].cx, fa[m1a].cy);
                let b1 = (fb[m1b].cx, fb[m1b].cy);
                let a2 = (fa[m2a].cx, fa[m2a].cy);
                let b2 = (fb[m2b].cx, fb[m2b].cy);
                // Near-twin widened tolerance — verbatim port of tweens.js's
                // uncrossMatches (2026-07, "un oeil est mal reconnu"; see the
                // JS comment for the measured rationale).
                let lr1 = fa[m1a].length.max(fa[m2a].length)
                    / fa[m1a].length.min(fa[m2a].length).max(1.0);
                let lr2 = fb[m1b].length.max(fb[m2b].length)
                    / fb[m1b].length.min(fb[m2b].length).max(1.0);
                let twin1 = lr1 < 1.5 || (fa[m1a].length - fa[m2a].length).abs() < 15.0;
                let twin2 = lr2 < 1.5 || (fb[m1b].length - fb[m2b].length).abs() < 15.0;
                let twins = fa[m1a].stype == fa[m2a].stype
                    && fb[m1b].stype == fb[m2b].stype
                    && twin1
                    && twin2;
                // Order-reversal gate — port of _orderReversed (tweens.js); see
                // that comment for the measured tie that motivated it, and for
                // why ORDER_ANCHOR guards it. The crossing test alone is blind
                // when the motion exceeds the spacing between two features: the
                // trajectories then run parallel however badly they are
                // swapped. The anchor stops this indirect signal from
                // overriding an assignment that already has strong direct
                // evidence.
                let cur1 = match_sc(&fa[m1a], &fb[m1b], m1a == m1b, None, match_norm);
                let cur2 = match_sc(&fa[m2a], &fb[m2b], m2a == m2b, None, match_norm);
                if !segs_intersect(a1, b1, a2, b2)
                    && !(twins && cur1.min(cur2) > ORDER_ANCHOR && order_reversed(a1, a2, b1, b2))
                {
                    continue;
                }
                let cur = cur1 + cur2;
                let swp = match_sc(&fa[m1a], &fb[m2b], m1a == m2b, None, match_norm)
                    + match_sc(&fa[m2a], &fb[m1b], m2a == m1b, None, match_norm);
                let tol = if twins { 0.25 } else { UNCROSS_TOL };
                if swp <= cur + tol {
                    ms[i].b = m2b;
                    ms[j].b = m1b;
                    ms[i].score = match_sc(&fa[m1a], &fb[m2b], m1a == m2b, None, match_norm);
                    ms[j].score = match_sc(&fa[m2a], &fb[m1b], m2a == m1b, None, match_norm);
                    swapped = true;
                }
            }
        }
        if !swapped {
            break;
        }
    }
}
