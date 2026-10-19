//! Detached paint-bucket document transactions. Real tracing, recolour and same-colour
//! merge enter the existing immutable native revision/history owner as one candidate.
use super::native::{self, NativeTrace};
use super::native_merge::{polygons, NativeProbe, NativeUnion};
use super::{find_fill_region, plan_fill_trace_commit, ChildFacts, CommitInput, FillTracePlan};
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    check_scope, detach, finish, unchanged, BooleanPlan, PlanError,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    apply_stack, find_record, fresh_ids, hex8, stack_of, ResultStyle, Scope,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    FillFacts, MergeDecision,
};
use crate::object_document::{
    FillTrace, ObjectDocument, ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment, Presence,
    SolidFill, VectorBody, VectorExt, VECTOR_FAMILY,
};
use serde_json::Number;

fn number(n: f64) -> Number {
    Number::from_f64(n).expect("admitted finite fill value")
}
fn point(p: [f64; 2]) -> ObjectPoint {
    ObjectPoint {
        x: number(p[0]),
        y: number(p[1]),
    }
}
fn body(opacity: f64) -> VectorBody {
    VectorBody {
        stroke: Presence::Present(None),
        has_real_stroke: false,
        stroke_width: Number::from(1),
        stroke_cap: "butt".to_string(),
        stroke_join: "miter".to_string(),
        miter_limit: Presence::Missing,
        dash_array: Presence::Missing,
        dash_offset: Presence::Missing,
        paint_order: Presence::Missing,
        opacity: number(opacity),
    }
}
pub(crate) fn fill_record(
    scope: &Scope,
    id: &str,
    hit: &super::TraceHit,
    click: [f64; 2],
    paint: &SolidFill,
    opacity: f64,
) -> ObjectRecord {
    ObjectRecord {
        schema_version: 1,
        family: VECTOR_FAMILY.to_string(),
        target: scope.target(id),
        geometry: ObjectGeometry {
            kind: "cubic-path".to_string(),
            closed: true,
            coordinate_space: "document-world".to_string(),
            handle_space: "relative".to_string(),
            segments: hit
                .ring
                .iter()
                .map(|s| ObjectSegment {
                    point: point(s.point),
                    handle_in: point(s.handle_in),
                    handle_out: point(s.handle_out),
                })
                .collect(),
        },
        fill: Presence::Present(Some(paint.clone())),
        effects: Presence::Missing,
        vector: Presence::Present(body(opacity)),
        vector_ext: Presence::Present(VectorExt {
            pressure: Presence::Missing,
            param_shape: Presence::Missing,
            box_angle: Presence::Missing,
            fill_gradient: Presence::Missing,
            fill_trace: Presence::Present(
                FillTrace::stamp(click, hit.wall_ids.clone(), hit.gap_px).unwrap(),
            ),
        }),
    }
}

fn recolor(
    document: &ObjectDocument,
    index: usize,
    paint: &SolidFill,
    opacity: f64,
    remove: bool,
) -> BooleanPlan {
    let original = &document.objects[index];
    let same = original.solid_fill().is_some_and(|p| {
        [&p.r, &p.g, &p.b, &p.a]
            .iter()
            .zip([&paint.r, &paint.g, &paint.b, &paint.a])
            .all(|(a, b)| a.as_f64() == b.as_f64())
    }) && original
        .vector()
        .map_or(opacity == 1.0, |v| v.opacity.as_f64() == Some(opacity))
        && original.fill_gradient().is_none();
    if !remove && same {
        return unchanged(document);
    }
    let mut work = document.clone();
    let r = &mut work.objects[index];
    // Promote the legacy fill-only DTO when it needs the nullable fill/opacity
    // fields. Identity, geometry, group membership and effects stay unchanged.
    if r.family != VECTOR_FAMILY {
        r.family = VECTOR_FAMILY.to_string();
        r.vector = Presence::Present(body(opacity));
    }
    r.fill = Presence::Present(if remove { None } else { Some(paint.clone()) });
    if let Presence::Present(v) = &mut r.vector {
        v.opacity = number(opacity);
    }
    if let Presence::Present(ext) = &mut r.vector_ext {
        ext.fill_gradient = Presence::Missing;
        if ext.pressure.missing()
            && ext.param_shape.missing()
            && ext.box_angle.missing()
            && ext.fill_trace.missing()
        {
            r.vector_ext = Presence::Missing;
        }
    }
    BooleanPlan {
        document: work,
        changed: true,
        created: Vec::new(),
        removed: Vec::new(),
    }
}

pub(crate) fn plan_trace(
    document: &ObjectDocument,
    scope: &Scope,
    click: [f64; 2],
    paint: &SolidFill,
    opacity: f64,
    remove: bool,
    revision: u64,
) -> Result<BooleanPlan, PlanError> {
    check_scope(document, scope)?;
    let ids = stack_of(document, scope);
    let records: Vec<&ObjectRecord> = ids
        .iter()
        .map(|id| {
            find_record(document, scope, id)
                .map(|i| &document.objects[i])
                .ok_or(PlanError::FillUnavailable)
        })
        .collect::<Result<_, _>>()?;
    let fallback = records
        .iter()
        .rev()
        .find(|r| r.solid_fill().is_some() && native::contains(&native::ring(r), click))
        .and_then(|r| find_record(document, scope, &r.target.stroke_id));
    if remove {
        return Ok(fallback.map_or_else(
            || unchanged(document),
            |i| recolor(document, i, paint, opacity, true),
        ));
    }
    let mut tracer = NativeTrace::new(document, scope, click, None, None)
        .map_err(|_| PlanError::FillUnavailable)?;
    let hit = find_fill_region(&mut tracer, None).map_err(|_| PlanError::FillUnavailable)?;
    let Some(hit) = hit else {
        return Ok(fallback.map_or_else(
            || unchanged(document),
            |i| recolor(document, i, paint, opacity, false),
        ));
    };
    let rings: Vec<_> = records.iter().map(|r| native::ring(r)).collect();
    let children: Vec<_> = records
        .iter()
        .zip(&rings)
        .map(|(r, ring)| ChildFacts {
            filled_shape: r.solid_fill().is_some(),
            excluded: false,
            bounds: native::bounds(ring),
            area: native::area(ring),
            covers_click: native::contains(ring, click),
        })
        .collect();
    let facts: Vec<_> = records
        .iter()
        .map(|r| FillFacts {
            is_path: true,
            closed: r.geometry.closed,
            fill_hex8: r.solid_fill().map(hex8),
            stroked: r.vector().is_some_and(|v| v.solid_stroke().is_some()),
            is_vector_brush: r.is_pressure_ribbon(),
            stroke_id: Some(r.target.stroke_id.clone()),
            fill_walls: r
                .vector_ext
                .value()
                .and_then(|e| e.fill_trace.value())
                .and_then(|t| t.walls.value())
                .cloned(),
            ..FillFacts::default()
        })
        .collect();
    let at = super::fill_insert_index(&children);
    let mut all_rings = rings.clone();
    all_rings.insert(at, hit.ring.clone());
    let mut stroked: Vec<_> = facts.iter().map(|f| f.stroked).collect();
    stroked.insert(at, false);
    let mut closed: Vec<_> = records.iter().map(|r| r.geometry.closed).collect();
    closed.insert(at, true);
    let mut probe = NativeProbe {
        rings: all_rings.clone(),
        stroked,
        closed,
        new_index: at,
    };
    let mut union = NativeUnion {
        rings: all_rings,
        result: polygons(&hit.ring),
    };
    let plan = plan_fill_trace_commit(
        &CommitInput {
            click,
            hit: Some(&hit),
            fill_hex8: &hex8(paint),
            bounds: native::bounds(&hit.ring),
            children: &children,
            facts: &facts,
            allow_fill_shape_absorb: false,
        },
        &mut probe,
        &mut union,
    )
    .map_err(|_| PlanError::FillUnavailable)?;
    let (at, merge) = match plan {
        FillTracePlan::NoRegion => return Ok(unchanged(document)),
        FillTracePlan::Recolor { child } => {
            return Ok(recolor(
                document,
                find_record(document, scope, &ids[child]).unwrap(),
                paint,
                opacity,
                false,
            ))
        }
        FillTracePlan::Place {
            insert_at, merge, ..
        } => (insert_at, merge),
    };
    if let Some(m) = &merge {
        if matches!(m.decision, MergeDecision::KeepExisting(_)) {
            return Ok(unchanged(document));
        }
    }
    let id = fresh_ids(document, revision, 1)
        .ok_or(PlanError::FillUnavailable)?
        .remove(0);
    let mut new = fill_record(scope, &id, &hit, click, paint, opacity);
    let mut stack = ids.clone();
    stack.insert(at, id.clone());
    let mut work = document.clone();
    apply_stack(&mut work, scope, &stack, &[], vec![new.clone()])
        .map_err(|_| PlanError::FillUnavailable)?;
    if let Some(m) = merge {
        if let MergeDecision::Unite(_) = m.decision {
            let removed: Vec<String> = std::iter::once(id.clone())
                .chain(m.absorbed.iter().map(|&i| stack[i].clone()))
                .collect();
            let mut tags = new
                .vector_ext
                .value()
                .unwrap()
                .fill_trace
                .value()
                .unwrap()
                .clone();
            let mut seeds = tags.points();
            let mut walls = tags.walls.value().cloned().unwrap_or_default();
            let mut gap = hit.gap_px as f64;
            for &i in &m.absorbed {
                if let Some(t) = work.objects[find_record(&work, scope, &stack[i]).unwrap()]
                    .vector_ext
                    .value()
                    .and_then(|e| e.fill_trace.value())
                {
                    seeds.extend(t.points());
                    if let Some(ids) = t.walls.value() {
                        for id in ids {
                            if !walls.contains(id) {
                                walls.push(id.clone());
                            }
                        }
                    }
                    gap = gap.max(t.gap_px.value().and_then(Number::as_f64).unwrap_or(0.0));
                }
            }
            tags.seeds = Presence::Present(
                seeds
                    .into_iter()
                    .map(|p| [number(p[0]), number(p[1])])
                    .collect(),
            );
            tags.walls = if walls.is_empty() {
                Presence::Missing
            } else {
                Presence::Present(walls)
            };
            tags.gap_px = Presence::Present(number(if gap == 0.0 { 24.0 } else { gap }));
            if let Presence::Present(ext) = &mut new.vector_ext {
                ext.fill_trace = Presence::Present(tags);
            }
            let islands = union.islands().map_err(|_| PlanError::FillUnavailable)?;
            let top = m
                .absorbed
                .iter()
                .copied()
                .chain(std::iter::once(at))
                .max()
                .unwrap();
            let below = m
                .absorbed
                .iter()
                .copied()
                .chain(std::iter::once(at))
                .filter(|&i| i < top)
                .count();
            work = detach(work, scope, &removed).map_err(|_| PlanError::FillUnavailable)?;
            let style = ResultStyle {
                source: &new,
                fill: paint.clone(),
                opacity: number(opacity),
                outline: None,
                box_angle: None,
                keep_source_stroke_style: false,
            };
            return finish(
                work,
                scope,
                &removed,
                Some(top - below),
                &islands,
                &id,
                &style,
                revision,
            )
            .map_err(|_| PlanError::FillUnavailable);
        }
    }
    Ok(BooleanPlan {
        document: work,
        changed: true,
        created: vec![id],
        removed: Vec::new(),
    })
}
