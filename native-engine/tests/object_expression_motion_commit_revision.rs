//! Current-revision value proof and finalized owner-ledger lifetime. ALL UNRUN.
use crate::commands::ObjectFillOwner;
use crate::object_codec::encode_storage_project;
use crate::object_document::Presence;
use crate::object_expression_runtime::motion_frame::tests::commit_tests::{
    calls, diagnostic, prepare, target,
};
use crate::object_expression_runtime::motion_frame::tests::{
    add_effector, fixture, owner, pins, x_values,
};
use crate::object_expression_runtime::{
    finalize_object_motion_frame_effects_for_owner,
    prepare_object_motion_fallback_frame_for_control, ObjectMotionFrameControl,
};
use serde_json::{json, Value};

#[cfg(feature = "export_job")]
#[test]
fn mixed_frame_timeout_retires_later_success_instead_of_rebinding_it_to_disabled_source() {
    let mut doc = fixture("if(frame === 0){while(true){}} return [100,0];");
    doc["propertyHolders"][0]["properties"][0]["static"] = json!([2, 0]);
    doc["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]["timeOffset"]
        ["enabled"] = json!(false);
    let mut o = owner(doc);
    let old = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &old, 0, &mut control).is_err());
    let later = prepare(&o, &old, 1, &mut control).unwrap();
    // Original grid spacing10 plus per-copy100 on held source x10.
    assert_eq!(x_values(&later), [10., 120., 230.]);
    let retired = calls();
    let commit = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&old, 0),
        &[0, 1],
        &mut control,
    )
    .unwrap();
    assert!(commit.changed() && !commit.frame_ready());
    assert_eq!(
        calls(),
        retired,
        "the finalizer must not execute either frame again"
    );
    assert!(!control.has_pending());
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let current = o.acquire_snapshot(1).unwrap();
    assert_eq!(
        diagnostic(&control, &current, "L0", "dupOffsetPos", 0.),
        Some(json!({"kind":"timeout"})),
    );
    assert_eq!(
        diagnostic(&control, &current, "L0", "dupOffsetPos", 1.),
        None
    );
    assert!(
        prepare_object_motion_fallback_frame_for_control(
            &o,
            &current,
            &pins(&current, 1),
            &control
        )
        .is_err(),
        "no actual R2 read has produced this frame",
    );
    let retry = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 0),
        &[0, 1],
        &mut control,
    )
    .unwrap();
    assert!(!retry.changed() && !retry.frame_ready());
    // Now disabled: the actual current-owner read uses raw2, not old success100.
    assert_eq!(
        x_values(&prepare(&o, &current, 1, &mut control).unwrap()),
        [10., 22., 34.],
    );
    assert_eq!(calls(), retired, "this disabled property needs no new VM");
    let fresh = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 1),
        &[1],
        &mut control,
    )
    .unwrap();
    assert!(!fresh.changed() && fresh.frame_ready());
    assert_eq!(
        diagnostic(&control, &current, "L0", "dupOffsetPos", 0.),
        Some(json!({"kind":"timeout"})),
        "current-source timeout diagnostics survive finalized-ledger retirement",
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}

#[cfg(feature = "export_job")]
#[test]
fn a_successful_outer_value_is_recomputed_after_its_nested_dependency_is_disabled() {
    let mut doc = fixture("[layer('C').opacity,0]");
    doc["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]["timeOffset"]
        ["enabled"] = json!(false);
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[2],"expression":{"enabled":true,
            "code":"if(frame === 0){while(true){}} return 100;"}}]),
    );
    let mut o = owner(doc);
    let old = o.acquire_snapshot(0).unwrap();
    let mut control = ObjectMotionFrameControl::default();
    assert!(prepare(&o, &old, 0, &mut control).is_err());
    assert_eq!(
        x_values(&prepare(&o, &old, 1, &mut control).unwrap()),
        [10., 120., 230.],
    );
    let retired = calls();
    let commit = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&old, 0),
        &[0, 1],
        &mut control,
    )
    .unwrap();
    assert!(commit.changed() && !commit.frame_ready());
    assert_eq!(calls(), retired);
    assert!(!control.has_pending());
    let current = o.acquire_snapshot(1).unwrap();
    let stored: Value =
        serde_json::from_slice(&encode_storage_project(current.document()).unwrap()).unwrap();
    assert_eq!(
        stored["propertyHolders"][0]["properties"][0]["expression"]["enabled"],
        true
    );
    assert_eq!(
        stored["propertyHolders"][1]["properties"][0]["expression"]["enabled"],
        false
    );
    assert_eq!(
        diagnostic(&control, &current, "L0", "dupOffsetPos", 1.),
        None
    );
    assert_eq!(
        diagnostic(&control, &current, "C", "opacity", 0.),
        Some(json!({"kind":"timeout"})),
    );
    assert!(prepare_object_motion_fallback_frame_for_control(
        &o,
        &current,
        &pins(&current, 1),
        &control
    )
    .is_err(),);
    assert_eq!(
        x_values(&prepare(&o, &current, 1, &mut control).unwrap()),
        [10., 22., 34.],
    );
    assert!(
        calls() > retired,
        "the still-enabled outer expression uses C's current raw2"
    );
    let fresh_calls = calls();
    let fresh = finalize_object_motion_frame_effects_for_owner(
        &mut o,
        &pins(&current, 1),
        &[1],
        &mut control,
    )
    .unwrap();
    assert!(!fresh.changed() && fresh.frame_ready());
    assert_eq!(calls(), fresh_calls);
    assert_eq!(
        diagnostic(&control, &current, "L0", "dupOffsetPos", 1.),
        Some(Value::Null)
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}

#[test]
fn cleared_finalized_expressions_release_obsolete_ledgers_without_touching_a_parked_owner() {
    let mut parked = owner(fixture("'parked'"));
    let parked_snapshot = parked.acquire_snapshot(0).unwrap();
    let mut parked_control = ObjectMotionFrameControl::default();
    assert!(prepare(&parked, &parked_snapshot, 2, &mut parked_control).is_err());
    finalize_object_motion_frame_effects_for_owner(
        &mut parked,
        &pins(&parked_snapshot, 2),
        &[2],
        &mut parked_control,
    )
    .unwrap();
    let parked_records = parked_control.diagnostic_records();
    let mut doc = fixture("[frame,0]");
    doc["propertyHolders"][0]["properties"][0]
        .as_object_mut()
        .unwrap()
        .remove("expression");
    let targets = ["D0", "D1", "D2", "D3"];
    for uid in targets {
        add_effector(
            &mut doc,
            uid,
            "L0",
            json!([
            {"property":"effOffOpacity","expression":{"enabled":true,"code":"'bad'"}}]),
        );
    }
    let mut o = owner(doc);
    let mut control = ObjectMotionFrameControl::default();
    let first = o.acquire_snapshot(0).unwrap();
    assert!(prepare(&o, &first, 2, &mut control).is_err());
    finalize_object_motion_frame_effects_for_owner(&mut o, &pins(&first, 2), &[2], &mut control)
        .unwrap();
    assert_eq!(control.diagnostic_records().as_array().unwrap().len(), 4);
    for (index, uid) in targets.into_iter().enumerate() {
        let before = o.acquire_snapshot(o.content_revision()).unwrap();
        let key = target(uid);
        let mut replacement = before
            .document()
            .property_holders()
            .unwrap()
            .iter()
            .find(|holder| holder.target() == &key)
            .unwrap()
            .properties()
            .iter()
            .find(|property| property.property() == "effOffOpacity")
            .unwrap()
            .clone();
        replacement.expression = Presence::Missing;
        replacement.static_value = Presence::Present(vec![0.into()]);
        let edit = ObjectFillOwner::prepare_baked_property(before, &key, replacement).unwrap();
        o.commit_property_edit(edit).unwrap();
        let current = o.acquire_snapshot(o.content_revision()).unwrap();
        let prepared = prepare(&o, &current, 2, &mut control);
        assert_eq!(prepared.is_ok(), index == targets.len() - 1);
        // Source changes retire finalized old snapshots even when expressions
        // are no longer visited. The newly pending reports are not discarded.
        assert_eq!(control.diagnostic_records(), json!([]));
        assert_eq!(control.pending_frames(), [2]);
        let retired = calls();
        let commit = finalize_object_motion_frame_effects_for_owner(
            &mut o,
            &pins(&current, 2),
            &[2],
            &mut control,
        )
        .unwrap();
        assert!(!commit.changed() && commit.frame_ready());
        assert_eq!(calls(), retired);
        let records = control.diagnostic_records();
        assert_eq!(records.as_array().unwrap().len(), targets.len() - index - 1);
        assert!(records
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["documentSnapshotId"] == current.snapshot_id()));
        assert_eq!(parked_control.diagnostic_records(), parked_records);
        assert_eq!(parked.history_depths(), (0, 0));
    }
    assert_eq!(control.diagnostic_records(), json!([]));
    assert_eq!((o.content_revision(), o.history_depths()), (4, (4, 0)));
}
