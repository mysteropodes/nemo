//! Document-level plans of the destructive boolean edits (G19 + G18, L02-I): the
//! boolean tool (`booleanOp`), the eraser bite (`eraseAtPoint`) and Flatten
//! (`flattenGroup`), each as a pure function from an admitted document to a
//! detached candidate document.
//!
//! One plan edits ONE frame of ONE layer. Operands are the authored records of
//! that frame; their rings must be straight-edged (every handle exactly zero):
//! flattening a curved record is the retained geometry owner's job and is a
//! typed refusal here, never an approximation. A stroke-only record needs the
//! stroker and is refused the same way. Results are island rings from
//! `flatten_polygons`, styled and identified like `insertBooleanResult`: the
//! first island keeps the style source's stroke id, later ones get fresh ids.
//! An empty boolean result is refused (the baseline inserted an empty path);
//! an erase that leaves nothing deletes its target, as the baseline does.
//! Group membership of consumed strokes is detached through the group plan in
//! the SAME candidate, so one Undo restores records, order and groups together.
use serde_json::Number;

use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    apply_stack, find_record, fresh_ids, island_record, stack_of, OutlineSpec, ResultStyle, Scope,
    StackError, MAX_SEGMENTS,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    flatten_polygons, fold_multi, BoolOp, ElementMat, Island, MultiPolygon, OpsError, Point,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    admit_polygons, DataValue, Polygon, SourceData, TopologyError,
};
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    CombineError, FlattenError,
};
use crate::object_document::object_group_kernels::object_group_plan::{
    plan_group_edit, GroupEdit, GroupPlanError,
};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, ObjectTarget};

#[path = "object_boolean_ops_erase.rs"]
mod erase;
#[path = "object_boolean_ops_flatten.rs"]
mod flatten;

/// One user action on a frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BooleanRequest {
    /// `booleanOp`: operands in selection order, the LAST one is the style source.
    Op { op: BoolOp, strokes: Vec<String> },
    /// `eraseAtPoint`: one target and the eraser footprint ring (world coordinates).
    Erase {
        stroke: String,
        footprint: Vec<Point>,
    },
    /// `flattenGroup`: the group's leaves, folded with its mode (`none` is `unite`).
    Flatten { group: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanError {
    UnknownLayer(String),
    Locked(String),
    /// Context, frame or scope kind the document does not have.
    InvalidScope,
    UnknownStroke(String),
    FewerThanTwo(usize),
    DuplicateOperand(String),
    /// A handle is not zero: the curve needs the retained flattener.
    Curved(String),
    /// No solid fill: a stroke-only record needs the stroker.
    Unfilled(String),
    /// Fewer than three points.
    Degenerate(String),
    /// An open, non-brush erase target needs the stroker.
    OpenTarget(String),
    UnknownGroup(String),
    TableUnreadable(String),
    /// A group member has no record on this frame.
    MemberAbsent(String),
    /// Element Motion drives this member and no evaluator is connected.
    MotionUnavailable(String),
    Flatten(FlattenError),
    Combine(CombineError),
    Ops(OpsError),
    Topology(TopologyError),
    Group(GroupPlanError),
    EmptyResult,
    TooManySegments {
        island: usize,
        segments: usize,
    },
    /// Removing the last drawing of an element that a property holder still animates.
    HolderOrphan(String),
    Stack(StackError),
    IdUnavailable,
    FillUnavailable,
    Internal(&'static str),
}

/// Element Motion at the request frame. `Ok(None)`: the element has no Motion.
pub(crate) trait ElementMotionSource {
    fn element_mat(&self, target: &ObjectTarget) -> Result<Option<ElementMat>, MotionUnavailable>;
    /// Only the real pinned provider may materialize and detach consumed Motion.
    /// Supplied legacy/test ports retain the original orphan-refusal contract.
    fn consumes_authored_motion(&self) -> bool {
        false
    }
    /// A snapshot-bound consumer transforms the original cubic before output-
    /// space approximation. Legacy supplied ports keep the original polygon API.
    fn materialized_polygons(
        &self,
        _record: &ObjectRecord,
    ) -> Result<MultiPolygon, MotionUnavailable> {
        Err(MotionUnavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MotionUnavailable;

/// The production port until the element-Motion evaluator is connected.
pub(crate) struct UnavailableMotion;

impl ElementMotionSource for UnavailableMotion {
    fn element_mat(&self, _target: &ObjectTarget) -> Result<Option<ElementMat>, MotionUnavailable> {
        Err(MotionUnavailable)
    }
}

/// The detached candidate of one edit.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BooleanPlan {
    pub(crate) document: ObjectDocument,
    pub(crate) changed: bool,
    /// New records' stroke ids in z-order (the first one keeps the style source's id).
    pub(crate) created: Vec<String>,
    /// Stroke ids whose record left the frame for good.
    pub(crate) removed: Vec<String>,
}

pub(crate) fn unchanged(document: &ObjectDocument) -> BooleanPlan {
    BooleanPlan {
        document: document.clone(),
        changed: false,
        created: Vec::new(),
        removed: Vec::new(),
    }
}

pub(crate) fn check_scope(document: &ObjectDocument, scope: &Scope) -> Result<(), PlanError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.layer_uid == scope.layer_uid)
        .ok_or_else(|| PlanError::UnknownLayer(scope.layer_uid.clone()))?;
    if layer.locked() {
        return Err(PlanError::Locked(scope.layer_uid.clone()));
    }
    if !document.valid_target(&scope.target("x")) || scope.frame >= document.total_frames() {
        return Err(PlanError::InvalidScope);
    }
    Ok(())
}

pub(crate) fn number_of(value: &Number) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

/// The straight-edged ring of a record.
pub(crate) fn record_ring(record: &ObjectRecord) -> Result<Vec<Point>, PlanError> {
    let stroke = record.target.stroke_id.clone();
    let mut ring: Vec<Point> = Vec::with_capacity(record.geometry.segments.len());
    for segment in &record.geometry.segments {
        for handle in [&segment.handle_in, &segment.handle_out] {
            if handle.x.as_f64() != Some(0.0) || handle.y.as_f64() != Some(0.0) {
                return Err(PlanError::Curved(stroke));
            }
        }
        ring.push([number_of(&segment.point.x), number_of(&segment.point.y)]);
    }
    if ring.len() < 3 {
        return Err(PlanError::Degenerate(stroke));
    }
    Ok(ring)
}

pub(crate) fn filled_polygons(record: &ObjectRecord) -> Result<MultiPolygon, PlanError> {
    if record.solid_fill().is_none() {
        return Err(PlanError::Unfilled(record.target.stroke_id.clone()));
    }
    Ok(vec![Polygon {
        exterior: record_ring(record)?,
        holes: Vec::new(),
    }])
}

pub(crate) fn is_brush(record: &ObjectRecord) -> bool {
    record
        .vector_ext
        .value()
        .is_some_and(|ext| ext.pressure.value().is_some())
}

pub(crate) fn box_angle(record: &ObjectRecord) -> Option<Number> {
    record
        .vector_ext
        .value()
        .and_then(|ext| ext.box_angle.value().cloned())
}

pub(crate) fn opacity_of(record: &ObjectRecord) -> Number {
    record
        .vector()
        .map_or_else(|| Number::from(1u64), |body| body.opacity.clone())
}

pub(crate) fn outline_of(record: &ObjectRecord) -> Option<OutlineSpec> {
    let body = record.vector()?;
    let paint = body.solid_stroke()?.clone();
    Some(OutlineSpec {
        paint,
        width: body.stroke_width.clone(),
        cap: body.stroke_cap.clone(),
        join: body.stroke_join.clone(),
    })
}

/// `origData` as far as the native record keeps it: the identity and the box angle.
pub(crate) fn source_data(record: &ObjectRecord) -> SourceData {
    let mut data = SourceData::new();
    data.insert(
        "strokeId".to_string(),
        DataValue::Text(record.target.stroke_id.clone()),
    );
    if let Some(angle) = box_angle(record) {
        data.insert("boxAngle".to_string(), DataValue::Number(number_of(&angle)));
    }
    data
}

/// Does a property holder animate this element? Then its Motion matters.
pub(crate) fn element_holder(document: &ObjectDocument, target: &ObjectTarget) -> bool {
    document
        .property_holders()
        .into_iter()
        .flatten()
        .any(|holder| {
            holder.target.context_id == target.context_id
                && holder.target.layer_uid == target.layer_uid
                && holder.target.stroke_id.value() == Some(&target.stroke_id)
        })
}

/// Detach consumed strokes from every group, in the same candidate.
pub(crate) fn detach(
    document: ObjectDocument,
    scope: &Scope,
    strokes: &[String],
) -> Result<ObjectDocument, PlanError> {
    let has_table = document
        .layers
        .iter()
        .any(|layer| layer.layer_uid == scope.layer_uid && layer.groups.value().is_some());
    if !has_table {
        return Ok(document);
    }
    let plan = plan_group_edit(
        &document,
        &scope.layer_uid,
        &scope.context_id,
        &GroupEdit::Detach {
            strokes: strokes.to_vec(),
        },
    )
    .map_err(PlanError::Group)?;
    Ok(plan.document)
}

/// Replace `removed` by the islands of `islands` in the frame stack and write it back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish(
    mut document: ObjectDocument,
    scope: &Scope,
    removed: &[String],
    insert_at: Option<usize>,
    islands: &[Island],
    first_id: &str,
    style: &ResultStyle<'_>,
    revision: u64,
) -> Result<BooleanPlan, PlanError> {
    let stack = stack_of(&document, scope);
    let kept: Vec<String> = stack
        .iter()
        .filter(|id| !removed.contains(id))
        .cloned()
        .collect();
    let at = insert_at.map_or(kept.len(), |index| index.min(kept.len()));
    let mut ids: Vec<String> = Vec::with_capacity(islands.len());
    if !islands.is_empty() {
        ids.push(first_id.to_string());
        ids.extend(
            fresh_ids(&document, revision, islands.len() - 1).ok_or(PlanError::IdUnavailable)?,
        );
    }
    let mut records = Vec::with_capacity(islands.len());
    for (k, island) in islands.iter().enumerate() {
        if island.segments.len() > MAX_SEGMENTS {
            return Err(PlanError::TooManySegments {
                island: k,
                segments: island.segments.len(),
            });
        }
        let record = island_record(style, scope.target(&ids[k]), &island.segments, island.build)
            .ok_or(PlanError::Internal("non-finite result coordinate"))?;
        records.push(record);
    }
    let mut final_stack: Vec<String> = kept[..at].to_vec();
    final_stack.extend(ids.iter().cloned());
    final_stack.extend(kept[at..].iter().cloned());
    apply_stack(&mut document, scope, &final_stack, removed, records).map_err(PlanError::Stack)?;
    let gone: Vec<String> = removed
        .iter()
        .filter(|id| !ids.contains(id))
        .cloned()
        .collect();
    for id in &gone {
        let target = scope.target(id);
        let remains = document.objects.iter().any(|record| {
            record.target.context_id == scope.context_id
                && record.target.layer_uid == scope.layer_uid
                && record.target.frame_scope.kind == FrameScopeKind::Authored
                && record.target.stroke_id == *id
        });
        if !remains && element_holder(&document, &target) {
            return Err(PlanError::HolderOrphan(id.clone()));
        }
    }
    Ok(BooleanPlan {
        document,
        changed: true,
        created: ids,
        removed: gone,
    })
}

pub(crate) fn result_style<'a>(record: &'a ObjectRecord) -> Result<ResultStyle<'a>, PlanError> {
    let fill = record
        .solid_fill()
        .cloned()
        .ok_or_else(|| PlanError::Unfilled(record.target.stroke_id.clone()))?;
    Ok(ResultStyle {
        source: record,
        fill,
        opacity: opacity_of(record),
        outline: None,
        box_angle: box_angle(record),
        keep_source_stroke_style: false,
    })
}

fn plan_op(
    document: &ObjectDocument,
    scope: &Scope,
    op: BoolOp,
    strokes: &[String],
    revision: u64,
) -> Result<BooleanPlan, PlanError> {
    if strokes.len() < 2 {
        return Err(PlanError::FewerThanTwo(strokes.len()));
    }
    let mut records: Vec<&ObjectRecord> = Vec::with_capacity(strokes.len());
    for (k, id) in strokes.iter().enumerate() {
        if strokes[..k].contains(id) {
            return Err(PlanError::DuplicateOperand(id.clone()));
        }
        let at =
            find_record(document, scope, id).ok_or_else(|| PlanError::UnknownStroke(id.clone()))?;
        records.push(&document.objects[at]);
    }
    let mut operands: Vec<MultiPolygon> = Vec::with_capacity(records.len());
    for record in &records {
        operands.push(filled_polygons(record)?);
    }
    let folded = fold_multi(op, &operands).map_err(PlanError::Ops)?;
    if folded.is_empty() {
        return Err(PlanError::EmptyResult);
    }
    admit_polygons(&folded).map_err(PlanError::Topology)?;
    let islands = flatten_polygons(&folded, false).map_err(PlanError::Topology)?;
    let style_record = records[records.len() - 1];
    let mut style = result_style(style_record)?;
    // booleanOp (not Flatten) sets the style source's stroke width / cap / join on the result path itself
    style.keep_source_stroke_style = true;
    let detached = detach(document.clone(), scope, strokes)?;
    finish(
        detached,
        scope,
        strokes,
        None,
        &islands,
        &style_record.target.stroke_id,
        &style,
        revision,
    )
}

/// Plan one boolean edit of one frame. `revision` is the origin content revision
/// (fresh ids are `native-stroke:{revision + 1}:{n}`).
pub(crate) fn plan_boolean(
    document: &ObjectDocument,
    scope: &Scope,
    request: &BooleanRequest,
    revision: u64,
    motion: &dyn ElementMotionSource,
) -> Result<BooleanPlan, PlanError> {
    check_scope(document, scope)?;
    match request {
        BooleanRequest::Op { op, strokes } => plan_op(document, scope, *op, strokes, revision),
        BooleanRequest::Erase { stroke, footprint } => {
            erase::plan_erase(document, scope, stroke, footprint, revision)
        }
        BooleanRequest::Flatten { group } => {
            flatten::plan_flatten_group(document, scope, group, revision, motion)
        }
    }
}
