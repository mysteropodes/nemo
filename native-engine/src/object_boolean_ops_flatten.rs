//! Flatten as a document plan (`flattenGroup` of `src/js/group-bridge.js` at fbe4b8a3,
//! G19, L02-I): every leaf of the group at ONE frame, element Motion baked in, folded
//! once with the group's mode (`none` is `unite`) through the combine plan of L02-B
//! and the real overlay provider; the originals leave in the same plan and the group
//! with its descendants is detached, so one Undo restores them all.
use std::collections::HashMap;

use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    detach, element_holder, filled_polygons, finish, is_brush, number_of, opacity_of, outline_of,
    result_style, BooleanPlan, ElementMotionSource, PlanError,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    find_record, fresh_ids, hex8, stack_of, Scope,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    bake_multi, flatten_polygons, MemberSource, MultiPolygon, OverlayFold,
};
use crate::object_document::object_group_kernels::object_boolean_topology::admit_polygons;
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    execute_flatten, plan_flatten, CombineLayer, FoldError, MemberFacts,
};
use crate::object_document::object_group_kernels::object_group_record::layer_stroke_ids;
use crate::object_document::ObjectDocument;
use crate::object_document::{Presence,FrameScopeKind};
use crate::object_document::object_group_kernels::object_fill_trace_ops::native::{ring,polygon};
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;

/// Precomputed (Motion-baked) operand polygons by group item.
struct Members {
    shapes: HashMap<usize, MultiPolygon>,
}

impl MemberSource for Members {
    fn member(
        &mut self,
        item: usize,
        _companion: Option<usize>,
    ) -> Result<MultiPolygon, FoldError> {
        self.shapes
            .get(&item)
            .cloned()
            .ok_or_else(|| FoldError::Failed(format!("no member shape for item {item}")))
    }
}

fn facts_of(document: &ObjectDocument, scope: &Scope, strokes: &[String]) -> Vec<MemberFacts> {
    strokes
        .iter()
        .map(|id| {
            let record = find_record(document, scope, id).map(|at| &document.objects[at]);
            MemberFacts {
                visible: true,
                linked_fill_id: None,
                brush_group_id: None,
                is_linked_fill_companion: false,
                is_vector_brush: record.is_some_and(is_brush),
                fill_color: record.and_then(|r| r.solid_fill()).map(hex8),
                stroke_color: record
                    .and_then(outline_of)
                    .map(|outline| hex8(&outline.paint)),
                stroke_width: record
                    .and_then(|r| r.vector())
                    .map_or(1.0, |body| number_of(&body.stroke_width)),
                opacity: record.map_or(1.0, |r| number_of(&opacity_of(r))),
            }
        })
        .collect()
}

pub(super) fn plan_flatten_group(
    document: &ObjectDocument,
    scope: &Scope,
    group: &str,
    revision: u64,
    motion: &dyn ElementMotionSource,
) -> Result<BooleanPlan, PlanError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.layer_uid == scope.layer_uid)
        .ok_or_else(|| PlanError::UnknownLayer(scope.layer_uid.clone()))?;
    let table = layer
        .groups
        .value()
        .ok_or_else(|| PlanError::UnknownGroup(group.to_string()))?;
    let strokes = layer_stroke_ids(&document.objects, &scope.layer_uid, &scope.context_id);
    let groups = table
        .to_layer(&strokes)
        .ok_or_else(|| PlanError::TableUnreadable(scope.layer_uid.clone()))?;
    let facts = facts_of(document, scope, &strokes);
    let combine = CombineLayer::new(&groups, &facts).map_err(PlanError::Combine)?;
    let flatten = plan_flatten(&combine, group).map_err(PlanError::Flatten)?;
    let mut shapes: HashMap<usize, MultiPolygon> = HashMap::new();
    let mut member_ids: Vec<String> = Vec::with_capacity(flatten.operands.len());
    for operand in &flatten.operands {
        let id = strokes[operand.item].clone();
        let at =
            find_record(document, scope, &id).ok_or_else(|| PlanError::MemberAbsent(id.clone()))?;
        let record = &document.objects[at];
        let mut polygons = match filled_polygons(record) {
            Err(PlanError::Curved(_)) if motion.consumes_authored_motion() => {
                let polygons=vec![Polygon { exterior:polygon(&ring(record),true), holes:Vec::new() }];
                admit_polygons(&polygons).map_err(PlanError::Topology)?;
                polygons
            },
            value => value?,
        };
        if element_holder(document, &record.target) {
            let mat = motion
                .element_mat(&record.target)
                .map_err(|_| PlanError::MotionUnavailable(id.clone()))?;
            if let Some(mat) = mat {
                polygons = bake_multi(&polygons, &mat);
            }
        }
        shapes.insert(operand.item, polygons);
        member_ids.push(id);
    }
    let mut port = OverlayFold::new(Members { shapes });
    let pieces = execute_flatten(&flatten, &mut port).map_err(PlanError::Combine)?;
    let folded: MultiPolygon = pieces.into_iter().flatten().collect();
    if folded.is_empty() {
        return Err(PlanError::EmptyResult);
    }
    admit_polygons(&folded).map_err(PlanError::Topology)?;
    let islands = flatten_polygons(&folded, false).map_err(PlanError::Topology)?;
    let style_id = strokes[flatten.style_item].clone();
    let style_at = find_record(document, scope, &style_id)
        .ok_or(PlanError::Internal("style member vanished"))?;
    let style_record = &document.objects[style_at];
    let style = result_style(style_record)?;
    let stack = stack_of(document, scope);
    let first_pos = stack
        .iter()
        .position(|id| *id == member_ids[0])
        .ok_or(PlanError::Internal(
            "first member missing from its frame stack",
        ))?;
    let below = stack[..first_pos]
        .iter()
        .filter(|id| member_ids.contains(id))
        .count();
    let leaves = groups.collect_group_stroke_ids(group);
    let mut detached = detach(document.clone(), scope, &leaves)?;
    let first_id = if motion.consumes_authored_motion() && element_holder(document,&style_record.target) {
        // Never reuse a still-animated identity for an already-baked result.
        // finish() mints its remaining n-1 IDs from the start of this same
        // sequence; selecting the nth one here keeps all IDs disjoint.
        fresh_ids(document,revision,islands.len()).and_then(|mut ids|ids.pop()).ok_or(PlanError::IdUnavailable)?
    } else { style_id.clone() };
    if motion.consumes_authored_motion() {
        if let Presence::Present(holders)=&mut detached.property_holders {
            holders.retain(|holder| {
                let target=holder.target();
                let consumed=target.context_id()==scope.context_id && target.layer_uid()==scope.layer_uid
                    && target.stroke_id().is_some_and(|id|member_ids.iter().any(|member|member==id));
                !consumed || document.objects().iter().any(|record| {
                    record.target.context_id==target.context_id() && record.target.layer_uid==target.layer_uid()
                        && Some(record.target.stroke_id.as_str())==target.stroke_id()
                        && record.target.frame_scope.kind==FrameScopeKind::Authored
                        && !scope.owns(&record.target)
                })
            });
        }
    }
    finish(
        detached,
        scope,
        &member_ids,
        Some(first_pos - below),
        &islands,
        &first_id,
        &style,
        revision,
    )
}
