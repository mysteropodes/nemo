//! E2 (#2088): typed Motion property expressions through a real native owner.
//! Expected values follow `evalExpressionFor`/`normalizeExprResult` in
//! `src/js/motion.js`, already pinned for E1 by its JSC/Node oracle.
use crate::evaluation::property_expression_runtime::Diagnostic;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_ordered_project, encode_storage_project};
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_expression_pin::{prepare_property_expression_request, PropertyEvaluation};
use crate::object_expression_runtime::{
    evaluate_property_detached, evaluate_property_for_owner, ExpressionLimits, ExpressionRefusal,
};
use crate::object_frame_packet::ObjectFrameError;
use serde_json::{json, Value};

fn expression(code: &str, enabled: bool) -> Value {
    json!({"enabled": enabled, "code": code})
}

fn owner(property: &str, code: &str, enabled: bool) -> NativeObjectHistory {
    let mut properties = vec![
        json!({"property":"position","static":[3,4]}),
        json!({"property":"opacity","static":[50]}),
        json!({"property":"fillColor","static":[255,0,0,255]}),
        json!({"property":"xc_point","static":[-5,6]}),
    ];
    match properties.iter_mut().find(|p| p["property"] == property) {
        Some(record) => record["expression"] = expression(code, enabled),
        None => {
            properties.push(json!({"property":property,"expression":expression(code, enabled)}))
        }
    }
    let document = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L0"}],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L0","strokeId":"s0"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
                {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                    {"layerUid":"L0","strokeId":"s0","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"},
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
            "controls":[{"key":"xc_point","name":"Point","type":"point"},
                {"key":"xc_wave","name":"Wave","type":"number"}],
            "properties":properties}]});
    let project = decode_ordered_project(&serde_json::to_vec(&document).unwrap()).unwrap();
    NativeObjectHistory::new("expression-property", project.into_storage()).unwrap()
}

fn layer() -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: "L0".into(),
        stroke_id: Presence::Missing,
    }
}

fn limits() -> ExpressionLimits {
    ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap()
}

fn run(property: &str, code: &str, frame: f64, depth: u32) -> PropertyEvaluation {
    let owner = owner(property, code, true);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(snapshot.document()).unwrap();
    let result = evaluate_property_for_owner(
        &owner,
        &snapshot,
        &layer(),
        property,
        frame,
        depth,
        limits(),
    )
    .unwrap_or_else(|e| panic!("{property} `{code}`: {e:?}"));
    let current = owner.acquire_snapshot(owner.content_revision()).unwrap();
    assert_eq!(encode_storage_project(current.document()).unwrap(), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    result
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Ok,
    Number,
    NumberOrXy,
    Runtime,
    Syntax,
    TooDeep,
    Timeout,
}

fn kind(diagnostic: &Option<Diagnostic>) -> Kind {
    match diagnostic {
        None => Kind::Ok,
        Some(Diagnostic::MustReturnNumber) => Kind::Number,
        Some(Diagnostic::MustReturnNumberOrXY) => Kind::NumberOrXy,
        Some(Diagnostic::Runtime { .. }) => Kind::Runtime,
        Some(Diagnostic::Syntax { .. }) => Kind::Syntax,
        Some(Diagnostic::TooDeep) => Kind::TooDeep,
        Some(Diagnostic::Timeout) => Kind::Timeout,
    }
}

#[rustfmt::skip]
#[test]
fn typed_results_follow_motion_normalization_and_fallback() {
    use Kind::*;
    let cases: &[(&str, &str, f64, &[f64], Kind)] = &[
        ("position", "value", 0., &[3., 4.], Ok),
        ("position", "[value[0] + frame, 7]", 2., &[5., 7.], Ok),
        ("position", "5", 0., &[5., 5.], Ok),
        ("position", "[9]", 0., &[9., 9.], Ok),
        ("position", "[1, 2, 3]", 0., &[1., 2.], Ok),
        ("position", "[1, 2, 'tail']", 0., &[1., 2.], Ok),
        ("position", "[1, 'x']", 0., &[3., 4.], NumberOrXy),
        ("position", "[]", 0., &[3., 4.], NumberOrXy),
        ("position", "({0: 1, 1: 2, length: 2})", 0., &[3., 4.], NumberOrXy),
        ("position", "[NaN]", 0., &[3., 4.], NumberOrXy),
        ("position", "[, 2]", 0., &[3., 4.], NumberOrXy),
        // Motion cuts with the guest's own `slice`, so an own override wins.
        ("position", "Object.defineProperty([1, 2], 'slice', {value: () => [8, 9]})", 0., &[8., 9.], Ok),
        // A short array never reaches `slice`; an undefined dim never reads.
        ("position", "Object.defineProperty([9], 'slice', {value: () => [1, 2]})", 0., &[9., 9.], Ok),
        ("position", "Object.defineProperty([9], 'slice', {value: () => { throw 1; }})", 0., &[9., 9.], Ok),
        ("opacity", "value * 2", 0., &[100.], Ok),
        ("opacity", "value + frame", 2.5, &[52.5], Ok),
        ("opacity", "value + frame", -1., &[49.], Ok),
        ("opacity", "[7, 8]", 0., &[7.], Ok),
        ("opacity", "Array.isArray(value) ? 1 : 2", 0., &[2.], Ok),
        ("opacity", "'7'", 0., &[50.], Number),
        ("opacity", "1 / 0", 0., &[50.], Number),
        ("opacity", "undefined", 0., &[50.], Number),
        ("xc_point", "[value[1], value[0]]", 0., &[6., -5.], Ok),
        ("xc_wave", "value + 1", 0., &[1.], Ok),
        // Catalog-only builtin: PROP_DIM has no entry, so a number fills
        // nothing and arrays are refused, exactly as normalizeExprResult.
        ("fillColor", "5", 0., &[], Ok),
        ("fillColor", "[1, 2, 3, 4]", 0., &[255., 0., 0., 255.], Number),
        ("fillColor", "Object.defineProperty([1], 'slice', {value: () => { while (true) {} }})", 0., &[255., 0., 0., 255.], Number),
    ];
    for (property, code, frame, value, expected) in cases {
        let result = run(property, code, *frame, 0);
        assert_eq!((result.value.as_slice(), kind(&result.diagnostic)), (*value, expected.clone()), "{property} `{code}`");
        assert!(result.disable.is_none());
    }
}

#[test]
fn guest_failures_fall_back_to_raw_with_typed_diagnostics() {
    let thrown = run("opacity", "throw new Error('boom')", 0., 0);
    assert_eq!(thrown.value, [50.]);
    assert_eq!(
        thrown.diagnostic,
        Some(Diagnostic::Runtime {
            message: "boom".into(),
            line: None
        })
    );
    for code in ["var = ;", "return (", "}{"] {
        let r = run("position", code, 0., 0);
        assert_eq!(
            (r.value, kind(&r.diagnostic)),
            (vec![3., 4.], Kind::Syntax),
            "{code}"
        );
    }
    for code in [
        "value + time",
        "function f(){return f();} return f();",
        "null.x",
    ] {
        let r = run("opacity", code, 0., 0);
        assert_eq!(
            (r.value, kind(&r.diagnostic)),
            (vec![50.], Kind::Runtime),
            "{code}"
        );
        assert!(r.disable.is_none());
    }
}

#[test]
fn timeout_prepares_a_disable_without_mutating_the_owner() {
    for code in [
        "while (true) {}",
        "while(true){try{while(true){}}catch(e){}}",
    ] {
        let r = run("position", code, 3., 0);
        assert_eq!(
            (r.value.clone(), kind(&r.diagnostic)),
            (vec![3., 4.], Kind::Timeout)
        );
        let disable = r.disable.expect("timeout disables");
        assert_eq!(
            (disable.content_revision, disable.property.as_str()),
            (0, "position")
        );
        assert_eq!(disable.target, layer());
    }
}

#[test]
fn compile_failure_wins_over_depth_and_depth_skips_the_guest() {
    let deep = run("opacity", "while (true) {}", 0., 8);
    assert_eq!(
        (deep.value, kind(&deep.diagnostic)),
        (vec![50.], Kind::TooDeep)
    );
    assert!(deep.disable.is_none(), "the guest never ran");
    let syntax = run("opacity", "var = ;", 0., 9);
    assert_eq!(kind(&syntax.diagnostic), Kind::Syntax);
    let shallow = run("opacity", "value + 1", 0., 7);
    assert_eq!(shallow.value, [51.]);
}

#[test]
fn unavailable_requests_are_refused_before_any_runtime() {
    let pin = |e| Err(ExpressionRefusal::Pin(e));
    let disabled = owner("opacity", "value", false);
    let snap = disabled.acquire_snapshot(0).unwrap();
    let eval = |owner: &NativeObjectHistory, target: &PropertyHolderTarget, prop, frame: f64| {
        let snapshot = owner.acquire_snapshot(0).unwrap();
        evaluate_property_for_owner(owner, &snapshot, target, prop, frame, 0, limits())
            .map(|r| r.value)
    };
    assert_eq!(
        eval(&disabled, &layer(), "opacity", 0.),
        pin(ObjectFrameError::Unavailable)
    );
    let empty = owner("opacity", "", true);
    assert_eq!(
        eval(&empty, &layer(), "opacity", 0.),
        pin(ObjectFrameError::Unavailable)
    );
    let live = owner("opacity", "value", true);
    assert_eq!(
        eval(&live, &layer(), "position", 0.),
        pin(ObjectFrameError::Unavailable)
    );
    assert_eq!(
        eval(&live, &layer(), "opacity", f64::NAN),
        pin(ObjectFrameError::FrameOutOfRange)
    );
    assert_eq!(
        eval(&live, &layer(), "order", 0.),
        pin(ObjectFrameError::Unavailable)
    );
    let mut gone = layer();
    gone.layer_uid = "missing".into();
    assert_eq!(
        eval(&live, &gone, "opacity", 0.),
        pin(ObjectFrameError::InvalidSource)
    );
    let other = owner("opacity", "value", true);
    let foreign = other.acquire_snapshot(0).unwrap();
    assert!(matches!(
        evaluate_property_for_owner(&live, &foreign, &layer(), "opacity", 0., 0, limits()),
        Err(ExpressionRefusal::Pin(_))
    ));
    drop(snap);
}

#[test]
fn a_candidate_from_an_old_revision_is_never_published() {
    let mut owner = owner("opacity", "value * 2", true);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let request =
        prepare_property_expression_request(&owner, &snapshot, &layer(), "opacity", 0., 0).unwrap();
    let candidate = evaluate_property_detached(&request, limits()).unwrap();
    let payload = json!({"command":"object.fill.set", "stableTarget":{
        "contextId":"scene-root", "frameScope":{"kind":"authored","frame":7},
        "layerUid":"L0", "strokeId":"s0"}, "fill":{"kind":"solid","r":0.5,"g":0.1,"b":0.3,"a":0.5}});
    let fill = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(1));
    assert_eq!(
        request.finalize(&owner, candidate),
        Err(ExpressionRefusal::Pin(ObjectFrameError::WrongRevision))
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let current = owner.acquire_snapshot(1).unwrap();
    let fresh = evaluate_property_for_owner(&owner, &current, &layer(), "opacity", 0., 0, limits());
    assert_eq!(fresh.map(|r| r.value), Ok(vec![100.]));
}

#[test]
fn code_cannot_close_the_compiled_wrapper() {
    // Motion parses the body alone with Function(), where this is a syntax
    // error: the guest never runs, even past the depth cap.
    for breakout in [
        "1);})(), (function(){while(true){}})(), (function(){return (1",
        "1);})(), (function(){throw new Error('x')})(), (function(){return (1",
        "1\n}); while (true) {} (function(){",
    ] {
        for depth in [0, 8] {
            let r = run("opacity", breakout, 0., depth);
            assert_eq!(
                (r.value, kind(&r.diagnostic)),
                (vec![50.], Kind::Syntax),
                "{breakout}"
            );
            assert!(r.disable.is_none());
        }
    }
}

// The defect #2088 repairs, kept as a control: the previous `compile`
// evaluated this concatenated wrapper as a whole script (`ctx.eval`, i.e.
// `JS_Eval`), so the breakout statement ran at compile time.
#[test]
fn concatenated_wrapper_ran_breakouts_as_a_script() {
    use rquickjs::{Context, Function, Runtime};
    let breakout = "1);})(), (ran = ran + 1), (function(){return (1";
    let rt = Runtime::new().unwrap();
    let context = Context::full(&rt).unwrap();
    context.with(|ctx| {
        ctx.globals().set("ran", 0).unwrap();
        let old = format!("(function(value,frame){{'use strict';return ({breakout}\n);}})");
        assert!(ctx.eval::<Function, _>(old).is_ok());
        assert_eq!(ctx.globals().get::<_, i32>("ran").unwrap(), 1);
    });
    for depth in [0, 8] {
        let r = run("opacity", breakout, 0., depth);
        assert_eq!((r.value, kind(&r.diagnostic)), (vec![50.], Kind::Syntax));
    }
}

#[test]
fn host_limits_stay_refusals_through_the_typed_entry() {
    let live = owner("opacity", "value + 123456789", true);
    let snapshot = live.acquire_snapshot(0).unwrap();
    let tight = ExpressionLimits::new(8, 8_388_608, 262_144, 32).unwrap();
    let eval = |limits| {
        evaluate_property_for_owner(&live, &snapshot, &layer(), "opacity", 0., 0, limits)
            .map(|r| r.value)
    };
    assert_eq!(eval(tight), Err(ExpressionRefusal::InputLimit));
    let queued = owner("opacity", "(async () => { await 0; })(), 1", true);
    let snapshot = queued.acquire_snapshot(0).unwrap();
    assert_eq!(
        evaluate_property_for_owner(&queued, &snapshot, &layer(), "opacity", 0., 0, limits())
            .map(|r| r.value),
        Err(ExpressionRefusal::QueuedAsyncWork)
    );
}
