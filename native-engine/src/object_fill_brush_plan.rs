//! Native Fill Brush commit (#2361): draw-bridge.js's Fill Brush branch @1065e9d8 once the stroke outline exists.
//! The shape is a closed, unstroked fill tagged `isFillShape`, placed by `applyFillBrushPlacement` and then merged by
//! `fillMergeSameColor(layer, shape, true)`, one candidate in the existing boolean/history owner:
//! - `above` keeps the new shape on top, `below` puts it at the back;
//! - `merge` unites it with the topmost filled child whose bounds intersect (`Rectangle.intersects`, strict), in the
//!   new paint and opacity and with the target's kept data and identity (`insertBooleanResult(..., target.data)`),
//!   or falls back to `above` when nothing overlaps;
//! - the same-colour merge may absorb other Fill Brush shapes (`allowFillShapeAbsorb`), carries the absorbed fills'
//!   seeds, walls and gap, and drops the shape when a near-identical copy already exists.
//! The outline stays as drawn: the baseline only swaps in its self-union when that union is one path, which has the
//! same nonzero coverage.
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    check_scope, detach, finish, BooleanPlan, PlanError,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    apply_stack, find_record, fresh_ids, hex8, stack_of, ResultStyle, Scope,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    flatten_polygons, fold_multi, BoolOp, RingSeg,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    merge_candidates, plan_union, resolve_union, select_absorbed, Absorbed, AreaFacts, Bounds,
    FillFacts, MergeDecision,
};
use crate::object_document::object_group_kernels::object_fill_trace_ops::native;
use crate::object_document::object_group_kernels::object_fill_trace_ops::native_merge::{
    polygons, NativeProbe, NativeUnion,
};
use crate::object_document::object_group_kernels::object_fill_trace_ops::UnionProvider;
use crate::object_document::{
    FillTrace, ObjectDocument, ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment, Presence, SolidFill,
    VectorBody, VectorExt, VECTOR_FAMILY,
};
use serde_json::Number;

/// `state.fillBrushMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Above,
    Below,
    Merge,
}

fn number(n: f64) -> Number {
    Number::from_f64(n).expect("admitted finite fill value")
}
fn point(p: [f64; 2]) -> ObjectPoint {
    ObjectPoint {
        x: number(p[0]),
        y: number(p[1]),
    }
}
/// `Rectangle.intersects(rect)` with Paper's default epsilon 0: strict on every edge.
fn intersects(a: &Bounds, b: &Bounds) -> bool {
    b.x + b.width > a.x && b.y + b.height > a.y && b.x < a.x + a.width && b.y < a.y + a.height
}
/// The new Fill Brush shape: a closed fill of the outline, no stroke, the drawn opacity, `isFillShape`.
fn shape_record(scope: &Scope, id: &str, outline: &[RingSeg], paint: &SolidFill, opacity: f64) -> ObjectRecord {
    ObjectRecord {
        schema_version: 1,
        family: VECTOR_FAMILY.to_string(),
        target: scope.target(id),
        geometry: ObjectGeometry {
            kind: "cubic-path".to_string(),
            closed: true,
            coordinate_space: "document-world".to_string(),
            handle_space: "relative".to_string(),
            segments: outline
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
        vector: Presence::Present(VectorBody {
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
        }),
        vector_ext: Presence::Present(VectorExt {
            is_fill_shape: Presence::Present(true),
            ..VectorExt::default()
        }),
    }
}
fn records<'a>(document: &'a ObjectDocument, scope: &Scope, ids: &[String]) -> Result<Vec<&'a ObjectRecord>, PlanError> {
    ids.iter()
        .map(|id| find_record(document, scope, id).map(|i| &document.objects[i]).ok_or(PlanError::FillUnavailable))
        .collect()
}
fn has_paint(r: &ObjectRecord) -> bool {
    r.solid_fill().is_some() || r.fill_gradient().is_some()
}
fn opacity_of(r: &ObjectRecord) -> Number {
    r.vector().map_or_else(|| Number::from(1), |v| v.opacity.clone())
}
/// The record as `carryBooleanData` sees its data: no fill-trace tags (fillSeed/fillWalls are not kept data).
fn without_trace(r: &ObjectRecord) -> ObjectRecord {
    let mut r = r.clone();
    if let Presence::Present(ext) = &mut r.vector_ext {
        ext.fill_trace = Presence::Missing;
    }
    r
}

/// The whole Fill Brush commit over one admitted scope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_brush(
    document: &ObjectDocument,
    scope: &Scope,
    outline: &[RingSeg],
    paint: &SolidFill,
    opacity: f64,
    placement: Placement,
    mask: Option<&str>,
    copies: &[[f64; 6]],
    revision: u64,
) -> Result<(BooleanPlan, String), PlanError> {
    check_scope(document, scope)?;
    if outline.len() < 3 {
        return Err(PlanError::Degenerate("fill brush outline".to_string()));
    }
    let ids = stack_of(document, scope);
    let shown = records(document, scope, &ids)?;
    let new_bounds = native::bounds(outline);
    let target = match placement {
        Placement::Merge => shown
            .iter()
            .rposition(|r| has_paint(r) && intersects(&native::bounds(&native::ring(r)), &new_bounds)),
        _ => None,
    };
    let (work, current) = match target {
        Some(at) => {
            // applyFillBrushPlacement: target.unite(path), both removed, the islands at the target's place in the new
            // paint and opacity, the first keeping the target's identity and data.
            let united = fold_multi(BoolOp::Unite, &[polygons(&native::ring(shown[at])), polygons(outline)])
                .map_err(|_| PlanError::FillUnavailable)?;
            let islands = flatten_polygons(&united, false).map_err(|_| PlanError::FillUnavailable)?;
            if islands.is_empty() {
                return Err(PlanError::FillUnavailable);
            }
            let source = without_trace(shown[at]);
            let style = ResultStyle {
                source: &source,
                fill: paint.clone(),
                opacity: number(opacity),
                outline: None,
                box_angle: source.vector_ext.value().and_then(|e| e.box_angle.value()).cloned(),
                keep_source_stroke_style: false,
            };
            let removed = vec![ids[at].clone()];
            let detached = detach(document.clone(), scope, &removed)?;
            let placed = finish(detached, scope, &removed, Some(at), &islands, &ids[at], &style, revision)?;
            (placed.document, ids[at].clone())
        }
        None => {
            let id = fresh_ids(document, revision, 1).ok_or(PlanError::IdUnavailable)?.remove(0);
            let mut stack = ids.clone();
            match placement {
                Placement::Below => stack.insert(0, id.clone()),
                _ => stack.push(id.clone()),
            }
            let mut work = document.clone();
            apply_stack(&mut work, scope, &stack, &[], vec![shape_record(scope, &id, outline, paint, opacity)])
                .map_err(PlanError::Stack)?;
            (work, id)
        }
    };
    // The receipt names what the whole commit created and removed in the scope (a merge-placed target keeps its id).
    let (mut plan, shown) = same_colour_merge(work, scope, &current, revision)?;
    // tagOwner in Mask mode (tools.js @1065e9d8): the path the merge hands back becomes a mask of the layer.
    if let Some(mode) = mask {
        let at = find_record(&plan.document, scope, &shown).ok_or(PlanError::FillUnavailable)?;
        let record = &mut plan.document.objects[at];
        let mut ext = record.vector_ext.value().cloned().unwrap_or_default();
        ext.is_mask = Presence::Present(Some(true));
        ext.mask_mode = Presence::Present(Some(mode.to_string()));
        record.vector_ext = Presence::Present(ext);
    }
    append_copies(&mut plan.document, scope, &shown, copies, revision)?;
    let after = stack_of(&plan.document, scope);
    plan.created = after.iter().filter(|id| !ids.contains(id)).cloned().collect();
    plan.removed = ids.iter().filter(|id| !after.contains(id)).cloned().collect();
    plan.changed = plan.document != *document;
    // `shown`: the path the commit hands back, mask tag included and symmetry copies aside (what Labs stamps).
    Ok((plan, shown))
}

/// symmetry-bridge.js onStrokeCommitted over the path the commit hands back: each copy directly above it, in the given
/// order (`[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f; handles through the linear part). A reflection
/// re-orients a closed copy (`mirrored.reverse()`); adoptFreshIdentity gives it a fresh id and keeps the rest of its data.
fn append_copies(
    document: &mut ObjectDocument,
    scope: &Scope,
    shown: &str,
    copies: &[[f64; 6]],
    revision: u64,
) -> Result<(), PlanError> {
    if copies.is_empty() {
        return Ok(());
    }
    let source = document.objects[find_record(document, scope, shown).ok_or(PlanError::FillUnavailable)?].clone();
    let ids = fresh_ids(document, revision, copies.len()).ok_or(PlanError::IdUnavailable)?;
    let value = |n: &Number| n.as_f64().ok_or(PlanError::Internal("non-finite source coordinate"));
    let mut created = Vec::with_capacity(copies.len());
    for (m, id) in copies.iter().zip(&ids) {
        let at = |x: f64, y: f64, e: f64, f: f64| -> Result<ObjectPoint, PlanError> {
            let (px, py) = (m[0] * x + m[2] * y + e, m[1] * x + m[3] * y + f);
            Ok(ObjectPoint {
                x: Number::from_f64(px).ok_or(PlanError::Internal("non-finite copy coordinate"))?,
                y: Number::from_f64(py).ok_or(PlanError::Internal("non-finite copy coordinate"))?,
            })
        };
        let mut segments = Vec::with_capacity(source.geometry.segments.len());
        for s in &source.geometry.segments {
            segments.push(ObjectSegment {
                point: at(value(&s.point.x)?, value(&s.point.y)?, m[4], m[5])?,
                handle_in: at(value(&s.handle_in.x)?, value(&s.handle_in.y)?, 0.0, 0.0)?,
                handle_out: at(value(&s.handle_out.x)?, value(&s.handle_out.y)?, 0.0, 0.0)?,
            });
        }
        if m[0] * m[3] - m[1] * m[2] < 0.0 && source.geometry.closed {
            segments.reverse();
            for s in &mut segments {
                std::mem::swap(&mut s.handle_in, &mut s.handle_out);
            }
        }
        let mut copy = source.clone();
        copy.target = scope.target(id);
        copy.geometry.segments = segments;
        created.push(copy);
    }
    let mut stack = stack_of(document, scope);
    let above = stack.iter().position(|id| id == shown).ok_or(PlanError::FillUnavailable)? + 1;
    for (k, id) in ids.iter().enumerate() {
        stack.insert(above + k, id.clone());
    }
    apply_stack(document, scope, &stack, &[], created).map_err(PlanError::Stack)
}

/// `fillMergeSameColor(layer, shape, true)` over the placed document, and the id of the path it hands back.
fn same_colour_merge(work: ObjectDocument, scope: &Scope, current: &str, revision: u64) -> Result<(BooleanPlan, String), PlanError> {
    let ids = stack_of(&work, scope);
    let at = ids.iter().position(|id| id == current).ok_or(PlanError::FillUnavailable)?;
    let shown = records(&work, scope, &ids)?;
    let rings: Vec<Vec<RingSeg>> = shown.iter().map(|r| native::ring(r)).collect();
    let ext = |r: &ObjectRecord| r.vector_ext.value().cloned().unwrap_or_default();
    let facts: Vec<FillFacts> = shown
        .iter()
        .map(|r| {
            let e = ext(r);
            FillFacts {
                is_path: true,
                closed: r.geometry.closed,
                fill_hex8: r.solid_fill().map(hex8),
                stroked: r.vector().is_some_and(|v| v.solid_stroke().is_some()),
                is_vector_brush: r.is_pressure_ribbon(),
                is_linked_fill_companion: e.is_linked_fill_companion.value() == Some(&true),
                is_brush_texture_copy: e
                    .brush_texture
                    .value()
                    .is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true)),
                is_fill_shape: e.is_fill_shape.value() == Some(&true),
                stroke_id: Some(r.target.stroke_id.clone()),
                fill_walls: e.fill_trace.value().and_then(|t| t.walls.value()).cloned(),
                ..FillFacts::default()
            }
        })
        .collect();
    let unchanged_shape = || BooleanPlan { document: work.clone(), changed: true, created: vec![current.to_string()], removed: Vec::new() };
    let candidates = merge_candidates(at, &facts, true);
    let mut probe = NativeProbe {
        rings: rings.clone(),
        stroked: facts.iter().map(|f| f.stroked).collect(),
        closed: facts.iter().map(|f| f.closed).collect(),
        new_index: at,
    };
    let absorbed = select_absorbed(&candidates, &mut probe).map_err(|_| PlanError::FillUnavailable)?;
    if absorbed.is_empty() {
        return Ok((unchanged_shape(), current.to_string()));
    }
    let area = |k: usize| AreaFacts { area: native::area(&rings[k]), bounds: native::bounds(&rings[k]) };
    let children: Vec<Absorbed> = absorbed.iter().map(|&index| Absorbed { index, area: area(index) }).collect();
    let union = plan_union(&area(at), &children);
    let mut provider = NativeUnion { rings: rings.clone(), result: polygons(&rings[at]) };
    let mut outcomes = Vec::with_capacity(union.steps.len());
    for &step in &union.steps {
        outcomes.push(provider.unite(step).map_err(|_| PlanError::FillUnavailable)?);
    }
    match resolve_union(&union, &outcomes).map_err(|_| PlanError::FillUnavailable)? {
        MergeDecision::NoChange => Ok((unchanged_shape(), current.to_string())),
        // A near-identical fill already covers it: the new shape goes, that fill stays.
        MergeDecision::KeepExisting(kept) => {
            let removed = vec![current.to_string()];
            let mut doc = detach(work, scope, &removed)?;
            let stack: Vec<String> = ids.iter().filter(|id| *id != current).cloned().collect();
            apply_stack(&mut doc, scope, &stack, &removed, Vec::new()).map_err(PlanError::Stack)?;
            Ok((BooleanPlan { document: doc, changed: true, created: Vec::new(), removed }, ids[kept].clone()))
        }
        MergeDecision::Unite(_) => {
            // The participants' seeds, walls and widest gap go onto every part (mergedSeeds, mergedWallIds, gap or 24).
            let participants = std::iter::once(at).chain(absorbed.iter().copied());
            let (mut seeds, mut walls, mut gap) = (Vec::new(), Vec::<String>::new(), 0.0f64);
            for k in participants {
                if let Some(t) = shown[k].vector_ext.value().and_then(|e| e.fill_trace.value()) {
                    seeds.extend(t.points());
                    for id in t.walls.value().into_iter().flatten() {
                        if !walls.contains(id) {
                            walls.push(id.clone());
                        }
                    }
                    gap = gap.max(t.gap_px.value().and_then(Number::as_f64).unwrap_or(0.0));
                }
            }
            let mut source = without_trace(shown[at]);
            if !seeds.is_empty() {
                let tags = FillTrace {
                    seed: Presence::Missing,
                    seeds: Presence::Present(seeds.into_iter().map(|p| [number(p[0]), number(p[1])]).collect()),
                    walls: if walls.is_empty() { Presence::Missing } else { Presence::Present(walls) },
                    gap_px: Presence::Present(number(if gap == 0.0 { 24.0 } else { gap })),
                };
                if let Presence::Present(ext) = &mut source.vector_ext {
                    ext.fill_trace = Presence::Present(tags);
                }
            }
            let islands = provider.islands().map_err(|_| PlanError::FillUnavailable)?;
            let removed: Vec<String> =
                std::iter::once(current.to_string()).chain(absorbed.iter().map(|&i| ids[i].clone())).collect();
            let top = absorbed.iter().copied().chain(std::iter::once(at)).max().unwrap_or(at);
            let below = absorbed.iter().copied().chain(std::iter::once(at)).filter(|&i| i < top).count();
            let style = ResultStyle {
                source: &source,
                fill: shown[at].solid_fill().cloned().ok_or(PlanError::FillUnavailable)?,
                opacity: opacity_of(shown[at]),
                outline: None,
                box_angle: source.vector_ext.value().and_then(|e| e.box_angle.value()).cloned(),
                keep_source_stroke_style: false,
            };
            let doc = detach(work.clone(), scope, &removed)?;
            Ok((finish(doc, scope, &removed, Some(top - below), &islands, current, &style, revision)?, current.to_string()))
        }
    }
}
