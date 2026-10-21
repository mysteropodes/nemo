//! Actual inline selector identity through C1/C02 and retained real-font Text.
//! All execution UNRUN. Existing VM limits, history and independent SVG oracle reused.
use super::*;
use crate::application::{
    finalize_object_motion_frame_effects_for_owner,
    prepare_object_motion_exposure_frame_for_control,
    prepare_object_motion_fallback_frame_for_control, ObjectMotionFrameControl,
};
use crate::evaluation::property_expression_runtime::Diagnostic;
use crate::object_document::ExpressionHolder;
use crate::object_expression_pin::{prepare_holder_expression_request, PropertyEvaluation};
use crate::object_expression_runtime::{ExpressionLimits, ExpressionRefusal};
fn holder(layer: &str, id: &str) -> ExpressionHolder {
    ExpressionHolder::TextAnimator {
        context_id: "scene-root".into(),
        layer_uid: layer.into(),
        animator_id: id.into(),
    }
}
fn limits() -> ExpressionLimits {
    ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap()
}
fn drawing(code: &str) -> Value {
    let mut doc = drawing_without_raster(true);
    doc["layers"][0]["textAnimators"] = json!([{"id":"same","selector":{},"props":{"position":[20,0]},
        "properties":[{"property":"amount","static":[25],"expression":{"enabled":true,"code":code}}]}]);
    doc
}
fn evaluate(
    o: &NativeObjectHistory,
    s: &crate::revision::ObjectSnapshot,
    target: &ExpressionHolder,
    depth: u32,
) -> PropertyEvaluation {
    let request = prepare_holder_expression_request(o, s, target, "amount", 2., depth).unwrap();
    let candidate =
        crate::object_expression_runtime::evaluate_property_detached(&request, limits()).unwrap();
    request.finalize(o, candidate).unwrap()
}
fn owner_plan(
    o: &NativeObjectHistory,
    s: &crate::revision::ObjectSnapshot,
    frame: u32,
) -> ObjectPaintPlan {
    let pins = selector(s, frame);
    let packet =
        crate::application::prepare_object_motion_exposure_frame_for_owner(o, s, &pins, &mut None)
            .unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &pins).unwrap(), &pins).unwrap()
}
fn x_bounds(p: &ObjectPaintPlan) -> [f64; 2] {
    let out = export_checked(p);
    let parsed = reader::parse(&out.bytes).unwrap();
    assert_eq!(parsed.paths.len(), 3);
    let mut x = [f64::INFINITY, f64::NEG_INFINITY];
    for path in parsed.paths {
        for point in std::iter::once(path.start).chain(path.curves.iter().map(|c| c[2])) {
            x[0] = x[0].min(point[0]);
            x[1] = x[1].max(point[0]);
        }
    }
    x
}
#[test]
fn real_inline_self_at_and_keys_reach_pinned_text_svg_without_raw_expression_fallback() {
    let mut doc = drawing("self.at(0) + numKeys * 10 + frame * 5");
    doc["layers"][0]["textAnimators"][0]["properties"][0]["track"] = json!({"keys":[
        {"frame":0,"v":[10]},{"frame":4,"v":[30]}]});
    let o = owner(&doc);
    let s = support::head(&o);
    let before = support::state(&o);
    let pins = selector(&s, 2);
    let raw = prepare_object_exposure_frame(&s, &pins).unwrap();
    assert!(prepare_object_render_scene(&raw, &pins).is_err());
    let value = evaluate(&o, &s, &holder("L0", "same"), 0);
    assert_eq!(value.value, [40.]);
    assert!(value.diagnostic.is_none() && value.disable.is_none());
    let p = owner_plan(&o, &s, 2); // amount40 * position20 = +8 over original Roboto A.
    for (got, want) in x_bounds(&p).into_iter().zip([18.33984375, 33.3515625]) {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
    let view = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert_eq!(view.document_snapshot_id(), export.document_snapshot_id());
    let versions = p.scene_resource_versions().unwrap().into_iter().collect();
    view.verify_scene_resources(&versions).unwrap();
    export.verify_scene_resources(&versions).unwrap();
    assert_eq!(support::state(&o), before);
    drop(o);
    assert_eq!(x_bounds(&p), [18.33984375, 33.3515625]);
}
#[test]
fn real_inline_identity_separates_same_ids_layers_and_rejects_wrong_source_or_owner() {
    let mut doc = drawing("25");
    let mut other = doc["layers"][0].clone();
    other["layerUid"] = json!("other");
    other["visible"] = json!(false);
    other["textAnimators"][0]["properties"][0]["expression"]["code"] = json!("75");
    doc["layers"].as_array_mut().unwrap().push(other);
    doc["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"other","naturalPosition":1,"order":{}}));
    let o = owner(&doc);
    let s = support::head(&o);
    let before = support::state(&o);
    let request =
        prepare_holder_expression_request(&o, &s, &holder("L0", "same"), "amount", 2., 0).unwrap();
    assert_eq!(
        request.code(),
        "25",
        "code comes from this actual animator record"
    );
    assert_eq!(evaluate(&o, &s, &holder("L0", "same"), 0).value, [25.]);
    assert_eq!(evaluate(&o, &s, &holder("other", "same"), 0).value, [75.]);
    for bad in [
        holder("L0", "missing"),
        holder("missing", "same"),
        ExpressionHolder::TextAnimator {
            context_id: "missing".into(),
            layer_uid: "L0".into(),
            animator_id: "same".into(),
        },
    ] {
        assert!(prepare_holder_expression_request(&o, &s, &bad, "amount", 2., 0).is_err());
    }
    let foreign = NativeObjectHistory::from_admitted_project(
        "foreign-selector",
        support::project(&doc, support::images_only()),
    )
    .unwrap();
    assert!(prepare_holder_expression_request(
        &foreign,
        &s,
        &holder("L0", "same"),
        "amount",
        2.,
        0
    )
    .is_err());
    // The unrelated hidden selector must never execute during the real visible frame.
    doc["layers"][1]["textAnimators"][0]["properties"][0]["expression"]["code"] =
        json!("while(true){}");
    let untouched = owner(&doc);
    let snapshot = support::head(&untouched);
    assert_eq!(
        x_bounds(&owner_plan(&untouched, &snapshot, 2)),
        [15.33984375, 30.3515625]
    );
    assert_eq!(support::state(&o), before);
}
#[test]
fn inline_depth_compile_precedence_and_stale_retirement_reuse_original_c1_limits() {
    let mut o = owner(&drawing("while(true){}"));
    let s = support::head(&o);
    let deep = evaluate(&o, &s, &holder("L0", "same"), 8);
    assert_eq!(deep.value, [25.]);
    assert_eq!(deep.diagnostic, Some(Diagnostic::TooDeep));
    assert!(deep.disable.is_none());
    let bad = owner(&drawing("var = ;"));
    let bs = support::head(&bad);
    assert!(matches!(
        evaluate(&bad, &bs, &holder("L0", "same"), 9).diagnostic,
        Some(Diagnostic::Syntax { .. })
    ));
    let request =
        prepare_holder_expression_request(&o, &s, &holder("L0", "same"), "amount", 2., 8).unwrap();
    let candidate =
        crate::object_expression_runtime::evaluate_property_detached(&request, limits()).unwrap();
    support::run(
        &mut o,
        &json!({"command":"object.text.animator.edit","layerUid":"L0","animatorId":"same",
        "edit":{"kind":"setValue","property":"amount","frame":2,"values":[30]}}),
    )
    .unwrap();
    assert_eq!(
        request.finalize(&o, candidate),
        Err(ExpressionRefusal::Pin(
            crate::object_frame_packet::ObjectFrameError::WrongRevision
        ))
    );
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn inline_and_ordinary_timeouts_seal_once_in_the_existing_atomic_history_and_keep_old_source() {
    let mut doc = drawing("while(true){}");
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"opacity","static":[50],"expression":{"enabled":true,"code":"while(true){}"}}]}]);
    let mut o = owner(&doc);
    let s = support::head(&o);
    let before = support::bytes(&o);
    let ordinary = ExpressionHolder::Property(crate::object_document::PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: "L0".into(),
        stroke_id: crate::object_document::Presence::Missing,
    });
    assert!(
        crate::commands::ObjectFillOwner::prepare_holder_expression_disables(
            s.clone(),
            &[
                (ordinary, "opacity".into()),
                (holder("L0", "missing"), "amount".into())
            ]
        )
        .is_err()
    );
    assert_eq!(
        support::bytes(&o),
        before,
        "a failed mixed seal publishes neither disable"
    );
    let pins = selector(&s, 2);
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare_object_motion_exposure_frame_for_control(&o, &s, &pins, &mut control).is_err());
    assert!(control.requires_revision());
    assert_eq!(control.pending_frames(), [2]);
    assert_eq!(support::bytes(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    // A proof fallback reconstructs the same retained numeric frame without draining it.
    let fallback =
        prepare_object_motion_fallback_frame_for_control(&o, &s, &pins, &control).unwrap();
    let p = prepare_object_paint_plan(
        &prepare_object_render_scene(&fallback, &pins).unwrap(),
        &pins,
    )
    .unwrap();
    for (got, want) in x_bounds(&p).into_iter().zip([15.33984375, 30.3515625]) {
        assert!((got - want).abs() < 1e-6);
    }
    let mut wrong = pins.clone();
    wrong.context_id = "missing";
    assert!(
        finalize_object_motion_frame_effects_for_owner(&mut o, &wrong, &[2], &mut control).is_err()
    );
    assert!(control.has_pending());
    assert_eq!(support::bytes(&o), before);
    let mut foreign = owner(&doc);
    let foreign_snapshot = support::head(&foreign);
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut foreign,
        &selector(&foreign_snapshot, 2),
        &[2],
        &mut control
    )
    .is_err());
    assert_eq!(foreign.history_depths(), (0, 0));
    assert_eq!(control.pending_frames(), [2]);
    let committed =
        finalize_object_motion_frame_effects_for_owner(&mut o, &pins, &[2], &mut control).unwrap();
    assert!(committed.changed());
    assert!(!committed.frame_ready());
    assert_eq!(o.history_depths(), (1, 0));
    let fresh = support::head(&o);
    let stored = serde_json::to_value(fresh.document()).unwrap();
    let mut expected = doc.clone();
    expected["layers"][0]["textAnimators"][0]["properties"][0]["expression"]["enabled"] =
        json!(false);
    expected["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
    assert_eq!(stored, expected);
    assert_eq!(
        control.diagnostic_for_holder(&fresh, &holder("L0", "same"), "amount", 2.),
        Some(json!({"kind":"timeout"}))
    );
    assert_eq!(
        serde_json::to_value(s.document()).unwrap(),
        doc,
        "original snapshot conserved"
    );
    let fresh_pins = selector(&fresh, 2);
    let again =
        finalize_object_motion_frame_effects_for_owner(&mut o, &fresh_pins, &[2], &mut control)
            .unwrap();
    assert!(!again.changed());
    assert_eq!(o.history_depths(), (1, 0));
    let packet =
        prepare_object_motion_exposure_frame_for_control(&o, &fresh, &fresh_pins, &mut control)
            .unwrap();
    let live = prepare_object_paint_plan(
        &prepare_object_render_scene(&packet, &fresh_pins).unwrap(),
        &fresh_pins,
    )
    .unwrap();
    assert_eq!(x_bounds(&live), x_bounds(&p));
    o.undo(1).unwrap();
    assert_eq!(support::bytes(&o), before);
    assert_eq!(
        control.diagnostic_for_holder(&support::head(&o), &holder("L0", "same"), "amount", 2.),
        None
    );
    o.redo(2).unwrap();
    assert_eq!(
        serde_json::to_value(support::head(&o).document()).unwrap(),
        expected
    );
}
#[test]
fn inline_diagnostic_error_then_clear_retires_actual_source_frame_without_history() {
    let doc = drawing("if(frame===0){throw new Error('selector');} return 50;");
    let mut o = owner(&doc);
    let s = support::head(&o);
    let before = support::state(&o);
    let mut control = ObjectMotionFrameControl::default();
    let pins = selector(&s, 0);
    assert!(prepare_object_motion_exposure_frame_for_control(&o, &s, &pins, &mut control).is_err());
    assert!(!control.requires_revision());
    finalize_object_motion_frame_effects_for_owner(&mut o, &pins, &[0], &mut control).unwrap();
    assert!(control
        .diagnostic_for_holder(&s, &holder("L0", "same"), "amount", 0.)
        .is_some_and(|d| d["kind"] == "runtime"));
    let pins = selector(&s, 1);
    prepare_object_motion_exposure_frame_for_control(&o, &s, &pins, &mut control).unwrap();
    finalize_object_motion_frame_effects_for_owner(&mut o, &pins, &[1], &mut control).unwrap();
    assert_eq!(
        control.diagnostic_for_holder(&s, &holder("L0", "same"), "amount", 1.),
        Some(Value::Null)
    );
    assert_eq!(
        control.diagnostic_for_holder(&s, &holder("L0", "same"), "amount", 0.),
        None
    );
    assert_eq!(support::state(&o), before);
}

#[test]
fn inline_outer_keeps_real_ordinary_recursion_depth_effect_identity() {
    let mut doc = drawing("layer('L0').opacity");
    doc["layers"][0]["name"] = json!("L0");
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"opacity","static":[50],
            "expression":{"enabled":true,"code":"layer('L0').opacity"}}]}]);
    let o = owner(&doc);
    let s = support::head(&o);
    let before = support::state(&o);
    let result = evaluate(&o, &s, &holder("L0", "same"), 0);
    assert!(result.dependencies.iter().any(|effect|matches!(&effect.target,
        ExpressionHolder::Property(target) if target.layer_uid()=="L0" && target.stroke_id().is_none())
        && effect.property=="opacity" && effect.diagnostic==Some(Diagnostic::TooDeep)));
    assert!(result.dependencies.iter().all(|effect| !effect.disable));
    assert_eq!(support::state(&o), before);
}

#[test]
fn inline_self_identity_preserves_original_unresolved_layer_semantics() {
    let doc=drawing("self.property === 'amount' && self.name === null && self.index === -1 && !self.isShape && !self.hasParent ? 25 : 0");
    let o = owner(&doc);
    let s = support::head(&o);
    let before = support::state(&o);
    assert_eq!(evaluate(&o, &s, &holder("L0", "same"), 0).value, [25.]);
    assert_eq!(support::state(&o), before);
}
