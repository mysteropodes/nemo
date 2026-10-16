//! 3D vertex projection kernel (K-3D), pure and I/O free.
//!
//! Mirrors `src/js/motion.js` at 1f663ac3: `rotate3D`, `project3DToScreen`,
//! `make3DProjector`, `project3DSegments` and `project3DImageRect`. Inputs are
//! explicit (`Pose3D` is what `valueAtFrame` returned); no layer or state is
//! read. Arithmetic keeps the JS operand order, so results are bit-identical
//! (NaN payloads aside) wherever the platform libm agrees with V8.
//!
//! Semantics kept on purpose:
//! - `CAMERA_DISTANCE` is 2000; `depthW = (2000 + wz) / 2000` is clamped to
//!   0.05 only when `< 0.05` (NaN is not clamped and propagates). Points at or
//!   behind the camera plane stay finite, never `None`;
//! - `rotate3D` applies X, then Y, then Z about the origin and skips an axis
//!   whose angle is falsy (`0`, `-0`, `NaN`); angles are `d * PI / 180`;
//! - the projector works per vertex: `lx = (px - pivotX) * sx`, rotate with
//!   `rot_x, rot_y, rot`, add pivot, position and minus canvas half, then
//!   project; the pivot is the bounds centre plus the anchor and there is no
//!   scale-0 guard;
//! - `ExtraDelta` fields default to 0 when absent, `0` or `NaN` (`x || 0`);
//!   `dsxPct`/`dsyPct` are added to the scale percent; position, rotation and scale
//!   are left untouched with no `ExtraDelta`, while z/rotX/rotY always add (`+ 0`);
//! - segment handles are projected as absolute points and then differenced
//!   from the projected anchor (no true Bezier perspective);
//! - the image rect probes `+-hw` / `+-hh` along its own rotation, returns an
//!   axis-aligned rect plus `atan2` of the x-vector in degrees; an absent or
//!   falsy rotation reads 0, edge-on gives a ~0 width without error.
//!
//! Not provided: `layerMotion3DPointMap`, gizmo helpers and stroke width.

pub(crate) const CAMERA_DISTANCE: f64 = 2000.0;
const MIN_DEPTH: f64 = 0.05;

/// `valueAtFrame` results for one layer: position, anchor, rotation (deg),
/// scale (percent), positionZ, rotationX, rotationY.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pose3D {
    pub(crate) position: [f64; 2],
    pub(crate) anchor: [f64; 2],
    pub(crate) rotation: f64,
    pub(crate) scale: [f64; 2],
    pub(crate) position_z: f64,
    pub(crate) rotation_x: f64,
    pub(crate) rotation_y: f64,
}

/// Optional additive contribution (`extraDelta`); absent fields are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ExtraDelta {
    pub(crate) dx: Option<f64>,
    pub(crate) dy: Option<f64>,
    pub(crate) drot: Option<f64>,
    pub(crate) dsx_pct: Option<f64>,
    pub(crate) dsy_pct: Option<f64>,
    pub(crate) dz: Option<f64>,
    pub(crate) drx: Option<f64>,
    pub(crate) dry: Option<f64>,
}

/// Layer content bounds (`bounds.{x,y,width,height}`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// One path segment; absent handles are `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Seg {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: Option<[f64; 2]>,
    pub(crate) handle_out: Option<[f64; 2]>,
}

/// Image rect; `rotation` is degrees, `None` reads as 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: Option<f64>,
}

/// Projected image rect (rotation always present).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectedRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: f64,
}

/// The closure `make3DProjector` returns, with everything it captured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Projector3D {
    pos: [f64; 2],
    pos_z: f64,
    rot_x: f64,
    rot_y: f64,
    rot: f64,
    sx: f64,
    sy: f64,
    pivot_x: f64,
    pivot_y: f64,
    canvas_w: f64,
    canvas_h: f64,
}

/// JS truthiness of a number.
fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// `v || 0` for an optional field.
fn or0(v: Option<f64>) -> f64 {
    match v {
        Some(x) if truthy(x) => x,
        _ => 0.0,
    }
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

/// `rotate3D`: X, then Y, then Z about the origin; falsy angles skip.
pub(crate) fn rotate_3d(mut x: f64, mut y: f64, mut z: f64, rx: f64, ry: f64, rz: f64) -> [f64; 3] {
    if truthy(rx) {
        let a = deg_to_rad(rx);
        let (c, s) = (a.cos(), a.sin());
        let (ny, nz) = (y * c - z * s, y * s + z * c);
        y = ny;
        z = nz;
    }
    if truthy(ry) {
        let a = deg_to_rad(ry);
        let (c, s) = (a.cos(), a.sin());
        let (nx, nz) = (x * c + z * s, -x * s + z * c);
        x = nx;
        z = nz;
    }
    if truthy(rz) {
        let a = deg_to_rad(rz);
        let (c, s) = (a.cos(), a.sin());
        let (nx, ny) = (x * c - y * s, x * s + y * c);
        x = nx;
        y = ny;
    }
    [x, y, z]
}

/// `project3DToScreen`: perspective divide with the depth floor.
pub(crate) fn project_to_screen(
    wx: f64,
    wy: f64,
    wz: f64,
    canvas_w: f64,
    canvas_h: f64,
) -> [f64; 2] {
    let mut depth_w = (CAMERA_DISTANCE + wz) / CAMERA_DISTANCE;
    if depth_w < MIN_DEPTH {
        depth_w = MIN_DEPTH;
    }
    [canvas_w / 2.0 + wx / depth_w, canvas_h / 2.0 + wy / depth_w]
}

/// `make3DProjector`: capture the pose once, return the per-vertex projector.
pub(crate) fn make_projector(
    pose: &Pose3D,
    bounds: &Bounds,
    canvas_w: f64,
    canvas_h: f64,
    extra: Option<&ExtraDelta>,
) -> Projector3D {
    let (mut pos, mut rot, mut scl) = (pose.position, pose.rotation, pose.scale);
    if let Some(e) = extra {
        pos = [pos[0] + or0(e.dx), pos[1] + or0(e.dy)];
        rot += or0(e.drot);
        scl = [scl[0] + or0(e.dsx_pct), scl[1] + or0(e.dsy_pct)];
    }
    let add = |v: f64, d: fn(&ExtraDelta) -> Option<f64>| v + extra.map_or(0.0, |e| or0(d(e)));
    Projector3D {
        pos,
        pos_z: add(pose.position_z, |e| e.dz),
        rot_x: add(pose.rotation_x, |e| e.drx),
        rot_y: add(pose.rotation_y, |e| e.dry),
        rot,
        sx: scl[0] / 100.0,
        sy: scl[1] / 100.0,
        pivot_x: bounds.x + bounds.width / 2.0 + pose.anchor[0],
        pivot_y: bounds.y + bounds.height / 2.0 + pose.anchor[1],
        canvas_w,
        canvas_h,
    }
}

impl Projector3D {
    /// Project one local point to screen space.
    pub(crate) fn project(&self, px: f64, py: f64) -> [f64; 2] {
        let lx = (px - self.pivot_x) * self.sx;
        let ly = (py - self.pivot_y) * self.sy;
        let r = rotate_3d(lx, ly, 0.0, self.rot_x, self.rot_y, self.rot);
        let wx = self.pivot_x + r[0] + self.pos[0] - self.canvas_w / 2.0;
        let wy = self.pivot_y + r[1] + self.pos[1] - self.canvas_h / 2.0;
        let wz = r[2] + self.pos_z;
        project_to_screen(wx, wy, wz, self.canvas_w, self.canvas_h)
    }
}

/// `project3DSegments`: project points, handles absolute-then-differenced.
pub(crate) fn project_segments(segments: &[Seg], projector: &Projector3D) -> Vec<Seg> {
    segments
        .iter()
        .map(|s| {
            let p = projector.project(s.point[0], s.point[1]);
            let rel = |h: [f64; 2]| {
                let q = projector.project(s.point[0] + h[0], s.point[1] + h[1]);
                [q[0] - p[0], q[1] - p[1]]
            };
            Seg {
                point: p,
                handle_in: s.handle_in.map(rel),
                handle_out: s.handle_out.map(rel),
            }
        })
        .collect()
}

/// V8's `Math.hypot` for two arguments (max-scaled Kahan sum), the engine the
/// oracle came from; `f64::hypot` rounds differently by up to 1 ulp.
fn hypot2(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = a.max(b);
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

/// `project3DImageRect`: axis-aligned projected rect plus rotation (deg).
pub(crate) fn project_image_rect(rect: &Rect, projector: &Projector3D) -> ProjectedRect {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let (hw, hh) = (rect.width / 2.0, rect.height / 2.0);
    let rot = or0(rect.rotation) * std::f64::consts::PI / 180.0;
    let (cos, sin) = (rot.cos(), rot.sin());
    let (ax_x, ax_y, ay_x, ay_y) = (cos, sin, -sin, cos);
    let p_c = projector.project(cx, cy);
    let x_pos = projector.project(cx + hw * ax_x, cy + hw * ax_y);
    let x_neg = projector.project(cx - hw * ax_x, cy - hw * ax_y);
    let y_pos = projector.project(cx + hh * ay_x, cy + hh * ay_y);
    let y_neg = projector.project(cx - hh * ay_x, cy - hh * ay_y);
    let x_vec = [(x_pos[0] - x_neg[0]) / 2.0, (x_pos[1] - x_neg[1]) / 2.0];
    let y_vec = [(y_pos[0] - y_neg[0]) / 2.0, (y_pos[1] - y_neg[1]) / 2.0];
    let half_w = hypot2(x_vec[0], x_vec[1]);
    let half_h = hypot2(y_vec[0], y_vec[1]);
    let rotation = x_vec[1].atan2(x_vec[0]) * 180.0 / std::f64::consts::PI;
    ProjectedRect {
        x: p_c[0] - half_w,
        y: p_c[1] - half_h,
        width: half_w * 2.0,
        height: half_h * 2.0,
        rotation,
    }
}

#[cfg(test)]
#[path = "../tests/property_projection_3d.rs"]
mod tests;
