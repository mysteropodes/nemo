//! Element Motion bake for combine and Flatten (`elementMotionBakedClone` of
//! `src/js/tools.js` at fbe4b8a3, G19, L02-I).
//!
//! A member animated by per-shape element Motion is drawn through a separate
//! transform, so a boolean over its raw rest geometry would snap it back. The
//! baseline therefore clones the member and applies, in this order and with the
//! same pivot for the first two steps, scale about the pivot, rotate about the
//! pivot (degrees), translate. The pivot is the centre of the member's bounds
//! plus the anchor offset, measured once on the unmoved geometry. Each step
//! keeps Paper.js's `Matrix` arithmetic so the points match the library to
//! rounding. The Motion values themselves come from the element-Motion
//! evaluator, which is not part of this module.
use crate::object_document::object_group_kernels::object_boolean_ops::{MultiPolygon, Point};

/// What `SMMotion.elementMotionAt` returns for the element at the current frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ElementMat {
    pub(crate) dx: f64,
    pub(crate) dy: f64,
    pub(crate) rot: f64,
    pub(crate) sx: f64,
    pub(crate) sy: f64,
    /// Anchor offset added to the bounds centre (`elMat.ax || 0`).
    pub(crate) ax: f64,
    pub(crate) ay: f64,
}

impl ElementMat {
    /// The baseline's `noop`: nothing to apply, the original geometry is used as is.
    pub(crate) fn is_noop(&self) -> bool {
        self.dx == 0.0 && self.dy == 0.0 && self.rot == 0.0 && self.sx == 1.0 && self.sy == 1.0
    }
}

fn bounds_centre(rings: &[&[Point]]) -> Point {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for ring in rings {
        for p in ring.iter() {
            min_x = min_x.min(p[0]);
            min_y = min_y.min(p[1]);
            max_x = max_x.max(p[0]);
            max_y = max_y.max(p[1]);
        }
    }
    [min_x + (max_x - min_x) / 2.0, min_y + (max_y - min_y) / 2.0]
}

/// The three Paper steps over one point.
fn apply(mat: &ElementMat, pivot: Point, point: Point) -> Point {
    // Matrix().scale(sx, sy, pivot): translate(pivot), scale, translate(-pivot).
    let tx = pivot[0] + (-pivot[0]) * mat.sx;
    let ty = pivot[1] + (-pivot[1]) * mat.sy;
    let x1 = point[0] * mat.sx + point[1] * 0.0 + tx;
    let y1 = point[0] * 0.0 + point[1] * mat.sy + ty;
    // Matrix().rotate(angle, pivot).
    // Paper: `t *= Math.PI / 180`, i.e. t * (PI / 180), not (t * PI) / 180.
    let angle = mat.rot * (std::f64::consts::PI / 180.0);
    let cos = angle.cos();
    let sin = angle.sin();
    let rtx = pivot[0] - pivot[0] * cos + pivot[1] * sin;
    let rty = pivot[1] - pivot[0] * sin - pivot[1] * cos;
    let x2 = x1 * cos + y1 * (-sin) + rtx;
    let y2 = x1 * sin + y1 * cos + rty;
    // Matrix().translate(dx, dy).
    [x2 * 1.0 + y2 * 0.0 + mat.dx, x2 * 0.0 + y2 * 1.0 + mat.dy]
}

/// Bake one ring (its own bounds give the pivot).
pub(crate) fn bake_ring(ring: &[Point], mat: &ElementMat) -> Vec<Point> {
    if mat.is_noop() {
        return ring.to_vec();
    }
    let centre = bounds_centre(&[ring]);
    let pivot = [centre[0] + mat.ax, centre[1] + mat.ay];
    ring.iter().map(|&p| apply(mat, pivot, p)).collect()
}

/// Bake a whole operand: ONE pivot from the bounds of every ring it owns.
pub(crate) fn bake_multi(multi: &MultiPolygon, mat: &ElementMat) -> MultiPolygon {
    if mat.is_noop() {
        return multi.clone();
    }
    let mut rings: Vec<&[Point]> = Vec::new();
    for polygon in multi {
        rings.push(&polygon.exterior);
        for hole in &polygon.holes {
            rings.push(hole);
        }
    }
    let centre = bounds_centre(&rings);
    let pivot = [centre[0] + mat.ax, centre[1] + mat.ay];
    multi
        .iter()
        .map(|polygon| {
            let mut baked = polygon.clone();
            baked.exterior = polygon
                .exterior
                .iter()
                .map(|&p| apply(mat, pivot, p))
                .collect();
            baked.holes = polygon
                .holes
                .iter()
                .map(|hole| hole.iter().map(|&p| apply(mat, pivot, p)).collect())
                .collect();
            baked
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/object_boolean_ops_bake.rs"]
mod tests;
