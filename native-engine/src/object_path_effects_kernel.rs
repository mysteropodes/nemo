//! Path-effect kernel (G13/#2107, leaf L01-A): the pure geometry of
//! `src/js/path-fx.js` at fbe4b8a3 (`walk`, `applyZigzag`, `applyRoughen`,
//! `roundCorners`, `activeStack`, `apply`, `paramValue`, `TYPES`).
//!
//! The kernel owns no document, history or scene state. A caller hands it
//! stored strokes and an ordered effect stack and gets the evaluated strokes
//! back; the input is only ever borrowed, so the rest geometry cannot change.
//! Arithmetic keeps the JavaScript operand order, so every result that does
//! not pass through `sin` (zigzag, walk, roundCorners) is bit-identical to the
//! oracle. Roughen goes through `sin(..) * 43758.5453`, which amplifies a
//! 1-ulp libm difference, so its results are compared with an absolute bound.
//!
//! Baseline semantics kept on purpose:
//! - an effect applies only to a stroke that is not a raster and has at least
//!   two segments; anything else, and any effect that would leave fewer than
//!   two points, returns the very same stroke (`Cow::Borrowed`);
//! - `size`/`amount` of 0 or NaN returns the input; ridges/detail go through
//!   JavaScript `Math.round` (half toward +infinity, see `js_round`) and
//!   `Math.max(1, ..)`; `evolution` of 0 or NaN freezes the phase at 0;
//! - a parameter that is absent or null reads as the type default (12,4,0 and
//!   8,6,0); an explicit 0 stays 0. The type id is the only discriminator:
//!   an unknown type is dropped by `active_stack`, `enabled` disables only
//!   when it is exactly `false`, and a null stack entry is skipped;
//! - an open path appends its last anchor with the normal of (last - previous)
//!   at index `last * perCurve`; a closed path has no such tail.
//!
//! Not baseline, native only: the sample count of one stroke is bounded by
//! `MAX_SAMPLES`. The JavaScript loop has no bound (an unbounded parameter
//! exhausts memory); the kernel reports `PathFxError::SampleBudget` and
//! publishes nothing. Falsy stroke entries and non-numeric parameters have no
//! typed representation here (a typed record never holds them).

use std::borrow::Cow;

/// Upper bound on the points one effect may produce for one stroke.
pub(crate) const MAX_SAMPLES: usize = 1 << 20;

/// An effect asked for more samples than `MAX_SAMPLES` (or an infinite count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathFxError {
    SampleBudget,
}

/// One entry of `TYPES`: the id and the `def` of params p1..p3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PathFxTypeDef {
    pub id: &'static str,
    pub defaults: [f64; 3],
}

pub(crate) const PATH_FX_TYPES: [PathFxTypeDef; 2] = [
    PathFxTypeDef {
        id: "zigzag",
        defaults: [12.0, 4.0, 0.0],
    },
    PathFxTypeDef {
        id: "roughen",
        defaults: [8.0, 6.0, 0.0],
    },
];

pub(crate) const P1: usize = 0;
pub(crate) const P2: usize = 1;
pub(crate) const P3: usize = 2;

/// `typeDef(id)`.
pub(crate) fn type_def(id: &str) -> Option<&'static PathFxTypeDef> {
    PATH_FX_TYPES.iter().find(|t| t.id == id)
}

/// One stored effect: `type`, `enabled` (absent and null both `None`; only
/// `Some(false)` disables) and `p1..p3` (absent and null both `None`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathFxEntry {
    pub fx_type: String,
    pub enabled: Option<bool>,
    pub params: [Option<f64>; 3],
}

/// `paramValue(fx, 'p<key+1>')`: the stored number, else the type default,
/// else 0 for an unknown type or a key outside p1..p3.
pub(crate) fn param_value(fx: &PathFxEntry, key: usize) -> f64 {
    if let Some(v) = fx.params.get(key).copied().flatten() {
        return v;
    }
    type_def(&fx.fx_type)
        .and_then(|d| d.defaults.get(key).copied())
        .unwrap_or(0.0)
}

/// One stored segment; an absent handle reads as `[0, 0]` while walking but
/// stays absent in a stroke that no effect touched.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathSegment {
    pub point: [f64; 2],
    pub handle_in: Option<[f64; 2]>,
    pub handle_out: Option<[f64; 2]>,
}

/// A stored stroke dict: the fields the kernel reads plus `rest`, the other
/// fields, which survive an effect unchanged (`shallowCopy`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathFxStroke<R> {
    pub is_raster: bool,
    pub closed: bool,
    pub segments: Option<Vec<PathSegment>>,
    pub rest: R,
}

/// One sample of `walk`: position, unit normal `(-dy, dx) / |d|`, index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WalkSample {
    pub point: [f64; 2],
    pub normal: [f64; 2],
    pub index: usize,
}

/// JavaScript `Math.round`: half toward +infinity, `-0` for a negative that
/// rounds to zero. Not `f64::round` (half away from zero: -0.5 -> -1) and not
/// `floor(x + 0.5)` (0.49999999999999994 + 0.5 rounds up to 1).
pub(crate) fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    if r == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        r
    }
}

/// `Math.max(1, v)` (NaN propagates).
fn max_one(v: f64) -> f64 {
    if v.is_nan() || v > 1.0 {
        v
    } else {
        1.0
    }
}

fn falsy(x: f64) -> bool {
    x == 0.0 || x.is_nan()
}

/// `hash2`: the reproducible lattice hash (the only libm use).
pub(crate) fn hash2(x: f64, y: f64) -> f64 {
    let n = (x * 127.1 + y * 311.7).sin() * 43758.5453;
    n - n.floor()
}

/// `smoothNoise`: smoothstep-interpolated lattice noise in [-1, 1].
pub(crate) fn smooth_noise(x: f64, y: f64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (xf, yf) = (x - xi, y - yi);
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let a = hash2(xi, yi);
    let b = hash2(xi + 1.0, yi);
    let c = hash2(xi, yi + 1.0);
    let d = hash2(xi + 1.0, yi + 1.0);
    ((a * (1.0 - u) + b * u) * (1.0 - v) + (c * (1.0 - u) + d * u) * v) * 2.0 - 1.0
}

fn bez(t: f64, a: [f64; 2], c1: [f64; 2], c2: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let u = 1.0 - t;
    [
        u * u * u * a[0] + 3.0 * u * u * t * c1[0] + 3.0 * u * t * t * c2[0] + t * t * t * b[0],
        u * u * u * a[1] + 3.0 * u * u * t * c1[1] + 3.0 * u * t * t * c2[1] + t * t * t * b[1],
    ]
}

fn bez_deriv(t: f64, a: [f64; 2], c1: [f64; 2], c2: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let u = 1.0 - t;
    [
        3.0 * u * u * (c1[0] - a[0]) + 6.0 * u * t * (c2[0] - c1[0]) + 3.0 * t * t * (b[0] - c2[0]),
        3.0 * u * u * (c1[1] - a[1]) + 6.0 * u * t * (c2[1] - c1[1]) + 3.0 * t * t * (b[1] - c2[1]),
    ]
}

/// `normalOf`: `(-dy, dx) / |d|`, the length falling back to 1 when it is 0/NaN.
fn normal_of(d: [f64; 2]) -> [f64; 2] {
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let len = if falsy(l) { 1.0 } else { l };
    [-d[1] / len, d[0] / len]
}

fn seg_of(point: [f64; 2]) -> PathSegment {
    PathSegment {
        point,
        handle_in: Some([0.0, 0.0]),
        handle_out: Some([0.0, 0.0]),
    }
}

/// `walk`: `per_curve` equispaced samples of every curve (closed: n curves,
/// open: n-1) plus, for an open path, its last anchor. Callers pass
/// `per_curve >= 1`. An empty path yields no samples (JavaScript throws on an
/// open one; `apply` never reaches that).
pub(crate) fn walk(segments: &[PathSegment], closed: bool, per_curve: usize) -> Vec<WalkSample> {
    let n = segments.len();
    if n == 0 {
        return Vec::new();
    }
    let last = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(last * per_curve + 1);
    for i in 0..last {
        let (s0, s1) = (&segments[i], &segments[(i + 1) % n]);
        let (a, b) = (s0.point, s1.point);
        let ho = s0.handle_out.unwrap_or([0.0, 0.0]);
        let hi = s1.handle_in.unwrap_or([0.0, 0.0]);
        let c1 = [a[0] + ho[0], a[1] + ho[1]];
        let c2 = [b[0] + hi[0], b[1] + hi[1]];
        for k in 0..per_curve {
            let t = k as f64 / per_curve as f64;
            out.push(WalkSample {
                point: bez(t, a, c1, c2, b),
                normal: normal_of(bez_deriv(t, a, c1, c2, b)),
                index: i * per_curve + k,
            });
        }
    }
    if !closed {
        let sl = &segments[n - 1];
        let sp = if n >= 2 { &segments[n - 2] } else { sl };
        out.push(WalkSample {
            point: sl.point,
            normal: normal_of([sl.point[0] - sp.point[0], sl.point[1] - sp.point[1]]),
            index: last * per_curve,
        });
    }
    out
}

/// `roundCorners`: Catmull-Rom tangent handles `(next - prev) / 6`; the
/// endpoints of an open path keep their handles.
pub(crate) fn round_corners(segs: &mut [PathSegment], closed: bool) {
    let n = segs.len();
    for i in 0..n {
        if !closed && (i == 0 || i == n - 1) {
            continue;
        }
        let (prev, next) = (segs[(i + n - 1) % n].point, segs[(i + 1) % n].point);
        let tx = (next[0] - prev[0]) / 6.0;
        let ty = (next[1] - prev[1]) / 6.0;
        segs[i].handle_in = Some([-tx, -ty]);
        segs[i].handle_out = Some([tx, ty]);
    }
}

/// The sample count per curve, `Ok(None)` when it is NaN (the JavaScript loop
/// never runs, leaves at most one point and returns the input).
fn per_curve_of(count: f64, n: usize, closed: bool) -> Result<Option<usize>, PathFxError> {
    if count.is_nan() {
        return Ok(None);
    }
    let curves = if closed { n } else { n - 1 };
    if !(count * curves as f64 + 1.0 <= MAX_SAMPLES as f64) {
        return Err(PathFxError::SampleBudget);
    }
    Ok(Some(count as usize))
}

fn with_segments<R: Clone>(sd: &PathFxStroke<R>, out: Vec<PathSegment>) -> PathFxStroke<R> {
    PathFxStroke {
        is_raster: sd.is_raster,
        closed: sd.closed,
        segments: Some(out),
        rest: sd.rest.clone(),
    }
}

/// `applyZigzag`: `Ok(None)` is "return the input itself".
pub(crate) fn apply_zigzag<R: Clone>(
    sd: &PathFxStroke<R>,
    fx: &PathFxEntry,
) -> Result<Option<PathFxStroke<R>>, PathFxError> {
    let size = param_value(fx, P1);
    let ridges = max_one(js_round(param_value(fx, P2)));
    let smooth = param_value(fx, P3) > 0.5;
    let Some(segs) = sd.segments.as_deref() else {
        return Ok(None);
    };
    if falsy(size) || segs.is_empty() {
        return Ok(None);
    }
    let Some(per_curve) = per_curve_of(ridges * 2.0, segs.len(), sd.closed)? else {
        return Ok(None);
    };
    let out: Vec<PathSegment> = walk(segs, sd.closed, per_curve)
        .iter()
        .map(|s| {
            let side = if s.index % 2 == 0 { 1.0 } else { -1.0 };
            seg_of([
                s.point[0] + s.normal[0] * size * side,
                s.point[1] + s.normal[1] * size * side,
            ])
        })
        .collect();
    finish(sd, out, smooth)
}

/// The roughen phase `z`: `evolution ? (frameIdx || 0) * (evolution / 100) : 0`.
pub(crate) fn roughen_phase(evolution: f64, frame_idx: Option<f64>) -> f64 {
    if falsy(evolution) {
        return 0.0;
    }
    let frame = match frame_idx {
        Some(f) if !falsy(f) => f,
        _ => 0.0,
    };
    frame * (evolution / 100.0)
}

/// `applyRoughen`: `Ok(None)` is "return the input itself".
pub(crate) fn apply_roughen<R: Clone>(
    sd: &PathFxStroke<R>,
    fx: &PathFxEntry,
    frame_idx: Option<f64>,
) -> Result<Option<PathFxStroke<R>>, PathFxError> {
    let amount = param_value(fx, P1);
    let detail = max_one(js_round(param_value(fx, P2)));
    let evolution = param_value(fx, P3);
    let Some(segs) = sd.segments.as_deref() else {
        return Ok(None);
    };
    if falsy(amount) || segs.is_empty() {
        return Ok(None);
    }
    let z = roughen_phase(evolution, frame_idx);
    let Some(per_curve) = per_curve_of(detail, segs.len(), sd.closed)? else {
        return Ok(None);
    };
    let out: Vec<PathSegment> = walk(segs, sd.closed, per_curve)
        .iter()
        .map(|s| {
            let i = s.index as f64;
            let d = smooth_noise(i * 0.35 + z, z * 0.7 + i * 0.017);
            seg_of([
                s.point[0] + s.normal[0] * d * amount,
                s.point[1] + s.normal[1] * d * amount,
            ])
        })
        .collect();
    finish(sd, out, true)
}

/// Shared tail: fewer than two points returns the input, else a shallow copy
/// with the new segments, rounded when asked.
fn finish<R: Clone>(
    sd: &PathFxStroke<R>,
    mut out: Vec<PathSegment>,
    round: bool,
) -> Result<Option<PathFxStroke<R>>, PathFxError> {
    if out.len() < 2 {
        return Ok(None);
    }
    if round {
        round_corners(&mut out, sd.closed);
    }
    Ok(Some(with_segments(sd, out)))
}

/// `activeStack`: the enabled entries of a known type in storage order, or
/// `None` for no stack, an empty stack, or nothing left after filtering.
pub(crate) fn active_stack(path_fx: Option<&[Option<PathFxEntry>]>) -> Option<Vec<&PathFxEntry>> {
    let on: Vec<&PathFxEntry> = path_fx?
        .iter()
        .flatten()
        .filter(|f| f.enabled != Some(false) && type_def(&f.fx_type).is_some())
        .collect();
    if on.is_empty() {
        None
    } else {
        Some(on)
    }
}

/// `hasAny`.
pub(crate) fn has_any(path_fx: Option<&[Option<PathFxEntry>]>) -> bool {
    active_stack(path_fx).is_some()
}

fn apply_one<'a, R: Clone>(
    strokes: Vec<Cow<'a, PathFxStroke<R>>>,
    fx: &PathFxEntry,
    frame_idx: Option<f64>,
) -> Result<Vec<Cow<'a, PathFxStroke<R>>>, PathFxError> {
    let mut out = Vec::with_capacity(strokes.len());
    for cow in strokes {
        let changed = {
            let sd: &PathFxStroke<R> = &cow;
            if sd.is_raster || sd.segments.as_ref().map_or(true, |s| s.len() < 2) {
                None
            } else if fx.fx_type == "zigzag" {
                apply_zigzag(sd, fx)?
            } else if fx.fx_type == "roughen" {
                apply_roughen(sd, fx, frame_idx)?
            } else {
                None
            }
        };
        out.push(match changed {
            Some(stroke) => Cow::Owned(stroke),
            None => cow,
        });
    }
    Ok(out)
}

/// `apply(strokes, ld, frameIdx)`: the active stack applied in storage order,
/// each effect to the previous effect's output. A stroke no effect changed is
/// `Cow::Borrowed` (the same value, not a copy); without an active stack every
/// stroke is. The result is all-or-nothing: an error returns no strokes.
pub(crate) fn apply<'a, R: Clone>(
    strokes: &'a [PathFxStroke<R>],
    path_fx: Option<&[Option<PathFxEntry>]>,
    frame_idx: Option<f64>,
) -> Result<Vec<Cow<'a, PathFxStroke<R>>>, PathFxError> {
    let mut out: Vec<Cow<'a, PathFxStroke<R>>> = strokes.iter().map(Cow::Borrowed).collect();
    let Some(stack) = active_stack(path_fx) else {
        return Ok(out);
    };
    if strokes.is_empty() {
        return Ok(out);
    }
    for fx in stack {
        out = apply_one(out, fx, frame_idx)?;
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/object_path_effects_kernel.rs"]
mod tests;
