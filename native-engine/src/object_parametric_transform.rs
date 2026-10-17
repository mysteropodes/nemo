//! Move and scale of a stored segment list with the Paper.js arithmetic (G22/#2107, leaf L01-G).
//!
//! The baseline selection gestures move a shape with `p.translate(delta)` and resize it with
//! `p.scale(sx, sy, anchor)` (select-bridge.js 372/1949/2225/2340, timeline.js 3957/3972), each followed by
//! the `paramShape.box` sync of tools.js that `object_parametric_geometry_box.rs` already holds. This file is
//! the missing half: what Paper.js does to the SEGMENTS. A translate or scale is
//! `Item.transform(new Matrix().translate|scale(...))`: the matrix is built with Paper's own operation order
//! (`Matrix#translate`, `Matrix#scale` with a centre), an identity matrix returns the path untouched, anything
//! else is prepended to the item's identity matrix and run through `Segment#_transformCoordinates`, which
//! transforms the point and every NON-zero handle as an absolute point and takes the handle back relative.
//! That round trip is not the plain `handle * factor` (it changes the last bit of a handle), so it is
//! reproduced literally, together with the `Point.isZero` rule (at most 1e-12 per axis keeps the stored handle).
//!
//! Everything here is plain `f64` arithmetic without trigonometry, so the results are bit-identical to the
//! oracle (`-0` included). Native only: a non-finite input, or a result that overflows to a non-finite
//! number, is a typed `ParamGeomError::NonFinite` and nothing is produced.
use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    check_finite, ParamGeomError, ParamSegment,
};

/// `Numerical.EPSILON` of Paper.js (`Point.isZero`).
const PAPER_EPSILON: f64 = 1e-12;

/// The six values of a Paper.js `Matrix`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl Matrix {
    fn identity() -> Matrix {
        Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// `Matrix#isIdentity` (strict `===` on the six values; `-0 === 0`).
    fn is_identity(&self) -> bool {
        self.a == 1.0
            && self.b == 0.0
            && self.c == 0.0
            && self.d == 1.0
            && self.tx == 0.0
            && self.ty == 0.0
    }

    /// `Matrix#translate(x, y)`: `_tx += x * _a + y * _c; _ty += x * _b + y * _d`.
    fn translate(&mut self, x: f64, y: f64) {
        self.tx += x * self.a + y * self.c;
        self.ty += x * self.b + y * self.d;
    }

    /// `new Matrix().translate(dx, dy)`.
    fn translated(dx: f64, dy: f64) -> Matrix {
        let mut m = Matrix::identity();
        m.translate(dx, dy);
        m
    }

    /// `new Matrix().scale(sx, sy, centre)`: translate to the centre, scale the four linear terms, translate
    /// back by `centre.negate()`.
    fn scaled(sx: f64, sy: f64, centre: [f64; 2]) -> Matrix {
        let mut m = Matrix::identity();
        m.translate(centre[0], centre[1]);
        m.a *= sx;
        m.b *= sx;
        m.c *= sy;
        m.d *= sy;
        m.translate(-centre[0], -centre[1]);
        m
    }
}

fn is_zero(v: f64) -> bool {
    v.abs() <= PAPER_EPSILON
}

fn finite_segments(segs: &[ParamSegment]) -> Result<(), ParamGeomError> {
    for s in segs {
        check_finite(&[
            s.point[0],
            s.point[1],
            s.handle_in[0],
            s.handle_in[1],
            s.handle_out[0],
            s.handle_out[1],
        ])?;
    }
    Ok(())
}

/// `Item#transform(matrix)` of a Path with `applyMatrix`: the early return for an identity matrix, then
/// `item._matrix.prepend(matrix)` against the identity (Paper's `prepend` formulas with the identity terms
/// kept so signed zeros survive) and `Segment#_transformCoordinates` for every segment.
#[allow(clippy::identity_op, clippy::erasing_op)]
fn transform(m: &Matrix, segs: &[ParamSegment]) -> Result<Vec<ParamSegment>, ParamGeomError> {
    if m.is_identity() {
        return Ok(segs.to_vec());
    }
    let a = m.a * 1.0 + m.c * 0.0;
    let c = m.a * 0.0 + m.c * 1.0;
    let b = m.b * 1.0 + m.d * 0.0;
    let d = m.b * 0.0 + m.d * 1.0;
    let tx = m.a * 0.0 + m.c * 0.0 + m.tx;
    let ty = m.b * 0.0 + m.d * 0.0 + m.ty;
    let apply = |p: [f64; 2]| [p[0] * a + p[1] * c + tx, p[0] * b + p[1] * d + ty];
    let moved: Vec<ParamSegment> = segs
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
        .collect();
    finite_segments(&moved)?;
    Ok(moved)
}

/// `p.translate(new Point(dx, dy))`.
pub(crate) fn translate_segments(
    segs: &[ParamSegment],
    dx: f64,
    dy: f64,
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[dx, dy])?;
    finite_segments(segs)?;
    transform(&Matrix::translated(dx, dy), segs)
}

/// `p.scale(sx, sy, new Point(anchor))`.
pub(crate) fn scale_segments(
    segs: &[ParamSegment],
    sx: f64,
    sy: f64,
    anchor: [f64; 2],
) -> Result<Vec<ParamSegment>, ParamGeomError> {
    check_finite(&[sx, sy, anchor[0], anchor[1]])?;
    finite_segments(segs)?;
    transform(&Matrix::scaled(sx, sy, anchor), segs)
}

#[cfg(test)]
#[path = "../tests/object_parametric_transform.rs"]
mod tests;
