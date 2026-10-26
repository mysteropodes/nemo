//! Baseline forced-pair admission, plausible-match threshold and locally supported rescue.
use super::*;
use crate::object_document::{tween::TweenOverride, ObjectRecord};
#[derive(Clone)]
pub(super) struct PairSpec {
    pub a: usize,
    pub b: usize,
    pub a_data: ObjectRecord,
    pub b_data: ObjectRecord,
    pub mi: i32,
    pub score: f64,
    pub forced: bool,
    pub piece: bool,
}
fn spec(
    a: &[ObjectRecord],
    b: &[ObjectRecord],
    ai: usize,
    bi: usize,
    mi: i32,
    score: f64,
    forced: bool,
) -> PairSpec {
    PairSpec {
        a: ai,
        b: bi,
        a_data: a[ai].clone(),
        b_data: b[bi].clone(),
        mi,
        score,
        forced,
        piece: false,
    }
}
pub(super) fn pair_plan(
    a: &[ObjectRecord],
    b: &[ObjectRecord],
    overrides: &[TweenOverride],
) -> R<(Vec<PairSpec>, Vec<usize>, Vec<usize>)> {
    let (mut used_a, mut used_b) = (vec![false; a.len()], vec![false; b.len()]);
    let mut pairs = vec![];
    for ov in overrides {
        if let Some(ids) = ov.a_ids.value() {
            // Multi-source splitting is added through the same pure piece planner below.
            let mut indices: Vec<_> = ids
                .iter()
                .filter_map(|id| a.iter().position(|r| r.target.stroke_id == *id))
                .collect();
            indices.dedup();
            let Some(bi) = b.iter().position(|r| r.target.stroke_id == ov.b_id) else {
                continue;
            };
            indices.retain(|&i| !used_a[i]);
            if used_b[bi] || indices.is_empty() {
                continue;
            }
            if indices.len() > 1 {
                return Err(crate::request_receipts::DispatchErrorCode::Unavailable);
            }
            let ai = indices[0];
            used_a[ai] = true;
            used_b[bi] = true;
            pairs.push(spec(a, b, ai, bi, -1 - pairs.len() as i32, 0., true));
            continue;
        }
        let Some(a_id) = ov.a_id.value() else {
            continue;
        };
        let ai = a
            .iter()
            .position(|r| r.target.stroke_id == *a_id || r.target.stroke_id == ov.b_id);
        let bi = b
            .iter()
            .position(|r| r.target.stroke_id == ov.b_id || r.target.stroke_id == *a_id);
        let (Some(ai), Some(bi)) = (ai, bi) else {
            continue;
        };
        if used_a[ai]
            || used_b[bi]
            || a.iter()
                .enumerate()
                .any(|(i, r)| i != ai && r.target.stroke_id == ov.b_id)
            || b.iter()
                .enumerate()
                .any(|(i, r)| i != bi && r.target.stroke_id == *a_id)
        {
            continue;
        }
        used_a[ai] = true;
        used_b[bi] = true;
        pairs.push(spec(a, b, ai, bi, -1 - pairs.len() as i32, 0., true));
    }
    let sa: Vec<_> = a.iter().map(stroke).collect();
    let sb: Vec<_> = b.iter().map(stroke).collect();
    let matches = auto_match_inner(&sa, &sb);
    let forced_a = used_a.clone();
    let forced_b = used_b.clone();
    for (i, m) in matches.iter().enumerate() {
        if forced_a[m.a] || forced_b[m.b] || m.score > 0.48 {
            continue;
        }
        used_a[m.a] = true;
        used_b[m.b] = true;
        pairs.push(spec(a, b, m.a, m.b, i as i32, m.score, false));
    }
    let ceiling = if a.len().max(b.len()) as f64 / a.len().min(b.len()).max(1) as f64 >= 3. {
        0.58
    } else {
        0.78
    };
    let mut candidates = vec![];
    for (i, m) in matches.iter().enumerate() {
        if m.score <= 0.48
            || m.score > ceiling
            || forced_a[m.a]
            || forced_b[m.b]
            || used_a[m.a]
            || used_b[m.b]
        {
            continue;
        }
        let fa = stroke_feat(&sa[m.a]);
        let fb = stroke_feat(&sb[m.b]);
        let clash = fa.fill_col.is_some()
            && fb.fill_col.is_some()
            && color_dist(&fa.fill_col, &fb.fill_col) > 0.35
            || fa.stroke_col.is_some()
                && fb.stroke_col.is_some()
                && color_dist(&fa.stroke_col, &fb.stroke_col) > 0.35;
        if fa.stype == fb.stype
            && (fa.closed == fb.closed || fa.closed_is_guess || fb.closed_is_guess)
            && !clash
        {
            candidates.push((i, m, fa, fb));
        }
    }
    let mut displacements: Vec<_> = pairs
        .iter()
        .map(|p| {
            let a = stroke_feat(&stroke(&p.a_data));
            let b = stroke_feat(&stroke(&p.b_data));
            (a.cx, a.cy, b.cx - a.cx, b.cy - a.cy, None)
        })
        .collect();
    displacements.extend(
        candidates
            .iter()
            .enumerate()
            .map(|(i, (_, _, a, b))| (a.cx, a.cy, b.cx - a.cx, b.cy - a.cy, Some(i))),
    );
    for (ci, (i, m, fa, fb)) in candidates.iter().enumerate() {
        let (dx, dy) = (fb.cx - fa.cx, fb.cy - fa.cy);
        let disp = (dx * dx + dy * dy).sqrt();
        if disp > fa.length.max(fb.length) {
            let mut near: Vec<_> = displacements.iter().filter(|r| r.4 != Some(ci)).collect();
            near.sort_by(|a, b| {
                ((a.0 - fa.cx).powi(2) + (a.1 - fa.cy).powi(2))
                    .total_cmp(&((b.0 - fa.cx).powi(2) + (b.1 - fa.cy).powi(2)))
            });
            if !near.iter().take(3).any(|r| {
                ((r.2 - dx).powi(2) + (r.3 - dy).powi(2)).sqrt()
                    <= 0.35 * disp.max((r.2 * r.2 + r.3 * r.3).sqrt()) + 15.
            }) {
                continue;
            }
        }
        used_a[m.a] = true;
        used_b[m.b] = true;
        pairs.push(spec(a, b, m.a, m.b, *i as i32, m.score, false));
    }
    let un_a = (0..a.len()).filter(|&i| !used_a[i]).collect();
    let un_b = (0..b.len()).filter(|&i| !used_b[i]).collect();
    cohere(&mut pairs);
    Ok((pairs, un_a, un_b))
}
fn center(r: &ObjectRecord) -> (f64, f64) {
    let s = stroke(r);
    let points: Vec<_> = if s.center_segments.as_ref().is_some_and(|s| s.len() > 1) {
        s.center_segments.unwrap().iter().map(|s| s.point).collect()
    } else {
        s.segments.iter().map(|s| s.point).collect()
    };
    let sum = points
        .iter()
        .fold((0., 0.), |a, p| (a.0 + p[0], a.1 + p[1]));
    (
        sum.0 / points.len().max(1) as f64,
        sum.1 / points.len().max(1) as f64,
    )
}
fn crossing(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let cr = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let opposite = |x: f64, y: f64| (x > 0. && y < 0.) || (x < 0. && y > 0.);
    opposite(cr(c, d, a), cr(c, d, b)) && opposite(cr(a, b, c), cr(a, b, d))
}
fn coherence_cost(a: &Feat, b: &Feat) -> f64 {
    let k = a.pts.len().min(b.pts.len());
    let sum = |a: &Feat, b: &Feat| {
        (0..k)
            .map(|i| {
                (0..k)
                    .map(|j| (a.pts[i].0 - b.pts[j].0).powi(2) + (a.pts[i].1 - b.pts[j].1).powi(2))
                    .fold(1e18, f64::min)
                    .sqrt()
            })
            .sum::<f64>()
    };
    let scale =
        a.bounds.width().hypot(a.bounds.height()) + b.bounds.width().hypot(b.bounds.height());
    let cham = (sum(a, b) + sum(b, a)) / (2 * k) as f64 / (scale * 0.5).max(1.);
    cham + (a.length.max(b.length) / a.length.min(b.length).max(1.) - 1.3).max(0.) * 0.3
        + if a.stype != b.stype { 0.5 } else { 0. }
        + (color_dist(&a.stroke_col, &b.stroke_col) + color_dist(&a.fill_col, &b.fill_col)) / 2.
            * 0.3
}
fn cohere(pairs: &mut [PairSpec]) {
    for i in 0..pairs.len() {
        if pairs[i].piece || pairs[i].forced {
            continue;
        }
        for j in i + 1..pairs.len() {
            if pairs[j].piece || pairs[j].forced {
                continue;
            }
            let p = &pairs[i];
            let q = &pairs[j];
            let (pa, pb, qa, qb) = (
                center(&p.a_data),
                center(&p.b_data),
                center(&q.a_data),
                center(&q.b_data),
            );
            if !crossing(pa, pb, qa, qb) || crossing(pa, qb, qa, pb) {
                continue;
            }
            let (ap, bp, aq, bq) = (
                stroke_feat(&stroke(&p.a_data)),
                stroke_feat(&stroke(&p.b_data)),
                stroke_feat(&stroke(&q.a_data)),
                stroke_feat(&stroke(&q.b_data)),
            );
            if coherence_cost(&ap, &bq) + coherence_cost(&aq, &bp)
                > coherence_cost(&ap, &bp) + coherence_cost(&aq, &bq) + 0.15
            {
                continue;
            }
            let (left, right) = pairs.split_at_mut(j);
            std::mem::swap(&mut left[i].b, &mut right[0].b);
            std::mem::swap(&mut left[i].b_data, &mut right[0].b_data);
        }
    }
}
