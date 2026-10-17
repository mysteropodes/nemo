//! Running and comparing a scenario (L04-L tests): the real join over a
//! scenario's own document and ports, and the comparison with the baseline.
//!
//! Reading and comparing only; the scenario grammar and the in-memory provider
//! are in the sibling `test_codec`.

use super::test_codec::{document_of, LogRow, Out, Scenario, World};
use super::{Entry, EvaluateError, Evaluation, Ports, Request, UnchangedReason};
use crate::evaluation::object_component_kernels::duplicator_placement_apply::{
    DuplicatorCopy, UnavailableField,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::truthy;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    close, fill_matches, same, segments_of, Want,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::SpanBounds;
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorSet;
use crate::object_document::ObjectLayer;

/// Relative tolerance of values that went through `sin`/`cos` (V8's fdlibm vs the platform libm).
pub(crate) const TRIG: f64 = 1e-9;

/// What one evaluation produced and asked for.
pub(crate) struct Run {
    pub result: Result<Evaluation, EvaluateError>,
    pub calls: Vec<String>,
}

/// Evaluate a scenario through the real join over its own document and ports.
pub(crate) fn run(sc: &Scenario) -> Run {
    let (set, layers) = document_of(sc);
    run_with(sc, &set, &layers, &World::new(sc))
}

/// [`run`] over a given document and world, so a control can change one input.
pub(crate) fn run_with(
    sc: &Scenario,
    set: &DuplicatorSet,
    layers: &[ObjectLayer],
    world: &World<'_>,
) -> Run {
    let entry = if sc.entry == "rendered" {
        Entry::Rendered {
            edit_source: sc.edit,
        }
    } else {
        Entry::Direct {
            can_resample: sc.can_resample,
        }
    };
    let req = Request {
        context_id: "scene-root",
        host_uid: "host",
        set,
        layers,
        entry,
        is_3d_layer: sc.three_d,
        base: &sc.base,
        frame_idx: sc.frame,
        canvas_w: sc.canvas[0],
        canvas_h: sc.canvas[1],
        span: sc.span.unwrap_or(SpanBounds {
            start: 0.0,
            len: 1.0,
        }),
    };
    let ports = Ports {
        motion: world,
        sources: world,
    };
    let result = super::evaluate_duplicator(&req, &ports);
    Run {
        result,
        calls: world.distinct_calls(),
    }
}

fn gaps(place: &[f64; 7]) -> Vec<UnavailableField> {
    let mut gaps = Vec::new();
    if place[5] != 1.0 {
        gaps.push(UnavailableField::Opacity);
    }
    if truthy(place[6]) {
        gaps.push(UnavailableField::StrokePaint);
    }
    gaps
}

fn copy_mismatch(
    sc: &Scenario,
    want: &Want,
    got: &DuplicatorCopy,
    row: Option<&LogRow>,
) -> Option<String> {
    let fail = |what: &str| Some(format!("{} copy {}: {what}", sc.name, want.clone_id));
    if got.record.target.stroke_id() != want.id {
        return fail("the source stroke id is not inherited");
    }
    if got.clone_id != want.clone_id || got.dup_index != want.dup_index {
        return fail("per-copy identity");
    }
    if got.record.geometry.closed != want.closed || !fill_matches(&got.record, &want.fill) {
        return fail("closure or fill");
    }
    let same3 = match (got.dup_3d, want.dup_3d) {
        (None, None) => true,
        (Some(g), Some(w)) => {
            close(g.dz, w[0], TRIG) && close(g.drx, w[1], TRIG) && close(g.dry, w[2], TRIG)
        }
        _ => false,
    };
    if !same3 {
        return fail("dup3D");
    }
    if let Some(row) = row {
        if got.unavailable != gaps(&row.place) {
            return fail("unavailable fields");
        }
        match want.opacity {
            Some(o) if same(o, row.place[5]) => {}
            _ => return fail("baseline opacity is not 1 * opacityFactor"),
        }
    }
    let rows = segments_of(&got.record);
    if rows.len() != want.segs.len() {
        return fail("segment count");
    }
    for (g, w) in rows.iter().zip(&want.segs) {
        if !g.iter().zip(w).all(|(a, b)| close(*a, *b, TRIG)) {
            return fail("geometry");
        }
    }
    None
}

fn placement_mismatch(sc: &Scenario, set: &super::CopySet) -> Option<String> {
    let fail = |what: &str| Some(format!("{}: {what}", sc.name));
    if set.placements.len() != sc.log.len() {
        return fail("placement calls");
    }
    for (got, row) in set.placements.iter().zip(&sc.log) {
        let p = &got.placement;
        let ours = [
            p.matrix.dx,
            p.matrix.dy,
            p.matrix.rot,
            p.matrix.sx,
            p.matrix.sy,
            p.opacity_factor,
            p.hue_deg,
        ];
        let pivot = [got.pivot.x, got.pivot.y];
        if got.k != row.k
            || !ours
                .iter()
                .zip(&row.place)
                .all(|(a, b)| close(*a, *b, TRIG))
            || !pivot
                .iter()
                .zip(&row.pivot)
                .all(|(a, b)| close(*a, *b, TRIG))
        {
            return fail(&format!("placement of clone {}", row.k));
        }
        let same3 = match (p.dup_3d, row.dup_3d) {
            (None, None) => true,
            (Some(g), Some(w)) => {
                close(g.dz, w[0], TRIG) && close(g.drx, w[1], TRIG) && close(g.dry, w[2], TRIG)
            }
            _ => false,
        };
        if !same3 {
            return fail(&format!("dup3D of clone {}", row.k));
        }
    }
    None
}

/// The first difference between what the join produced and the baseline.
pub(crate) fn mismatch(sc: &Scenario, run: &Run) -> Option<String> {
    let fail = |what: &str| Some(format!("{}: {what}", sc.name));
    let expected: Vec<String> = sc.calls.clone();
    if run.calls != expected {
        return fail(&format!("port calls {:?} != {:?}", run.calls, expected));
    }
    let want = sc.out.as_ref().expect("out row");
    match (&run.result, want) {
        (Err(EvaluateError::PathInfoRequired), Out::Throws) => None,
        (Ok(Evaluation::Unchanged(_)), Out::Same) => None,
        (Ok(Evaluation::Copies(set)), Out::Big { n, first, last }) => {
            if set.copies.len() != *n {
                return fail("copy count");
            }
            copy_mismatch(sc, first, &set.copies[0], None)
                .or_else(|| copy_mismatch(sc, last, &set.copies[*n - 1], None))
        }
        (Ok(Evaluation::Copies(set)), Out::List(wants)) => {
            if set.copies.len() != wants.len() {
                return fail("copy count");
            }
            placement_mismatch(sc, set).or_else(|| {
                set.copies.iter().zip(wants).find_map(|(got, w)| {
                    copy_mismatch(sc, w, got, sc.log.get(w.dup_index as usize))
                })
            })
        }
        (other, _) => fail(&format!(
            "unexpected result {:?}",
            other.as_ref().map(|_| ())
        )),
    }
}

/// The reason of an unchanged result.
pub(crate) fn unchanged_reason(run: &Run) -> Option<UnchangedReason> {
    match &run.result {
        Ok(Evaluation::Unchanged(reason)) => Some(*reason),
        _ => None,
    }
}

/// The copies of a successful run.
pub(crate) fn copies_of(run: &Run) -> &super::CopySet {
    match &run.result {
        Ok(Evaluation::Copies(set)) => set,
        other => panic!("expected copies, got {:?}", other.as_ref().map(|_| ())),
    }
}
