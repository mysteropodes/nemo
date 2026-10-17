//! Parametric-shape builders (G22/#2107, leaf L01-B): the pure geometry of the
//! baseline dynamic-shape generators at fbe4b8a3.
//!
//! - `tools.js` `buildRoundRectPath`, `buildArcEllipsePath`, `buildStarPolygonPath`;
//! - `shape-bridge.js` `shapeGeom` with its `ellipseGeomJS`/`rectGeomJS`/`lineGeomJS`
//!   fallbacks, and the `geometry-wasm` `line_segments`/`rect_segments`/`ellipse_segments`
//!   functions that `shapeGeom` tries first.
//!
//! `shapeGeom` lives in the child `shapes` (`object_parametric_geometry_shapes.rs`, re-exported here). The
//! box bookkeeping and the rebuilds (`stampParamShapeBox`, `syncParamShapeBoxOn*`, `applyParamShape*`,
//! `applyParamShapeFor`) live in the child `boxed` (`object_parametric_geometry_box.rs`, whose own child
//! `object_parametric_geometry_anim.rs` holds the Motion-driven rebuild).
//!
//! The kernel owns no document, history or scene state: a caller hands it
//! numbers and gets segments back. Arithmetic keeps the JavaScript operand
//! order, so every result that does not pass through `cos`/`sin` is
//! bit-identical to the oracle (`-0` included); the trigonometric results are
//! compared by the tests with a per-group rounding bound because libm may
//! differ from V8 in the last ulp.
//!
//! Baseline semantics kept on purpose:
//! - `Math.min`/`Math.max` (`js_min`/`js_max`) give `min(-0, 0) = -0`; the
//!   radii clamp `max(0, min(r, half))` therefore yields `+0` for a negative
//!   radius and for `-0`;
//! - a rounded rectangle corner with radius 0 is ONE anchor, a rounded one is
//!   two (the arc start and end), and the top-left corner's second anchor is
//!   appended last, so the path starts at `(x1 + tl, y1)`;
//! - the arc ellipse is a polyline (no Bezier arcs): `n = max(8, ceil(sweep/4))`
//!   steps over a sweep clamped to `[0.1, 359.9]`; a pie closes through the
//!   centre, a donut walks the inner ring back;
//! - the star `pointCount` goes through `Math.round(pointCount || 5)` (so 0
//!   reads as 5 and 2.5 as 3), an absent `innerRatio` is 0.5, `>= 0.999`
//!   is a regular polygon, and a corner radius of 0.01 or less adds no anchors;
//! - `shapeGeom` has two sources that really differ: the shipped wasm
//!   (`GeomSource::Wasm`, rect anchors in drag order, every handle present) and
//!   the JavaScript fallback (`GeomSource::JsFallback`, rect min/max
//!   normalised, handles absent for line and rect).
//!
//! Not baseline, native only: every numeric input must be finite and a star
//! may hold at most `MAX_STAR_POINTS` points. The JavaScript functions accept
//! NaN/Infinity (and build an unbounded vertex list); the kernel reports a
//! typed `ParamGeomError` and publishes nothing, because a typed record never
//! holds a non-finite number.

use crate::evaluation::object_vector_kernels::object_path_effects_kernel::js_round;
use std::f64::consts::PI;

#[path = "object_parametric_geometry_box.rs"]
pub(crate) mod boxed;
#[path = "object_parametric_geometry_shapes.rs"]
mod shapes;

// Re-exported for the later consumer joins; nothing outside the tests names them yet.
#[allow(unused_imports)]
pub(crate) use shapes::{shape_geom, GeomSegment, GeomSource, ShapeGeom, ShapeTool, StarState};

/// `KAPPA` of `buildRoundRectPath`/`buildStarPolygonPath`/`ellipseGeomJS` and
/// `K` of the wasm `ellipse_segments`.
pub(crate) const KAPPA: f64 = 0.5522847498;

/// Upper bound on `Math.round(pointCount)` (native guard; see the module notes).
pub(crate) const MAX_STAR_POINTS: f64 = 65_536.0;

/// Why a builder refused its input (nothing is produced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamGeomError {
    /// A numeric input is NaN or infinite.
    NonFinite,
    /// `Math.round(pointCount || 5)` exceeds `MAX_STAR_POINTS`.
    StarPointBudget,
    /// A rebuild needs the stored anchors but there are none.
    EmptyGeometry,
    /// A rebuild needs the shape box and neither the box nor the current bounds were supplied.
    MissingBox,
}

/// A Paper.js segment as the builders emit it: point and RELATIVE handles, all present.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParamSegment {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

/// `Math.min`: NaN propagates, `min(-0, +0)` is `-0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max`: NaN propagates, `max(-0, +0)` is `+0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            b
        } else {
            a
        }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `x || fallback`: an absent, 0, -0 or NaN value reads as the fallback.
pub(crate) fn or_default(x: Option<f64>, fallback: f64) -> f64 {
    match x {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => fallback,
    }
}

pub(crate) fn check_finite(values: &[f64]) -> Result<(), ParamGeomError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(ParamGeomError::NonFinite)
    }
}

fn seg(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> ParamSegment {
    ParamSegment {
        point,
        handle_in,
        handle_out,
    }
}

fn corner(point: [f64; 2]) -> ParamSegment {
    seg(point, [0.0, 0.0], [0.0, 0.0])
}

/// `buildRoundRectPath(x1, y1, x2, y2, tl, tr, br, bl)`: always a closed path.
pub(crate) fn build_round_rect(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    radii: [f64; 4],
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[x1, y1, x2, y2, radii[0], radii[1], radii[2], radii[3]])?;
    let max_r = js_min(x2 - x1, y2 - y1) / 2.0;
    let clamp = |r: f64| js_max(0.0, js_min(r, max_r));
    let (tl, tr, br, bl) = (
        clamp(radii[0]),
        clamp(radii[1]),
        clamp(radii[2]),
        clamp(radii[3]),
    );
    let k = KAPPA;
    let mut segs = Vec::with_capacity(9);
    if tl > 0.0 {
        segs.push(seg([x1 + tl, y1], [-tl * k, 0.0], [0.0, 0.0]));
    } else {
        segs.push(corner([x1, y1]));
    }
    if tr > 0.0 {
        segs.push(seg([x2 - tr, y1], [0.0, 0.0], [tr * k, 0.0]));
        segs.push(seg([x2, y1 + tr], [0.0, -tr * k], [0.0, 0.0]));
    } else {
        segs.push(corner([x2, y1]));
    }
    if br > 0.0 {
        segs.push(seg([x2, y2 - br], [0.0, 0.0], [0.0, br * k]));
        segs.push(seg([x2 - br, y2], [br * k, 0.0], [0.0, 0.0]));
    } else {
        segs.push(corner([x2, y2]));
    }
    if bl > 0.0 {
        segs.push(seg([x1 + bl, y2], [0.0, 0.0], [-bl * k, 0.0]));
        segs.push(seg([x1, y2 - bl], [0.0, bl * k], [0.0, 0.0]));
    } else {
        segs.push(corner([x1, y2]));
    }
    if tl > 0.0 {
        segs.push(seg([x1, y1 + tl], [0.0, 0.0], [0.0, -tl * k]));
    }
    Ok(segs)
}

/// `buildArcEllipsePath(cx, cy, rx, ry, startDeg, sweepDeg, innerRatio)`: always closed.
/// `inner_ratio` is read as `innerRatio || 0`.
pub(crate) fn build_arc_ellipse(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    start_deg: f64,
    sweep_deg: f64,
    inner_ratio: Option<f64>,
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[cx, cy, rx, ry, start_deg, sweep_deg])?;
    if let Some(v) = inner_ratio {
        check_finite(&[v])?;
    }
    let sweep = js_max(0.1, js_min(359.9, sweep_deg));
    let ir = js_max(0.0, js_min(0.95, or_default(inner_ratio, 0.0)));
    let steps = js_max(8.0, (sweep / 4.0).ceil());
    let n = steps as usize;
    let angle = |i: usize| (start_deg + sweep * i as f64 / steps) * PI / 180.0;
    let mut segs = Vec::with_capacity(2 * n + 3);
    for i in 0..=n {
        let a = angle(i);
        segs.push(corner([cx + rx * a.cos(), cy + ry * a.sin()]));
    }
    if ir > 0.0 {
        for j in (0..=n).rev() {
            let a2 = angle(j);
            segs.push(corner([cx + rx * ir * a2.cos(), cy + ry * ir * a2.sin()]));
        }
    } else {
        segs.push(corner([cx, cy]));
    }
    Ok(segs)
}

/// `buildStarPolygonPath(cx, cy, outerR, pointCount, innerRatio, cornerRadius)`: always closed.
/// `point_count` is read as `Math.round(pointCount || 5)`, `inner_ratio` as
/// `innerRatio === undefined ? 0.5 : innerRatio`, `corner_radius` as `cornerRadius || 0`.
pub(crate) fn build_star_polygon(
    cx: f64,
    cy: f64,
    outer_r: f64,
    point_count: Option<f64>,
    inner_ratio: Option<f64>,
    corner_radius: Option<f64>,
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[cx, cy, outer_r])?;
    for v in [point_count, inner_ratio, corner_radius]
        .into_iter()
        .flatten()
    {
        check_finite(&[v])?;
    }
    let count = js_max(3.0, js_round(or_default(point_count, 5.0)));
    if count > MAX_STAR_POINTS {
        return Err(ParamGeomError::StarPointBudget);
    }
    let ratio = js_max(0.05, js_min(1.0, inner_ratio.unwrap_or(0.5)));
    let is_polygon = ratio >= 0.999;
    let n = if is_polygon {
        count as usize
    } else {
        count as usize * 2
    };
    let inner_r = outer_r * ratio;
    let mut verts: Vec<[f64; 2]> = Vec::with_capacity(n);
    for i in 0..n {
        let r = if is_polygon || i % 2 == 0 {
            outer_r
        } else {
            inner_r
        };
        let a = -PI / 2.0 + i as f64 * PI * 2.0 / n as f64;
        verts.push([cx + r * a.cos(), cy + r * a.sin()]);
    }
    let cr = js_max(0.0, or_default(corner_radius, 0.0));
    let mut segs = Vec::with_capacity(n * 2);
    for (j, &v) in verts.iter().enumerate() {
        let prev = verts[(j + n - 1) % n];
        let next = verts[(j + 1) % n];
        if cr <= 0.01 {
            segs.push(corner(v));
            continue;
        }
        let d_prev = distance(v, prev);
        let d_next = distance(v, next);
        let r_p = js_min(cr, d_prev * 0.5);
        let r_n = js_min(cr, d_next * 0.5);
        let a = toward(v, prev, r_p);
        let b = toward(v, next, r_n);
        let h_out_a = [(v[0] - a[0]) * KAPPA, (v[1] - a[1]) * KAPPA];
        let h_in_b = [(v[0] - b[0]) * KAPPA, (v[1] - b[1]) * KAPPA];
        segs.push(seg(a, [0.0, 0.0], h_out_a));
        segs.push(seg(b, h_in_b, [0.0, 0.0]));
    }
    Ok(segs)
}

/// `V.getDistance(P)`.
fn distance(v: [f64; 2], p: [f64; 2]) -> f64 {
    let x = p[0] - v[0];
    let y = p[1] - v[1];
    (x * x + y * y).sqrt()
}

/// `V.add(P.subtract(V).normalize(length))`.
fn toward(v: [f64; 2], p: [f64; 2], length: f64) -> [f64; 2] {
    let d = [p[0] - v[0], p[1] - v[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let scale = if len != 0.0 { length / len } else { 0.0 };
    [v[0] + d[0] * scale, v[1] + d[1] * scale]
}

#[cfg(test)]
#[path = "../tests/object_parametric_geometry.rs"]
mod tests;
