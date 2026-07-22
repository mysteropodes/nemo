// Phase C4b — tween auto-matching, ported verbatim (formula-for-formula,
// including the tuned constants) from strokeFeat/matchSc/hungarian/
// fitSimilarityTransform/autoMatch/resampleP/resampleCenterline/
// alignResampledPair in src/js/tweens.js. See the migration plan for why
// this isn't a redesign: the JS already implements the "line of force"
// motion model (fitSimilarityTransform's own comment names it), and the
// FADE_COST dummy-assignment mechanism already refuses to force-match
// strokes that don't correspond — both explicit user requirements, both
// pre-existing. This module only moves the same math to Rust.
use vello::kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathSeg, Point, Rect, Shape};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ---- shared stroke shape (mirrors the app's frame.strokes[i] records) ----
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegIn {
    pub point: [f64; 2],
    #[serde(default)]
    pub handle_in: [f64; 2],
    #[serde(default)]
    pub handle_out: [f64; 2],
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CenterSegIn {
    pub point: [f64; 2],
    #[serde(default)]
    pub handle_in: [f64; 2],
    #[serde(default)]
    pub handle_out: [f64; 2],
    #[serde(default = "default_width")]
    pub width: f64,
}
fn default_width() -> f64 {
    3.0
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StrokeIn {
    #[serde(default)]
    pub segments: Vec<SegIn>,
    pub center_segments: Option<Vec<CenterSegIn>>,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    #[serde(default)]
    pub is_vector_brush: bool,
    #[serde(default)]
    pub closed: bool,
}

fn is_vb(s: &StrokeIn) -> bool {
    s.is_vector_brush && s.center_segments.as_ref().map_or(false, |c| c.len() > 1)
}

// ---- arc-length sampling over a whole (possibly multi-segment) BezPath ----
struct PathSampler {
    segs: Vec<PathSeg>,
    cum_lens: Vec<f64>,
    seg_lens: Vec<f64>,
    total: f64,
}
impl PathSampler {
    fn new(path: &BezPath) -> Self {
        let segs: Vec<PathSeg> = path.segments().collect();
        let mut cum_lens = Vec::with_capacity(segs.len());
        let mut seg_lens = Vec::with_capacity(segs.len());
        let mut cum = 0.0;
        for s in &segs {
            cum_lens.push(cum);
            let l = s.arclen(0.1);
            seg_lens.push(l);
            cum += l;
        }
        PathSampler { segs, cum_lens, seg_lens, total: cum }
    }
    fn point_at(&self, dist: f64) -> Point {
        if self.segs.is_empty() {
            return Point::ZERO;
        }
        let d = dist.clamp(0.0, self.total);
        let mut idx = self.segs.len() - 1;
        for i in 0..self.segs.len() {
            if d < self.cum_lens[i] + self.seg_lens[i] {
                idx = i;
                break;
            }
        }
        let local = (d - self.cum_lens[idx]).max(0.0);
        let t = if self.seg_lens[idx] > 1e-9 { self.segs[idx].inv_arclen(local, 0.1) } else { 0.0 };
        self.segs[idx].eval(t)
    }
    fn tangent_at(&self, dist: f64) -> (f64, f64) {
        if self.total < 1e-9 {
            return (1.0, 0.0);
        }
        let eps = (self.total * 0.001).max(0.01);
        let p0 = self.point_at((dist - eps).max(0.0));
        let p1 = self.point_at((dist + eps).min(self.total));
        let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            (1.0, 0.0)
        } else {
            (dx / len, dy / len)
        }
    }
}

// Bug found by stress-testing (2026-07-17): never calling `close_path()`
// here meant every downstream feature (stroke_feat's dense samples,
// turning-angle profile, centroid, Fourier descriptor — AND
// resample_stroke_inner's own resampling, which shares this same path
// builder) measured a CLOSED shape's own length short by its implicit
// closing segment — the same root cause fixed in tweens.js's
// buildTPFeat/resamplePJS/resamplePairFeatureAware, but here it's the
// WASM path real users actually hit (tried before the JS fallback). A
// vector-brush centerline stays open regardless (it's the drawn stroke,
// never a closed loop even when its rendered ribbon outline is) — mirrors
// stroke_feat's own `is_vb(s)` exemption a few lines below.
fn build_feat_path(s: &StrokeIn) -> BezPath {
    let mut path = BezPath::new();
    let vb = is_vb(s);
    if vb {
        let cs = s.center_segments.as_ref().unwrap();
        add_segs(&mut path, cs.iter().map(|c| (c.point, c.handle_in, c.handle_out)));
    } else {
        add_segs(&mut path, s.segments.iter().map(|c| (c.point, c.handle_in, c.handle_out)));
        if s.closed {
            path.close_path();
        }
    }
    path
}
fn add_segs(path: &mut BezPath, iter: impl Iterator<Item = ([f64; 2], [f64; 2], [f64; 2])>) {
    let items: Vec<_> = iter.collect();
    if items.is_empty() {
        return;
    }
    path.move_to((items[0].0[0], items[0].0[1]));
    for i in 0..items.len() - 1 {
        let (a_pt, _a_in, a_out) = items[i];
        let (b_pt, b_in, _b_out) = items[i + 1];
        let c1 = (a_pt[0] + a_out[0], a_pt[1] + a_out[1]);
        let c2 = (b_pt[0] + b_in[0], b_pt[1] + b_in[1]);
        path.curve_to(c1, c2, (b_pt[0], b_pt[1]));
    }
}

// ---- color parsing / distance (parseHexColor / colorDist) ----
fn parse_hex_color(css: &Option<String>) -> Option<(f64, f64, f64)> {
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
fn color_dist(a: &Option<(f64, f64, f64)>, b: &Option<(f64, f64, f64)>) -> f64 {
    match (a, b) {
        (None, None) => 0.0,
        (Some(_), None) | (None, Some(_)) => 1.0,
        (Some(a), Some(b)) => {
            let (dr, dg, db) = (a.0 - b.0, a.1 - b.1, a.2 - b.2);
            (dr * dr + dg * dg + db * db).sqrt() / 441.672_955_930_063_7
        }
    }
}
fn stroke_type(s: &StrokeIn) -> &'static str {
    if is_vb(s) {
        return "vb";
    }
    let has_s = s.stroke_color.is_some();
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
struct Feat {
    cx: f64,
    cy: f64,
    length: f64,
    #[allow(dead_code)]
    dir: (f64, f64),
    bounds: Rect,
    #[allow(dead_code)]
    shape: Vec<(f64, f64)>,
    pts: Vec<(f64, f64)>,
    turn: Vec<f64>,
    closed: bool,
    closed_is_guess: bool,
    stroke_col: Option<(f64, f64, f64)>,
    fill_col: Option<(f64, f64, f64)>,
    stype: &'static str,
    rel_x: f64,
    rel_y: f64,
    fourier: Vec<f64>,
}
// Mirrors fourierDescriptor/fourierDist in tweens.js — see that function's
// own comment for the rationale (style-agnostic silhouette descriptor via
// low-frequency DFT magnitudes, invariant to translation/rotation/scale).
const FOURIER_BINS: usize = 6;
fn fourier_descriptor(pts: &[(f64, f64)], cx: f64, cy: f64) -> Vec<f64> {
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
    let mags: Vec<f64> = (1..=FOURIER_BINS).map(|m| (re[m] * re[m] + im[m] * im[m]).sqrt()).collect();
    let norm = mags.iter().map(|v| v * v).sum::<f64>().sqrt();
    let norm = if norm > 0.0 { norm } else { 1.0 };
    mags.iter().map(|v| v / norm).collect()
}
fn fourier_dist(a: &[f64], b: &[f64]) -> f64 {
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
fn stroke_feat(s: &StrokeIn) -> Feat {
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

    let n_points = if is_vb(s) { s.center_segments.as_ref().unwrap().len() } else { s.segments.len() };
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
        shape.push(((p.x - cx) / b.width().max(1.0), (p.y - cy) / b.height().max(1.0)));
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
    let closed = if !is_vb(s) { s.closed } else { closed_heuristic };
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
        stroke_col: parse_hex_color(&s.stroke_color),
        fill_col: parse_hex_color(&s.fill_color),
        stype: stroke_type(s),
        rel_x: 0.0,
        rel_y: 0.0,
        fourier,
    }
}
fn union_bounds(feats: &[Feat]) -> (f64, f64, f64, f64) {
    let (mut x1, mut y1, mut x2, mut y2) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
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
fn match_sc(fa: &Feat, fb: &Feat, same_index: bool, a_pts_override: Option<&[(f64, f64)]>, match_norm: f64) -> f64 {
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
        let (dxr, dyr) = (pts[kk].0 - fb.pts[k - 1 - kk].0, pts[kk].1 - fb.pts[k - 1 - kk].1);
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
    let curve_t = if nt > 0 { (cf.min(cr) / nt as f64 / std::f64::consts::PI).min(1.0) } else { 0.0 };
    let four_d = fourier_dist(&fa.fourier, &fb.fourier);

    let (rdx, rdy) = (fa.rel_x - fb.rel_x, fa.rel_y - fb.rel_y);
    let rel = (rdx * rdx + rdy * rdy).sqrt().min(1.0);
    let len_ratio = fa.length.max(fb.length) / fa.length.min(fb.length).max(1.0);
    let ratio_pen = if len_ratio > 2.0 { ((len_ratio - 2.0) * 0.35).min(0.7) } else { 0.0 };
    // 0.35 only when BOTH flags are ground truth (plain strokes' real
    // sd.closed). When either side is a heuristic guess (vector-brush
    // centerline), a disagreement is as likely a drawing accident as a real
    // topological difference — measured live: the SAME arm redrawn between
    // two keys scored 0.551 (rejected at the 0.48 threshold) purely from
    // this penalty, leaving the limb to fade/trim as two unrelated strokes
    // instead of swinging. Softened to a nudge there; every other cost
    // term still separates genuinely-different shapes.
    let closed_pen = if fa.closed != fb.closed {
        if fa.closed_is_guess || fb.closed_is_guess { 0.12 } else { 0.35 }
    } else { 0.0 };
    let (a_area, b_area) = (fa.bounds.width() * fa.bounds.height(), fb.bounds.width() * fb.bounds.height());
    let sz_d = (a_area - b_area).abs() / a_area.max(b_area).max(1.0);
    let col_d = (color_dist(&fa.stroke_col, &fb.stroke_col) + color_dist(&fa.fill_col, &fb.fill_col)) / 2.0;
    let type_penalty = if fa.stype != fb.stype { 0.5 } else { 0.0 };
    // HARD color-identity penalty — verbatim port of matchSc's colorPenalty
    // (tweens.js, 2026-07-17): a clearly-different hue on the same channel
    // gets the same flat-penalty treatment a type mismatch already has, so
    // two same-shape strokes of different colors crossing paths follow
    // their color identity instead of standing still and hue-swapping at
    // the tween midpoint. See the JS comment for the full rationale.
    let fill_clash = fa.fill_col.is_some() && fb.fill_col.is_some() && color_dist(&fa.fill_col, &fb.fill_col) > 0.35;
    let stroke_clash = fa.stroke_col.is_some() && fb.stroke_col.is_some() && color_dist(&fa.stroke_col, &fb.stroke_col) > 0.35;
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
        + closed_pen
        + idx_bonus
}

// ---- "force line" similarity transform (fitSimilarityTransform) ----
struct SimT {
    w_re: f64,
    w_im: f64,
    ca: (f64, f64),
    cb: (f64, f64),
}
fn fit_similarity_transform(pts_a: &[(f64, f64)], pts_b: &[(f64, f64)]) -> Option<SimT> {
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
    Some(SimT { w_re: num_re / den, w_im: num_im / den, ca, cb })
}
fn apply_similarity_transform(t: &SimT, x: f64, y: f64) -> (f64, f64) {
    let (dx, dy) = (x - t.ca.0, y - t.ca.1);
    let (rx, ry) = (t.w_re * dx - t.w_im * dy, t.w_im * dx + t.w_re * dy);
    (rx + t.cb.0, ry + t.cb.1)
}

// ---- Hungarian (Kuhn-Munkres), direct port ----
fn hungarian(cost: &[Vec<f64>]) -> Vec<i64> {
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

// ---- autoMatch, two-pass ----
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MatchOut {
    pub a: usize,
    pub b: usize,
    pub score: f64,
}

pub(crate) fn auto_match_inner(sa: &[StrokeIn], sb: &[StrokeIn]) -> Vec<MatchOut> {
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
            matches.push(MatchOut { a, b: b as usize, score: cost[a][b as usize] });
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
    let pts_t: Vec<Vec<(f64, f64)>> = fa
        .iter()
        .map(|f| {
            let mut tf = &transform;
            let local_tf;
            if seeds.len() > K_LOCAL {
                let mut near: Vec<&MatchOut> = seeds.iter().collect();
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
            f.pts.iter().map(|p| apply_similarity_transform(tf, p.0, p.1)).collect()
        })
        .collect();
    let cost2 = build_cost(Some(&pts_t));
    let assign2 = hungarian(&cost2);
    let mut matches2 = Vec::new();
    for a in 0..n {
        let b = assign2[a];
        if b >= 0 && (b as usize) < m {
            matches2.push(MatchOut { a, b: b as usize, score: cost2[a][b as usize] });
        }
    }
    uncross_matches(&mut matches2, &fa, &fb, match_norm);
    matches2
}

// ---- trajectory uncrossing — verbatim port of tweens.js's uncrossMatches
// (2026-07-17, "les yeux s'inversent"; see the JS comment for the full
// rationale): two nearly-identical strokes whose straight-line centroid
// trajectories intersect are almost always a matching error (cel features
// keep their spatial arrangement) — swap the B partners when the swapped
// pairing doesn't cost meaningfully more than the crossed one. A genuine
// crossing (different-colored objects passing) survives via the
// color-clash/type penalties, far above the tolerance.
fn segs_intersect(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    fn ccw(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
        (c.1 - a.1) * (b.0 - a.0) > (b.1 - a.1) * (c.0 - a.0)
    }
    ccw(p1, p3, p4) != ccw(p2, p3, p4) && ccw(p1, p2, p3) != ccw(p1, p2, p4)
}
const UNCROSS_TOL: f64 = 0.08;
fn uncross_matches(ms: &mut [MatchOut], fa: &[Feat], fb: &[Feat], match_norm: f64) {
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
                if !segs_intersect(a1, b1, a2, b2) {
                    continue;
                }
                let cur = match_sc(&fa[m1a], &fb[m1b], m1a == m1b, None, match_norm)
                    + match_sc(&fa[m2a], &fb[m2b], m2a == m2b, None, match_norm);
                let swp = match_sc(&fa[m1a], &fb[m2b], m1a == m2b, None, match_norm)
                    + match_sc(&fa[m2a], &fb[m1b], m2a == m1b, None, match_norm);
                if swp <= cur + UNCROSS_TOL {
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

#[wasm_bindgen]
pub fn auto_match(strokes_a_json: &str, strokes_b_json: &str) -> Result<String, JsValue> {
    let sa: Vec<StrokeIn> = serde_json::from_str(strokes_a_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let sb: Vec<StrokeIn> = serde_json::from_str(strokes_b_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let matches = auto_match_inner(&sa, &sb);
    serde_json::to_string(&matches).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---- resampleP / resampleCenterline ----
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResampledOut {
    pub segments: Vec<SegOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widths: Option<Vec<f64>>,
    pub is_vector_brush: bool,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegOut {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

pub(crate) fn resample_stroke_inner(s: &StrokeIn, n: usize) -> ResampledOut {
    let vb = is_vb(s);
    let path = build_feat_path(s);
    let sampler = PathSampler::new(&path);
    let len = sampler.total;
    if len < 1.0 || n < 2 {
        // JS falls back to returning `sd` unchanged in this edge case;
        // mirrored here by emitting the un-resampled points as-is.
        if vb {
            let cs = s.center_segments.as_ref().unwrap();
            return ResampledOut {
                segments: cs.iter().map(|c| SegOut { point: c.point, handle_in: c.handle_in, handle_out: c.handle_out }).collect(),
                widths: Some(cs.iter().map(|c| c.width).collect()),
                is_vector_brush: true,
                stroke_color: None,
                fill_color: s.fill_color.clone(),
            };
        }
        return ResampledOut {
            segments: s.segments.iter().map(|c| SegOut { point: c.point, handle_in: c.handle_in, handle_out: c.handle_out }).collect(),
            widths: None,
            is_vector_brush: false,
            stroke_color: s.stroke_color.clone(),
            fill_color: s.fill_color.clone(),
        };
    }

    let mut segments = Vec::with_capacity(n);
    let mut widths: Option<Vec<f64>> = if vb { Some(Vec::with_capacity(n)) } else { None };

    // Centerline width interpolation walks the RAW anchor-to-anchor chord
    // lengths (not the smooth arc length), exactly like resampleCenterline —
    // widths are stored per anchor point, linearly interpolated between them.
    let width_lens: Vec<f64> = if vb {
        let cs = s.center_segments.as_ref().unwrap();
        let mut acc = vec![0.0];
        for i in 1..cs.len() {
            let (dx, dy) = (cs[i].point[0] - cs[i - 1].point[0], cs[i].point[1] - cs[i - 1].point[1]);
            acc.push(acc[i - 1] + (dx * dx + dy * dy).sqrt());
        }
        acc
    } else {
        Vec::new()
    };
    let width_total = width_lens.last().copied().unwrap_or(1.0).max(1.0);

    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let off = t * len;
        let pt = sampler.point_at(off);
        let (tx, ty) = sampler.tangent_at(off);
        let hl = len / (n - 1) as f64 / 3.0;
        segments.push(SegOut {
            point: [pt.x, pt.y],
            handle_in: if i == 0 { [0.0, 0.0] } else { [-tx * hl, -ty * hl] },
            handle_out: if i == n - 1 { [0.0, 0.0] } else { [tx * hl, ty * hl] },
        });
        if vb {
            let cs = s.center_segments.as_ref().unwrap();
            let target = t * width_total;
            let mut wi = 0;
            while wi < width_lens.len().saturating_sub(2) && width_lens[wi + 1] < target {
                wi += 1;
            }
            let span = (width_lens[wi + 1] - width_lens[wi]).max(0.0001);
            let lt = ((target - width_lens[wi]) / span).clamp(0.0, 1.0);
            widths.as_mut().unwrap().push(cs[wi].width + (cs[wi + 1].width - cs[wi].width) * lt);
        }
    }

    ResampledOut {
        segments,
        widths,
        is_vector_brush: vb,
        stroke_color: if vb { None } else { s.stroke_color.clone() },
        fill_color: s.fill_color.clone(),
    }
}

#[wasm_bindgen]
pub fn resample_stroke(stroke_json: &str, n: usize) -> Result<String, JsValue> {
    let s: StrokeIn = serde_json::from_str(stroke_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let out = resample_stroke_inner(&s, n);
    serde_json::to_string(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---- alignResampledPair (reverse / cyclic-rotate search) ----
// Loosened 2026-07-17 — verbatim port of tweens.js's resampledIsClosed
// near-closed test (see the JS comment for the full rationale): a
// hand-drawn head outline is a nearly-closed loop left open by a small pen
// gap and typically drawn from a different start point in each keyframe —
// the old 5%-of-diagonal gap test kept the cyclic-rotation search off for
// it, and the resulting index offset read as a spurious ~80° whole-head
// rotation, knotting the outline mid-tween. Near-closed = endpoint gap
// under 15% of the diagonal AND polyline length over 1.5x the diagonal
// (real loops only — open arcs and straight strokes stay reversal-only).
fn resampled_is_closed(r: &ResampledOut) -> bool {
    let s = &r.segments;
    if s.len() < 4 {
        return false;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for sg in s {
        minx = minx.min(sg.point[0]);
        miny = miny.min(sg.point[1]);
        maxx = maxx.max(sg.point[0]);
        maxy = maxy.max(sg.point[1]);
    }
    let diag2 = (maxx - minx).powi(2) + (maxy - miny).powi(2);
    let (dx, dy) = (s[0].point[0] - s[s.len() - 1].point[0], s[0].point[1] - s[s.len() - 1].point[1]);
    let gap2 = dx * dx + dy * dy;
    if gap2 >= diag2.max(1.0) * 0.0225 {
        return false; // 0.15^2 of the diagonal
    }
    let mut poly_len = 0.0;
    for i in 1..s.len() {
        let (ddx, ddy) = (s[i].point[0] - s[i - 1].point[0], s[i].point[1] - s[i - 1].point[1]);
        poly_len += (ddx * ddx + ddy * ddy).sqrt();
    }
    poly_len * poly_len > diag2 * 2.25 // length > 1.5x diagonal — real loops only
}
fn align_cost(a: &ResampledOut, b: &ResampledOut) -> f64 {
    let n = a.segments.len().min(b.segments.len());
    let (mut cax, mut cay, mut cbx, mut cby) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        cax += a.segments[i].point[0];
        cay += a.segments[i].point[1];
        cbx += b.segments[i].point[0];
        cby += b.segments[i].point[1];
    }
    cax /= n as f64;
    cay /= n as f64;
    cbx /= n as f64;
    cby /= n as f64;
    let mut sum = 0.0;
    for i in 0..n {
        let dx = (a.segments[i].point[0] - cax) - (b.segments[i].point[0] - cbx);
        let dy = (a.segments[i].point[1] - cay) - (b.segments[i].point[1] - cby);
        sum += dx * dx + dy * dy;
    }
    sum
}
fn reverse_resampled(r: &ResampledOut) -> ResampledOut {
    let mut segments: Vec<SegOut> =
        r.segments.iter().rev().map(|s| SegOut { point: s.point, handle_in: s.handle_out, handle_out: s.handle_in }).collect();
    segments.shrink_to_fit();
    ResampledOut {
        segments,
        widths: r.widths.as_ref().map(|w| w.iter().rev().copied().collect()),
        is_vector_brush: r.is_vector_brush,
        stroke_color: r.stroke_color.clone(),
        fill_color: r.fill_color.clone(),
    }
}
fn rotate_resampled(r: &ResampledOut, k: usize) -> ResampledOut {
    let n = r.segments.len();
    let mut segments = Vec::with_capacity(n);
    segments.extend_from_slice(&r.segments[k..]);
    segments.extend_from_slice(&r.segments[..k]);
    let widths = r.widths.as_ref().map(|w| {
        let mut out = Vec::with_capacity(w.len());
        out.extend_from_slice(&w[k..]);
        out.extend_from_slice(&w[..k]);
        out
    });
    ResampledOut { segments, widths, is_vector_brush: r.is_vector_brush, stroke_color: r.stroke_color.clone(), fill_color: r.fill_color.clone() }
}

// Residual after fitting the best RIGID rotation+scale for a candidate B
// ordering, instead of align_cost's raw centroid-relative point distance
// — verbatim port of tweens.js's rotationFitResidual (2026-07-17 fix).
// Found by stress-testing: a 300x40 rectangle rotated a clean 90° picked
// the mathematically-optimal-by-align_cost correspondence (confirmed by
// exhaustive brute-force search — not a search bug), yet the fitted
// rotation on that exact correspondence came back mag≈0.34/theta≈-10°
// instead of the expected mag≈1/theta≈90° — visibly non-rigid, "melting"
// instead of turning. Raw point distance has no notion of "does this
// correspondence read as one coherent rigid motion" — only "are the
// points numerically close after centering" — and a shape with some
// rotational/reflective symmetry (any non-square rectangle included) can
// have multiple orderings that tie or nearly tie on that measure while
// only one of them is the clean turn. Scoring candidates by how well they
// fit a single rigid rotation+scale (what interpStroke/interp_stroke will
// actually apply) picks the one that turns instead of the one that merely
// minimizes raw distance.
fn rotation_fit_residual(a: &ResampledOut, b: &ResampledOut) -> f64 {
    let n = a.segments.len().min(b.segments.len());
    let pts_a: Vec<(f64, f64)> = a.segments[..n].iter().map(|s| (s.point[0], s.point[1])).collect();
    let pts_b: Vec<(f64, f64)> = b.segments[..n].iter().map(|s| (s.point[0], s.point[1])).collect();
    match fit_similarity_transform(&pts_a, &pts_b) {
        None => align_cost(a, b), // degenerate (coincident points) — fall back to plain distance
        Some(t) => {
            let mut sum = 0.0;
            for i in 0..n {
                let (qx, qy) = apply_similarity_transform(&t, pts_a[i].0, pts_a[i].1);
                let dx = qx - pts_b[i].0;
                let dy = qy - pts_b[i].1;
                sum += dx * dx + dy * dy;
            }
            sum
        }
    }
}

// The rotation-fit criterion is CLOSED-shapes-only — verbatim port of
// alignResampledPairJS's 2026-07-17 fix (see the JS comment): for a
// near-straight OPEN stroke, reversing the point order is geometrically
// indistinguishable from a ~180° rotation, so the rotation-fit residual
// "explained" reversals with a perfect half-spin and eyebrows twirled
// -145°..-165° for no reason on a real hand-drawn face. Open strokes use
// the plain raw-distance test; closed loops keep the rotation-fit
// criterion (the 90°-rectangle / rotated-start-star fix).
#[wasm_bindgen]
pub fn align_pair(a_json: &str, b_json: &str) -> Result<String, JsValue> {
    let a: ResampledJsonIn = serde_json::from_str(a_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let b: ResampledJsonIn = serde_json::from_str(b_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let a = a.into_out();
    let b = b.into_out();
    let closed = resampled_is_closed(&a) && resampled_is_closed(&b);
    let cost_fn = if closed { rotation_fit_residual } else { align_cost };
    let mut best = b.clone();
    let mut best_cost = cost_fn(&a, &b);
    let rev = reverse_resampled(&b);
    let rc = cost_fn(&a, &rev);
    if rc < best_cost {
        best_cost = rc;
        best = rev.clone();
    }
    if closed {
        for base in [&b, &rev] {
            let n = base.segments.len();
            for k in 1..n {
                let cand = rotate_resampled(base, k);
                let c = cost_fn(&a, &cand);
                if c < best_cost {
                    best_cost = c;
                    best = cand;
                }
            }
        }
    }
    serde_json::to_string(&best).map_err(|e| JsValue::from_str(&e.to_string()))
}

// align_pair accepts the exact ResampledOut JSON shape (round-trips what
// resample_stroke produced) — a thin Deserialize counterpart since
// ResampledOut only derives Serialize.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResampledJsonIn {
    segments: Vec<SegIn>,
    widths: Option<Vec<f64>>,
    is_vector_brush: bool,
    stroke_color: Option<String>,
    fill_color: Option<String>,
}
impl ResampledJsonIn {
    fn into_out(self) -> ResampledOut {
        ResampledOut {
            segments: self.segments.iter().map(|s| SegOut { point: s.point, handle_in: s.handle_in, handle_out: s.handle_out }).collect(),
            widths: self.widths,
            is_vector_brush: self.is_vector_brush,
            stroke_color: self.stroke_color,
            fill_color: self.fill_color,
        }
    }
}
