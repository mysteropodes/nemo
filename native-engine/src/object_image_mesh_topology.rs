//! Image-mesh topology kernel (F17), pure and I/O free.
//!
//! Mirrors `createMesh`, `setOutline`, `rebuild`, `triangulate` and `signedArea2` of
//! `src/js/image-mesh.js` at fbe4b8a3. A mesh is supplied by reference and every result
//! is a new owned [`MeshTopology`]: the baseline mutates the stored mesh in place, here
//! the caller publishes (or discards) the returned value, so a refusal can never leave
//! a half-rebuilt mesh behind. No store, id, raster, document or command is read.
//!
//! The geometry comes from the accepted siblings: `pointInPoly`, `densify` and
//! `resampleOffsets` from `object_image_mesh_geometry` and the Delaunator 5.0.1 port from
//! `object_image_mesh_delaunay`. Arithmetic keeps the JS operand order, so every result is
//! bit-identical to the baseline oracle (no trigonometry is involved).
//!
//! Semantics kept on purpose:
//! - `verts[0..outline.len())` ARE the outline points, in order; interior grid points
//!   `(gx/cols, gy/rows)` for `gx in 1..cols-1`, `gy in 1..rows-1` follow, and only when
//!   they lie inside the outline. `offsets` is a separate array and is never folded into
//!   `verts`.
//! - `createMesh` clamps `cols`/`rows` as `max(1, min(64, v || 4))`: `0`, `-0` and `NaN`
//!   fall to 4, the result is NOT truncated (3.5 stays 3.5 and builds a ragged outline).
//! - `rebuild` reads the density as `max(0, min(64, v | 0))` (JS `ToInt32` first, so
//!   `2^32 + 3` is 3 and `2^31` is 0) but leaves the stored `cols`/`rows` untouched.
//! - `setOutline` applies a truthy `opts.cols`/`opts.rows` to the mesh BEFORE it sizes the
//!   densify step (`1 / max(cols || 4, rows || 4)`), WITHOUT clamping it; a polygon of
//!   fewer than three points is refused (`None`); a negative signed area reverses it.
//! - `rebuild` carries the old offsets through `resampleOffsets` only when some old offset
//!   is truthy (non-zero, not `NaN`); otherwise the new offsets are positive zeros.
//! - the triangle filter drops every Delaunay triangle whose centroid is outside the
//!   outline. This is unconstrained Delaunay, so a deeply concave outline can lose a
//!   sliver of coverage (the documented v1 limit; pinned by the `lost` oracle row).
//!
//! Deliberate differences from the baseline (typed refusals, never invented values):
//! a densify step the baseline could never finish, and a non-finite coordinate (where
//! the baseline Delaunator returns garbage), are [`TopologyError`]s. Any `DelaunayError` is
//! a typed `Triangulation` refusal: the baseline wraps Delaunator in try/catch and returns
//! `[]`, but that catch never fires for finite numeric coordinates, so there is no silent
//! empty-on-error fallback here.
//!
//! Not provided: the store, ids, `attach`/`propagate`, serialization, edit commands,
//! offsets editing, or the scene/export join.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay::{
    delaunay_from, DelaunayError,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    densify, point_in_poly, resample_offsets, MeshGeometryError,
};

/// Densities the native record admits (`cols`/`rows` are integers in `1..=64`).
const MIN_RECORD_DENSITY: f64 = 1.0;
const MAX_RECORD_DENSITY: f64 = 64.0;
/// `Math.min(64, ...)` of the baseline.
const MAX_DENSITY: f64 = 64.0;
/// `opts.cols || 4` of the baseline.
const DEFAULT_DENSITY: f64 = 4.0;

/// Typed refusal of an input the baseline cannot finish or turns into garbage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TopologyError {
    /// The outline densify step would insert an unbounded number of points.
    Densify(MeshGeometryError),
    /// The Delaunay port refused the point set (non-finite coordinate, ...).
    Triangulation(DelaunayError),
    /// `cols`/`rows` is not an integer in `1..=64`, so no native record can hold it.
    DensityNotRecordable,
}

/// One editable mesh as the baseline holds it, in normalized `(0,0)..(1,1)` space.
/// `cols`/`rows` are the stored JavaScript numbers (not necessarily integers).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshTopology {
    pub(crate) outline: Vec<[f64; 2]>,
    pub(crate) cols: f64,
    pub(crate) rows: f64,
    pub(crate) verts: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) offsets: Vec<[f64; 2]>,
}

/// `opts.cols` / `opts.rows` of `createMesh` and `setOutline`. `None` is an absent
/// property; `Some(0.0)`, `Some(-0.0)` and `Some(NaN)` are present but falsy.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DensityOpts {
    pub(crate) cols: Option<f64>,
    pub(crate) rows: Option<f64>,
}

impl MeshTopology {
    /// The `(cols, rows)` pair a native record can store, or a typed refusal.
    pub(crate) fn record_density(&self) -> Result<(u32, u32), TopologyError> {
        let fits =
            |v: f64| v.fract() == 0.0 && (MIN_RECORD_DENSITY..=MAX_RECORD_DENSITY).contains(&v);
        if fits(self.cols) && fits(self.rows) {
            Ok((self.cols as u32, self.rows as u32))
        } else {
            Err(TopologyError::DensityNotRecordable)
        }
    }
}

/// JavaScript truthiness of a number.
fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// `v || fallback`.
fn or_default(v: f64, fallback: f64) -> f64 {
    if truthy(v) {
        v
    } else {
        fallback
    }
}

/// JavaScript `ToInt32` (the `| 0` of `rebuild`).
fn to_int32(v: f64) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let m = v.trunc() % 4_294_967_296.0;
    let m = if m < 0.0 { m + 4_294_967_296.0 } else { m };
    if m >= 2_147_483_648.0 {
        (m - 4_294_967_296.0) as i32
    } else {
        m as i32
    }
}

/// `Math.max(0, Math.min(64, v | 0))`.
fn rebuild_density(v: f64) -> i32 {
    to_int32(v).clamp(0, MAX_DENSITY as i32)
}

/// `Math.max(1, Math.min(64, opt || 4))`.
fn create_density(opt: Option<f64>) -> f64 {
    let v = or_default(opt.unwrap_or(f64::NAN), DEFAULT_DENSITY);
    v.clamp(1.0, MAX_DENSITY)
}

/// `signedArea2`: twice the signed area; only its sign is used.
pub(crate) fn signed_area2(poly: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        a += (poly[j][0] * poly[i][1]) - (poly[i][0] * poly[j][1]);
        j = i;
    }
    a
}

/// `triangulate(pts, outlineCount)`: the Delaunay triangles of `pts` whose centroid lies
/// inside `pts[..outline_count]`. Fewer than three points give no triangles.
pub(crate) fn triangulate(
    pts: &[[f64; 2]],
    outline_count: usize,
) -> Result<Vec<u32>, TopologyError> {
    if pts.len() < 3 {
        return Ok(Vec::new());
    }
    let delaunay = delaunay_from(pts).map_err(TopologyError::Triangulation)?;
    let poly = &pts[..outline_count.min(pts.len())];
    let mut out = Vec::new();
    for t in delaunay.triangles.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let cx = (pts[a][0] + pts[b][0] + pts[c][0]) / 3.0;
        let cy = (pts[a][1] + pts[b][1] + pts[c][1]) / 3.0;
        if !point_in_poly(poly, cx, cy) {
            continue;
        }
        out.extend_from_slice(&[t[0], t[1], t[2]]);
    }
    Ok(out)
}

/// `rebuild(mesh)`: new `verts`/`tris`/`offsets` from the outline and the stored density.
/// The stored `cols`/`rows` are kept as they are.
pub(crate) fn rebuild(mesh: &MeshTopology) -> Result<MeshTopology, TopologyError> {
    let cols = rebuild_density(mesh.cols);
    let rows = rebuild_density(mesh.rows);
    let outline = mesh.outline.clone();
    let mut pts = outline.clone();
    for gy in 1..rows {
        for gx in 1..cols {
            let u = f64::from(gx) / f64::from(cols);
            let v = f64::from(gy) / f64::from(rows);
            if point_in_poly(&outline, u, v) {
                pts.push([u, v]);
            }
        }
    }
    let tris = triangulate(&pts, outline.len())?;
    let sculpted = mesh.offsets.iter().any(|o| truthy(o[0]) || truthy(o[1]));
    let offsets = if sculpted {
        resample_offsets(&mesh.verts, Some(mesh.offsets.as_slice()), &mesh.tris, &pts)
    } else {
        pts.iter().map(|_| [0.0, 0.0]).collect()
    };
    Ok(MeshTopology {
        outline,
        cols: mesh.cols,
        rows: mesh.rows,
        verts: pts,
        tris,
        offsets,
    })
}

/// `createMesh(opts)`: a rectangular outline over the whole image plus a `cols x rows` grid.
pub(crate) fn create_mesh(opts: DensityOpts) -> Result<MeshTopology, TopologyError> {
    let cols = create_density(opts.cols);
    let rows = create_density(opts.rows);
    let mut outline = Vec::new();
    let mut x = 0.0;
    while x < cols {
        outline.push([x / cols, 0.0]);
        x += 1.0;
    }
    let mut y = 0.0;
    while y < rows {
        outline.push([1.0, y / rows]);
        y += 1.0;
    }
    let mut x = cols;
    while x > 0.0 {
        outline.push([x / cols, 1.0]);
        x -= 1.0;
    }
    let mut y = rows;
    while y > 0.0 {
        outline.push([0.0, y / rows]);
        y -= 1.0;
    }
    rebuild(&MeshTopology {
        outline,
        cols,
        rows,
        verts: Vec::new(),
        tris: Vec::new(),
        offsets: Vec::new(),
    })
}

/// `setOutline(meshId, poly, opts)` over an already resolved mesh: `Ok(None)` for fewer
/// than three points (the baseline `null`), otherwise the retriangulated copy.
pub(crate) fn set_outline(
    mesh: &MeshTopology,
    poly: &[[f64; 2]],
    opts: DensityOpts,
) -> Result<Option<MeshTopology>, TopologyError> {
    if poly.len() < 3 {
        return Ok(None);
    }
    let mut p = poly.to_vec();
    if signed_area2(&p) < 0.0 {
        p.reverse();
    }
    let mut next = mesh.clone();
    if let Some(cols) = opts.cols.filter(|v| truthy(*v)) {
        next.cols = cols;
    }
    if let Some(rows) = opts.rows.filter(|v| truthy(*v)) {
        next.rows = rows;
    }
    let step =
        1.0 / or_default(next.cols, DEFAULT_DENSITY).max(or_default(next.rows, DEFAULT_DENSITY));
    next.outline = densify(&p, step).map_err(TopologyError::Densify)?;
    rebuild(&next).map(Some)
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_topology_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_image_mesh_topology.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_topology_limits.rs"]
mod limits_tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_topology_controls.rs"]
mod controls_tests;
