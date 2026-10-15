//! Pure Motion parent-chain affine math (N22G/#2028, R18.2 under #916).
//!
//! Mirrors the arithmetic of `src/js/motion.js`: `rotPt`/`rotVec`,
//! `transformSegments`, `transformImageRect`, the tail of `computeMotionMat`,
//! `invertVectorThroughParentChain`, `applyParentChainToSegments`/
//! `applyParentChainToImageRect`, the ancestor walks of
//! `legacyParentChainMats`/`composeChainTransform` (visited set plus a
//! 64-node guard), `layerParentUidAt`, `lerpAngleDeg` and the two- and
//! N-parent blends of `blendedAncestorMat`/`blendWeighted`.
//!
//! The caller owns layers, uids, keyframe sampling, expressions and Paper
//! bounds; this module receives already-resolved numbers and closures and
//! never reads a document, a context or a global.
//!
//! JavaScript `undefined` entries in a sampled value are passed as NaN:
//! every consumer here treats the two identically (falsy, NaN arithmetic).
//! Angles go through `f64::cos`/`sin`/`atan2`, which may differ from the
//! JavaScript engine's by an ulp on some hosts; `Math.hypot` is reproduced
//! exactly instead of calling the host's `hypot`.

use std::f64::consts::PI;

/// Most nodes either chain walk visits (`guard++ < 64`).
pub const CHAIN_NODE_LIMIT: usize = 64;

/// The `{dx, dy, rot, sx, sy, op, ax, ay}` value of `computeMotionMat`.
/// `rot` is in degrees; `sx`/`sy`/`op` are fractions (not percent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionMat {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
    pub op: f64,
    pub ax: f64,
    pub ay: f64,
}

/// The shear-free `{dx, dy, rot, sx, sy}` of `composeChainTransform`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
}

/// A missing parent's stand-in inside both blends.
const IDENTITY_POSE: Pose = Pose {
    dx: 0.0,
    dy: 0.0,
    rot: 0.0,
    sx: 1.0,
    sy: 1.0,
};

/// One `{mat, pivot}` element of a parent chain, immediate parent first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainEntry {
    pub mat: MotionMat,
    pub pivot: [f64; 2],
}

/// A segment with handles relative to its point (`serP` convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub point: [f64; 2],
    pub handle_in: Option<[f64; 2]>,
    pub handle_out: Option<[f64; 2]>,
}

/// A raster's on-canvas rect; `rotation` is degrees around its centre and
/// reads through `|| 0` on input. An absent JavaScript rotation is NaN here;
/// an empty chain hands the rect back untouched, NaN included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

/// What `legacyParentChainMats` knows about one ancestor besides its mat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AncestorBase {
    /// A Null layer: its `nullPos`, if set; the canvas centre otherwise.
    Null(Option<[f64; 2]>),
    /// Any other layer: its Paper bounds centre, if it has a Paper layer.
    Content(Option<[f64; 2]>),
}

/// `rotPt`: rotate a point about a centre; a falsy angle (0, -0, NaN) is a
/// no-op that returns the input unchanged.
pub fn rot_pt(px: f64, py: f64, cx: f64, cy: f64, deg: f64) -> [f64; 2] {
    if falsy(deg) {
        return [px, py];
    }
    let a = deg * PI / 180.0;
    let (c, s) = (a.cos(), a.sin());
    let dx = px - cx;
    let dy = py - cy;
    [cx + dx * c - dy * s, cy + dx * s + dy * c]
}

/// `rotVec`: the linear part of `rot_pt`.
pub fn rot_vec(dx: f64, dy: f64, deg: f64) -> [f64; 2] {
    if falsy(deg) {
        return [dx, dy];
    }
    let a = deg * PI / 180.0;
    let (c, s) = (a.cos(), a.sin());
    [dx * c - dy * s, dx * s + dy * c]
}

/// One element of `transformSegments`: scale in the pivot's frame, rotate
/// about the pivot, translate last. Handles take scale and rotation only.
pub fn transform_segment(s: &Segment, pivot: [f64; 2], m: &MotionMat) -> Segment {
    let lx = (s.point[0] - pivot[0]) * m.sx;
    let ly = (s.point[1] - pivot[1]) * m.sy;
    // JavaScript re-adds and re-subtracts the pivot; keep that rounding.
    let r = rot_pt(pivot[0] + lx, pivot[1] + ly, pivot[0], pivot[1], m.rot);
    Segment {
        point: [r[0] + m.dx, r[1] + m.dy],
        handle_in: s
            .handle_in
            .map(|h| rot_vec(h[0] * m.sx, h[1] * m.sy, m.rot)),
        handle_out: s
            .handle_out
            .map(|h| rot_vec(h[0] * m.sx, h[1] * m.sy, m.rot)),
    }
}

/// `transformSegments`.
pub fn transform_segments(segments: &[Segment], pivot: [f64; 2], m: &MotionMat) -> Vec<Segment> {
    segments
        .iter()
        .map(|s| transform_segment(s, pivot, m))
        .collect()
}

/// `transformImageRect`: the centre orbits the pivot, the size scales and
/// the rect's own rotation accumulates `m.rot`.
pub fn transform_image_rect(rb: &ImageRect, pivot: [f64; 2], m: &MotionMat) -> ImageRect {
    let cx = rb.x + rb.width / 2.0;
    let cy = rb.y + rb.height / 2.0;
    let mut ncx = pivot[0] + (cx - pivot[0]) * m.sx + m.dx;
    let mut ncy = pivot[1] + (cy - pivot[1]) * m.sy + m.dy;
    if !falsy(m.rot) {
        let o = rot_pt(ncx - m.dx, ncy - m.dy, pivot[0], pivot[1], m.rot);
        ncx = o[0] + m.dx;
        ncy = o[1] + m.dy;
    }
    let w = rb.width * m.sx;
    let h = rb.height * m.sy;
    ImageRect {
        x: ncx - w / 2.0,
        y: ncy - h / 2.0,
        width: w,
        height: h,
        rotation: or0(rb.rotation) + or0(m.rot),
    }
}

/// The tail of `computeMotionMat` after sampling: `None` for the exact
/// identity (falsy position/anchor/rotation, scale exactly 100/100, opacity
/// exactly 100). Scale and opacity arrive in percent.
pub fn motion_mat_from_values(
    position: [f64; 2],
    anchor: [f64; 2],
    rotation: f64,
    scale: [f64; 2],
    opacity: f64,
) -> Option<MotionMat> {
    if falsy(position[0])
        && falsy(position[1])
        && falsy(anchor[0])
        && falsy(anchor[1])
        && falsy(rotation)
        && scale[0] == 100.0
        && scale[1] == 100.0
        && opacity == 100.0
    {
        return None;
    }
    Some(MotionMat {
        dx: position[0],
        dy: position[1],
        rot: rotation,
        sx: scale[0] / 100.0,
        sy: scale[1] / 100.0,
        op: js_max0(opacity / 100.0),
        ax: anchor[0],
        ay: anchor[1],
    })
}

/// `applyParentChainToSegments`: each ancestor in order, immediate first.
pub fn apply_chain_to_segments(segments: &[Segment], chain: &[ChainEntry]) -> Vec<Segment> {
    let mut out = segments.to_vec();
    for e in chain {
        out = transform_segments(&out, e.pivot, &e.mat);
    }
    out
}

/// `applyParentChainToImageRect`: the rect plus the multiplied opacity.
pub fn apply_chain_to_image_rect(rect: &ImageRect, chain: &[ChainEntry]) -> (ImageRect, f64) {
    let mut r = *rect;
    let mut op_mul = 1.0;
    for e in chain {
        r = transform_image_rect(&r, e.pivot, &e.mat);
        op_mul *= e.mat.op;
    }
    (r, op_mul)
}

/// `invertVectorThroughParentChain`: undo each ancestor's rotation, then its
/// scale (a zero or NaN scale reads as 1), outermost ancestor first. No
/// translation or pivot: this maps a delta vector, not a point.
pub fn invert_vector_through_chain(chain: &[ChainEntry], dx: f64, dy: f64) -> [f64; 2] {
    let (mut dx, mut dy) = (dx, dy);
    for e in chain.iter().rev() {
        let m = &e.mat;
        let rad = -m.rot * PI / 180.0;
        let (c, s) = (rad.cos(), rad.sin());
        let rx = dx * c - dy * s;
        let ry = dx * s + dy * c;
        dx = rx / or1(m.sx);
        dy = ry / or1(m.sy);
    }
    [dx, dy]
}

/// `layerParentUidAt`: with keys, the uid of the last key at or before
/// `frame` (the first key's when none is); without keys, the static parent.
/// Keys are `(frame, uid)` in stored order; `None` is a falsy uid.
pub fn parent_uid_at<U: Clone>(
    keys: &[(f64, Option<U>)],
    static_parent: Option<&U>,
    frame: f64,
) -> Option<U> {
    let Some(first) = keys.first() else {
        return static_parent.cloned();
    };
    let mut v = &first.1;
    for (key_frame, uid) in keys {
        if *key_frame <= frame {
            v = uid;
        } else {
            break;
        }
    }
    v.clone()
}

/// The node walk shared by both chain readers: from `first`, follow `next`
/// until it ends, revisits a node (including `exclude`, the walking layer
/// itself for `legacyParentChainMats`) or has visited `CHAIN_NODE_LIMIT`
/// nodes. `composeChainTransform` starts at the chain's own first node and
/// excludes nothing.
pub fn walk_chain(
    first: Option<usize>,
    exclude: Option<usize>,
    mut next: impl FnMut(usize) -> Option<usize>,
) -> Vec<usize> {
    let mut nodes = Vec::new();
    let mut cur = first;
    while let Some(idx) = cur {
        if nodes.len() >= CHAIN_NODE_LIMIT || exclude == Some(idx) || nodes.contains(&idx) {
            break;
        }
        nodes.push(idx);
        cur = next(idx);
    }
    nodes
}

/// The pivot rule of `legacyParentChainMats` for one ancestor: a Null pivots
/// at its `nullPos` (or the canvas centre) plus anchor; another layer at its
/// bounds centre plus anchor and is skipped without bounds. No mat, no entry.
pub fn ancestor_entry(
    mat: Option<MotionMat>,
    base: AncestorBase,
    canvas: [f64; 2],
) -> Option<ChainEntry> {
    let m = mat?;
    let b = match base {
        AncestorBase::Null(pos) => pos.unwrap_or([canvas[0] / 2.0, canvas[1] / 2.0]),
        AncestorBase::Content(centre) => centre?,
    };
    Some(ChainEntry {
        mat: m,
        pivot: [b[0] + m.ax, b[1] + m.ay],
    })
}

/// `composeChainTransform` after its walk: track the origin and the two unit
/// points through every entry and read back translation, rotation and the
/// two axis lengths. `None` when no ancestor contributed.
pub fn compose_chain(entries: &[ChainEntry]) -> Option<Pose> {
    if entries.is_empty() {
        return None;
    }
    let zero = Some([0.0, 0.0]);
    let unit = |x: f64, y: f64| Segment {
        point: [x, y],
        handle_in: zero,
        handle_out: zero,
    };
    let segs = apply_chain_to_segments(&[unit(0.0, 0.0), unit(1.0, 0.0), unit(0.0, 1.0)], entries);
    let (o, ux, uy) = (segs[0].point, segs[1].point, segs[2].point);
    let vx = [ux[0] - o[0], ux[1] - o[1]];
    let vy = [uy[0] - o[0], uy[1] - o[1]];
    Some(Pose {
        dx: o[0],
        dy: o[1],
        rot: vx[1].atan2(vx[0]) * 180.0 / PI,
        sx: js_hypot2(vx[0], vx[1]),
        sy: js_hypot2(vy[0], vy[1]),
    })
}

/// `legacyParentChainMats`: from the walking layer `start`, follow
/// `parent_of` (its keyed parent at the sampled frame) and turn each
/// ancestor into a chain entry, immediate parent first, keeping its index.
/// `ancestor` gives an ancestor's mat and pivot base.
pub fn legacy_chain(
    start: usize,
    parent_of: impl Fn(usize) -> Option<usize>,
    ancestor: impl Fn(usize) -> (Option<MotionMat>, AncestorBase),
    canvas: [f64; 2],
) -> Vec<(usize, ChainEntry)> {
    walk_chain(parent_of(start), Some(start), &parent_of)
        .into_iter()
        .filter_map(|i| {
            let (mat, base) = ancestor(i);
            ancestor_entry(mat, base, canvas).map(|e| (i, e))
        })
        .collect()
}

/// `composeChainTransform`: from `first` itself, follow the static
/// `parent_of` and compose. It has no Null branch: `ancestor` gives each
/// node's mat and bounds centre (`None` without bounds; an empty Null
/// layer's Paper bounds centre is the origin).
pub fn compose_static_chain(
    first: Option<usize>,
    parent_of: impl Fn(usize) -> Option<usize>,
    ancestor: impl Fn(usize) -> (Option<MotionMat>, Option<[f64; 2]>),
    canvas: [f64; 2],
) -> Option<Pose> {
    let entries: Vec<ChainEntry> = walk_chain(first, None, &parent_of)
        .into_iter()
        .filter_map(|i| {
            let (mat, centre) = ancestor(i);
            ancestor_entry(mat, AncestorBase::Content(centre), canvas)
        })
        .collect();
    compose_chain(&entries)
}

/// `lerpAngleDeg`: shortest-path angle interpolation.
pub fn lerp_angle_deg(a: f64, b: f64, t: f64) -> f64 {
    let d = ((b - a + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    a + d * t
}

/// The two-parent half of `blendedAncestorMat`: `parent_blend` is the raw
/// sampled percent (read through `|| 0`); `a`/`b` are each parent's
/// composed pose. `None` when neither parent contributes.
pub fn blend_two(a: Option<Pose>, b: Option<Pose>, parent_blend: f64) -> Option<MotionMat> {
    let t = or0(parent_blend) / 100.0;
    if a.is_none() && b.is_none() {
        return None;
    }
    let a = a.unwrap_or(IDENTITY_POSE);
    let b = b.unwrap_or(IDENTITY_POSE);
    Some(MotionMat {
        dx: a.dx + (b.dx - a.dx) * t,
        dy: a.dy + (b.dy - a.dy) * t,
        rot: lerp_angle_deg(a.rot, b.rot, t),
        sx: a.sx + (b.sx - a.sx) * t,
        sy: a.sy + (b.sy - a.sy) * t,
        op: 1.0,
        ax: 0.0,
        ay: 0.0,
    })
}

/// `weightedParentsOf`' weights: A gets `1 - t`, B gets `t`, then one
/// absolute percent per extra parent (each read through `|| 0`). The caller
/// drops extras without a uid before sampling, as the JavaScript does.
pub fn parent_weights(parent_blend: f64, extras: &[f64]) -> Vec<f64> {
    let t = or0(parent_blend) / 100.0;
    let mut w = vec![1.0 - t, t];
    w.extend(extras.iter().map(|&x| or0(x) / 100.0));
    w
}

/// `blendWeighted`: a weighted average of decomposed poses with a circular
/// mean for rotation. A sum not above 1e-9 (or NaN) falls back to the first
/// parent at full weight. `None` when no weighted parent contributed, or
/// when `parts` is empty (JavaScript would throw there; it never calls
/// this with fewer than two parts).
pub fn blend_weighted(parts: &[(f64, Option<Pose>)]) -> Option<MotionMat> {
    let first = *parts.first()?;
    let mut sum = 0.0;
    for p in parts {
        sum += p.0;
    }
    let single = [(1.0, first.1)];
    let parts = if sum.is_nan() || sum <= 1e-9 {
        sum = 1.0;
        &single[..]
    } else {
        parts
    };
    let (mut dx, mut dy, mut sx, mut sy, mut cx, mut cy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut any = false;
    for &(weight, pose) in parts {
        let w = weight / sum;
        if falsy(w) {
            continue;
        }
        any |= pose.is_some();
        let m = pose.unwrap_or(IDENTITY_POSE);
        dx += m.dx * w;
        dy += m.dy * w;
        sx += m.sx * w;
        sy += m.sy * w;
        let r = m.rot * PI / 180.0;
        cx += r.cos() * w;
        cy += r.sin() * w;
    }
    if !any {
        return None;
    }
    let rot = if cx == 0.0 && cy == 0.0 {
        0.0
    } else {
        cy.atan2(cx) * 180.0 / PI
    };
    Some(MotionMat {
        dx,
        dy,
        rot,
        sx,
        sy,
        op: 1.0,
        ax: 0.0,
        ay: 0.0,
    })
}

/// `Math.hypot(a, b)` as V8 computes it (max scaling plus Kahan-compensated
/// sum of squares). Infinity wins over NaN; all-zero is `+0`.
pub fn js_hypot2(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = if b > a { b } else { a };
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut compensation) = (0.0_f64, 0.0_f64);
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// JavaScript falsiness of a number: 0, -0 and NaN.
fn falsy(x: f64) -> bool {
    x == 0.0 || x.is_nan()
}

/// JavaScript `x || 0`.
fn or0(x: f64) -> f64 {
    if falsy(x) {
        0.0
    } else {
        x
    }
}

/// JavaScript `x || 1`.
fn or1(x: f64) -> f64 {
    if falsy(x) {
        1.0
    } else {
        x
    }
}

/// `Math.max(0, x)`: NaN propagates, `-0` and negatives read as `+0`.
fn js_max0(x: f64) -> f64 {
    if x.is_nan() || x > 0.0 {
        x
    } else {
        0.0
    }
}
