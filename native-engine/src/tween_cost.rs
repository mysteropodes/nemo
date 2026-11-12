//! #2358: tween cost; adapted from geometry-wasm at 46442ea9.
use crate::tween_kernels::*;

pub(super) fn match_sc(
    fa: &Feat,
    fb: &Feat,
    same_index: bool,
    a_pts_override: Option<&[(f64, f64)]>,
    match_norm: f64,
) -> f64 {
    let pts = a_pts_override.unwrap_or(&fa.pts);
    let k = pts.len().min(fb.pts.len());

    let mut sum_ab = 0.0;
    for i in 0..k {
        let mut best = f64::MAX;
        for j in 0..k {
            let (dx, dy) = (pts[i].0 - fb.pts[j].0, pts[i].1 - fb.pts[j].1);
            let d = dx * dx + dy * dy;
            if d < best {
                best = d;
            }
        }
        sum_ab += best.sqrt();
    }
    let mut sum_ba = 0.0;
    for j in 0..k {
        let mut best = f64::MAX;
        for i in 0..k {
            let (dx, dy) = (pts[i].0 - fb.pts[j].0, pts[i].1 - fb.pts[j].1);
            let d = dx * dx + dy * dy;
            if d < best {
                best = d;
            }
        }
        sum_ba += best.sqrt();
    }
    let cham = (sum_ab + sum_ba) / (2.0 * k as f64);

    let (mut fwd, mut rev) = (0.0, 0.0);
    for kk in 0..k {
        let (dxf, dyf) = (pts[kk].0 - fb.pts[kk].0, pts[kk].1 - fb.pts[kk].1);
        fwd += (dxf * dxf + dyf * dyf).sqrt();
        let (dxr, dyr) = (
            pts[kk].0 - fb.pts[k - 1 - kk].0,
            pts[kk].1 - fb.pts[k - 1 - kk].1,
        );
        rev += (dxr * dxr + dyr * dyr).sqrt();
    }
    let alg = fwd.min(rev) / k as f64;

    let da = (fa.bounds.width().powi(2) + fa.bounds.height().powi(2)).sqrt();
    let db = (fb.bounds.width().powi(2) + fb.bounds.height().powi(2)).sqrt();
    let scale_ab = (da + db) / 2.0 + match_norm * 0.04 + 1.0;
    let prox_t = cham / (cham + scale_ab * 0.5);
    let align_t = alg / (alg + scale_ab);

    let nt = fa.turn.len().min(fb.turn.len());
    let (mut cf, mut cr) = (0.0, 0.0);
    for q in 0..nt {
        cf += (fa.turn[q] - fb.turn[q]).abs();
        cr += (fa.turn[q] + fb.turn[nt - 1 - q]).abs();
    }
    let curve_t = if nt > 0 {
        (cf.min(cr) / nt as f64 / std::f64::consts::PI).min(1.0)
    } else {
        0.0
    };
    let four_d = fourier_dist(&fa.fourier, &fb.fourier);

    let (rdx, rdy) = (fa.rel_x - fb.rel_x, fa.rel_y - fb.rel_y);
    let rel = (rdx * rdx + rdy * rdy).sqrt().min(1.0);
    let len_ratio = fa.length.max(fb.length) / fa.length.min(fb.length).max(1.0);
    // Recalibrated 2026-07 — verbatim port of tweens.js's ratioPen (see the
    // JS comment for the measured rationale): ramp from 1.6 instead of 2.0,
    // gated on >15px absolute difference so micro-stroke pen noise stays
    // exempt.
    let ratio_pen = if len_ratio > 1.6 && (fa.length - fb.length).abs() > 15.0 {
        ((len_ratio - 1.6) * 0.5).min(0.7)
    } else {
        0.0
    };
    // 0.35 only when BOTH flags are ground truth (plain strokes' real
    // sd.closed). When either side is a heuristic guess (vector-brush
    // centerline), a disagreement is as likely a drawing accident as a real
    // topological difference — measured live: the SAME arm redrawn between
    // two keys scored 0.551 (rejected at the 0.48 threshold) purely from
    // this penalty, leaving the limb to fade/trim as two unrelated strokes
    // instead of swinging. Softened to a nudge there; every other cost
    // term still separates genuinely-different shapes.
    let length_identity = if (fa.length - fb.length).abs() > 15.0 {
        ((len_ratio.ln() - 1.3_f64.ln()).max(0.0) / 2.0_f64.ln()).min(1.0) * 0.15
    } else {
        0.0
    };
    let closed_pen = if fa.closed != fb.closed {
        if fa.closed_is_guess || fb.closed_is_guess {
            0.12
        } else {
            0.35
        }
    } else {
        0.0
    };
    let (a_area, b_area) = (
        fa.bounds.width() * fa.bounds.height(),
        fb.bounds.width() * fb.bounds.height(),
    );
    let sz_d = (a_area - b_area).abs() / a_area.max(b_area).max(1.0);
    let col_d =
        (color_dist(&fa.stroke_col, &fb.stroke_col) + color_dist(&fa.fill_col, &fb.fill_col)) / 2.0;
    let type_penalty = if fa.stype != fb.stype { 0.5 } else { 0.0 };
    // HARD color-identity penalty — verbatim port of matchSc's colorPenalty
    // (tweens.js, 2026-07-17): a clearly-different hue on the same channel
    // gets the same flat-penalty treatment a type mismatch already has, so
    // two same-shape strokes of different colors crossing paths follow
    // their color identity instead of standing still and hue-swapping at
    // the tween midpoint. See the JS comment for the full rationale.
    let fill_clash = fa.fill_col.is_some()
        && fb.fill_col.is_some()
        && color_dist(&fa.fill_col, &fb.fill_col) > 0.35;
    let stroke_clash = fa.stroke_col.is_some()
        && fb.stroke_col.is_some()
        && color_dist(&fa.stroke_col, &fb.stroke_col) > 0.35;
    let color_penalty = if fill_clash || stroke_clash { 0.4 } else { 0.0 };
    let idx_bonus = if same_index { -0.03 } else { 0.0 };

    prox_t * 0.48
        + align_t * 0.15
        + curve_t * 0.12
        + four_d * 0.10
        + rel * 0.10
        + sz_d * 0.06
        + col_d * 0.15
        + type_penalty
        + color_penalty
        + ratio_pen
        + length_identity
        + closed_pen
        + idx_bonus
}
