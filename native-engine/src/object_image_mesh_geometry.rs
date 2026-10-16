//! Image-mesh normalized-space geometry kernel (F17/F18), pure and I/O free.
//!
//! Mirrors `src/js/image-mesh.js` at fbe4b8a3: `worldOf`, `normalizedOf`,
//! `worldVerts`, `scenePayloadFor`, `pointInPoly`, `densify`,
//! `resampleOffsets` and the scene-JSON rounders `R2`/`R5`. The mesh is
//! supplied as borrowed slices and every result is owned, so nothing here
//! mutates a stored mesh; no store, id, raster or document is read. Arithmetic
//! keeps the JS operand order, so results are bit-identical wherever the
//! platform libm agrees with V8 (only `cos`/`sin` of a non-zero rotation can
//! differ, by an ulp).
//!
//! Semantics kept on purpose:
//! - a mesh lives in normalized `(0,0)..(1,1)` space over the item's display
//!   rect; `verts` hold REST positions and `offsets` are a separate array,
//!   deformed = rest + offset (+ the optional animated pose), never folded;
//! - `verts[0..outline.len())` are the outline points, in order;
//! - the rect is TOP-LEFT based here (`cx = x + width / 2`), unlike the test
//!   corpus helper `meshVertexWorld`, which reads `x`,`y` as the centre;
//! - `rotation` reads `rect.rotation || 0` (`0`, `-0`, `NaN` count as 0),
//!   converted as `r * PI / 180`; a rotation about the rect centre follows;
//! - `normalizedOf` divides by `width || 1` / `height || 1` (a zero or NaN
//!   size divides by 1), `worldOf` has no such guard;
//! - an offset slice shorter than `verts` reads `[0, 0]` for the missing
//!   entries; a pose is added per vertex when its entry is `Some`
//!   (`poseAt(i)` returning a falsy value leaves the offset alone);
//! - the scene payload rounds world positions with `R2` and the (rest) uvs
//!   with `R5`, both `Math.round` (half toward +infinity, `-0.5` gives `-0`),
//!   copies the triangle list and numbers the outline `0..outline.len()`;
//!   it is `None` for a mesh without triangles;
//! - `resample_offsets` is barycentric inside the first old triangle that
//!   contains the new point (weights down to `-1e-6` count as inside),
//!   falls back to the first nearest old vertex, and is all zeros when there
//!   are no old points or no old offsets.
//!
//! One deliberate difference: `densify` with a step that would insert an
//! unbounded number of points (JavaScript never returns) is refused.
//!
//! Not provided: the store, `createMesh`/`rebuild`/`setOutline` (Delaunay),
//! `propagate`/`attach`, serialization, or any command.

use std::f64::consts::PI;

/// Most points `densify` inserts into one edge before it refuses (the
/// baseline would run out of memory long before).
pub(crate) const MAX_INSERTS_PER_EDGE: f64 = 1_000_000.0;

/// Typed refusal of an input the baseline cannot finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshGeometryError {
    /// `densify` step too small: more than [`MAX_INSERTS_PER_EDGE`] points
    /// (or an infinite count, for a step of zero) on one edge.
    DensifyUnbounded,
}

/// The item's final display rect (`x`,`y` = top-left, `rotation` in degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeshRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: f64,
}

/// A stored mesh, borrowed. `outline.len()` is the outline vertex count.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshView<'a> {
    pub(crate) outline: &'a [[f64; 2]],
    pub(crate) verts: &'a [[f64; 2]],
    pub(crate) offsets: &'a [[f64; 2]],
    pub(crate) tris: &'a [u32],
}

/// Animated per-vertex offsets (`poseAt`): `None` entries add nothing.
pub(crate) type Pose<'a> = Option<&'a [Option<[f64; 2]>]>;

/// The `mesh` payload of an image scene item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScenePayload {
    pub(crate) verts: Vec<[f64; 2]>,
    pub(crate) uvs: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) outline: Vec<u32>,
}

/// `Math.round`: nearest integer, ties toward +infinity, sign of zero kept.
pub(crate) fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        rounded
    }
}

/// `R2`: scene JSON rounds positions to 2 decimals.
pub(crate) fn r2(n: f64) -> f64 {
    js_round(n * 100.0) / 100.0
}

/// `R5`: scene JSON rounds uvs to 5 decimals.
pub(crate) fn r5(n: f64) -> f64 {
    js_round(n * 100000.0) / 100000.0
}

/// `(rect.rotation || 0) * Math.PI / 180`.
fn rotation_radians(rect: &MeshRect) -> f64 {
    let degrees = if rect.rotation == 0.0 || rect.rotation.is_nan() {
        0.0
    } else {
        rect.rotation
    };
    degrees * PI / 180.0
}

/// Rotation and centre of a rect, computed once per mesh like the JS callers.
#[derive(Debug, Clone, Copy)]
struct RectBasis {
    cos: f64,
    sin: f64,
    cx: f64,
    cy: f64,
}

fn rect_basis(rect: &MeshRect) -> RectBasis {
    let rad = rotation_radians(rect);
    RectBasis {
        cos: rad.cos(),
        sin: rad.sin(),
        cx: rect.x + rect.width / 2.0,
        cy: rect.y + rect.height / 2.0,
    }
}

/// `worldOf`: a normalized point through the rect (scale, rotate about the
/// centre, translate), with no guard on a zero size.
fn world_of(rect: &MeshRect, u: f64, v: f64, b: &RectBasis) -> [f64; 2] {
    let lx = (u - 0.5) * rect.width;
    let ly = (v - 0.5) * rect.height;
    [
        b.cx + lx * b.cos - ly * b.sin,
        b.cy + lx * b.sin + ly * b.cos,
    ]
}

/// `normalizedOf`: a world point back into the rect's normalized space.
pub(crate) fn normalized_of(rect: &MeshRect, wx: f64, wy: f64) -> [f64; 2] {
    let b = rect_basis(rect);
    let dx = wx - b.cx;
    let dy = wy - b.cy;
    let lx = dx * b.cos + dy * b.sin;
    let ly = -dx * b.sin + dy * b.cos;
    let width = if rect.width == 0.0 || rect.width.is_nan() {
        1.0
    } else {
        rect.width
    };
    let height = if rect.height == 0.0 || rect.height.is_nan() {
        1.0
    } else {
        rect.height
    };
    [lx / width + 0.5, ly / height + 0.5]
}

/// Deformed normalized position of vertex `i`: rest + stored offset + pose.
fn deformed(mesh: &MeshView, i: usize, pose: Pose) -> [f64; 2] {
    let rest = mesh.verts[i];
    let off = mesh.offsets.get(i).copied().unwrap_or([0.0, 0.0]);
    let (mut du, mut dv) = (off[0], off[1]);
    if let Some(a) = pose.and_then(|p| p.get(i)).copied().flatten() {
        du += a[0];
        dv += a[1];
    }
    [rest[0] + du, rest[1] + dv]
}

/// `worldVerts`: deformed vertex positions in world space.
pub(crate) fn world_verts(mesh: &MeshView, rect: &MeshRect, pose: Pose) -> Vec<[f64; 2]> {
    let b = rect_basis(rect);
    (0..mesh.verts.len())
        .map(|i| {
            let d = deformed(mesh, i, pose);
            world_of(rect, d[0], d[1], &b)
        })
        .collect()
}

/// `scenePayloadFor` for an already resolved mesh and rect; `None` when the
/// mesh has no triangles. Uvs are the REST positions: the offset moves where
/// pixels land, never where they are sampled.
pub(crate) fn scene_payload(mesh: &MeshView, rect: &MeshRect, pose: Pose) -> Option<ScenePayload> {
    if mesh.tris.is_empty() {
        return None;
    }
    let b = rect_basis(rect);
    let mut verts = Vec::with_capacity(mesh.verts.len());
    let mut uvs = Vec::with_capacity(mesh.verts.len());
    for (i, rest) in mesh.verts.iter().enumerate() {
        let d = deformed(mesh, i, pose);
        let w = world_of(rect, d[0], d[1], &b);
        verts.push([r2(w[0]), r2(w[1])]);
        uvs.push([r5(rest[0]), r5(rest[1])]);
    }
    Some(ScenePayload {
        verts,
        uvs,
        tris: mesh.tris.to_vec(),
        outline: (0..mesh.outline.len() as u32).collect(),
    })
}

/// `pointInPoly`: crossing-number test, points on an edge are arbitrary.
pub(crate) fn point_in_poly(poly: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// `densify`: extra points along every closed-polygon edge longer than
/// `max_len` (`floor(len / max_len)` evenly spaced). A non-positive or NaN
/// step inserts nothing; a step that would insert more than
/// [`MAX_INSERTS_PER_EDGE`] points on one edge is refused.
pub(crate) fn densify(poly: &[[f64; 2]], max_len: f64) -> Result<Vec<[f64; 2]>, MeshGeometryError> {
    let mut out = Vec::with_capacity(poly.len());
    for (i, p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        out.push([p[0], p[1]]);
        let dx = q[0] - p[0];
        let dy = q[1] - p[1];
        let len = (dx * dx + dy * dy).sqrt();
        let n = (len / max_len).floor();
        if n > MAX_INSERTS_PER_EDGE {
            return Err(MeshGeometryError::DensifyUnbounded);
        }
        let mut s = 1.0;
        while s <= n {
            let t = s / (n + 1.0);
            out.push([p[0] + dx * t, p[1] + dy * t]);
            s += 1.0;
        }
    }
    Ok(out)
}

/// `resampleOffsets`: the old piecewise-linear offset field sampled at the
/// new points. `old_offs` of `None` (JS falsy) or no old points give zeros.
pub(crate) fn resample_offsets(
    old_pts: &[[f64; 2]],
    old_offs: Option<&[[f64; 2]]>,
    old_tris: &[u32],
    new_pts: &[[f64; 2]],
) -> Vec<[f64; 2]> {
    let offs = match old_offs {
        Some(offs) if !old_pts.is_empty() => offs,
        _ => return new_pts.iter().map(|_| [0.0, 0.0]).collect(),
    };
    let off_at = |i: usize| offs.get(i).copied().unwrap_or([0.0, 0.0]);
    new_pts
        .iter()
        .map(|q| resample_at(old_pts, &off_at, old_tris, q[0], q[1]))
        .collect()
}

fn resample_at(
    old_pts: &[[f64; 2]],
    off_at: &dyn Fn(usize) -> [f64; 2],
    old_tris: &[u32],
    qx: f64,
    qy: f64,
) -> [f64; 2] {
    for tri in old_tris.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (Some(a), Some(b), Some(c)) = (old_pts.get(ia), old_pts.get(ib), old_pts.get(ic))
        else {
            continue;
        };
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d == 0.0 || d.is_nan() {
            continue;
        }
        let w0 = ((b[1] - c[1]) * (qx - c[0]) + (c[0] - b[0]) * (qy - c[1])) / d;
        let w1 = ((c[1] - a[1]) * (qx - c[0]) + (a[0] - c[0]) * (qy - c[1])) / d;
        let w2 = 1.0 - w0 - w1;
        if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
            continue;
        }
        let (oa, ob, oc) = (off_at(ia), off_at(ib), off_at(ic));
        return [
            oa[0] * w0 + ob[0] * w1 + oc[0] * w2,
            oa[1] * w0 + ob[1] * w1 + oc[1] * w2,
        ];
    }
    let mut best: Option<usize> = None;
    let mut best_d = f64::INFINITY;
    for (i, p) in old_pts.iter().enumerate() {
        let dx = p[0] - qx;
        let dy = p[1] - qy;
        let dd = dx * dx + dy * dy;
        if dd < best_d {
            best_d = dd;
            best = Some(i);
        }
    }
    best.map_or([0.0, 0.0], off_at)
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_geometry_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_image_mesh_geometry.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_geometry_controls.rs"]
mod controls_tests;
