//! Rig deform kernel (I12), pure and I/O free: `applyRigDeform` and its `displaceOne`.
//!
//! Mirrors `applyRigDeform` of the baseline at fbe4b8a3 (`src/js/app.js`). A rig holds binds;
//! each bind carries the REST positions it was made on and, per rest point, the weight entries
//! that pull it. The kernel reads the rig and returns what the baseline wrote into the live
//! objects, as a detached value: it never touches a rest array, a weight list, a bone or a
//! mesh, and a failure produces nothing (the baseline throws after earlier binds already
//! mutated their targets; here the whole call fails with the offending bind index).
//!
//! One displacement formula serves all three destinations (stroke boundary, stroke
//! `centerSegments`, mesh), as in the baseline: every resolvable entry adds
//! `w * (cur - rest)` (non-rotate), or `w * ((cur + R(da) * (p - rest)) - p)` (rotate, `R` the
//! rotation by the entry's own `da`), the result is divided by `sumW` only when `sumW > 1`
//! (otherwise by 1: a single bone's own falloff must not be undone), and an entry whose anchor
//! is `null` is skipped and not folded into `sumW`.
//! - STROKE BOUNDARY (`PathBind`): point = rest + displacement; for every loop index up to
//!   `min(live segments, rest points)` the handles are written only when the bind has
//!   `restHandles` and that entry's handle is non-null; with rotate and `sumWRot > 0` the REST
//!   handle is rotated by `avgDa = sumDaW / sumWRot`, otherwise it is copied verbatim; the live
//!   handles are never an input (idempotent, nothing compounds). Absent `restHandles` leaves
//!   every handle untouched (`None` in the output).
//! - CENTER SEGMENTS (`CenterBind`): the same formula over `centerRest` / `centerWeights`,
//!   handles taken from the rest record. Because `exportJSON` drops them, the caller supplies
//!   them only when it has them (runtime-only); the linked-fill copy of the boundary is not
//!   part of this kernel.
//! - MESH (`MeshBind`): the displaced WORLD point is mapped back with `normalizedOf` over the
//!   raster rect, and the result is `normalized - stored rest vertex` so a rigged image keeps
//!   working when it is moved; a displacement of `(0, 0)` (either component `0`, `-0` or NaN
//!   counts as zero, and BOTH must) writes `[0, 0]`. The output covers `min(verts, rest)`
//!   vertices and the caller leaves the rest of its offsets alone.
//!
//! Skipped (the baseline's early `return`): a path bind with no live segment list, a mesh bind
//! with no live raster/mesh. Float semantics: operand order is the baseline's, `cos`/`sin`/`atan2`
//! ride on the platform libm (they may differ from V8 by an ulp), everything else is exact.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    normalized_of, MeshRect,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    AnchorResolver, DeformBone, DeformError,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{Point2, WeightEntry};

/// `restHandles[i]`: `{i, o}` where a zero handle was stored as `null`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RestHandle {
    pub(crate) handle_in: Option<Point2>,
    pub(crate) handle_out: Option<Point2>,
}

/// `centerRest[i]`: `{point, handleIn, handleOut}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CenterRest {
    pub(crate) point: Point2,
    pub(crate) handle_in: Option<Point2>,
    pub(crate) handle_out: Option<Point2>,
}

/// A vector brush's `centerRest` / `centerWeights` and the live `centerSegments` length.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CenterBind<'a> {
    pub(crate) rest: &'a [CenterRest],
    pub(crate) weights: &'a [Vec<WeightEntry>],
    pub(crate) live_len: usize,
}

/// A stroke bind. `live_len` is the live segment count (`None`: no live stroke, skipped);
/// `rest_handles` `None` is a rig bound before handles were stored; a list shorter than the
/// rest points leaves the later entries undefined.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PathBind<'a> {
    pub(crate) rest: &'a [Point2],
    pub(crate) rest_handles: Option<&'a [RestHandle]>,
    pub(crate) weights: &'a [Vec<WeightEntry>],
    pub(crate) rotate: bool,
    pub(crate) live_len: Option<usize>,
    pub(crate) center: Option<CenterBind<'a>>,
}

/// The live raster of a mesh bind: the mesh's stored REST vertices and the display rect.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshTarget<'a> {
    pub(crate) verts: &'a [Point2],
    pub(crate) rect: MeshRect,
}

/// A mesh bind (`rest` in world space); `target` `None` is a bind with no live raster.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshBind<'a> {
    pub(crate) rest: &'a [Point2],
    pub(crate) weights: &'a [Vec<WeightEntry>],
    pub(crate) rotate: bool,
    pub(crate) target: Option<MeshTarget<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BindInput<'a> {
    Path(PathBind<'a>),
    Mesh(MeshBind<'a>),
}

/// One written segment: the point, and each handle only if the baseline assigned it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SegmentPose {
    pub(crate) point: Point2,
    pub(crate) handle_in: Option<Point2>,
    pub(crate) handle_out: Option<Point2>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathPose {
    pub(crate) segments: Vec<SegmentPose>,
    pub(crate) center: Option<Vec<SegmentPose>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BindPose {
    Skipped,
    Path(PathPose),
    /// The written offsets of the first `min(verts, rest)` mesh vertices.
    Mesh(Vec<Point2>),
}

/// The first bind the baseline would have thrown on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeformFailure {
    pub(crate) bind_index: usize,
    pub(crate) error: DeformError,
}

struct Sums {
    dx: f64,
    dy: f64,
    sum_w: f64,
    sum_da_w: f64,
    sum_w_rot: f64,
}

fn entry_weight(entry: &WeightEntry) -> f64 {
    match entry {
        WeightEntry::Control(c) => c.w,
        WeightEntry::Offset(o) => o.w,
    }
}

fn entries_at(weights: &[Vec<WeightEntry>], i: usize) -> &[WeightEntry] {
    weights.get(i).map(Vec::as_slice).unwrap_or(&[])
}

/// The weighted pull of every resolvable entry on `rest_pt` (`displaceOne` / the loop bodies).
fn accumulate(
    resolver: &mut AnchorResolver<'_>,
    rotate: bool,
    rest_pt: Point2,
    entries: &[WeightEntry],
) -> Result<Sums, DeformError> {
    let mut s = Sums {
        dx: 0.0,
        dy: 0.0,
        sum_w: 0.0,
        sum_da_w: 0.0,
        sum_w_rot: 0.0,
    };
    for entry in entries {
        let Some(a) = resolver.resolve(entry)? else {
            continue;
        };
        let w = entry_weight(entry);
        s.sum_w += w;
        if rotate {
            let cos_a = a.da.cos();
            let sin_a = a.da.sin();
            let off_x = rest_pt[0] - a.rest_x;
            let off_y = rest_pt[1] - a.rest_y;
            let r_off_x = off_x * cos_a - off_y * sin_a;
            let r_off_y = off_x * sin_a + off_y * cos_a;
            s.dx += w * ((a.cur_x + r_off_x) - rest_pt[0]);
            s.dy += w * ((a.cur_y + r_off_y) - rest_pt[1]);
            s.sum_da_w += a.da * w;
            s.sum_w_rot += w;
        } else {
            s.dx += w * (a.cur_x - a.rest_x);
            s.dy += w * (a.cur_y - a.rest_y);
        }
    }
    Ok(s)
}

/// `norm = sumW > 1 ? sumW : 1` (a NaN sum divides by 1).
fn divisor(sum_w: f64) -> f64 {
    if sum_w > 1.0 {
        sum_w
    } else {
        1.0
    }
}

/// Rest handle -> written handle: rotated by the weighted-average angle when any rotating
/// entry contributed (`sumWRot > 0`), else copied.
fn posed_handle(handle: Option<Point2>, s: &Sums) -> Option<Point2> {
    let h = handle?;
    if s.sum_w_rot > 0.0 {
        let avg_da = s.sum_da_w / s.sum_w_rot;
        let hc = avg_da.cos();
        let hs = avg_da.sin();
        Some([h[0] * hc - h[1] * hs, h[0] * hs + h[1] * hc])
    } else {
        Some(h)
    }
}

fn posed_point(rest_pt: Point2, s: &Sums) -> Point2 {
    let norm = divisor(s.sum_w);
    [rest_pt[0] + s.dx / norm, rest_pt[1] + s.dy / norm]
}

fn zero_like(x: f64) -> bool {
    x == 0.0 || x.is_nan()
}

/// The stroke boundary and (optionally) the `centerSegments` of one bind.
/// `Ok(None)` is the baseline's skip of a bind without live segments.
pub(crate) fn deform_path_bind(
    resolver: &mut AnchorResolver<'_>,
    bind: &PathBind,
) -> Result<Option<PathPose>, DeformError> {
    let Some(live_len) = bind.live_len else {
        return Ok(None);
    };
    let n = live_len.min(bind.rest.len());
    let mut segments = Vec::with_capacity(n);
    for i in 0..n {
        let rest_pt = bind.rest[i];
        let s = accumulate(resolver, bind.rotate, rest_pt, entries_at(bind.weights, i))?;
        let rh = bind.rest_handles.and_then(|list| list.get(i));
        segments.push(SegmentPose {
            point: posed_point(rest_pt, &s),
            handle_in: rh.and_then(|r| posed_handle(r.handle_in, &s)),
            handle_out: rh.and_then(|r| posed_handle(r.handle_out, &s)),
        });
    }
    let center = match &bind.center {
        None => None,
        Some(c) => {
            let m = c.live_len.min(c.rest.len());
            let mut out = Vec::with_capacity(m);
            for i in 0..m {
                let rest = &c.rest[i];
                let s = accumulate(resolver, bind.rotate, rest.point, entries_at(c.weights, i))?;
                out.push(SegmentPose {
                    point: posed_point(rest.point, &s),
                    handle_in: posed_handle(rest.handle_in, &s),
                    handle_out: posed_handle(rest.handle_out, &s),
                });
            }
            Some(out)
        }
    };
    Ok(Some(PathPose { segments, center }))
}

/// The normalized offsets of one mesh bind (`Ok(None)`: no live raster, skipped).
pub(crate) fn deform_mesh_bind(
    resolver: &mut AnchorResolver<'_>,
    bind: &MeshBind,
) -> Result<Option<Vec<Point2>>, DeformError> {
    let Some(target) = &bind.target else {
        return Ok(None);
    };
    let n = target.verts.len().min(bind.rest.len());
    let mut offsets = Vec::with_capacity(n);
    for i in 0..n {
        let rp = bind.rest[i];
        let s = accumulate(resolver, bind.rotate, rp, entries_at(bind.weights, i))?;
        let norm = divisor(s.sum_w);
        let d = [s.dx / norm, s.dy / norm];
        if zero_like(d[0]) && zero_like(d[1]) {
            offsets.push([0.0, 0.0]);
            continue;
        }
        let nd = normalized_of(&target.rect, rp[0] + d[0], rp[1] + d[1]);
        offsets.push([nd[0] - target.verts[i][0], nd[1] - target.verts[i][1]]);
    }
    Ok(Some(offsets))
}

/// `applyRigDeform(ld)` over the whole rig: one pose per bind, in bind order, or the first
/// failure. An empty bind list gives an empty result (the baseline returns at once).
pub(crate) fn apply_rig_deform(
    bones: &[DeformBone],
    binds: &[BindInput],
) -> Result<Vec<BindPose>, DeformFailure> {
    let mut resolver = AnchorResolver::new(bones);
    let mut poses = Vec::with_capacity(binds.len());
    for (bind_index, bind) in binds.iter().enumerate() {
        let fail = |error: DeformError| DeformFailure { bind_index, error };
        let pose = match bind {
            BindInput::Path(p) => deform_path_bind(&mut resolver, p)
                .map_err(fail)?
                .map_or(BindPose::Skipped, BindPose::Path),
            BindInput::Mesh(m) => deform_mesh_bind(&mut resolver, m)
                .map_err(fail)?
                .map_or(BindPose::Skipped, BindPose::Mesh),
        };
        poses.push(pose);
    }
    Ok(poses)
}

#[cfg(test)]
#[path = "../tests/rig_pose_deform_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_deform_mesh.rs"]
mod mesh_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_deform_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/rig_pose_deform.rs"]
mod tests;
