//! Original C02 effects and numeric fallback on actual retained owner frames. ALL UNRUN.
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_expression_runtime::motion_frame::tests::{
    add_effector, fixture, ordinary_editable_source, owner, pins, x_values,
};
use crate::object_expression_runtime::{
    finalize_object_motion_frame_effects_for_owner,
    prepare_object_motion_exposure_frame_for_control,
    prepare_object_motion_exposure_frame_for_owner,
    prepare_object_motion_fallback_frame_for_control, ObjectMotionFrameControl,
};
use crate::object_frame_packet::ObjectFramePacket;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn target(uid: &str) -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: uid.into(),
        stroke_id: Presence::Missing,
    }
}
fn calls() -> usize {
    crate::object_expression_runtime::motion_frame::MOTION_EVALUATIONS.with(|count| count.get())
}
fn prepare(
    o: &NativeObjectHistory,
    s: &ObjectSnapshot,
    frame: u32,
    control: &mut ObjectMotionFrameControl,
) -> Result<ObjectFramePacket, String> {
    prepare_object_motion_exposure_frame_for_control(o, s, &pins(s, frame), control)
}
fn diagnostic(
    control: &ObjectMotionFrameControl,
    s: &ObjectSnapshot,
    uid: &str,
    key: &str,
    frame: f64,
) -> Option<Value> {
    control.diagnostic_for(s, &target(uid), key, frame)
}

#[test]
fn diagnostic_only_fallback_completes_same_source_without_history_or_another_guest() {
    let mut o = owner(fixture("'wrong'"));
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    let entered = calls();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    assert!(calls() > entered);
    let retired = calls();
    assert_eq!(control.pending_frames(), [2]);
    assert!(!control.requires_revision());
    assert_eq!(diagnostic(&control, &s, "L0", "dupOffsetPos", 2.), None);
    for _ in 0..2 {
        let p = prepare_object_motion_fallback_frame_for_control(&o, &s, &pins(&s, 2), &control)
            .unwrap();
        assert_eq!(x_values(&p), [20., 20., 30.]); // Existing original raw-source literal.
    }
    assert_eq!(calls(), retired);
    assert!(
        control.has_pending(),
        "proof preview did not drain or mutate the owner",
    );
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    let committed =
        finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 2), &[2], &mut control)
            .unwrap();
    assert!(!committed.changed());
    assert!(committed.frame_ready());
    assert_eq!(committed.document_snapshot_id(), s.snapshot_id());
    assert!(!control.has_pending());
    assert_eq!(
        diagnostic(&control, &s, "L0", "dupOffsetPos", 2.),
        Some(json!({"kind":"mustReturnNumberOrXY"})),
    );
    assert_eq!(
        x_values(&prepare(&o, &s, 2, &mut control).unwrap()),
        [20., 20., 30.],
    );
    assert_eq!(calls(), retired);
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), (0, 0));
    // The old strict API remains strict; the control cannot globally waive it.
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut None).is_err(),
    );
}

#[test]
fn successful_nested_evaluation_clears_its_real_original_diagnostic_and_keeps_unvisited_distinct() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
        {"property":"effOffOpacity","expression":{"enabled":true,"code":"-layer('C').opacity/8"}}]),
    );
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"frame === 2 ? 'bad' : 40"}}]),
    );
    let mut o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 2), &[2], &mut control)
        .unwrap();
    assert_eq!(
        diagnostic(&control, &s, "C", "opacity", 2.),
        Some(json!({"kind":"mustReturnNumber"})),
    );
    assert_eq!(
        diagnostic(&control, &s, "E", "effOffOpacity", 2.),
        Some(Value::Null),
    );
    prepare(&o, &s, 3, &mut control).unwrap();
    let retired = calls();
    assert!(
        control.has_pending(),
        "actual successful clears still require owner publication",
    );
    finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 3), &[3], &mut control)
        .unwrap();
    assert_eq!(
        diagnostic(&control, &s, "C", "opacity", 3.),
        Some(Value::Null),
    );
    assert_eq!(diagnostic(&control, &s, "C", "opacity", 2.), None);
    for frame in [-1., 2.5, f64::NAN, f64::INFINITY] {
        assert_eq!(diagnostic(&control, &s, "C", "opacity", frame), None);
    }
    assert_eq!(diagnostic(&control, &s, "C", "rotation", 3.), None);
    assert_eq!(
        calls(),
        retired,
        "diagnostic queries and finalization never run a guest",
    );
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}

#[test]
fn original_cycle_warning_survives_successful_outer_hops_and_clear_capture() {
    let mut doc = fixture("[frame,0]");
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
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"layer('C').opacity"}}]),
    );
    let mut o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    let retired = calls();
    let committed =
        finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 2), &[2], &mut control)
            .unwrap();
    assert!(committed.frame_ready());
    assert!(!committed.changed());
    assert_eq!(
        diagnostic(&control, &s, "C", "opacity", 2.),
        Some(json!({"kind":"tooDeep"})),
    );
    prepare(&o, &s, 2, &mut control).unwrap();
    assert_eq!(calls(), retired);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn wrong_owner_snapshot_context_and_unvisited_frames_keep_pending_effects_and_history() {
    let mut o = owner(fixture("'wrong'"));
    let s = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    let retired = calls();
    for fault in ["snapshot", "context", "frame"] {
        let mut selector = pins(&s, 2);
        match fault {
            "snapshot" => selector.document_snapshot_id = "wrong",
            "context" => selector.context_id = "missing",
            _ => selector.frame = 3,
        }
        assert!(finalize_object_motion_frame_effects_for_owner(
            &mut o,
            &selector,
            &[selector.frame],
            &mut control,
        )
        .is_err());
    }
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&s, 2),
        &[2, 3],
        &mut control,
    )
    .is_err());
    let mut other = owner(fixture("[frame,0]"));
    let foreign = other.acquire_snapshot(0).unwrap();
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut other,
        &pins(&foreign, 2),
        &[2],
        &mut control,
    )
    .is_err());
    assert!(
        prepare_object_motion_fallback_frame_for_control(&o, &s, &pins(&s, 3), &control).is_err(),
    );
    assert_eq!(control.pending_frames(), [2]);
    assert_eq!(control.diagnostic_records(), json!([]));
    assert_eq!(calls(), retired);
    assert_eq!(
        (o.history_depths(), other.history_depths()),
        ((0, 0), (0, 0))
    );
}

#[cfg(feature = "export_job")]
#[test]
fn ordered_request_frames_finalize_together_and_never_bless_or_evaluate_a_missing_frame() {
    let mut o = owner(fixture("frame === 2 ? 'bad' : [frame,0]"));
    let s = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &s, 2, &mut control).is_err());
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&s, 2),
        &[2, 3],
        &mut control,
    )
    .is_err());
    prepare(&o, &s, 3, &mut control).unwrap();
    let retired = calls();
    assert_eq!(control.pending_frames(), [2, 3]);
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&s, 2),
        &[2],
        &mut control,
    )
    .is_err());
    let committed =
        finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&s, 2), &[2, 3], &mut control)
            .unwrap();
    assert!(committed.frame_ready() && !committed.changed());
    assert_eq!(
        diagnostic(&control, &s, "L0", "dupOffsetPos", 2.),
        Some(json!({"kind":"mustReturnNumberOrXY"})),
    );
    assert_eq!(
        diagnostic(&control, &s, "L0", "dupOffsetPos", 3.),
        Some(Value::Null),
    );
    assert_eq!(diagnostic(&control, &s, "L0", "dupOffsetPos", 4.), None);
    prepare(&o, &s, 2, &mut control).unwrap();
    prepare(&o, &s, 3, &mut control).unwrap();
    assert_eq!(calls(), retired);
    assert!(!control.has_pending());
    assert_eq!(o.history_depths(), (0, 0));
}

#[path = "object_expression_motion_commit_history.rs"]
mod history_tests;

#[test]
fn ordinary_raw_and_no_holder_reads_complete_without_a_guest_or_history_change() {
    for no_holder in [false, true] {
        let mut doc = fixture("[frame,0]");
        if no_holder {
            doc.as_object_mut().unwrap().remove("propertyHolders");
        } else {
            doc["propertyHolders"][0]["properties"][0]
                .as_object_mut()
                .unwrap()
                .remove("expression");
        }
        let mut o = owner(doc);
        let s = o.acquire_snapshot(0).unwrap();
        let mut control = ObjectMotionFrameControl::default();
        let retired = calls();
        assert_eq!(
            x_values(&prepare(&o, &s, 2, &mut control).unwrap()),
            [20., 20., 30.],
        );
        assert_eq!(control.pending_frames(), [2]);
        let committed = finalize_object_motion_frame_effects_for_owner(
            &mut o,
            &pins(&s, 2),
            &[2],
            &mut control,
        )
        .unwrap();
        assert!(committed.frame_ready() && !committed.changed());
        assert_eq!(control.diagnostic_records(), json!([]));
        assert_eq!(calls(), retired);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn stale_empty_raw_receipts_retire_only_after_valid_current_pins_and_survive_fill_undo() {
    use crate::commands::ObjectFillOwner;
    for no_holder in [false, true] {
        let mut doc = ordinary_editable_source(fixture("[frame,0]"));
        if no_holder {
            doc.as_object_mut().unwrap().remove("propertyHolders");
        } else {
            doc["propertyHolders"][0]["properties"][0]
                .as_object_mut()
                .unwrap()
                .remove("expression");
        }
        let mut o = owner(doc);
        let old = o.acquire_snapshot(0).unwrap();
        let original = encode_storage_project(old.document()).unwrap();
        let mut control = ObjectMotionFrameControl::default();
        let entered = calls();
        assert_eq!(
            x_values(&prepare(&o, &old, 2, &mut control).unwrap()),
            [20., 20., 30.]
        );
        assert_eq!(control.pending_frames(), [2]);
        assert!(control.has_pending() && !control.requires_revision());
        let pending = format!("{control:?}");
        assert_eq!(
            x_values(&prepare(&o, &old, 2, &mut control).unwrap()),
            [20., 20., 30.]
        );
        assert_eq!(format!("{control:?}"), pending);
        let fill = o.prepare_fill_json(&serde_json::to_vec(&json!({
            "command":"object.fill.set",
            "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
                "layerUid":"Edit","strokeId":"editable"},
            "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1}
        })).unwrap()).unwrap();
        o.commit_fill(fill).unwrap();
        let current = o.acquire_snapshot(1).unwrap();
        let filled = encode_storage_project(current.document()).unwrap();
        assert_ne!(filled, original);
        for fault in ["snapshot", "context", "scope", "frame"] {
            let mut invalid = pins(&current, 2);
            match fault {
                "snapshot" => invalid.document_snapshot_id = "wrong",
                "context" => invalid.context_id = "missing",
                "scope" => invalid.scope_kind = crate::object_document::FrameScopeKind::Reference,
                _ => invalid.frame = u32::MAX,
            }
            assert!(prepare_object_motion_exposure_frame_for_control(
                &o,
                &current,
                &invalid,
                &mut control,
            )
            .is_err());
            assert_eq!(format!("{control:?}"), pending);
        }
        assert_eq!(calls(), entered);
        assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
        assert_eq!(encode_storage_project(current.document()).unwrap(), filled);
        assert_eq!(
            x_values(&prepare(&o, &current, 2, &mut control).unwrap()),
            [20., 20., 30.]
        );
        let committed = finalize_object_motion_frame_effects_for_owner(
            &mut o,
            &pins(&current, 2),
            &[2],
            &mut control,
        )
        .unwrap();
        assert_eq!(
            (committed.before_revision(), committed.content_revision()),
            (1, 1)
        );
        assert_eq!(committed.document_snapshot_id(), current.snapshot_id());
        assert!(committed.frame_ready() && !committed.changed());
        assert!(!control.has_pending());
        assert_eq!(control.diagnostic_records(), json!([]));
        assert_eq!(calls(), entered);
        assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
        o.undo(1).unwrap();
        let undone = o.acquire_snapshot(2).unwrap();
        assert_eq!(encode_storage_project(undone.document()).unwrap(), original);
        assert_eq!(
            x_values(&prepare(&o, &undone, 2, &mut control).unwrap()),
            [20., 20., 30.]
        );
        let restored = finalize_object_motion_frame_effects_for_owner(
            &mut o,
            &pins(&undone, 2),
            &[2],
            &mut control,
        )
        .unwrap();
        assert_eq!(
            (restored.before_revision(), restored.content_revision()),
            (2, 2)
        );
        assert_eq!(restored.document_snapshot_id(), undone.snapshot_id());
        assert!(restored.frame_ready() && !restored.changed());
        assert!(!control.has_pending());
        assert_eq!(control.diagnostic_records(), json!([]));
        assert_eq!(calls(), entered);
        assert_eq!((o.content_revision(), o.history_depths()), (2, (0, 1)));
        assert_eq!(encode_storage_project(old.document()).unwrap(), original);
        assert_eq!(encode_storage_project(current.document()).unwrap(), filled);
    }
}

#[test]
fn successful_nonempty_effect_receipts_remain_pending_after_an_actual_fill_source_change() {
    use crate::commands::ObjectFillOwner;
    let mut o = owner(ordinary_editable_source(fixture("[frame,0]")));
    let old = o.acquire_snapshot(0).unwrap();
    let original = encode_storage_project(old.document()).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    let entered = calls();
    assert_eq!(
        x_values(&prepare(&o, &old, 2, &mut control).unwrap()),
        [20., 22., 34.]
    );
    assert!(calls() > entered);
    let retired = calls();
    assert_eq!(control.pending_frames(), [2]);
    assert!(control.has_pending() && !control.requires_revision());
    let pending = format!("{control:?}");
    assert!(pending.contains("effects: [Evaluated {"));
    let fill = o
        .prepare_fill_json(
            &serde_json::to_vec(&json!({
                "command":"object.fill.set",
                "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
                    "layerUid":"Edit","strokeId":"editable"},
                "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1}
            }))
            .unwrap(),
        )
        .unwrap();
    o.commit_fill(fill).unwrap();
    let current = o.acquire_snapshot(1).unwrap();
    let filled = encode_storage_project(current.document()).unwrap();
    assert_ne!(filled, original);
    assert!(prepare(&o, &current, 2, &mut control).is_err());
    assert_eq!(format!("{control:?}"), pending);
    assert!(finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 2),
        &[2],
        &mut control,
    )
    .is_err());
    assert_eq!(format!("{control:?}"), pending);
    assert_eq!(control.pending_frames(), [2]);
    assert_eq!(control.diagnostic_records(), json!([]));
    assert_eq!(calls(), retired);
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    assert_eq!(encode_storage_project(old.document()).unwrap(), original);
    assert_eq!(encode_storage_project(current.document()).unwrap(), filled);
}
