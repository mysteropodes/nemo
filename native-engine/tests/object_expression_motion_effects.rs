//! Original evaluated-Motion receipts, dependencies, retirement and guest lifetime. ALL UNRUN.
use super::*;
use crate::evaluation::property_expression_runtime::Diagnostic;

#[test]
fn invalid_results_syntax_and_timeout_refuse_and_retain_complete_original_effects() {
    for (code, timeout) in [
        ("'wrong dimension'", false),
        ("var = ;", false),
        ("while(true){}", true),
    ] {
        let o = owner(fixture(code));
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        let mut effects = None;
        assert!(
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .is_err()
        );
        let failure = effects.as_ref().unwrap();
        assert_eq!(failure.effects.len(), 1);
        match &failure.effects[0] {
            MotionEffect::Evaluated {
                target,
                property,
                result,
            } => {
                assert_eq!(target.layer_uid(), "L0");
                assert_eq!(property, "dupOffsetPos");
                assert!(result.diagnostic.is_some());
                assert_eq!(result.disable.is_some(), timeout);
                if timeout {
                    assert_eq!(result.diagnostic, Some(Diagnostic::Timeout));
                }
            }
            _ => panic!("expected the original typed fallback receipt"),
        }
        let receipt = failure.to_string();
        // Another frame cannot silently retire unresolved disables/diagnostics.
        assert_eq!(
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 6), &mut effects)
                .unwrap_err(),
            receipt
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn dependency_disables_keep_their_actual_target_without_duplicate_effects() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
        {"property":"effOffPosition","expression":{"enabled":true,"code":"layer('C').opacity"}}]),
    );
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"while(true){}"}}]),
    );
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut effects = None;
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).is_err()
    );
    let failure = effects.as_ref().unwrap();
    let dependencies = failure
        .effects
        .iter()
        .flat_map(|e| match e {
            MotionEffect::Evaluated { result, .. } => result.dependencies.as_slice(),
            MotionEffect::Refused { dependencies, .. } => dependencies.as_slice(),
        })
        .collect::<Vec<_>>();
    assert!(dependencies
        .iter()
        .any(|d| d.target.layer_uid() == "C" && d.property == "opacity" && d.disable));
    assert_eq!(
        failure.effects.len(),
        1,
        "one attempted outer property, no duplicated receipt"
    );
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn queued_async_retirement_and_input_limits_keep_typed_refusals_at_the_owner_sink() {
    for code in ["(async()=>{await 0;})(), [1,2]", "[frame+123456789,0]"] {
        let o = owner(fixture(code));
        let s = o.acquire_snapshot(0).unwrap();
        let mut effects = None;
        let budget = if code.starts_with('(') { 4096 } else { 8 };
        let limits = ExpressionLimits::new(budget, 8_388_608, 262_144, 32).unwrap();
        assert!(prepare_with_limits(&o, &s, &pins(&s, 2), &mut effects, limits).is_err());
        let failure = effects.as_ref().unwrap();
        assert_eq!(
            failure.cause,
            if budget == 8 {
                ExpressionRefusal::InputLimit
            } else {
                ExpressionRefusal::QueuedAsyncWork
            }
        );
        assert!(matches!(
            failure.effects.as_slice(),
            [MotionEffect::Refused { .. }]
        ));
        assert_eq!(o.history_depths(), (0, 0));
    }
}

#[test]
fn stale_foreign_and_wrong_frame_pins_never_clear_a_retained_effect() {
    let o = owner(fixture("while(true){}"));
    let s = o.acquire_snapshot(0).unwrap();
    let mut retained = None;
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut retained)
            .is_err()
    );
    let before = retained.as_ref().unwrap().to_string();
    let other = owner(fixture("[frame,0]"));
    assert!(prepare_object_motion_exposure_frame_for_owner(
        &other,
        &s,
        &pins(&s, 2),
        &mut retained
    )
    .is_err());
    assert_eq!(retained.as_ref().unwrap().to_string(), before);
    let clean = other.acquire_snapshot(0).unwrap();
    let mut bad = pins(&clean, 2);
    bad.document_snapshot_id = "wrong";
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&other, &clean, &bad, &mut None).is_err()
    );
    let bad = pins(&clean, 8);
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&other, &clean, &bad, &mut None).is_err()
    );
}

#[test]
fn a_verified_new_revision_retires_old_effects_without_applying_stale_disables() {
    let mut o = owner(fixture("while(true){}"));
    let old = o.acquire_snapshot(0).unwrap();
    let mut retained = None;
    assert!(prepare_object_motion_exposure_frame_for_owner(
        &o,
        &old,
        &pins(&old, 2),
        &mut retained
    )
    .is_err());
    let fill = o
        .prepare_fill_json(
            &serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":"stroke-0"},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1}}))
            .unwrap(),
        )
        .unwrap();
    o.commit_fill(fill).unwrap();
    assert!(prepare_object_motion_exposure_frame_for_owner(
        &o,
        &old,
        &pins(&old, 2),
        &mut retained
    )
    .is_err());
    assert!(retained.as_ref().unwrap().same_source(&old));
    let current = o.acquire_snapshot(o.content_revision()).unwrap();
    assert!(prepare_object_motion_exposure_frame_for_owner(
        &o,
        &current,
        &pins(&current, 2),
        &mut retained
    )
    .is_err());
    let fresh = retained.as_ref().unwrap();
    assert!(fresh.same_source(&current));
    assert!(!fresh.same_source(&old));
    match &fresh.effects[0] {
        MotionEffect::Evaluated { result, .. } => {
            assert_eq!(result.disable.as_ref().unwrap().content_revision, 1)
        }
        _ => panic!("fresh timeout receipt required"),
    }
    assert_eq!(o.history_depths(), (1, 0)); // Only the explicit fill edit changed history.
}

#[test]
fn dropping_a_prepared_packet_leaves_no_guest_state_for_the_next_frame() {
    let o = owner(fixture("var n=0; return [++n+frame,0];"));
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let mut effects = None;
    for _ in 0..2 {
        let packet =
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .unwrap();
        assert_eq!(x_values(&packet), [20., 23., 36.]);
        drop(packet);
        assert!(effects.is_none());
    }
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn dependency_effects_survive_both_successful_guest_fallback_and_refused_retirement_exactly_once() {
    for (code, queued) in [
        ("layer('C').opacity", false),
        (
            "layer('C').opacity; (async()=>{await 0;})(); return [1,2];",
            true,
        ),
    ] {
        let mut doc = fixture("[frame,0]");
        add_effector(
            &mut doc,
            "E",
            "L0",
            json!([
            {"property":"effOffPosition","expression":{"enabled":true,"code":code}}]),
        );
        add_effector(
            &mut doc,
            "C",
            "missing",
            json!([
            {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"throw new Error('nested')"}}]),
        );
        let o = owner(doc);
        let s = o.acquire_snapshot(0).unwrap();
        let mut effects = None;
        assert!(
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .is_err()
        );
        let failure = effects.unwrap();
        assert_eq!(failure.effects.len(), 1);
        let dependencies = match &failure.effects[0] {
            MotionEffect::Evaluated { result, .. } => {
                assert!(!queued);
                assert_eq!(result.diagnostic, None);
                assert_eq!(result.value, [20., 20.]);
                &result.dependencies
            }
            MotionEffect::Refused {
                refusal,
                dependencies,
                ..
            } => {
                assert!(queued);
                assert_eq!(*refusal, ExpressionRefusal::QueuedAsyncWork);
                dependencies
            }
        };
        assert_eq!(
            dependencies.len(),
            1,
            "the sibling must not duplicate successful dependencies"
        );
        assert_eq!(dependencies[0].target.layer_uid(), "C");
        assert_eq!(dependencies[0].property, "opacity");
        assert!(matches!(
            dependencies[0].diagnostic,
            Some(Diagnostic::Runtime { .. })
        ));
        assert!(!dependencies[0].disable);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
