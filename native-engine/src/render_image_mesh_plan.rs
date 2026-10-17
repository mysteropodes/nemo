//! Image-mesh draw plan (F17/F18), pure and I/O free.
//!
//! Reproduces the numbers of the shipped renderer `draw_image_mesh`
//! (`geometry-wasm/src/engine.rs`, identical at fbe4b8a3 and at the pinned
//! base) as typed data, without importing vello or kurbo: the outline clip,
//! the per-triangle image affine and the seam-dilated destination triangle. A
//! GPU consumer replays the plan as one outer clip layer (carrying the opacity
//! ONCE) and one filled triangle per entry, so a scene render and an export
//! render of the same payload draw the same geometry.
//!
//! The input is exactly the L03-A scene payload (`verts`, `uvs`, `tris`,
//! `outline`); [`MeshPlanInput::from_payload`] borrows one.
//!
//! Semantics kept from the baseline:
//! - fewer than 3 vertices, fewer than 3 triangle indices or fewer uvs than
//!   vertices draws nothing ([`PlanSkip::IncompleteMesh`]); an image of zero
//!   width or height draws nothing ([`PlanSkip::EmptyImage`]); an outline index
//!   without a vertex aborts the whole draw ([`PlanSkip::OutlineVertexMissing`]);
//! - an outline of 3 or more indices is the clip polygon in order; a shorter
//!   one falls back to the bounding box of ALL vertices (`f64::min`/`max`, so
//!   a NaN coordinate is ignored), corners in kurbo `Rect` path order;
//! - triangles are the complete index triples in order; a triple with an index
//!   at or past the vertex count, or whose source triangle (uv times the image
//!   size, in image pixels) has `|det| < 1e-12`, is skipped. A NaN `det` is NOT
//!   skipped: the comparison is false and NaN flows through, as in the baseline;
//! - `place` is the unique affine `[a, b, c, d, tx, ty]` taking each source
//!   vertex onto its destination vertex; it is never dilated;
//! - the destination triangle is scaled about its centroid by `k`, where
//!   `k = 1` for `area2 <= 1e-12` (or NaN) and otherwise
//!   `1 + dilate * 3 * longest_edge / area2`, with non-finite `k` and `k > 4`
//!   both set to 4; `dilate = 0.5 / max(sqrt(|det(view)|), 1e-9)` is half a
//!   device pixel in world units;
//! - opacity is a property of the plan, never of a triangle.
//!
//! Not provided: the vello scene calls, the image upload or retention, the
//! mesh record, and the scene/export join that feeds a payload and a view.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::ScenePayload;

/// A point as `[x, y]`.
pub(crate) type Point2 = [f64; 2];

/// An affine as `[a, b, c, d, tx, ty]` (kurbo `Affine::as_coeffs` order).
pub(crate) type AffineCoeffs = [f64; 6];

/// Largest scale factor a destination triangle is dilated by.
pub(crate) const MAX_DILATION_FACTOR: f64 = 4.0;

/// Smallest view scale; a degenerate view must not divide by zero.
const MIN_VIEW_SCALE: f64 = 1e-9;
/// A source triangle with a smaller `|det|` has no invertible map.
const MIN_SOURCE_DET: f64 = 1e-12;
/// A destination triangle with an `area2` at or below this is not dilated.
const MIN_DEST_AREA2: f64 = 1e-12;
/// Half a device pixel.
const HALF_PIXEL: f64 = 0.5;

/// The mesh as the plan reads it, borrowed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshPlanInput<'a> {
    pub(crate) verts: &'a [Point2],
    pub(crate) uvs: &'a [Point2],
    pub(crate) tris: &'a [u32],
    pub(crate) outline: &'a [u32],
}

impl<'a> MeshPlanInput<'a> {
    /// Borrows an L03-A scene payload without copying.
    pub(crate) fn from_payload(payload: &'a ScenePayload) -> Self {
        Self {
            verts: payload.verts.as_slice(),
            uvs: payload.uvs.as_slice(),
            tris: payload.tris.as_slice(),
            outline: payload.outline.as_slice(),
        }
    }
}

/// Why the baseline draws nothing (no layer is pushed at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanSkip {
    /// `n < 3 || tris.len() < 3 || uvs.len() < n`.
    IncompleteMesh,
    /// Image width or height is zero.
    EmptyImage,
    /// An outline index has no vertex.
    OutlineVertexMissing,
}

/// How the clip was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutlineKind {
    /// The outline indices, in order.
    Polygon,
    /// Bounding box of all vertices (outline shorter than 3).
    BoundingBox,
}

/// The closed clip of the outer layer, in world units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutlineClip {
    pub(crate) kind: OutlineKind,
    pub(crate) points: Vec<Point2>,
}

/// One drawn triangle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrianglePlan {
    /// Position of the triple in `tris` divided by 3.
    pub(crate) triangle: usize,
    /// Source vertices in image pixels (uv times image size).
    pub(crate) source: [Point2; 3],
    /// Destination vertices (`verts`), undilated.
    pub(crate) dest: [Point2; 3],
    /// Image affine, `source -> dest`, never dilated.
    pub(crate) place: AffineCoeffs,
    /// Scale factor about the destination centroid.
    pub(crate) dilation: f64,
    /// Destination triangle after the dilation: the clip of the fill.
    pub(crate) clip: [Point2; 3],
}

/// What a consumer replays: one clip layer, then each triangle fill.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshDrawPlan {
    /// Carried once, by the outer layer.
    pub(crate) opacity: f32,
    pub(crate) outline: OutlineClip,
    pub(crate) triangles: Vec<TrianglePlan>,
}

/// Half a device pixel in world units: `0.5 / max(sqrt(|det(view)|), 1e-9)`.
pub(crate) fn half_pixel_dilation(view: &AffineCoeffs) -> f64 {
    let view_scale = (view[0] * view[3] - view[1] * view[2])
        .abs()
        .sqrt()
        .max(MIN_VIEW_SCALE);
    HALF_PIXEL / view_scale
}

fn outline_clip(input: &MeshPlanInput<'_>) -> Result<OutlineClip, PlanSkip> {
    if input.outline.len() >= 3 {
        let mut points = Vec::with_capacity(input.outline.len());
        for &index in input.outline {
            let Some(p) = input.verts.get(index as usize) else {
                return Err(PlanSkip::OutlineVertexMissing);
            };
            points.push([p[0], p[1]]);
        }
        return Ok(OutlineClip {
            kind: OutlineKind::Polygon,
            points,
        });
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in input.verts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    Ok(OutlineClip {
        kind: OutlineKind::BoundingBox,
        points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
    })
}

/// The affine with `A(source[i]) = dest[i]`; `None` when the source triangle
/// is degenerate (`|det| < 1e-12`).
fn place_affine(source: &[Point2; 3], dest: &[Point2; 3]) -> Option<AffineCoeffs> {
    let (e1x, e1y) = (source[1][0] - source[0][0], source[1][1] - source[0][1]);
    let (e2x, e2y) = (source[2][0] - source[0][0], source[2][1] - source[0][1]);
    let det = e1x * e2y - e2x * e1y;
    if det.abs() < MIN_SOURCE_DET {
        return None;
    }
    let (f1x, f1y) = (dest[1][0] - dest[0][0], dest[1][1] - dest[0][1]);
    let (f2x, f2y) = (dest[2][0] - dest[0][0], dest[2][1] - dest[0][1]);
    let a = (f1x * e2y - f2x * e1y) / det;
    let b = (f1y * e2y - f2y * e1y) / det;
    let c = (f2x * e1x - f1x * e2x) / det;
    let d = (f2y * e1x - f1y * e2x) / det;
    let tx = dest[0][0] - (a * source[0][0] + c * source[0][1]);
    let ty = dest[0][1] - (b * source[0][0] + d * source[0][1]);
    Some([a, b, c, d, tx, ty])
}

/// Scale about the centroid by the factor that moves the longest edge out by
/// `dilate`; returns the factor and the dilated triangle.
fn dilated_clip(dest: &[Point2; 3], dilate: f64) -> (f64, [Point2; 3]) {
    let gx = (dest[0][0] + dest[1][0] + dest[2][0]) / 3.0;
    let gy = (dest[0][1] + dest[1][1] + dest[2][1]) / 3.0;
    let area2 = ((dest[1][0] - dest[0][0]) * (dest[2][1] - dest[0][1])
        - (dest[2][0] - dest[0][0]) * (dest[1][1] - dest[0][1]))
        .abs();
    let mut k = 1.0_f64;
    if area2 > MIN_DEST_AREA2 {
        let l0 = ((dest[1][0] - dest[0][0]).powi(2) + (dest[1][1] - dest[0][1]).powi(2)).sqrt();
        let l1 = ((dest[2][0] - dest[1][0]).powi(2) + (dest[2][1] - dest[1][1]).powi(2)).sqrt();
        let l2 = ((dest[0][0] - dest[2][0]).powi(2) + (dest[0][1] - dest[2][1]).powi(2)).sqrt();
        let longest = l0.max(l1).max(l2);
        k = 1.0 + dilate * 3.0 * longest / area2;
        if !k.is_finite() || k > MAX_DILATION_FACTOR {
            k = MAX_DILATION_FACTOR;
        }
    }
    let grow = |p: &Point2| [gx + (p[0] - gx) * k, gy + (p[1] - gy) * k];
    (k, [grow(&dest[0]), grow(&dest[1]), grow(&dest[2])])
}

/// Plans one mesh draw. `Err` is the baseline's "draws nothing"; an `Ok` plan
/// may still hold no triangle (every triple skipped), which draws nothing
/// visible. `view` is the item's view transform.
pub(crate) fn plan_mesh_draw(
    input: &MeshPlanInput<'_>,
    image_width: u32,
    image_height: u32,
    opacity: f32,
    view: &AffineCoeffs,
) -> Result<MeshDrawPlan, PlanSkip> {
    let n = input.verts.len();
    if n < 3 || input.tris.len() < 3 || input.uvs.len() < n {
        return Err(PlanSkip::IncompleteMesh);
    }
    let iw = f64::from(image_width);
    let ih = f64::from(image_height);
    if iw <= 0.0 || ih <= 0.0 {
        return Err(PlanSkip::EmptyImage);
    }
    let dilate = half_pixel_dilation(view);
    let outline = outline_clip(input)?;
    let mut triangles = Vec::new();
    for (ordinal, triple) in input.tris.chunks_exact(3).enumerate() {
        let corners = [triple[0] as usize, triple[1] as usize, triple[2] as usize];
        if corners.iter().any(|&index| index >= n) {
            continue;
        }
        let source = corners.map(|i| [input.uvs[i][0] * iw, input.uvs[i][1] * ih]);
        let dest = corners.map(|i| [input.verts[i][0], input.verts[i][1]]);
        let Some(place) = place_affine(&source, &dest) else {
            continue;
        };
        let (dilation, clip) = dilated_clip(&dest, dilate);
        triangles.push(TrianglePlan {
            triangle: ordinal,
            source,
            dest,
            place,
            dilation,
            clip,
        });
    }
    Ok(MeshDrawPlan {
        opacity,
        outline,
        triangles,
    })
}

#[cfg(test)]
#[path = "../tests/render_image_mesh_plan_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/render_image_mesh_plan_literals.rs"]
mod literal_tests;
#[cfg(test)]
#[path = "../tests/render_image_mesh_plan_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/render_image_mesh_plan.rs"]
mod tests;
