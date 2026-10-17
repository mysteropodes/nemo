//! Mutation and coverage controls of the evaluation join. Each mistake is injected by re-composing the join's REAL stages
//! (`load_stored`, `run_path_fx`, `run_param_shape`, the L01-B kernel) with one wrong input, one wrong order or one skipped step,
//! and the oracle comparison must fail for it, and only on the rows where the mistake can act. Production code carries no hook.
use super::super::{
    evaluate_vector, from_param, from_path, load_stored, params, run_param_shape, run_path_fx,
    EvalSegment, EvaluatedVector,
};
use super::{check, rows, snapshot, Class, Row};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::boxed::apply_param_shape_animated;
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamSegment;
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::{
    PathFxStroke, PathSegment,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// The rebuild runs first, on the stored anchors, and the effects deform its result (the order one would design).
    ParamBeforeEffects,
    /// The rebuild frames itself on the STORED anchors instead of the post-effect anchors.
    StoredBounds,
    /// The ordered stack is ignored.
    NoEffects,
    /// A parametric shape is rebuilt even when none of its parameters is animated.
    AlwaysRebuild,
    /// The tracks are sampled at frame 0 instead of the pinned frame.
    FrameZero,
}

fn stroke_of(closed: bool, anchors: Vec<PathSegment>) -> PathFxStroke<()> {
    PathFxStroke {
        is_raster: false,
        closed,
        segments: Some(anchors),
        rest: (),
    }
}

fn path_of(s: &ParamSegment) -> PathSegment {
    PathSegment {
        point: s.point,
        handle_in: Some(s.handle_in),
        handle_out: Some(s.handle_out),
    }
}

fn param_of(s: &PathSegment) -> ParamSegment {
    ParamSegment {
        point: s.point,
        handle_in: s.handle_in.unwrap_or([0.0, 0.0]),
        handle_out: s.handle_out.unwrap_or([0.0, 0.0]),
    }
}

fn anchors_of(stroke: &PathFxStroke<()>) -> &[PathSegment] {
    stroke.segments.as_deref().expect("anchors")
}

pub(crate) fn mutated(snap: &ObjectSnapshot, frame: u32, m: Mutation) -> EvaluatedVector {
    let stored = load_stored(snap, 0).expect("stored vector");
    let stack = if m == Mutation::NoEffects {
        None
    } else {
        stored.stack.as_deref()
    };
    let sampled = if m == Mutation::FrameZero { 0 } else { frame };
    let rebuild = |anchors: &[PathSegment]| {
        run_param_shape(snap, stored.record, anchors, sampled).expect("rebuild stage")
    };
    let closed = stored.stroke.closed;
    let segments: Vec<EvalSegment> = match m {
        Mutation::ParamBeforeEffects => {
            let input = match rebuild(anchors_of(&stored.stroke)) {
                Some(built) => stroke_of(closed, built.iter().map(path_of).collect()),
                None => stored.stroke.clone(),
            };
            let out = run_path_fx(stack, &input, frame).expect("effects");
            anchors_of(&out).iter().map(from_path).collect()
        }
        Mutation::AlwaysRebuild => {
            let after = run_path_fx(stack, &stored.stroke, frame).expect("effects");
            let anchors = anchors_of(&after);
            match params::shape_of(stored.record).expect("shape") {
                Some(shape) => {
                    let animated = params::animated_params(
                        snap,
                        stored.record,
                        shape.kind(),
                        f64::from(frame),
                    )
                    .expect("tracks");
                    let given: Vec<ParamSegment> = anchors.iter().map(param_of).collect();
                    let built =
                        apply_param_shape_animated(&shape, &given, &animated).expect("rebuild");
                    built.iter().map(from_param).collect()
                }
                None => anchors.iter().map(from_path).collect(),
            }
        }
        _ => {
            let after = run_path_fx(stack, &stored.stroke, frame).expect("effects");
            let anchors = anchors_of(&after);
            let basis = if m == Mutation::StoredBounds {
                anchors_of(&stored.stroke)
            } else {
                anchors
            };
            match rebuild(basis) {
                Some(built) => built.iter().map(from_param).collect(),
                None => anchors.iter().map(from_path).collect(),
            }
        }
    };
    EvaluatedVector {
        source_object_index: 0,
        closed,
        segments,
    }
}

fn active_fx(doc: &Value) -> bool {
    doc["layers"][0]["pathFx"]
        .as_array()
        .is_some_and(|stack| stack.iter().any(|entry| entry["enabled"] != json!(false)))
}

fn has_shape(doc: &Value) -> bool {
    doc["objects"][0]["vectorExt"]["paramShape"].is_object()
}

/// Whether the mistake changes the output of this row at all.
fn acts_on(m: Mutation, doc: &Value, row: &Row) -> bool {
    match m {
        Mutation::None => false,
        Mutation::ParamBeforeEffects | Mutation::StoredBounds => active_fx(doc) && row.rebuilt,
        Mutation::NoEffects => active_fx(doc),
        Mutation::AlwaysRebuild => has_shape(doc) && !row.rebuilt,
        Mutation::FrameZero => row.rebuilt && row.frame != 0,
    }
}

#[test]
fn the_recomposed_model_without_a_mistake_equals_the_production_join() {
    let (_, all) = rows();
    for row in &all {
        let snap = snapshot(&row.doc);
        let model = mutated(&snap, row.frame, Mutation::None);
        assert_eq!(
            model,
            evaluate_vector(&snap, 0, row.frame).expect("join"),
            "{}",
            row.id
        );
        check(&model, row).unwrap_or_else(|m| panic!("{m}"));
    }
}

#[test]
fn every_mistake_is_caught_by_the_oracle_and_only_where_it_acts() {
    let (_, all) = rows();
    for m in [
        Mutation::ParamBeforeEffects,
        Mutation::StoredBounds,
        Mutation::NoEffects,
        Mutation::AlwaysRebuild,
        Mutation::FrameZero,
    ] {
        let mut failing = Vec::new();
        for row in &all {
            let doc: Value = serde_json::from_str(&row.doc).expect("document json");
            let got = mutated(&snapshot(&row.doc), row.frame, m);
            if check(&got, row).is_err() {
                assert!(
                    acts_on(m, &doc, row),
                    "{m:?} failed a row it cannot act on: {}",
                    row.id
                );
                failing.push(row.id.clone());
            }
        }
        assert!(!failing.is_empty(), "{m:?} was not caught by any row");
    }
}

#[test]
fn the_rough_bound_is_neither_vacuous_nor_deaf() {
    let (_, all) = rows();
    let scaled_failures = |factor: f64| -> usize {
        let mut failures = 0;
        for row in all.iter().filter(|r| r.class == Class::Rough) {
            let mut doc: Value = serde_json::from_str(&row.doc).expect("document json");
            for entry in doc["layers"][0]["pathFx"].as_array_mut().expect("stack") {
                if entry["type"] == json!("roughen") {
                    let p1 = entry["p1"].as_f64().expect("explicit amount");
                    entry["p1"] = json!(p1 * factor);
                }
            }
            let got = evaluate_vector(&snapshot(&doc.to_string()), 0, row.frame).expect("join");
            if check(&got, row).is_err() {
                failures += 1;
            }
        }
        failures
    };
    assert_eq!(scaled_failures(1.0), 0, "the unmodified amount passes");
    assert_eq!(
        scaled_failures(1.0 + 1e-14),
        0,
        "a 1e-14 relative change is inside the bound"
    );
    assert!(
        scaled_failures(1.0 + 1e-6) >= 10,
        "a 1e-6 relative change must be caught"
    );
}

#[test]
fn the_trig_bound_and_the_signed_zero_rule_cut_both_ways() {
    let (_, all) = rows();
    let star = all
        .iter()
        .find(|r| r.id == "star-inner-animated@10")
        .expect("row");
    assert_eq!(star.class, Class::Trig);
    let got = evaluate_vector(&snapshot(&star.doc), 0, star.frame).expect("join");
    let mut one_ulp = star.clone();
    one_ulp.expected[0][0] = f64::from_bits(one_ulp.expected[0][0].to_bits() + 1);
    assert!(
        check(&got, &one_ulp).is_ok(),
        "one ulp is inside the trig bound"
    );
    let mut off = star.clone();
    off.expected[0][0] += 1e-9;
    assert!(check(&got, &off).is_err(), "1e-9 is outside the trig bound");
    for id in ["star-inner-animated@10", "rect-corner-tl-animated@5"] {
        let source = all.iter().find(|r| r.id == id).expect("row");
        let got = evaluate_vector(&snapshot(&source.doc), 0, source.frame).expect("join");
        let mut flipped = source.clone();
        let zero = flipped
            .expected
            .iter_mut()
            .flatten()
            .find(|v| v.to_bits() == 0)
            .expect("the row has a +0 member");
        *zero = -0.0;
        assert!(
            check(&got, &flipped).is_err(),
            "{id}: a signed-zero mismatch never passes"
        );
    }
}

#[test]
fn the_oracle_still_covers_the_branches_the_controls_rely_on() {
    let (_, all) = rows();
    let docs: Vec<Value> = all
        .iter()
        .map(|r| serde_json::from_str(&r.doc).expect("document json"))
        .collect();
    let count =
        |f: &dyn Fn(&Value, &Row) -> bool| docs.iter().zip(&all).filter(|&(d, r)| f(d, r)).count();
    assert_eq!(count(&|_, _| true), 57);
    assert_eq!(count(&|_, r| !r.closed), 5, "open paths");
    assert_eq!(count(&|_, r| r.rebuilt), 37, "rebuilt rows");
    assert_eq!(
        count(&|_, r| r.rebuilt && r.frame != 0),
        24,
        "rebuilt away from frame 0"
    );
    assert_eq!(
        count(&|d, _| active_fx(d)),
        25,
        "rows with an active effect"
    );
    assert_eq!(
        count(&|d, r| active_fx(d) && r.rebuilt),
        11,
        "effect and rebuild together"
    );
    assert_eq!(
        count(&|d, r| has_shape(d) && !r.rebuilt),
        4,
        "stored shapes that must not be rebuilt"
    );
    assert_eq!(count(&|d, _| has_shape(d)), 41);
    assert_eq!(
        count(&|d, _| d.get("propertyHolders").is_some()),
        40,
        "rows with tracks"
    );
    assert_eq!(count(&|_, r| r.class == Class::Exact), 31);
    assert_eq!(count(&|_, r| r.class == Class::Trig), 13);
    assert_eq!(count(&|_, r| r.class == Class::Rough), 13);
    assert_eq!(
        count(&|d, _| d["layers"][0]["pathFx"]
            .as_array()
            .is_some_and(Vec::is_empty)),
        1,
        "an explicit empty stack"
    );
    for kind in ["rect", "ellipse", "star"] {
        let n = count(&|d, _| d["objects"][0]["vectorExt"]["paramShape"]["kind"] == json!(kind));
        assert!(n >= 8, "{kind}: {n} rows");
    }
}
