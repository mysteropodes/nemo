//! Plans of the rig edit transaction (I12), command layer: what `rigBindStroke`, `rigAutoAssignLayer`
//! and the weight popover compute from the pinned document, as plain values. Nothing here publishes
//! anything and nothing mutates the document; the command layer turns a plan into a rig record.
//!
//! Reading the stroke is `path.segments.map(s => [s.point.x, s.point.y])` plus the rest tangents
//! `{i, o}` (`null` for a zero handle, the baseline's `(x || y) ? [x, y] : null`); the weights come
//! from the kernels through the read-queries facade `crate::object_snapshot::object_rig_edit_read`
//! (the command layer may not import the evaluation layer). A stroke is a plain
//! path record of the layer at one authored frame (the stable stroke identity); the held-frame
//! resolution of layer timing and component contexts are joins of later leaves.
//!
//! What the baseline selects, kept: `rigAutoAssignLayer` binds every path of the layer against every
//! bone, in layer order, with the radius grown to the farthest vertex of any path from any bone
//! without a radius of its own; a CompoundPath is skipped and counted (`skippedUnsupported`). A
//! record of any object family other than the two cubic-path families is the native equivalent of
//! that unsupported target and is skipped and counted the same way. A pressure ribbon (the baseline
//! vector brush) is NOT skipped: the baseline binds and poses it, centreline included, which the commit
//! cannot do yet, so binding one is the typed `PressureRibbon` refusal for `bindStroke` and for an
//! `autoAssign` pass that meets one, never a bind whose centreline would silently stay at rest.
use crate::object_document::object_rig_record::{
    BindTarget, RigBind, RigPair, RigRecord, RigSegment, PATH_FAMILIES,
};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectPoint, ObjectRecord, Presence};
use crate::object_snapshot::object_rig_edit_read::{
    auto_radius, ik_lengths, offset_for, weigh_control, weigh_offset, ControlEntry, KernelBone,
    KernelRef, KernelRefusal, KernelVertex, OffsetEntry,
};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

/// Why a plan could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanRefusal {
    NoLayer,
    NoRig,
    NoStroke,
    NoBind,
    AmbiguousBind,
    NoBone,
    UnknownBone,
    DuplicateBone,
    UnsupportedFamily,
    /// A pressure ribbon: a bind poses `geometry.segments` only, so its centreline and width profile would go
    /// stale at the first commit (the same reason L01-G refuses a ribbon).
    PressureRibbon,
    EmptyStroke,
    BadVertex,
    Kernel(KernelRefusal),
}

impl PlanRefusal {
    /// The dispatch code the command layer reports.
    pub(crate) fn code(self) -> DispatchErrorCode {
        match self {
            Self::NoLayer | Self::NoRig | Self::NoStroke | Self::NoBind | Self::NoBone => {
                DispatchErrorCode::NotFound
            }
            Self::Kernel(KernelRefusal::Unavailable) | Self::PressureRibbon => {
                DispatchErrorCode::Unavailable
            }
            Self::AmbiguousBind
            | Self::UnknownBone
            | Self::DuplicateBone
            | Self::UnsupportedFamily
            | Self::EmptyStroke
            | Self::BadVertex
            | Self::Kernel(_) => DispatchErrorCode::InvalidRequest,
        }
    }
}

impl From<KernelRefusal> for PlanRefusal {
    fn from(refusal: KernelRefusal) -> Self {
        Self::Kernel(refusal)
    }
}

/// What `rigBindStroke` pushes, before it becomes a record: the authored rest numbers, the rest
/// tangents (`None` is the baseline `null`) and the control-point weights per vertex.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlannedBind {
    pub(crate) stroke_id: String,
    pub(crate) rest: Vec<RigPair>,
    pub(crate) rest_handles: Vec<[Option<RigPair>; 2]>,
    pub(crate) weights: Vec<Vec<ControlEntry>>,
}

/// The binds one `rigAutoAssignLayer` pass makes, in layer order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AutoPlan {
    pub(crate) binds: Vec<PlannedBind>,
    pub(crate) skipped_unsupported: u32,
    /// The grown fallback radius (`None` when the rig has no bones and nothing was scanned).
    pub(crate) radius: Option<f64>,
}

fn float(number: &Number) -> f64 {
    // Admitted numbers are finite; NaN marks a caller that skipped admission.
    number.as_f64().unwrap_or(f64::NAN)
}

fn pair(value: &RigPair) -> [f64; 2] {
    [float(&value[0]), float(&value[1])]
}

fn handle(slot: &Presence<Option<RigPair>>) -> [f64; 2] {
    match slot.value() {
        Some(Some(value)) => pair(value),
        _ => [0.0, 0.0],
    }
}

fn vertex(segment: &RigSegment) -> KernelVertex {
    KernelVertex {
        point: pair(&segment.point),
        handle_in: handle(&segment.handle_in),
        handle_out: handle(&segment.handle_out),
    }
}

/// The rig's bones in table order, as the kernels read them.
pub(crate) fn kernel_bones(rig: &RigRecord) -> Vec<KernelBone> {
    rig.bones
        .entries()
        .iter()
        .map(|(id, bone)| KernelBone {
            id: id.clone(),
            radius: bone.radius_value(),
            closed: bone.closed,
            live: bone.segments.iter().map(vertex).collect(),
            rest: bone.rest_segments.iter().map(vertex).collect(),
        })
        .collect()
}

fn every_bone_id(rig: &RigRecord) -> Vec<String> {
    rig.bones
        .entries()
        .iter()
        .map(|(id, _)| id.clone())
        .collect()
}

/// The rig of `layer_uid`.
pub(crate) fn rig_of<'a>(
    document: &'a ObjectDocument,
    layer_uid: &str,
) -> Result<&'a RigRecord, PlanRefusal> {
    document
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == layer_uid)
        .ok_or(PlanRefusal::NoLayer)?
        .rig()
        .ok_or(PlanRefusal::NoRig)
}

/// The position of the single bind that drives `target`. The baseline's re-bind rule keeps one bind per
/// target; an old save with two is ambiguous and refused instead of guessed.
pub(crate) fn bind_index(rig: &RigRecord, target: BindTarget<'_>) -> Result<usize, PlanRefusal> {
    let mut found = rig
        .binds
        .iter()
        .enumerate()
        .filter(|(_, bind)| bind.target() == Some(target))
        .map(|(index, _)| index);
    let first = found.next().ok_or(PlanRefusal::NoBind)?;
    if found.next().is_some() {
        return Err(PlanRefusal::AmbiguousBind);
    }
    Ok(first)
}

fn authored_at(record: &ObjectRecord, layer_uid: &str, frame: u32) -> bool {
    let target = record.target();
    target.layer_uid() == layer_uid
        && target.frame() == frame
        && *target.scope_kind() == FrameScopeKind::Authored
}

/// The path records the rig can bind on `layer_uid` at `frame` (in record order) and the number of
/// records of an unsupported family (`skippedUnsupported`).
pub(crate) fn split_records<'a>(
    objects: &'a [ObjectRecord],
    layer_uid: &str,
    frame: u32,
) -> (Vec<&'a ObjectRecord>, u32) {
    let mut paths = Vec::new();
    let mut skipped = 0u32;
    for record in objects
        .iter()
        .filter(|record| authored_at(record, layer_uid, frame))
    {
        if PATH_FAMILIES.contains(&record.family.as_str()) {
            paths.push(record);
        } else {
            skipped += 1;
        }
    }
    (paths, skipped)
}

/// `(handle && (handle.x || handle.y)) ? [x, y] : null`.
fn rest_handle(handle: &ObjectPoint) -> Option<RigPair> {
    if float(&handle.x) != 0.0 || float(&handle.y) != 0.0 {
        Some([handle.x.clone(), handle.y.clone()])
    } else {
        None
    }
}

/// The bones a bind pass weighs against: all of them, or the requested ones (each must exist and be
/// named once; the baseline weigher silently skips an unknown id).
fn resolve_bone_ids(
    rig: &RigRecord,
    requested: Option<&[String]>,
) -> Result<Vec<String>, PlanRefusal> {
    let Some(ids) = requested else {
        return Ok(every_bone_id(rig));
    };
    for (index, id) in ids.iter().enumerate() {
        if rig.bone(id).is_none() {
            return Err(PlanRefusal::UnknownBone);
        }
        if ids[..index].contains(id) {
            return Err(PlanRefusal::DuplicateBone);
        }
    }
    Ok(ids.to_vec())
}

fn bind_for(
    rig: &RigRecord,
    record: &ObjectRecord,
    bone_ids: &[String],
    radius: Option<f64>,
    softness: Option<f64>,
) -> Result<PlannedBind, PlanRefusal> {
    if !PATH_FAMILIES.contains(&record.family.as_str()) {
        return Err(PlanRefusal::UnsupportedFamily);
    }
    if record.is_pressure_ribbon() {
        return Err(PlanRefusal::PressureRibbon);
    }
    let segments = &record.geometry.segments;
    if segments.is_empty() {
        return Err(PlanRefusal::EmptyStroke);
    }
    let points: Vec<[f64; 2]> = segments
        .iter()
        .map(|s| [float(&s.point.x), float(&s.point.y)])
        .collect();
    let weights = weigh_control(&kernel_bones(rig), bone_ids, &points, radius, softness)?;
    Ok(PlannedBind {
        stroke_id: record.target().stroke_id().to_owned(),
        rest: segments
            .iter()
            .map(|s| [s.point.x.clone(), s.point.y.clone()])
            .collect(),
        rest_handles: segments
            .iter()
            .map(|s| [rest_handle(&s.handle_in), rest_handle(&s.handle_out)])
            .collect(),
        weights,
    })
}

/// `rigBindStroke(ld, path, boneIds, radius, rotate, softness)` for the stroke `stroke_id` of the
/// layer at `frame`; `bone_ids` of `None` means every bone.
pub(crate) fn plan_stroke_bind(
    document: &ObjectDocument,
    layer_uid: &str,
    stroke_id: &str,
    frame: u32,
    bone_ids: Option<&[String]>,
    radius: Option<f64>,
    softness: Option<f64>,
) -> Result<PlannedBind, PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    let record = document
        .objects()
        .iter()
        .find(|record| {
            authored_at(record, layer_uid, frame) && record.target().stroke_id() == stroke_id
        })
        .ok_or(PlanRefusal::NoStroke)?;
    let ids = resolve_bone_ids(rig, bone_ids)?;
    bind_for(rig, record, &ids, radius, softness)
}

/// `rigAutoAssignLayer`: one bind per supported path record, the radius grown first. Without bones
/// the baseline returns `0` before touching anything; the plan is then empty.
pub(crate) fn plan_auto_assign(
    document: &ObjectDocument,
    layer_uid: &str,
    frame: u32,
    default_radius: Option<f64>,
    softness: Option<f64>,
    want_paths: bool,
) -> Result<AutoPlan, PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    let empty = AutoPlan {
        binds: Vec::new(),
        skipped_unsupported: 0,
        radius: None,
    };
    if rig.bones.len() == 0 {
        return Ok(empty);
    }
    let (paths, skipped) = split_records(document.objects(), layer_uid, frame);
    let bones = kernel_bones(rig);
    let mut points = Vec::new();
    for record in &paths {
        for segment in &record.geometry.segments {
            points.push([float(&segment.point.x), float(&segment.point.y)]);
        }
    }
    let needed = auto_radius(default_radius, &bones, &points)?;
    if !want_paths {
        return Ok(AutoPlan {
            radius: needed,
            ..empty
        });
    }
    let ids = every_bone_id(rig);
    let mut binds = Vec::new();
    for record in paths {
        binds.push(bind_for(rig, record, &ids, needed, softness)?);
    }
    Ok(AutoPlan {
        binds,
        skipped_unsupported: skipped,
        radius: needed,
    })
}

fn rest_point_of(bind: &RigBind, vertex: usize) -> Result<[f64; 2], PlanRefusal> {
    bind.rest
        .get(vertex)
        .map(pair)
        .ok_or(PlanRefusal::BadVertex)
}

/// The weight popover's "reset (auto)": `rigWeighOnePoint` for the vertex's rest point against every
/// bone, with the panel's radius (`|| 200`) and softness as given.
pub(crate) fn plan_reset_weights(
    document: &ObjectDocument,
    layer_uid: &str,
    target: BindTarget<'_>,
    vertex: usize,
    radius: Option<f64>,
    softness: f64,
) -> Result<Vec<OffsetEntry>, PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    let bind = &rig.binds[bind_index(rig, target)?];
    let point = rest_point_of(bind, vertex)?;
    Ok(weigh_offset(
        &kernel_bones(rig),
        &every_bone_id(rig),
        point,
        radius,
        softness,
    )?)
}

/// The weight popover's "+ add": the offset of the vertex's rest point on the bone's rest curve.
pub(crate) fn plan_add_weight(
    document: &ObjectDocument,
    layer_uid: &str,
    target: BindTarget<'_>,
    vertex: usize,
    bone_id: &str,
) -> Result<f64, PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    let bind = &rig.binds[bind_index(rig, target)?];
    let point = rest_point_of(bind, vertex)?;
    let bones = kernel_bones(rig);
    let bone = bones
        .iter()
        .find(|bone| bone.id == bone_id)
        .ok_or(PlanRefusal::NoBone)?;
    Ok(offset_for(bone, point)?)
}

/// `rigSetIK`: the two lengths of the chain, measured on the live pose.
pub(crate) fn plan_ik(
    document: &ObjectDocument,
    layer_uid: &str,
    root: &KernelRef,
    joint: &KernelRef,
    end: &KernelRef,
) -> Result<[f64; 2], PlanRefusal> {
    let rig = rig_of(document, layer_uid)?;
    Ok(ik_lengths(&kernel_bones(rig), root, joint, end)?)
}
