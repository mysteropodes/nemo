//! The eraser bite as a document plan (`eraseAtPoint` of `src/js/tools.js` at fbe4b8a3,
//! G18, L02-I): the target's ring minus the footprint, replaced IN PLACE by its islands
//! (the first keeps the stroke id), or deleted when nothing is left. The decision half
//! (no fill, no area, outline redraw, vector-brush ink) is `plan_erase_polygons` of
//! `object_boolean_topology`; this file feeds it and writes the records.
use crate::object_document::object_group_kernels::object_boolean_ops::plan::number_of;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    box_angle, detach, finish, is_brush, opacity_of, outline_of, record_ring, source_data,
    unchanged, BooleanPlan, PlanError,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    find_record, hex8, stack_of, ResultStyle, Scope,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    erase_difference, flatten_polygons, Point,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    erase_stroke_info, plan_erase_polygons, DataValue, EraseOutcome, EraseTarget, StrokeIdAction,
    StrokeInfo,
};
use crate::object_document::{ObjectDocument, SolidFill};

fn extent(points: &[Point]) -> [f64; 4] {
    let mut out = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        out[0] = out[0].min(p[0]);
        out[1] = out[1].min(p[1]);
        out[2] = out[2].max(p[0]);
        out[3] = out[3].max(p[1]);
    }
    out
}

/// The two boxes share no point (touching boxes are not apart).
fn boxes_apart(a: &[Point], b: &[Point]) -> bool {
    let (a, b) = (extent(a), extent(b));
    a[2] < b[0] || b[2] < a[0] || a[3] < b[1] || b[3] < a[1]
}

pub(super) fn plan_erase(
    document: &ObjectDocument,
    scope: &Scope,
    stroke: &str,
    footprint: &[Point],
    revision: u64,
) -> Result<BooleanPlan, PlanError> {
    let at = find_record(document, scope, stroke)
        .ok_or_else(|| PlanError::UnknownStroke(stroke.to_string()))?;
    let record = &document.objects[at];
    let brush = is_brush(record);
    if !record.geometry.closed && !brush {
        return Err(PlanError::OpenTarget(stroke.to_string()));
    }
    let outline = if brush { None } else { outline_of(record) };
    let paint: Option<SolidFill> = match record.solid_fill() {
        Some(fill) => Some(fill.clone()),
        None if !brush => outline.as_ref().map(|o| o.paint.clone()),
        None => None,
    };
    let Some(paint) = paint else {
        return Ok(unchanged(document));
    };
    let ring = record_ring(record)?;
    // A footprint whose box misses the target's box cannot change it: nothing is rewritten
    // (the baseline replaced the target with an identical copy, refit included).
    if boxes_apart(&ring, footprint) {
        return Ok(unchanged(document));
    }
    let polygons = erase_difference(&ring, footprint).map_err(PlanError::Ops)?;
    let stack = stack_of(document, scope);
    let index = stack
        .iter()
        .position(|id| id == stroke)
        .ok_or(PlanError::Internal("target missing from its frame stack"))?;
    let data = source_data(record);
    let info = outline.as_ref().map(|o| StrokeInfo {
        color: hex8(&o.paint),
        width: number_of(&o.width),
        cap: o.cap.clone(),
        join: o.join.clone(),
    });
    let target = EraseTarget {
        index,
        layer_len: stack.len(),
        fill: Some(hex8(&paint)),
        opacity: number_of(&opacity_of(record)),
        stroke: erase_stroke_info(brush, info),
        data: Some(&data),
    };
    let decision = plan_erase_polygons(&target, &polygons).map_err(PlanError::Topology)?;
    match decision.outcome {
        EraseOutcome::Unchanged => Ok(unchanged(document)),
        EraseOutcome::DeleteTarget => {
            let removed = vec![stroke.to_string()];
            let still_drawn = document.objects.iter().any(|other| {
                other.target.context_id == scope.context_id
                    && other.target.layer_uid == scope.layer_uid
                    && other.target.stroke_id == stroke
                    && !scope.owns(&other.target)
            });
            let base = if still_drawn {
                document.clone()
            } else {
                detach(document.clone(), scope, &removed)?
            };
            let style = ResultStyle {
                source: record,
                fill: paint,
                opacity: opacity_of(record),
                outline: None,
                box_angle: box_angle(record),
                keep_source_stroke_style: false,
            };
            finish(
                base,
                scope,
                &removed,
                Some(index),
                &[],
                stroke,
                &style,
                revision,
            )
        }
        EraseOutcome::Replace(insert) => {
            let islands = flatten_polygons(&polygons, false).map_err(PlanError::Topology)?;
            if islands.len() != insert.islands.len() {
                return Err(PlanError::Internal(
                    "island count differs from the topology plan",
                ));
            }
            let first = insert
                .islands
                .first()
                .map(|placed| &placed.carry.stroke_id)
                .ok_or(PlanError::Internal("replace plan without island"))?;
            let kept_id = match first {
                StrokeIdAction::Carry(DataValue::Text(id)) => id.clone(),
                _ => {
                    return Err(PlanError::Internal(
                        "first island does not carry the stroke id",
                    ))
                }
            };
            let style = ResultStyle {
                source: record,
                fill: paint,
                opacity: opacity_of(record),
                outline: if brush { None } else { outline },
                box_angle: box_angle(record),
                keep_source_stroke_style: false,
            };
            let removed = vec![stroke.to_string()];
            finish(
                document.clone(),
                scope,
                &removed,
                Some(index),
                &islands,
                &kept_id,
                &style,
                revision,
            )
        }
    }
}
