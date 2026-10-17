//! A tiny CPU reference rasterizer for a [`MeshDrawPlan`] (test support, L03-S; 0 tests).
//!
//! It answers one question per sample point: which SOURCE image coordinate does the plan paint
//! there, if any? The answer follows the replayed scene's semantics without a GPU: the outer clip
//! (the outline polygon) masks everything, triangles paint in order so the last dilated clip that
//! contains the point wins, and the painted pixel is the one `place` maps back to that point (the
//! inverse of the image affine). It has no antialiasing, so it proves geometry (where pixels land and
//! what is masked) and not the GPU's coverage; the GPU pixel A/B stays with the compositor leaf.

use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    AffineCoeffs, MeshDrawPlan,
};

const EDGE_EPS: f64 = 1e-9;

/// Crossing-number point-in-polygon test (the outline is a simple polygon here).
pub(crate) fn inside_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Edges inclusive, either winding.
pub(crate) fn inside_triangle(t: &[[f64; 2]; 3], p: [f64; 2]) -> bool {
    let s = [
        cross(t[0], t[1], p),
        cross(t[1], t[2], p),
        cross(t[2], t[0], p),
    ];
    s.iter().all(|v| *v >= -EDGE_EPS) || s.iter().all(|v| *v <= EDGE_EPS)
}

/// The image-space point an affine `[a, b, c, d, tx, ty]` maps onto `p`.
pub(crate) fn invert_apply(m: &AffineCoeffs, p: [f64; 2]) -> Option<[f64; 2]> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let (dx, dy) = (p[0] - m[4], p[1] - m[5]);
    Some([
        (m[3] * dx - m[2] * dy) / det,
        (-m[1] * dx + m[0] * dy) / det,
    ])
}

/// The source coordinate the mesh plan paints at world point `p` (identity view).
pub(crate) fn mesh_source_at(plan: &MeshDrawPlan, p: [f64; 2]) -> Option<[f64; 2]> {
    if !inside_polygon(&plan.outline.points, p) {
        return None;
    }
    let mut hit = None;
    for triangle in &plan.triangles {
        if inside_triangle(&triangle.clip, p) {
            hit = invert_apply(&triangle.place, p);
        }
    }
    hit
}

/// The source coordinate of the plain, unrotated image drawn into `rect = (x, y, width, height)`.
pub(crate) fn plain_source_at(
    rect: (f64, f64, f64, f64),
    image: (u32, u32),
    p: [f64; 2],
) -> Option<[f64; 2]> {
    let (x, y, w, h) = rect;
    if p[0] < x || p[0] >= x + w || p[1] < y || p[1] >= y + h {
        return None;
    }
    Some([
        (p[0] - x) / w * f64::from(image.0),
        (p[1] - y) / h * f64::from(image.1),
    ])
}

/// The texel a source coordinate falls in.
pub(crate) fn texel(s: [f64; 2]) -> (i64, i64) {
    (s[0].floor() as i64, s[1].floor() as i64)
}

/// Pixel centres of the half-open pixel box `[x0, x1) x [y0, y1)`, row by row.
pub(crate) fn pixel_centers(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            out.push([x as f64 + 0.5, y as f64 + 0.5]);
        }
    }
    out
}
