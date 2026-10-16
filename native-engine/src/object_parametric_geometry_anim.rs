//! Motion-driven rebuild of a parametric shape (G22/#2107, leaf L01-B): `applyParamShapeFor` and
//! `hasParamShapeMotionFor` of `motion.js` at fbe4b8a3. Split out of `object_parametric_geometry_box.rs`,
//! which re-exports everything here.
//!
//! The rebuild reads the min/max rectangle of the STORED anchors (post path-effect bounds in the baseline), not the
//! stored `paramShape.box`; each animated track replaces the stored parameter, an explicit animated 0 stays 0, and
//! the animated ellipse inner radius and star inner ratio are PERCENT values (the stored fields are 0..1 and fall
//! back through `Math.round(value * 100)`, so a stored 0.335 reads as 0.34).

use crate::evaluation::object_vector_kernels::object_parametric_geometry::boxed::{
    ParamShape, ParamShapeKind,
};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    build_arc_ellipse, build_round_rect, build_star_polygon, check_finite, js_max, js_min,
    or_default, ParamGeomError, ParamSegment,
};
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::js_round;

/// The Motion tracks of `applyParamShapeFor`: `Some` where `isAnimated(holder, key)`,
/// holding `valueAtFrame(holder, key, frame)[0]`. Arc inner and star inner are percents.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct AnimatedParams {
    pub corner_tl: Option<f64>,
    pub corner_tr: Option<f64>,
    pub corner_br: Option<f64>,
    pub corner_bl: Option<f64>,
    pub arc_start: Option<f64>,
    pub arc_sweep: Option<f64>,
    pub arc_inner: Option<f64>,
    pub star_inner: Option<f64>,
    pub star_corner: Option<f64>,
}

/// `hasParamShapeMotionFor`: whether any track of the holder's kind is animated.
pub(crate) fn has_param_shape_motion(
    holder: Option<ParamShapeKind>,
    anim: &AnimatedParams,
) -> bool {
    match holder {
        Some(ParamShapeKind::Rect) => [
            anim.corner_tl,
            anim.corner_tr,
            anim.corner_br,
            anim.corner_bl,
        ]
        .iter()
        .any(Option::is_some),
        Some(ParamShapeKind::Ellipse) => [anim.arc_start, anim.arc_sweep, anim.arc_inner]
            .iter()
            .any(Option::is_some),
        Some(ParamShapeKind::Star) => [anim.star_inner, anim.star_corner]
            .iter()
            .any(Option::is_some),
        None => false,
    }
}

/// `applyParamShapeFor`: rebuild the anchors of `stored` (its min/max rectangle
/// is the frame) with each animated track replacing the stored parameter.
pub(crate) fn apply_param_shape_animated(
    shape: &ParamShape,
    stored: &[ParamSegment],
    anim: &AnimatedParams,
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    if stored.is_empty() {
        return Err(ParamGeomError::EmptyGeometry);
    }
    let fold = |axis: usize, init: f64, f: fn(f64, f64) -> f64| {
        stored.iter().map(|s| s.point[axis]).fold(init, f)
    };
    let x1 = fold(0, f64::INFINITY, js_min);
    let x2 = fold(0, f64::NEG_INFINITY, js_max);
    let y1 = fold(1, f64::INFINITY, js_min);
    let y2 = fold(1, f64::NEG_INFINITY, js_max);
    check_finite(&[x1, x2, y1, y2])?;
    match *shape {
        ParamShape::Rect { tl, tr, br, bl } => {
            let cv = |track: Option<f64>, fallback: Option<f64>| {
                track.unwrap_or(or_default(fallback, 0.0))
            };
            let radii = [
                cv(anim.corner_tl, tl),
                cv(anim.corner_tr, tr),
                cv(anim.corner_br, br),
                cv(anim.corner_bl, bl),
            ];
            build_round_rect(x1, y1, x2, y2, radii)
        }
        ParamShape::Ellipse {
            start_angle,
            sweep,
            inner_radius,
        } => {
            let start = anim.arc_start.unwrap_or(or_default(start_angle, 0.0));
            let sweep_a = anim.arc_sweep.unwrap_or(sweep.unwrap_or(359.9));
            let inner_pct = anim
                .arc_inner
                .unwrap_or(js_round(or_default(inner_radius, 0.0) * 100.0));
            let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let (rx, ry) = ((x2 - x1) / 2.0, (y2 - y1) / 2.0);
            build_arc_ellipse(cx, cy, rx, ry, start, sweep_a, Some(inner_pct / 100.0))
        }
        ParamShape::Star {
            point_count,
            inner_ratio,
            corner_radius,
        } => {
            let inner_pct = anim
                .star_inner
                .unwrap_or(js_round(inner_ratio.unwrap_or(0.5) * 100.0));
            let corner = anim.star_corner.unwrap_or(or_default(corner_radius, 0.0));
            let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let outer_r = js_min(x2 - x1, y2 - y1) / 2.0;
            build_star_polygon(
                cx,
                cy,
                outer_r,
                Some(or_default(point_count, 5.0)),
                Some(inner_pct / 100.0),
                Some(corner),
            )
        }
    }
}
