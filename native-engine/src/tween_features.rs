//! #2358: tween features; adapted from geometry-wasm at 46442ea9.
use super::*;

pub(super) fn parse_hex_color(css: &Option<String>) -> Option<(f64, f64, f64)> {
    let s = css.as_ref()?;
    let h = s.trim_start_matches('#');
    let h6 = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        h.to_string()
    };
    if h6.len() != 6 && h6.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&h6[0..2], 16).ok()? as f64;
    let g = u8::from_str_radix(&h6[2..4], 16).ok()? as f64;
    let b = u8::from_str_radix(&h6[4..6], 16).ok()? as f64;
    Some((r, g, b))
}
pub(super) fn color_dist(a: &Option<(f64, f64, f64)>, b: &Option<(f64, f64, f64)>) -> f64 {
    match (a, b) {
        (None, None) => 0.0,
        (Some(_), None) | (None, Some(_)) => 1.0,
        (Some(a), Some(b)) => {
            let (dr, dg, db) = (a.0 - b.0, a.1 - b.1, a.2 - b.2);
            (dr * dr + dg * dg + db * db).sqrt() / 441.672_955_930_063_7
        }
    }
}
pub(super) fn stroke_type(s: &StrokeIn) -> &'static str {
    if is_vb(s) {
        return "vb";
    }
    let has_s = s.has_real_stroke != Some(false) && s.stroke_color.is_some();
    let has_f = s.fill_color.is_some();
    if has_s && has_f {
        "both"
    } else if has_f {
        "fill"
    } else {
        "stroke"
    }
}

// ---- strokeFeat ----
pub(super) struct Feat {
    pub(super) cx: f64,
    pub(super) cy: f64,
    pub(super) length: f64,
    #[allow(dead_code)]
    pub(super) dir: (f64, f64),
    pub(super) bounds: Rect,
    #[allow(dead_code)]
    pub(super) shape: Vec<(f64, f64)>,
    pub(super) pts: Vec<(f64, f64)>,
    pub(super) turn: Vec<f64>,
    pub(super) closed: bool,
    pub(super) closed_is_guess: bool,
    pub(super) stroke_col: Option<(f64, f64, f64)>,
    pub(super) fill_col: Option<(f64, f64, f64)>,
    pub(super) stype: &'static str,
    pub(super) rel_x: f64,
    pub(super) rel_y: f64,
    pub(super) fourier: Vec<f64>,
}
// Mirrors fourierDescriptor/fourierDist in tweens.js — see that function's
// own comment for the rationale (style-agnostic silhouette descriptor via
// low-frequency DFT magnitudes, invariant to translation/rotation/scale).
const FOURIER_BINS: usize = 6;
pub(super) fn fourier_descriptor(pts: &[(f64, f64)], cx: f64, cy: f64) -> Vec<f64> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let mut re = vec![0.0; FOURIER_BINS + 1];
    let mut im = vec![0.0; FOURIER_BINS + 1];
    for k in 0..n {
        let (x, y) = (pts[k].0 - cx, pts[k].1 - cy);
        for m in 1..=FOURIER_BINS {
            let ang = -2.0 * std::f64::consts::PI * m as f64 * k as f64 / n as f64;
            let (c, s) = (ang.cos(), ang.sin());
            re[m] += x * c - y * s;
            im[m] += x * s + y * c;
        }
    }
    let mags: Vec<f64> = (1..=FOURIER_BINS)
        .map(|m| (re[m] * re[m] + im[m] * im[m]).sqrt())
        .collect();
    let norm = mags.iter().map(|v| v * v).sum::<f64>().sqrt();
    let norm = if norm > 0.0 { norm } else { 1.0 };
    mags.iter().map(|v| v / norm).collect()
}
pub(super) fn fourier_dist(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let n = a.len().min(b.len());
    let mut s = 0.0;
    for i in 0..n {
        let d = a[i] - b[i];
        s += d * d;
    }
    s.sqrt().min(1.0)
}
pub(super) fn stroke_feat(s: &StrokeIn) -> Feat {
    let path = build_feat_path(s);
    let sampler = PathSampler::new(&path);
    let b = path.bounding_box();
    let len = sampler.total;

    const N_S: usize = 12;
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..N_S {
        let p = sampler.point_at(i as f64 / (N_S - 1) as f64 * len);
        cx += p.x;
        cy += p.y;
    }
    cx /= N_S as f64;
    cy /= N_S as f64;

    let n_points = if is_vb(s) {
        s.center_segments.as_ref().unwrap().len()
    } else {
        s.segments.len()
    };
    let first = sampler.point_at(0.0);
    let last = sampler.point_at(len);
    let (mut dx, mut dy) = (last.x - first.x, last.y - first.y);
    let dl = (dx * dx + dy * dy).sqrt();
    if dl > 0.0 {
        dx /= dl;
        dy /= dl;
    }

    const N_SHAPE: usize = 8;
    let mut shape = Vec::with_capacity(N_SHAPE);
    for i in 0..N_SHAPE {
        let p = sampler.point_at(i as f64 / (N_SHAPE - 1) as f64 * len);
        shape.push((
            (p.x - cx) / b.width().max(1.0),
            (p.y - cy) / b.height().max(1.0),
        ));
    }

    const K: usize = 16;
    let mut pts = Vec::with_capacity(K);
    for i in 0..K {
        let p = sampler.point_at(i as f64 / (K - 1) as f64 * len);
        pts.push((p.x, p.y));
    }
    let mut turn = Vec::with_capacity(K.saturating_sub(2));
    for i in 1..K - 1 {
        let (v1x, v1y) = (pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1);
        let (v2x, v2y) = (pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1);
        let cross = v1x * v2y - v1y * v2x;
        let dot = v1x * v2x + v1y * v2y;
        let dot = if dot == 0.0 { 1e-9 } else { dot };
        turn.push(cross.atan2(dot));
    }

    let diag = (b.width() * b.width() + b.height() * b.height()).sqrt();
    let closed_heuristic = n_points > 3 && dl < 4.0_f64.max(diag * 0.08) && len > diag * 1.2;
    // Mirrors strokeFeat's own comment in tweens.js: trust the real
    // open/closed flag over a geometric guess, except for vector-brush
    // strokes whose feature path is the drawn centerline (never actually
    // closed) rather than the always-closed rendered ribbon outline s.closed
    // refers to.
    let closed = if !is_vb(s) {
        s.closed
    } else {
        closed_heuristic
    };
    // A vector-brush centerline's closed flag is a geometric GUESS (the
    // heuristic above), not ground truth — two hand-drawn versions of the
    // same limb can flip it (measured on a real animation: the same arm
    // redrawn between keys read open in one key, closed in the other).
    let closed_is_guess = is_vb(s);
    let fourier = fourier_descriptor(&pts, cx, cy);

    Feat {
        cx,
        cy,
        length: len,
        dir: (dx, dy),
        bounds: b,
        shape,
        pts,
        turn,
        closed,
        closed_is_guess,
        stroke_col: parse_hex_color(&if s.has_real_stroke == Some(false) {
            None
        } else {
            s.stroke_color.clone()
        }),
        fill_col: parse_hex_color(&s.fill_color),
        stype: stroke_type(s),
        rel_x: 0.0,
        rel_y: 0.0,
        fourier,
    }
}
pub(super) fn union_bounds(feats: &[Feat]) -> (f64, f64, f64, f64) {
    let (mut x1, mut y1, mut x2, mut y2) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for f in feats {
        x1 = x1.min(f.bounds.x0);
        y1 = y1.min(f.bounds.y0);
        x2 = x2.max(f.bounds.x1);
        y2 = y2.max(f.bounds.y1);
    }
    if x1 > x2 {
        (0.0, 0.0, 1.0, 1.0)
    } else {
        (x1, y1, (x2 - x1).max(1.0), (y2 - y1).max(1.0))
    }
}

// ---- matchSc ----
