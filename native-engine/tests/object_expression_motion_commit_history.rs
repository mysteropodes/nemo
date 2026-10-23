//! Atomic original timeout transactions, stale effects and retained history. ALL UNRUN.
use crate::commands::ObjectFillOwner;
use crate::object_codec::encode_storage_project;
use crate::object_expression_runtime::motion_frame::tests::commit_tests::{
    calls, diagnostic, prepare, target,
};
use crate::object_expression_runtime::motion_frame::tests::{
    add_effector, fixture, ordinary_editable_source, owner, pins, x_values,
};
use crate::object_expression_runtime::{
    finalize_object_motion_frame_effects_for_owner,
    prepare_object_motion_fallback_frame_for_control, ObjectMotionFrameControl,
};
use serde_json::{json, Value};

#[test]
fn outer_and_nested_timeout_disables_are_one_atomic_undo_and_require_a_fresh_owner_read() {
    let mut doc = fixture("while(true){}");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
        {"property":"effOffOpacity","expression":{"enabled":true,"code":"layer('C').opacity"}}]),
    );
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"while(true){}"}}]),
    );
    let mut o = owner(doc);
    let original = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(original.document()).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &original, 2, &mut control).is_err());
    assert!(control.requires_revision());
    let retired = calls();
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    let committed = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&original, 2),
        &[2],
        &mut control,
    )
    .unwrap();
    assert_eq!(
        (committed.before_revision(), committed.content_revision()),
        (0, 1),
    );
    assert!(committed.changed() && !committed.frame_ready());
    assert_eq!(
        calls(),
        retired,
        "the atomic finalizer never executes a guest"
    );
    assert!(!control.has_pending());
    assert_eq!(o.history_depths(), (1, 0));
    let current = o.acquire_snapshot(1).unwrap();
    let disabled = encode_storage_project(current.document()).unwrap();
    let stored: Value = serde_json::from_slice(&disabled).unwrap();
    assert_eq!(
        stored["propertyHolders"][0]["properties"][0]["expression"]["enabled"],
        false,
    );
    assert_eq!(
        stored["propertyHolders"][2]["properties"][0]["expression"]["enabled"],
        false,
    );
    assert_eq!(
        stored["propertyHolders"][1]["properties"][0]["expression"]["enabled"],
        true,
    );
    assert_eq!(
        current.project().unwrap().project(),
        original.project().unwrap().project(),
    );
    assert!(
        prepare(&o, &original, 2, &mut control).is_err(),
        "old pins stay stale after the commit",
    );
    assert!(
        prepare_object_motion_fallback_frame_for_control(
            &o,
            &current,
            &pins(&current, 2),
            &control
        )
        .is_err(),
        "R1 numeric values cannot become a current-source cached fallback",
    );
    assert_eq!(
        diagnostic(&control, &current, "E", "effOffOpacity", 2.),
        None
    );
    assert_eq!(
        x_values(&prepare(&o, &current, 2, &mut control).unwrap()),
        [20., 20., 30.],
    );
    assert!(
        calls() > retired,
        "the actual R2 read evaluates its still-enabled outer expression"
    );
    let fresh_calls = calls();
    assert_eq!(
        diagnostic(&control, &current, "C", "opacity", 2.),
        Some(json!({"kind":"timeout"})),
    );
    assert_eq!(diagnostic(&control, &original, "C", "opacity", 2.), None);
    let retry = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 2),
        &[2],
        &mut control,
    )
    .unwrap();
    assert!(!retry.changed());
    assert_eq!(
        calls(),
        fresh_calls,
        "sealing the fresh R2 read does not rerun its guest"
    );
    assert_eq!(o.history_depths(), (1, 0));
    o.undo(1).unwrap();
    let undone = o.acquire_snapshot(o.content_revision()).unwrap();
    assert_eq!(encode_storage_project(undone.document()).unwrap(), before);
    assert_eq!(diagnostic(&control, &undone, "C", "opacity", 2.), None);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(
        encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document())
            .unwrap(),
        disabled,
    );
}

#[test]
fn actual_source_edit_refuses_old_effect_commit_and_keeps_its_complete_receipts() {
    let mut o = owner(ordinary_editable_source(fixture("while(true){}")));
    let old = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &old, 2, &mut control).is_err());
    let retired = calls();
    let fill = o
        .prepare_fill_json(
            &serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"Edit","strokeId":"editable"},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1}}))
            .unwrap(),
        )
        .unwrap();
    o.commit_fill(fill).unwrap();
    let current = o.acquire_snapshot(1).unwrap();
    let before = encode_storage_project(current.document()).unwrap();
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 2),
        &[2],
        &mut control,
    )
    .is_err());
    assert!(prepare(&o, &current, 2, &mut control).is_err());
    assert!(control.requires_revision() && control.has_pending());
    assert_eq!(control.pending_frames(), [2]);
    assert_eq!(
        calls(),
        retired,
        "stale pending effects are retained, never evaluated again"
    );
    assert_eq!(control.diagnostic_records(), json!([]));
    assert_eq!(encode_storage_project(current.document()).unwrap(), before);
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}

#[test]
fn failed_retirement_keeps_dependency_timeout_commit_but_never_fabricates_a_frame() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
        {"property":"effOffPosition","expression":{"enabled":true,
            "code":"layer('C').opacity; (async()=>{await 0;})(); return [1,2];"}}]),
    );
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"while(true){}"}}]),
    );
    let mut o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    let retired = calls();
    assert!(
        prepare_object_motion_fallback_frame_for_control(&o, &s, &pins(&s, 2), &control).is_err(),
    );
    let committed =
        finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 2), &[2], &mut control)
            .unwrap();
    assert!(committed.changed());
    assert!(
        !committed.frame_ready(),
        "no E1 fallback exists for refused retirement",
    );
    let current = o.acquire_snapshot(1).unwrap();
    assert_eq!(
        calls(),
        retired,
        "retiring old values never executes a guest"
    );
    assert!(!control.has_pending());
    assert!(prepare(&o, &current, 2, &mut control).is_err());
    assert!(
        calls() > retired,
        "a new real read may retry the still-enabled expression"
    );
    assert!(
        control.has_pending(),
        "its new refusal is retained at the actual R2 source"
    );
    assert_eq!(
        diagnostic(&control, &current, "C", "opacity", 2.),
        Some(json!({"kind":"timeout"})),
    );
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn atomic_seal_cannot_publish_a_partial_disable_or_a_foreign_candidate() {
    let mut o = owner(fixture("while(true){}"));
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let good = (target("L0"), "dupOffsetPos".to_owned());
    assert!(ObjectFillOwner::prepare_expression_disables(
        s.clone(),
        &[good.clone(), (target("missing"), "opacity".into())],
    )
    .is_err());
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    let (candidate, _) = ObjectFillOwner::prepare_expression_disables(s.clone(), &[good]).unwrap();
    let mut foreign = owner(fixture("while(true){}"));
    assert!(foreign.commit_property_edit(candidate.clone()).is_err());
    o.commit_property_edit(candidate).unwrap();
    assert_eq!(
        (o.history_depths(), foreign.history_depths()),
        ((1, 0), (0, 0))
    );
    let current = o.acquire_snapshot(1).unwrap();
    assert!(ObjectFillOwner::prepare_expression_disables(
        current,
        &[(target("L0"), "dupOffsetPos".into())],
    )
    .is_err());
    assert_eq!(o.history_depths(), (1, 0));
}

#[path = "object_expression_motion_commit_revision.rs"]
mod revision_tests;
