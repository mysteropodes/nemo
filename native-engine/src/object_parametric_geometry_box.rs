//! Parametric-shape frame (G22/#2107, leaf L01-B): the `paramShape.box` bookkeeping
//! and the rebuilds of `tools.js` / `motion.js` at fbe4b8a3.
//!
//! Covered baseline functions: `stampParamShapeBox`, `syncParamShapeBoxOnTranslate`,
//! `syncParamShapeBoxOnScale` (a rect is rebuilt from the new box with its
//! UNCHANGED radii), `paramShapeBoxOf` (a missing box self-heals from the
//! current bounds), `reapplyParamShapeAngle` (`Path.rotate(boxAngle, centre)`
//! with Paper.js `Matrix.rotate`, `Item.transform` and
//! `Segment._transformCoordinates` arithmetic), `applyParamShapeEllipse/Star/Rect`,
//! `applyParamShapeFor` and `hasParamShapeMotionFor` (motion.js: the rebuild
//! reads the min/max of the STORED anchors, not the stored box).
//!
//! Baseline quirks kept: the ellipse rebuild never re-applies `boxAngle`;
//! a box-less shape ignores translate and scale (nothing is stamped); the
//! animated ellipse inner radius and star inner ratio are PERCENT values while
//! the stored fields are 0..1; handles of at most 1e-12 are "zero" and are
//! left untouched by a rotation.
//!
//! Not covered (listed in HANDOFF): computing `path.bounds` of a CURVED path.
//! `straight_bounds` answers only when every handle is exactly zero; a caller
//! with curves passes the bounds from the native geometry-bounds provider.

use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    build_arc_ellipse, build_round_rect, build_star_polygon, check_finite, js_max, js_min,
    or_default, ParamGeomError, ParamSegment,
};
use std::f64::consts::PI;

#[path = "object_parametric_geometry_anim.rs"]
mod animated;

#[allow(unused_imports)]
pub(crate) use animated::{apply_param_shape_animated, has_param_shape_motion, AnimatedParams};

/// `Numerical.EPSILON` of Paper.js (`Point.isZero`).
const PAPER_EPSILON: f64 = 1e-12;

/// `paramShape.box`: the axis-aligned generator frame `{ x1, y1, x2, y2 }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParamBox {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Paper.js `path.bounds` as `{ x, y, width, height }` (`right = x + width`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PathBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `paramShape.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamShapeKind {
    Rect,
    Ellipse,
    Star,
}

/// The declared intent `data.paramShape` without its box. An absent field is
/// `None`; the baseline reads it with `||` (0, -0 and NaN count as absent)
/// except `sweep` and `innerRatio`, which only test for `undefined`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ParamShape {
    Rect {
        tl: Option<f64>,
        tr: Option<f64>,
        br: Option<f64>,
        bl: Option<f64>,
    },
    Ellipse {
        start_angle: Option<f64>,
        sweep: Option<f64>,
        inner_radius: Option<f64>,
    },
    Star {
        point_count: Option<f64>,
        inner_ratio: Option<f64>,
        corner_radius: Option<f64>,
    },
}

impl ParamShape {
    pub(crate) fn kind(&self) -> ParamShapeKind {
        match self {
            ParamShape::Rect { .. } => ParamShapeKind::Rect,
            ParamShape::Ellipse { .. } => ParamShapeKind::Ellipse,
            ParamShape::Star { .. } => ParamShapeKind::Star,
        }
    }
}

/// The result of a static rebuild: the new anchors and the box to store
/// (the stored one, or the one stamped from the current bounds).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppliedShape {
    pub segments: Vec<ParamSegment>,
    pub box_: ParamBox,
}

/// The result of `syncParamShapeBoxOnScale`: the new box, and for a rect the
/// rebuilt anchors (`applyParamShapeRect`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScaleSync {
    pub box_: ParamBox,
    pub rebuilt: Option<Vec<ParamSegment>>,
}

impl ParamBox {
    /// `stampParamShapeBox`: `{ x1: b.left, y1: b.top, x2: b.right, y2: b.bottom }`.
    pub(crate) fn stamp(b: PathBounds) -> ParamBox {
        ParamBox {
            x1: b.x,
            y1: b.y,
            x2: b.x + b.width,
            y2: b.y + b.height,
        }
    }

    fn values(&self) -> [f64; 4] {
        [self.x1, self.y1, self.x2, self.y2]
    }
}

/// Paper.js `path.bounds` of a path whose handles are all exactly zero (every
/// rect, line, polygon and uncorrected star); `None` for an empty path or any
/// non-zero handle, where the curve extrema are the bounds provider's job.
pub(crate) fn straight_bounds(segs: &[ParamSegment]) -> Option<PathBounds> {
    let first = segs.first()?;
    if segs
        .iter()
        .any(|s| s.handle_in != [0.0, 0.0] || s.handle_out != [0.0, 0.0])
    {
        return None;
    }
    let (mut min, mut max) = (first.point, first.point);
    for s in segs {
        let [x, y] = s.point;
        if x < min[0] {
            min[0] = x;
        }
        if x > max[0] {
            max[0] = x;
        }
        if y < min[1] {
            min[1] = y;
        }
        if y > max[1] {
            max[1] = y;
        }
    }
    Some(PathBounds {
        x: min[0],
        y: min[1],
        width: max[0] - min[0],
        height: max[1] - min[1],
    })
}

/// `syncParamShapeBoxOnTranslate`; a shape without a box is left alone.
pub(crate) fn sync_box_on_translate(
    stored: Option<ParamBox>,
    dx: f64,
    dy: f64,
) -> Result<Option<ParamBox>, ParamGeomError> {
    check_finite(&[dx, dy])?;
    let Some(b) = stored else { return Ok(None) };
    check_finite(&b.values())?;
    Ok(Some(ParamBox {
        x1: b.x1 + dx,
        y1: b.y1 + dy,
        x2: b.x2 + dx,
        y2: b.y2 + dy,
    }))
}

/// `syncParamShapeBoxOnScale(p, sx, sy, anchor)`: the box is scaled about the
/// anchor and re-ordered (min/max) so a negative factor keeps `x1 <= x2`; a
/// rect is rebuilt from the new box with its unchanged radii and `box_angle`.
/// `Ok(None)` when the shape has no box.
pub(crate) fn sync_on_scale(
    shape: &ParamShape,
    stored: Option<ParamBox>,
    box_angle: f64,
    sx: f64,
    sy: f64,
    anchor: [f64; 2],
) -> Result<Option<ScaleSync>, ParamGeomError> {
    check_finite(&[sx, sy, anchor[0], anchor[1], box_angle])?;
    let Some(b) = stored else { return Ok(None) };
    check_finite(&b.values())?;
    let nx1 = anchor[0] + (b.x1 - anchor[0]) * sx;
    let nx2 = anchor[0] + (b.x2 - anchor[0]) * sx;
    let ny1 = anchor[1] + (b.y1 - anchor[1]) * sy;
    let ny2 = anchor[1] + (b.y2 - anchor[1]) * sy;
    let scaled = ParamBox {
        x1: js_min(nx1, nx2),
        y1: js_min(ny1, ny2),
        x2: js_max(nx1, nx2),
        y2: js_max(ny1, ny2),
    };
    let rebuilt = match shape {
        ParamShape::Rect { .. } => {
            Some(apply_param_shape(shape, Some(scaled), None, box_angle)?.segments)
        }
        _ => None,
    };
    Ok(Some(ScaleSync {
        box_: scaled,
        rebuilt,
    }))
}

fn is_zero(v: f64) -> bool {
    v.abs() <= PAPER_EPSILON
}

/// `path.rotate(angle, centre)` on a Paper.js path of segments: `Matrix.rotate`
/// from identity, `Item.transform` (identity matrices return the path as is),
/// then every segment through `_transformCoordinates(matrix, coords, true)`:
/// the point and the non-zero handles (as absolute points) are transformed and
/// the handles taken back relative; a "zero" handle (at most 1e-12 per axis) is
/// skipped and keeps its stored value.
#[allow(clippy::identity_op, clippy::erasing_op)]
pub(crate) fn rotate_segments(
    segs: &[ParamSegment],
    angle_deg: f64,
    centre: [f64; 2],
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[angle_deg, centre[0], centre[1]])?;
    // The chain below deliberately keeps Paper's `x * 1 + y * 0` terms: they preserve signed
    // zeros and the operand order bit for bit.
    let t = angle_deg * (PI / 180.0);
    let (i, r) = (t.cos(), t.sin());
    let (cx, cy) = (centre[0], centre[1]);
    let s = cx - cx * i + cy * r;
    let n = cy - cx * r - cy * i;
    // new Matrix().rotate(): the identity (a 1, b 0, c 0, d 1) times the rotation.
    let (ma, mb) = (i * 1.0 + r * 0.0, i * 0.0 + r * 1.0);
    let (mc, md) = (-r * 1.0 + i * 0.0, -r * 0.0 + i * 1.0);
    let (mtx, mty) = (0.0 + (s * 1.0 + n * 0.0), 0.0 + (s * 0.0 + n * 1.0));
    if ma == 1.0 && mb == 0.0 && mc == 0.0 && md == 1.0 && mtx == 0.0 && mty == 0.0 {
        return Ok(segs.to_vec());
    }
    // item._matrix (identity).prepend(matrix, true)
    let a = ma * 1.0 + mc * 0.0;
    let c = ma * 0.0 + mc * 1.0;
    let b = mb * 1.0 + md * 0.0;
    let d = mb * 0.0 + md * 1.0;
    let tx = ma * 0.0 + mc * 0.0 + mtx;
    let ty = mb * 0.0 + md * 0.0 + mty;
    let apply = |p: [f64; 2]| [p[0] * a + p[1] * c + tx, p[0] * b + p[1] * d + ty];
    Ok(segs
        .iter()
        .map(|sg| {
            let point = apply(sg.point);
            let mut out = *sg;
            out.point = point;
            if !(is_zero(sg.handle_in[0]) && is_zero(sg.handle_in[1])) {
                let h = apply([sg.handle_in[0] + sg.point[0], sg.handle_in[1] + sg.point[1]]);
                out.handle_in = [h[0] - point[0], h[1] - point[1]];
            }
            if !(is_zero(sg.handle_out[0]) && is_zero(sg.handle_out[1])) {
                let h = apply([
                    sg.handle_out[0] + sg.point[0],
                    sg.handle_out[1] + sg.point[1],
                ]);
                out.handle_out = [h[0] - point[0], h[1] - point[1]];
            }
            out
        })
        .collect())
}

/// `reapplyParamShapeAngle(path, box)`: `data.boxAngle || 0` rotated about the box centre.
fn reapply_angle(
    segs: Vec<ParamSegment>,
    b: ParamBox,
    box_angle: f64,
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    if box_angle == 0.0 || box_angle.is_nan() {
        return Ok(segs);
    }
    rotate_segments(&segs, box_angle, [(b.x1 + b.x2) / 2.0, (b.y1 + b.y2) / 2.0])
}

/// `applyParamShapeEllipse` / `applyParamShapeStar` / `applyParamShapeRect`: rebuild
/// the anchors from the stored box (`paramShapeBoxOf` stamps a missing box from
/// `current_bounds`). `box_angle` is `data.boxAngle`; the ellipse ignores it.
pub(crate) fn apply_param_shape(
    shape: &ParamShape,
    stored: Option<ParamBox>,
    current_bounds: Option<PathBounds>,
    box_angle: f64,
) -> Result<AppliedShape, ParamGeomError> {
    check_finite(&[box_angle])?;
    let b = match (stored, current_bounds) {
        (Some(b), _) => b,
        (None, Some(bounds)) => ParamBox::stamp(bounds),
        (None, None) => return Err(ParamGeomError::MissingBox),
    };
    check_finite(&b.values())?;
    let (cx, cy) = ((b.x1 + b.x2) / 2.0, (b.y1 + b.y2) / 2.0);
    let segments = match *shape {
        ParamShape::Ellipse {
            start_angle,
            sweep,
            inner_radius,
        } => {
            let rx = (b.x2 - b.x1) / 2.0;
            let ry = (b.y2 - b.y1) / 2.0;
            let start = or_default(start_angle, 0.0);
            build_arc_ellipse(cx, cy, rx, ry, start, sweep.unwrap_or(359.9), inner_radius)?
        }
        ParamShape::Star {
            point_count,
            inner_ratio,
            corner_radius,
        } => {
            let outer_r = js_min(b.x2 - b.x1, b.y2 - b.y1) / 2.0;
            let built =
                build_star_polygon(cx, cy, outer_r, point_count, inner_ratio, corner_radius)?;
            reapply_angle(built, b, box_angle)?
        }
        ParamShape::Rect { tl, tr, br, bl } => {
            let radii = [tl, tr, br, bl].map(|r| or_default(r, 0.0));
            let built = build_round_rect(b.x1, b.y1, b.x2, b.y2, radii)?;
            reapply_angle(built, b, box_angle)?
        }
    };
    Ok(AppliedShape { segments, box_: b })
}
