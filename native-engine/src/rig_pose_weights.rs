//! Rig control-point weights and anchors kernel (I12), pure and I/O free.
//!
//! Mirrors the control-point anchoring model of the baseline at fbe4b8a3
//! (`src/js/app.js`): `rigWeighOnePointByControl`, `_rigChordAngle`,
//! `_rigNormalizeAngle`, the control-point branch of `anchorFor` (nested in
//! `applyRigDeform`) and the two bind-time statements of `rigBindStroke` that
//! feed the weigher (`radius = radius || 200`, the softness clamp). It owns no
//! rig record, bind, deformation, offset-model curve sampling or command.
//!
//! Semantics kept from the baseline:
//! - a vertex is weighed against every CONTROL POINT of each listed bone's REST
//!   segments, in `boneIds` order then point order; an entry `{boneId, ptIndex,
//!   w}` is emitted only when the influence is `> 0` (a NaN influence is not);
//! - `ptIndex` is the position in the segment list, so a segment without a
//!   `.point` is skipped but still counts; a bone missing from the rig is
//!   skipped; an absent `restSegments` is an empty list;
//! - the reach is `bone.radius || radius` (a zero, NaN or absent bone radius
//!   falls back to the bind radius); `t = max(0, 1 - dist / reach)` is clamped
//!   BEFORE the softness blend `t + (t*t*(3-2t) - t) * softness`, which only
//!   runs for a truthy (non-zero, non-NaN) softness;
//! - the chord angle at control point `k` is `atan2` of the neighbours' chord
//!   with the neighbour index clamped into `[0, len-1]`, so the first and last
//!   point borrow their only neighbour, and it is `0` for fewer than two
//!   segments; `normalize_angle` is `((a + pi) % 2pi + 2pi) % 2pi - pi` (exactly
//!   `fmod`, so `pi` maps to `-pi`);
//! - weight entries keep two shapes distinct: `ptIndex` (control point) and the
//!   legacy `offset` (curve location). The discriminator is the PRESENCE of
//!   `ptIndex` (`!= null`, so `0` counts), exactly as `anchorFor` routes it.
//!
//! Deliberate differences, all typed:
//! - the baseline throws a `TypeError` when a segment it must read has no
//!   `.point`; [`anchor_for`] and [`chord_angle`] return the missing index
//!   instead, before any result is produced;
//! - the offset model needs Paper's curve location; [`anchor_for`] reports it as
//!   [`Anchor::OffsetModel`] for a later leaf and never invents a position;
//! - the `window.SM_RIG_CONTROL_POINT_WEIGHTS === false` switch (legacy weigher)
//!   is a JavaScript surface toggle and is not modelled.
//!
//! Float semantics: `dist` is `sqrt(dx*dx + dy*dy)` (not `hypot`), the blend and
//! clamps are the baseline's expressions in the baseline's order, `Math.max` /
//! `Math.min` are [`js_max`] / [`js_min`] (NaN in, NaN out), and `%` on `f64` is
//! `fmod`, as in JavaScript. Only `atan2` rides on the platform libm.

use std::f64::consts::PI;

/// A point as `[x, y]`.
pub(crate) type Point2 = [f64; 2];

/// `radius = radius || 200` in `rigBindStroke`.
const DEFAULT_BIND_RADIUS: f64 = 200.0;

/// One bone as the weigher and the anchors read it. A segment is its `.point`
/// (`None` when the segment object carries none); `None` for the whole list is an
/// absent `segments` / `restSegments` property, distinct from an empty list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WeightBone {
    pub(crate) id: String,
    /// `bone.radius`; `None`, `0` and NaN are all falsy in the baseline.
    pub(crate) radius: Option<f64>,
    /// `bone.segments`: the live pose.
    pub(crate) segments: Option<Vec<Option<Point2>>>,
    /// `bone.restSegments`: the bind pose the weights are measured on.
    pub(crate) rest_segments: Option<Vec<Option<Point2>>>,
}

/// A control-point weight as `rigWeighOnePointByControl` emits it:
/// `{boneId, ptIndex, w}`. `stray_offset` is an `offset` a persisted entry may
/// carry next to its `ptIndex` (ignored by the deform, kept so it round-trips);
/// the weigher itself always emits `None`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ControlWeight {
    pub(crate) bone_id: String,
    pub(crate) pt_index: usize,
    pub(crate) w: f64,
    pub(crate) stray_offset: Option<f64>,
}

/// A legacy weight `{boneId, offset, w}` (no `ptIndex`). `offset` is `None` when
/// the stored entry has neither field; the baseline then asks the curve for an
/// undefined offset, which is not a position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OffsetWeight {
    pub(crate) bone_id: String,
    pub(crate) offset: Option<f64>,
    pub(crate) w: f64,
}

/// The two coexisting weight shapes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WeightEntry {
    Control(ControlWeight),
    Offset(OffsetWeight),
}

/// A stored weight entry field by field (`ptIndex` / `offset` already folded
/// from "undefined or null" to `None`, which the baseline does not tell apart).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WeightParts {
    pub(crate) bone_id: String,
    pub(crate) pt_index: Option<usize>,
    pub(crate) offset: Option<f64>,
    pub(crate) w: f64,
}

impl WeightEntry {
    /// The discriminator of `anchorFor`: `ptIndex != null` selects the control
    /// point model (index `0` included), anything else the offset model.
    pub(crate) fn classify(parts: WeightParts) -> WeightEntry {
        match parts.pt_index {
            Some(pt_index) => WeightEntry::Control(ControlWeight {
                bone_id: parts.bone_id,
                pt_index,
                w: parts.w,
                stray_offset: parts.offset,
            }),
            None => WeightEntry::Offset(OffsetWeight {
                bone_id: parts.bone_id,
                offset: parts.offset,
                w: parts.w,
            }),
        }
    }

    /// The stored fields back, the inverse of [`WeightEntry::classify`].
    pub(crate) fn parts(&self) -> WeightParts {
        match self {
            WeightEntry::Control(c) => WeightParts {
                bone_id: c.bone_id.clone(),
                pt_index: Some(c.pt_index),
                offset: c.stray_offset,
                w: c.w,
            },
            WeightEntry::Offset(o) => WeightParts {
                bone_id: o.bone_id.clone(),
                pt_index: None,
                offset: o.offset,
                w: o.w,
            },
        }
    }
}

impl From<ControlWeight> for WeightEntry {
    fn from(value: ControlWeight) -> WeightEntry {
        WeightEntry::Control(value)
    }
}

/// JavaScript `Math.max`: NaN in, NaN out; `+0` beats `-0`.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// JavaScript `Math.min`: NaN in, NaN out; `-0` beats `+0`.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b {
        a
    } else {
        b
    }
}

/// JavaScript truthiness of a number: not `0`, `-0` or NaN.
fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// `radius = radius || 200`: an absent, zero or NaN bind radius becomes 200.
pub(crate) fn bind_radius(radius: Option<f64>) -> f64 {
    match radius {
        Some(r) if truthy(r) => r,
        _ => DEFAULT_BIND_RADIUS,
    }
}

/// `softness = Math.max(0, Math.min(1, softness || 0))`.
pub(crate) fn clamp_softness(softness: Option<f64>) -> f64 {
    let s = match softness {
        Some(s) if truthy(s) => s,
        _ => 0.0,
    };
    js_max(0.0, js_min(1.0, s))
}

/// `bone.radius || radius`: the bone's own reach wins unless it is falsy.
pub(crate) fn effective_radius(bone: &WeightBone, radius: f64) -> f64 {
    match bone.radius {
        Some(r) if truthy(r) => r,
        _ => radius,
    }
}

/// The falloff of one control point: `t` is clamped to `>= 0` first, then a
/// truthy softness blends it with the smoothstep `t*t*(3-2t)`.
pub(crate) fn influence(dist: f64, reach: f64, softness: f64) -> f64 {
    let t = js_max(0.0, 1.0 - dist / reach);
    if truthy(softness) {
        t + (t * t * (3.0 - 2.0 * t) - t) * softness
    } else {
        t
    }
}

fn find_bone<'a>(bones: &'a [WeightBone], id: &str) -> Option<&'a WeightBone> {
    bones.iter().find(|b| b.id == id)
}

/// `rigWeighOnePointByControl(rig, boneIds, p, radius, softness)`.
pub(crate) fn weigh_one_point_by_control(
    bones: &[WeightBone],
    bone_ids: &[String],
    p: Point2,
    radius: f64,
    softness: f64,
) -> Vec<ControlWeight> {
    let mut out = Vec::new();
    for bid in bone_ids {
        let Some(bone) = find_bone(bones, bid) else {
            continue;
        };
        let segs = bone.rest_segments.as_deref().unwrap_or(&[]);
        let reach = effective_radius(bone, radius);
        for (k, seg) in segs.iter().enumerate() {
            let Some(cp) = seg else {
                continue;
            };
            let dist = ((p[0] - cp[0]) * (p[0] - cp[0]) + (p[1] - cp[1]) * (p[1] - cp[1])).sqrt();
            let infl = influence(dist, reach, softness);
            if infl > 0.0 {
                out.push(ControlWeight {
                    bone_id: bid.clone(),
                    pt_index: k,
                    w: infl,
                    stray_offset: None,
                });
            }
        }
    }
    out
}

/// The weights of every point of a PATH bind pass (`weighForPoints` of
/// `rigBindStroke`), with its `radius || 200` and softness clamp applied first.
/// `rigBindMesh` passes its radius through unchanged and only clamps the
/// softness: that caller uses [`clamp_softness`] and
/// [`weigh_one_point_by_control`] directly.
pub(crate) fn weigh_bind_points(
    bones: &[WeightBone],
    bone_ids: &[String],
    pts: &[Point2],
    radius: Option<f64>,
    softness: Option<f64>,
) -> Vec<Vec<ControlWeight>> {
    let radius = bind_radius(radius);
    let softness = clamp_softness(softness);
    pts.iter()
        .map(|p| weigh_one_point_by_control(bones, bone_ids, *p, radius, softness))
        .collect()
}

/// A segment the chord needed had no usable `.point` (or the index is past the
/// list): the baseline's `TypeError`, with the first offending index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChordError {
    pub(crate) index: usize,
}

/// `_rigChordAngle(segs, k)`: `atan2(next.y - prev.y, next.x - prev.x)` with
/// `prev = segs[max(k-1, 0)]`, `next = segs[min(k+1, len-1)]`; `0` for fewer
/// than two segments (an absent list is the empty list).
pub(crate) fn chord_angle(segs: &[Option<Point2>], k: usize) -> Result<f64, ChordError> {
    let len = segs.len();
    if len < 2 {
        return Ok(0.0);
    }
    let prev_index = k.saturating_sub(1);
    let next_index = std::cmp::min(k.saturating_add(1), len - 1);
    let prev_slot: Option<Point2> = *segs
        .get(prev_index)
        .ok_or(ChordError { index: prev_index })?;
    let next = segs[next_index].ok_or(ChordError { index: next_index })?;
    let prev = prev_slot.ok_or(ChordError { index: prev_index })?;
    Ok((next[1] - prev[1]).atan2(next[0] - prev[0]))
}

/// `_rigNormalizeAngle(a)`: wrap into `[-pi, pi)` with JavaScript's `%` (fmod).
pub(crate) fn normalize_angle(a: f64) -> f64 {
    ((a + PI) % (2.0 * PI) + 2.0 * PI) % (2.0 * PI) - PI
}

/// Which segment list a missing point was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSource {
    Rest,
    Live,
}

/// Where the baseline would throw while resolving an anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnchorError {
    /// A segment that must supply a point has none.
    MissingPoint { source: PointSource, index: usize },
    /// An offset-model entry names a bone the rig does not have.
    OffsetBoneMissing,
}

/// The resolved control-point anchor: where the point was, where it is, and how
/// far its local chord turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ControlAnchor {
    pub(crate) rest_x: f64,
    pub(crate) rest_y: f64,
    pub(crate) cur_x: f64,
    pub(crate) cur_y: f64,
    pub(crate) da: f64,
}

/// What `anchorFor` makes of one weight entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Anchor {
    Control(ControlAnchor),
    /// The baseline's `null`: the entry cannot be resolved and is skipped.
    Unresolved,
    /// Offset model: needs the Paper curve location kernel (a later leaf).
    OffsetModel {
        offset: Option<f64>,
    },
}

/// `anchorFor(wentry)`.
pub(crate) fn anchor_for(bones: &[WeightBone], entry: &WeightEntry) -> Result<Anchor, AnchorError> {
    match entry {
        WeightEntry::Offset(o) => {
            find_bone(bones, &o.bone_id).ok_or(AnchorError::OffsetBoneMissing)?;
            Ok(Anchor::OffsetModel { offset: o.offset })
        }
        WeightEntry::Control(c) => control_anchor(bones, &c.bone_id, c.pt_index),
    }
}

fn control_anchor(bones: &[WeightBone], bone_id: &str, k: usize) -> Result<Anchor, AnchorError> {
    let Some(bone) = find_bone(bones, bone_id) else {
        return Ok(Anchor::Unresolved);
    };
    let (Some(rest), Some(live)) = (bone.rest_segments.as_deref(), bone.segments.as_deref()) else {
        return Ok(Anchor::Unresolved);
    };
    let (Some(rs), Some(cs)) = (rest.get(k), live.get(k)) else {
        return Ok(Anchor::Unresolved);
    };
    let missing = |source: PointSource, index: usize| AnchorError::MissingPoint { source, index };
    let rp = (*rs).ok_or(missing(PointSource::Rest, k))?;
    let cp = (*cs).ok_or(missing(PointSource::Live, k))?;
    let live_chord = chord_angle(live, k).map_err(|e| missing(PointSource::Live, e.index))?;
    let rest_chord = chord_angle(rest, k).map_err(|e| missing(PointSource::Rest, e.index))?;
    Ok(Anchor::Control(ControlAnchor {
        rest_x: rp[0],
        rest_y: rp[1],
        cur_x: cp[0],
        cur_y: cp[1],
        da: normalize_angle(live_chord - rest_chord),
    }))
}

#[cfg(test)]
#[path = "../tests/rig_pose_weights_contract.rs"]
mod contract_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_weights_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_weights_flawed.rs"]
mod flawed;
#[cfg(test)]
#[path = "../tests/rig_pose_weights_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/rig_pose_weights.rs"]
mod tests;
