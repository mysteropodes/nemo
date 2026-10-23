//! Pure rules of the rig edit transaction (I12): every record mutation, on a detached copy of the rig.
//! Each function takes the stored rig and returns the proposed one; none of them publishes, reads a
//! document or calls a kernel (the weights and lengths arrive as plain values from the plans).
//!
//! The baseline statements they mirror are cut verbatim by the oracle generator: the pose write of
//! `onMove` (`moved_handles`), the influence-circle write (`radius_floor`), the weight popover handlers
//! (`weight_fraction`, `remove_weight`, `add_weight`, `replace_weights`), `finalizeRigBone`
//! (`draw_bone`), `rigBindStroke`'s replace-not-stack (`apply_binds`) and `rigSetIK` (`set_ik`).
//!
//! Numbers: a value computed here is spelled like `JSON.stringify` would (integral values without a
//! fraction, `-0` as `0`), and a value equal to the stored one keeps the stored spelling, so an edit
//! that changes nothing cannot rewrite bytes.
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::plan::{
    bind_index, PlanRefusal, PlannedBind,
};
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::{
    RigRef, RigTarget, SegmentPart,
};
use crate::object_document::object_rig_record::{
    IkChainRecord, IkRefRecord, KeyedTable, RestHandle, RigBind, RigBone, RigPair, RigRecord,
    RigSegment, RigWeight, BONE_ID_PREFIX,
};
use crate::object_document::Presence;
use crate::object_snapshot::object_rig_edit_read::{ControlEntry, OffsetEntry};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

type Rule<T> = Result<T, DispatchErrorCode>;

fn bad<T>() -> Rule<T> {
    Err(DispatchErrorCode::InvalidRequest)
}

/// A computed value spelled like `JSON.stringify` would: integral values without a fraction, `-0` as `0`.
pub(super) fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Number::from(0i64));
    }
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

fn spelled(value: f64) -> Rule<Number> {
    js_number(value).ok_or(DispatchErrorCode::InvalidRequest)
}

/// Equal values keep their stored spelling so a no-op cannot rewrite bytes.
fn keep_equal(before: &Number, after: Number) -> Number {
    if before.as_f64() == after.as_f64() {
        before.clone()
    } else {
        after
    }
}

fn value_of(number: &Number) -> f64 {
    number.as_f64().unwrap_or(f64::NAN)
}

/// The table with `key` set: in place when the key exists (a JavaScript property write keeps its
/// position), appended otherwise.
fn replaced<T: Clone>(table: &KeyedTable<T>, key: &str, value: T) -> KeyedTable<T> {
    let mut entries: Vec<(String, T)> = table.entries().to_vec();
    match entries.iter().position(|(entry_key, _)| entry_key == key) {
        Some(index) => entries[index].1 = value,
        None => entries.push((key.to_owned(), value)),
    }
    KeyedTable::new(entries)
}

fn removed<T: Clone>(table: &KeyedTable<T>, key: &str) -> KeyedTable<T> {
    KeyedTable::new(
        table
            .entries()
            .iter()
            .filter(|(entry_key, _)| entry_key != key)
            .cloned()
            .collect(),
    )
}

/// `finalizeRigBone`: the bone stored under a fresh `boneN` (counter incremented) or, with `bone_id`,
/// the existing bone redrawn with its radius kept; `restSegments` is a copy of `segments`.
pub(super) fn draw_bone(
    rig: &RigRecord,
    bone_id: Option<&str>,
    segments: Vec<RigSegment>,
    closed: bool,
) -> Rule<RigRecord> {
    let mut next = rig.clone();
    let (id, radius) = match bone_id {
        Some(id) => {
            let old = next.bone(id).ok_or(DispatchErrorCode::NotFound)?;
            (id.to_owned(), old.radius.clone())
        }
        None => (
            next.fresh_id(BONE_ID_PREFIX)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?,
            Presence::Missing,
        ),
    };
    let bone = RigBone {
        segments: segments.clone(),
        rest_segments: segments,
        closed,
        radius,
    };
    next.bones = replaced(&next.bones, &id, bone);
    Ok(next)
}

/// The vector a dragged handle gets and, unless the tangent is broken (Alt), the mirrored vector of the
/// other handle: `hVec = pointer - anchor`, `other = [-hVec.x, -hVec.y]`.
pub(super) fn moved_handles(
    anchor: [f64; 2],
    to: [f64; 2],
    break_tangent: bool,
) -> ([f64; 2], Option<[f64; 2]>) {
    let vector = [to[0] - anchor[0], to[1] - anchor[1]];
    let mirrored = if break_tangent {
        None
    } else {
        Some([-vector[0], -vector[1]])
    };
    (vector, mirrored)
}

fn handle_slot(
    old: &Presence<Option<RigPair>>,
    value: [f64; 2],
) -> Rule<Presence<Option<RigPair>>> {
    let fresh = [spelled(value[0])?, spelled(value[1])?];
    if let Some(Some(before)) = old.value() {
        if value_of(&before[0]) == value_of(&fresh[0])
            && value_of(&before[1]) == value_of(&fresh[1])
        {
            return Ok(Presence::Present(Some(before.clone())));
        }
    }
    Ok(Presence::Present(Some(fresh)))
}

/// The pose write of `onMove`: an anchor, or a handle (relative to the anchor) with its mirror.
pub(super) fn move_segment(
    rig: &RigRecord,
    bone_id: &str,
    vi: usize,
    part: SegmentPart,
    to: [f64; 2],
    break_tangent: bool,
) -> Rule<RigRecord> {
    let mut bone = rig
        .bone(bone_id)
        .ok_or(DispatchErrorCode::NotFound)?
        .clone();
    let segment = bone
        .segments
        .get_mut(vi)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    match part {
        SegmentPart::Point => {
            let point = [
                keep_equal(&segment.point[0], spelled(to[0])?),
                keep_equal(&segment.point[1], spelled(to[1])?),
            ];
            segment.point = point;
        }
        SegmentPart::HandleIn | SegmentPart::HandleOut => {
            let anchor = [value_of(&segment.point[0]), value_of(&segment.point[1])];
            let (this, other) = moved_handles(anchor, to, break_tangent);
            let (first, second) = if part == SegmentPart::HandleIn {
                (&mut segment.handle_in, &mut segment.handle_out)
            } else {
                (&mut segment.handle_out, &mut segment.handle_in)
            };
            *first = handle_slot(&*first, this)?;
            if let Some(mirror) = other {
                *second = handle_slot(&*second, mirror)?;
            }
        }
    }
    let mut next = rig.clone();
    next.bones = replaced(&next.bones, bone_id, bone);
    Ok(next)
}

/// `Math.max(1, distance)` of the influence-circle drag.
pub(super) fn radius_floor(radius: f64) -> f64 {
    if radius > 1.0 {
        radius
    } else {
        1.0
    }
}

pub(super) fn set_radius(rig: &RigRecord, bone_id: &str, radius: f64) -> Rule<RigRecord> {
    let old = rig.bone(bone_id).ok_or(DispatchErrorCode::NotFound)?;
    let mut bone = old.clone();
    let value = spelled(radius_floor(radius))?;
    bone.radius = Presence::Present(match old.radius.value() {
        Some(before) => keep_equal(before, value),
        None => value,
    });
    let mut next = rig.clone();
    next.bones = replaced(&next.bones, bone_id, bone);
    Ok(next)
}

fn control_weight(entry: &ControlEntry) -> Rule<RigWeight> {
    Ok(RigWeight {
        bone_id: entry.bone_id.clone(),
        pt_index: Presence::Present(entry.pt_index as u32),
        offset: Presence::Missing,
        w: spelled(entry.w)?,
    })
}

fn offset_weight(entry: &OffsetEntry) -> Rule<RigWeight> {
    Ok(RigWeight {
        bone_id: entry.bone_id.clone(),
        pt_index: Presence::Missing,
        offset: Presence::Present(spelled(entry.offset)?),
        w: spelled(entry.w)?,
    })
}

/// What `rigBindStroke` pushes: `{strokeId, rest, restHandles, weights, rotate}`.
fn bind_from_plan(plan: &PlannedBind, rotate: bool) -> Rule<RigBind> {
    let weights = plan
        .weights
        .iter()
        .map(|list| list.iter().map(control_weight).collect::<Rule<Vec<_>>>())
        .collect::<Rule<Vec<_>>>()?;
    let handles = plan
        .rest_handles
        .iter()
        .map(|pair| RestHandle {
            i: Presence::Present(pair[0].clone()),
            o: Presence::Present(pair[1].clone()),
        })
        .collect();
    Ok(RigBind {
        stroke_id: Presence::Present(plan.stroke_id.clone()),
        mesh_id: Presence::Missing,
        rest: plan.rest.clone(),
        rest_handles: Presence::Present(handles),
        weights,
        rotate,
    })
}

/// Binds REPLACE, never stack: each planned bind removes every bind of its stroke and is appended.
pub(super) fn apply_binds(
    rig: &RigRecord,
    planned: &[PlannedBind],
    rotate: bool,
) -> Rule<RigRecord> {
    let mut next = rig.clone();
    for plan in planned {
        let bind = bind_from_plan(plan, rotate)?;
        next.binds.retain(|old| {
            old.stroke_id.value().map(String::as_str) != Some(plan.stroke_id.as_str())
        });
        next.binds.push(bind);
    }
    Ok(next)
}

fn bind_slot<'a>(
    rig: &'a mut RigRecord,
    target: &RigTarget,
    vertex: usize,
) -> Rule<&'a mut Vec<RigWeight>> {
    let index = bind_index(rig, target.bind()).map_err(PlanRefusal::code)?;
    rig.binds[index]
        .weights
        .get_mut(vertex)
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// `Math.max(0, Math.min(100, parseFloat(input) || 0)) / 100` of the popover's percent input.
pub(super) fn weight_fraction(percent: f64) -> f64 {
    percent.min(100.0).max(0.0) / 100.0
}

pub(super) fn set_weight(
    rig: &RigRecord,
    target: &RigTarget,
    vertex: usize,
    entry: usize,
    percent: f64,
) -> Rule<RigRecord> {
    let mut next = rig.clone();
    let list = bind_slot(&mut next, target, vertex)?;
    let slot = list
        .get_mut(entry)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    let value = spelled(weight_fraction(percent))?;
    slot.w = keep_equal(&slot.w, value);
    Ok(next)
}

pub(super) fn remove_weight(
    rig: &RigRecord,
    target: &RigTarget,
    vertex: usize,
    entry: usize,
) -> Rule<RigRecord> {
    let mut next = rig.clone();
    let list = bind_slot(&mut next, target, vertex)?;
    if entry >= list.len() {
        return bad();
    }
    list.remove(entry);
    Ok(next)
}

/// The popover's add row: `{boneId, offset, w: 0.5}`, offered only for a bone the vertex lacks.
pub(super) fn add_weight(
    rig: &RigRecord,
    target: &RigTarget,
    vertex: usize,
    bone_id: &str,
    offset: f64,
) -> Rule<RigRecord> {
    rig.bone(bone_id).ok_or(DispatchErrorCode::NotFound)?;
    let added = RigWeight {
        bone_id: bone_id.to_owned(),
        pt_index: Presence::Missing,
        offset: Presence::Present(spelled(offset)?),
        w: spelled(0.5)?,
    };
    let mut next = rig.clone();
    let list = bind_slot(&mut next, target, vertex)?;
    if list.iter().any(|weight| weight.bone_id == bone_id) {
        return bad();
    }
    list.push(added);
    Ok(next)
}

/// "Reset (auto)": the vertex's list replaced by the offset-model weights `rigWeighOnePoint` returned.
pub(super) fn replace_weights(
    rig: &RigRecord,
    target: &RigTarget,
    vertex: usize,
    entries: &[OffsetEntry],
) -> Rule<RigRecord> {
    let fresh = entries
        .iter()
        .map(offset_weight)
        .collect::<Rule<Vec<_>>>()?;
    let mut next = rig.clone();
    let list = bind_slot(&mut next, target, vertex)?;
    *list = fresh;
    Ok(next)
}

/// `rigSetIK`: the chain filed under `end.boneId:end.vi` (an existing key is overwritten in place).
pub(super) fn set_ik(
    rig: &RigRecord,
    refs: [&RigRef; 3],
    flip: bool,
    lengths: [f64; 2],
) -> Rule<RigRecord> {
    let make = |reference: &RigRef| IkRefRecord {
        bone_id: reference.bone_id.clone(),
        vi: reference.vi,
    };
    let mut chain = IkChainRecord {
        root: make(refs[0]),
        joint: make(refs[1]),
        end: make(refs[2]),
        l1: spelled(lengths[0])?,
        l2: spelled(lengths[1])?,
        flip,
    };
    let key = chain.expected_key();
    if let Some(old) = rig.ik_chains.get(&key) {
        chain.l1 = keep_equal(&old.l1, chain.l1.clone());
        chain.l2 = keep_equal(&old.l2, chain.l2.clone());
    }
    let mut next = rig.clone();
    next.ik_chains = replaced(&next.ik_chains, &key, chain);
    Ok(next)
}

pub(super) fn remove_ik(rig: &RigRecord, end_key: &str) -> Rule<RigRecord> {
    rig.ik_chains
        .get(end_key)
        .ok_or(DispatchErrorCode::NotFound)?;
    let mut next = rig.clone();
    next.ik_chains = removed(&next.ik_chains, end_key);
    Ok(next)
}

/// A bone still named by an IK chain or a weight entry is refused: nothing cascades.
pub(super) fn remove_bone(rig: &RigRecord, bone_id: &str) -> Rule<RigRecord> {
    rig.bone(bone_id).ok_or(DispatchErrorCode::NotFound)?;
    let in_chain = rig.ik_chains.entries().iter().any(|(_, chain)| {
        [&chain.root, &chain.joint, &chain.end]
            .iter()
            .any(|reference| reference.bone_id == bone_id)
    });
    let in_weights = rig.binds.iter().any(|bind| {
        bind.weights
            .iter()
            .flatten()
            .any(|weight| weight.bone_id == bone_id)
    });
    if in_chain || in_weights {
        return bad();
    }
    let mut next = rig.clone();
    next.bones = removed(&next.bones, bone_id);
    Ok(next)
}

/// Every bind that drives `target` goes (an old save may hold two).
pub(super) fn remove_bind(rig: &RigRecord, target: &RigTarget) -> Rule<RigRecord> {
    let wanted = target.bind();
    let mut next = rig.clone();
    next.binds.retain(|bind| bind.target() != Some(wanted));
    if next.binds.len() == rig.binds.len() {
        return Err(DispatchErrorCode::NotFound);
    }
    Ok(next)
}
