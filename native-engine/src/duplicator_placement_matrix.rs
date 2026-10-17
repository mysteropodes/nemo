//! Duplicator placement matrix and its application to native geometry (L04-D, I08).
//!
//! Mirrors `dupMatrixFromDescriptor`, `applyMatrixToStrokeData` and
//! `_boundsCenterOfStrokes` of `src/js/app.js` at baseline `fbe4b8a3`.
//!
//! - The matrix is ONE closed-form expression: scale in the pivot's local
//!   frame, rotate around the pivot, translate last (`a = cos * sx`,
//!   `b = sin * sx`, `c = -sin * sy`, `d = cos * sy`, `tx/ty` rebased on the
//!   pivot). `rot`, `dx` and `dy` read as `x || 0`; `sx` and `sy` are used raw.
//! - A point takes the full affine (`x*a + y*c + tx`, `x*b + y*d + ty`, the
//!   operand order of Paper.js `Matrix.transform`, checked bit for bit in the
//!   oracle); a handle is a vector, so it takes the matrix minus the image of
//!   the origin, computed exactly as the baseline does (transform, then
//!   subtract the transformed `(0, 0)`), which is NOT always bit-identical to
//!   the linear part alone.
//! - The pivot of a stroke set is the center of the ANCHOR POINTS' bounds
//!   (handles and curve extrema are ignored); a set without any anchor takes
//!   the canvas center.
//!
//! Nothing here mutates its input: a transformed geometry is a new value whose
//! kind, closure, coordinate space and handle space are the source's. A result
//! the JSON number type cannot hold (NaN, infinity: the baseline serialized
//! those as `null`) is a typed error, never a silent zero.
//!
//! `sin` and `cos` come from the platform `libm`, V8 uses its own fdlibm port,
//! so rows with a nonzero rotation are compared within a bounded relative
//! error; rows whose rotation is falsy are exact.

use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::{
    js_max, js_min, truthy,
};
use crate::object_document::{ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment};
use serde_json::Number;
use std::f64::consts::PI;

/// Why a geometry could not be transformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatrixError {
    /// A source coordinate is not a finite number.
    SourceNotFinite,
    /// A transformed coordinate is NaN or infinite.
    ResultNotFinite,
}

/// The pivot a clone is scaled and rotated around.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pivot {
    pub x: f64,
    pub y: f64,
}

/// The fields of the placement descriptor the matrix reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Descriptor {
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
    pub dx: f64,
    pub dy: f64,
}

/// The affine `[a c tx; b d ty]` of Paper.js `new Matrix(a, b, c, d, tx, ty)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

/// `x || 0`.
fn or_zero(x: f64) -> f64 {
    if truthy(x) {
        x
    } else {
        0.0
    }
}

/// `dupMatrixFromDescriptor(m, pivot)`.
pub(crate) fn matrix_from_descriptor(m: &Descriptor, pivot: Pivot) -> Matrix {
    matrix_from_descriptor_with(m, pivot, true)
}

/// [`matrix_from_descriptor`]; `rebase_on_pivot = false` drops the pivot terms
/// of the translation (a control feeds it to prove the oracle notices).
pub(crate) fn matrix_from_descriptor_with(
    m: &Descriptor,
    pivot: Pivot,
    rebase_on_pivot: bool,
) -> Matrix {
    let rad = or_zero(m.rot) * PI / 180.0;
    let cs = rad.cos();
    let sn = rad.sin();
    let a = cs * m.sx;
    let b = sn * m.sx;
    let c = -sn * m.sy;
    let d = cs * m.sy;
    let (tx, ty) = if rebase_on_pivot {
        (
            pivot.x + or_zero(m.dx) - a * pivot.x - c * pivot.y,
            pivot.y + or_zero(m.dy) - b * pivot.x - d * pivot.y,
        )
    } else {
        (or_zero(m.dx), or_zero(m.dy))
    };
    Matrix { a, b, c, d, tx, ty }
}

impl Matrix {
    /// Paper.js `matrix.transform(new Point(x, y))`.
    pub(crate) fn transform(&self, x: f64, y: f64) -> (f64, f64) {
        (
            x * self.a + y * self.c + self.tx,
            x * self.b + y * self.d + self.ty,
        )
    }

    /// The baseline's vector transform: the image of `(x, y)` minus the image
    /// of `(0, 0)` (`m.transform(v).subtract(m.transform(origin))`).
    pub(crate) fn transform_vector(&self, x: f64, y: f64) -> (f64, f64) {
        let (ox, oy) = self.transform(0.0, 0.0);
        let (px, py) = self.transform(x, y);
        (px - ox, py - oy)
    }
}

fn source(n: &Number) -> Result<f64, MatrixError> {
    n.as_f64()
        .filter(|v| v.is_finite())
        .ok_or(MatrixError::SourceNotFinite)
}

fn result(x: f64) -> Result<Number, MatrixError> {
    Number::from_f64(x).ok_or(MatrixError::ResultNotFinite)
}

fn point_of(p: &ObjectPoint) -> Result<(f64, f64), MatrixError> {
    Ok((source(&p.x)?, source(&p.y)?))
}

fn point_out(xy: (f64, f64)) -> Result<ObjectPoint, MatrixError> {
    Ok(ObjectPoint {
        x: result(xy.0)?,
        y: result(xy.1)?,
    })
}

/// One segment: the point takes the affine, the two handles the vector transform.
pub(crate) fn transform_segment(
    m: &Matrix,
    s: &ObjectSegment,
) -> Result<ObjectSegment, MatrixError> {
    let (px, py) = point_of(&s.point)?;
    let (ix, iy) = point_of(&s.handle_in)?;
    let (ox, oy) = point_of(&s.handle_out)?;
    Ok(ObjectSegment {
        point: point_out(m.transform(px, py))?,
        handle_in: point_out(m.transform_vector(ix, iy))?,
        handle_out: point_out(m.transform_vector(ox, oy))?,
    })
}

/// The geometry with every segment transformed; the other fields are the source's.
pub(crate) fn transform_geometry(
    m: &Matrix,
    g: &ObjectGeometry,
) -> Result<ObjectGeometry, MatrixError> {
    let segments = g
        .segments
        .iter()
        .map(|s| transform_segment(m, s))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ObjectGeometry {
        kind: g.kind.clone(),
        closed: g.closed,
        coordinate_space: g.coordinate_space.clone(),
        handle_space: g.handle_space.clone(),
        segments,
    })
}

/// `_boundsCenterOfStrokes(strokes)` over native records: the center of the
/// anchor points' bounds, or the canvas center when there is no anchor.
pub(crate) fn bounds_center(
    records: &[ObjectRecord],
    canvas_w: f64,
    canvas_h: f64,
) -> Result<Pivot, MatrixError> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for record in records {
        for segment in &record.geometry.segments {
            let (x, y) = point_of(&segment.point)?;
            min_x = js_min(min_x, x);
            max_x = js_max(max_x, x);
            min_y = js_min(min_y, y);
            max_y = js_max(max_y, y);
        }
    }
    if min_x == f64::INFINITY {
        return Ok(Pivot {
            x: canvas_w / 2.0,
            y: canvas_h / 2.0,
        });
    }
    Ok(Pivot {
        x: (min_x + max_x) / 2.0,
        y: (min_y + max_y) / 2.0,
    })
}

/// A raster stroke's placement: its center, size and optional opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RasterRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: Option<f64>,
}

/// The `isRaster` branch of `applyMatrixToStrokeData`: the center takes the
/// affine and the size the magnitude of the matrix's two linear columns (a
/// rotation therefore degrades to a scale, as in the baseline).
pub(crate) fn transform_raster_rect(m: &Matrix, r: &RasterRect) -> RasterRect {
    let (x, y) = m.transform(r.x, r.y);
    let scale_x = (m.a * m.a + m.b * m.b).sqrt();
    let scale_y = (m.c * m.c + m.d * m.d).sqrt();
    RasterRect {
        x,
        y,
        width: r.width * scale_x,
        height: r.height * scale_y,
        opacity: r.opacity,
    }
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_matrix_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_matrix_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/duplicator_placement_matrix.rs"]
mod tests;
