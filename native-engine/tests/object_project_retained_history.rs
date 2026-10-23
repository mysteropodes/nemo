//! Retained activation moves complete histories; original replacement stays destructive.
use super::{attach, bytes, current, file, pin};
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

#[test]
fn retained_project_moves_keep_identity_history_replay_and_old_reads() {
    let mut active = NativeObjectHistory::from_project_file("tabs-engine", &file(64)).unwrap();
    attach(&mut active);
    let original = current(&active);
    let original_bytes = bytes(&active);
    let remove = OpacityRequest::history_stage(
        "remove-once",
        active.instance_id(),
        active.document_id(),
        Some(active.content_revision()),
        "command.document.reference.remove",
        json!({}),
    );
    let receipt =
        serde_json::to_value(active.dispatch_reference_command(&remove, None).unwrap()).unwrap();
    active.undo(active.content_revision()).unwrap();
    let a_origin = current(&active);
    let a_info = active.project_file_info().unwrap();
    let a_bytes = bytes(&active);
    let b = active
        .prepare_inactive_project(&pin(&active), &file(96))
        .unwrap();
    let b_info = b.metadata().unwrap();
    let b_pin = b_info["documentSnapshotId"].as_str().unwrap().to_owned();
    let mut incoming = Some(b);
    let mut parked = active
        .activate_inactive_project(&a_origin, &b_pin, &mut incoming)
        .unwrap();
    assert!(incoming.is_none());
    assert_eq!(active.document_id(), b_info["documentId"].as_str().unwrap());
    assert_eq!(pin(&active), b_pin);
    let mut expected_b: Value = serde_json::from_slice(&file(96)).unwrap();
    expected_b["project"]["fps"] = json!(24.0);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes(&active)).unwrap(),
        expected_b
    );
    assert_eq!(
        active.project_file_info().unwrap()["documentDigest"],
        b_info["documentDigest"]
    );
    assert_eq!(parked.project_file_info().unwrap(), a_info);
    attach(&mut active);
    let b_changed = active.project_file_info().unwrap();
    let b_origin = current(&active);
    let a_target = pin(&parked);
    active
        .swap_project_history(&b_origin, &a_target, &mut parked)
        .unwrap();
    assert_eq!(active.project_file_info().unwrap(), a_info);
    assert_eq!(bytes(&active), a_bytes);
    assert_eq!(original.serialize_project_file().unwrap(), original_bytes);
    assert_eq!(
        active
            .acquire_snapshot(original.content_revision())
            .unwrap()
            .serialize_project_file()
            .unwrap(),
        original_bytes
    );
    let replay =
        serde_json::to_value(active.dispatch_reference_command(&remove, None).unwrap()).unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(
        active.project_file_info().unwrap(),
        a_info,
        "replay cannot execute again"
    );
    active.redo(active.content_revision()).unwrap();
    let a_changed = active.project_file_info().unwrap();
    let a_origin = current(&active);
    let b_target = pin(&parked);
    active
        .swap_project_history(&a_origin, &b_target, &mut parked)
        .unwrap();
    assert_eq!(active.project_file_info().unwrap(), b_changed);
    let b_origin = current(&active);
    let a_target = pin(&parked);
    active
        .swap_project_history(&b_origin, &a_target, &mut parked)
        .unwrap();
    assert_eq!(active.project_file_info().unwrap(), a_changed);
    assert_eq!(parked.project_file_info().unwrap(), b_changed);
}

#[test]
fn retained_project_refusals_leave_both_values_and_incoming_slot_unchanged() {
    let mut active = NativeObjectHistory::from_project_file("tabs-refuse", &file(64)).unwrap();
    let stale = current(&active);
    let b = active
        .prepare_inactive_project(&pin(&active), &file(96))
        .unwrap();
    let b_info = b.metadata().unwrap();
    let target = b_info["documentSnapshotId"].as_str().unwrap().to_owned();
    let mut incoming = Some(b);
    attach(&mut active);
    let unchanged = active.project_file_info().unwrap();
    assert!(matches!(
        active.activate_inactive_project(&stale, &target, &mut incoming),
        Err(DispatchErrorCode::StaleRevision)
    ));
    let origin = current(&active);
    assert!(matches!(
        active.activate_inactive_project(&origin, "wrong-target", &mut incoming),
        Err(DispatchErrorCode::StaleRevision)
    ));
    for (instance, expected) in [
        ("foreign", DispatchErrorCode::WrongInstance),
        ("tabs-refuse", DispatchErrorCode::WrongDocument),
    ] {
        let mut foreign = NativeObjectHistory::from_project_file(instance, &file(80)).unwrap();
        let before = foreign.project_file_info().unwrap();
        assert!(
            matches!(foreign.activate_inactive_project(&origin, &target, &mut incoming), Err(code) if code == expected)
        );
        assert_eq!(foreign.project_file_info().unwrap(), before);
        assert_eq!(incoming.as_ref().unwrap().metadata().unwrap(), b_info);
    }
    assert_eq!(active.project_file_info().unwrap(), unchanged);
    let mut parked = active
        .activate_inactive_project(&origin, &target, &mut incoming)
        .unwrap();
    let active_before = active.project_file_info().unwrap();
    let parked_before = parked.project_file_info().unwrap();
    let origin = current(&active);
    assert_eq!(
        active.swap_project_history(&origin, "wrong-target", &mut parked),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert!(matches!(
        active.activate_inactive_project(&origin, &target, &mut incoming),
        Err(DispatchErrorCode::Unavailable)
    ));
    assert_eq!(active.project_file_info().unwrap(), active_before);
    assert_eq!(parked.project_file_info().unwrap(), parked_before);
}
