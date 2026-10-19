//! Linked-fill regeneration in the caller's detached native document candidate.
//! A wall edit and every dependent fill therefore publish together through the
//! existing sealed vector transaction, with one revision and one undo entry.
use crate::object_document::object_group_kernels::object_boolean_ops::plan::PlanError;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{finish, opacity_of};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    find_record, stack_of, ResultStyle, Scope,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    flatten_polygons, fold_multi, BoolOp, Island,
};
use crate::object_document::object_group_kernels::object_boolean_topology::IslandBuild;
use crate::object_document::object_group_kernels::object_fill_trace_ops::native::{
    self, NativeTrace,
};
use crate::object_document::object_group_kernels::object_fill_trace_ops::native_merge::polygons;
use crate::object_document::object_group_kernels::object_fill_trace_ops::{
    find_fill_region, TraceHit,
};
use crate::object_document::{ObjectDocument, Presence};
use serde_json::Number;

fn trace(
    document: &ObjectDocument,
    scope: &Scope,
    id: &str,
    seed: [f64; 2],
    walls: Option<&[String]>,
    gap: Option<u32>,
) -> Result<Option<TraceHit>, PlanError> {
    let mut provider = NativeTrace::new(document, scope, seed, Some(id), walls)
        .map_err(|_| PlanError::FillUnavailable)?;
    find_fill_region(&mut provider, gap).map_err(|_| PlanError::FillUnavailable)
}

pub(crate) fn regenerate(
    document: ObjectDocument,
    scope: &Scope,
    touched: &str,
    revision: u64,
) -> Result<ObjectDocument, PlanError> {
    regenerate_many(document, scope, &[touched.to_string()], revision)
}

pub(crate) fn regenerate_many(
    document: ObjectDocument,
    scope: &Scope,
    touched: &[String],
    revision: u64,
) -> Result<ObjectDocument, PlanError> {
    let ids: Vec<String> = document
        .objects
        .iter()
        .filter(|r| {
            scope.owns(&r.target)
                && r.solid_fill().is_some()
                && r.vector_ext
                    .value()
                    .is_some_and(|e| !e.fill_trace.missing())
        })
        .map(|r| r.target.stroke_id.clone())
        .collect();
    let mut work = document;
    for id in ids {
        let Some(index) = find_record(&work, scope, &id) else {
            continue;
        };
        let original = work.objects[index].clone();
        let Some(original_trace) = original
            .vector_ext
            .value()
            .and_then(|e| e.fill_trace.value())
        else {
            continue;
        };
        let walls = original_trace
            .walls
            .value()
            .filter(|ids| !ids.is_empty())
            .map(Vec::as_slice);
        if walls.is_some_and(|ids| !ids.iter().any(|id| touched.contains(id))) {
            continue;
        }
        let gap = if walls.is_some() {
            None
        } else {
            original_trace
                .gap_px
                .value()
                .and_then(Number::as_f64)
                .map(|n| n.floor() as u32)
        };
        let mut hits = Vec::new();
        for seed in original_trace.points() {
            if let Some(hit) = trace(&work, scope, &id, seed, walls, gap)? {
                hits.push(hit);
            }
        }
        let multi = original_trace.seeds.value().is_some_and(|s| !s.is_empty());
        if hits.is_empty() && !multi {
            let mut fallback = native::interior(&native::ring(&original))
                .into_iter()
                .collect::<Vec<_>>();
            for touched in touched {
                if let Some(i) = find_record(&work, scope, touched) {
                    if let Some(p) = native::interior(&native::ring(&work.objects[i])) {
                        fallback.push(p);
                    }
                }
            }
            for seed in fallback {
                if let Some(hit) = trace(&work, scope, &id, seed, walls, gap)? {
                    hits.push(hit);
                    break;
                }
            }
        }
        // A genuinely open region retains its last good geometry and original
        // association, exactly as fillRegenerateLinked; it never grabs new walls.
        if hits.is_empty() {
            continue;
        }
        let islands = if hits.len() == 1 {
            vec![Island {
                build: IslandBuild::Single,
                segments: hits[0].ring.clone(),
                refit: false,
            }]
        } else {
            let operands: Vec<_> = hits.iter().map(|h| polygons(&h.ring)).collect();
            let union =
                fold_multi(BoolOp::Unite, &operands).map_err(|_| PlanError::FillUnavailable)?;
            flatten_polygons(&union, true).map_err(|_| PlanError::FillUnavailable)?
        };
        let mut source = original.clone();
        let mut tags = original_trace.clone();
        let seeds: Vec<_> = hits
            .iter()
            .filter_map(|h| native::interior(&h.ring))
            .map(|p| {
                [
                    Number::from_f64(p[0]).unwrap(),
                    Number::from_f64(p[1]).unwrap(),
                ]
            })
            .collect();
        if multi {
            tags.seeds = Presence::Present(seeds);
        } else if let Some(seed) = seeds.first() {
            tags.seed = Presence::Present(seed.clone());
        }
        tags.gap_px = Presence::Present(Number::from(hits[0].gap_px));
        if walls.is_none() {
            let mut ids = Vec::new();
            for h in &hits {
                for id in &h.wall_ids {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
            if !ids.is_empty() {
                tags.walls = Presence::Present(ids);
            }
        }
        if let Presence::Present(ext) = &mut source.vector_ext {
            ext.fill_trace = Presence::Present(tags);
        }
        let style = ResultStyle {
            source: &source,
            fill: original.solid_fill().unwrap().clone(),
            opacity: opacity_of(&original),
            outline: None,
            box_angle: None,
            keep_source_stroke_style: false,
        };
        let at = stack_of(&work, scope)
            .iter()
            .position(|s| s == &id)
            .ok_or(PlanError::FillUnavailable)?;
        work = finish(
            work,
            scope,
            &[id.clone()],
            Some(at),
            &islands,
            &id,
            &style,
            revision,
        )
        .map_err(|_| PlanError::FillUnavailable)?
        .document;
    }
    Ok(work)
}
