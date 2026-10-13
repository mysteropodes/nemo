//! Actual object-owner controls for the common scheduler and counted leases.
use super::*;
use crate::commands::NativeOpacityApplication;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_project, encode_project};
use crate::scheduler::{FrameFailure, FrameFailureKind, OutputSpec};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}

fn owner(instance: &str) -> NativeObjectHistory {
    let source = json!({"format":"nemo.native-object-document", "formatVersion":1,
        "totalFrames":21, "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases()["records"]});
    NativeObjectHistory::new(
        instance,
        decode_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

fn pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}

fn key(snapshot: &ObjectSnapshot, frame: u32) -> EvaluationKey {
    EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        frame,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        [
            ("contour-a".into(), "v1".into()),
            ("contour-b".into(), "v2".into()),
        ],
    )
    .unwrap()
}

fn schedule(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> ScheduledFrame {
    scheduler
        .schedule_object(
            snapshot.clone(),
            pins(snapshot, frame),
            key(snapshot, frame),
        )
        .unwrap()
}

fn assert_refused(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    pins: ObjectSchedulePins,
    key: EvaluationKey,
) {
    let before = (
        scheduler.next_generation,
        scheduler.next_work_id,
        scheduler.current_owner.clone(),
        scheduler.active_by_key.clone(),
        scheduler.frames.len(),
        scheduler.lease_counters(),
    );
    assert!(scheduler
        .schedule_object(snapshot.clone(), pins, key)
        .is_err());
    assert_eq!(
        before,
        (
            scheduler.next_generation,
            scheduler.next_work_id,
            scheduler.current_owner.clone(),
            scheduler.active_by_key.clone(),
            scheduler.frames.len(),
            scheduler.lease_counters()
        )
    );
}

#[test]
fn real_owner_admission_deduplicates_complete_structural_keys_and_retains_typed_snapshot() {
    let owner = owner("object-schedule");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let first = schedule(&mut scheduler, &snapshot, 0);
    assert_eq!(first.work_id().value(), 1);
    assert_eq!(first.view_generation().value(), 1);
    assert_eq!(first.admission(), ScheduleAdmission::Scheduled);
    let duplicate = schedule(&mut scheduler, &snapshot, 0);
    assert_eq!(duplicate.work_id(), first.work_id());
    assert_eq!(duplicate.admission(), ScheduleAdmission::Deduplicated);
    assert_eq!(scheduler.lease_counters().acquired(), 3);
    let pinned = scheduler.pinned_object_snapshot(first.work_id()).unwrap();
    assert_eq!(
        (
            pinned.instance_id(),
            pinned.document_id(),
            pinned.snapshot_id(),
            pinned.content_revision()
        ),
        (
            snapshot.instance_id(),
            snapshot.document_id(),
            snapshot.snapshot_id(),
            0
        )
    );
    assert!(scheduler.pinned_snapshot(first.work_id()).is_none());
    let mut reordered = key(&snapshot, 0);
    reordered.declared_resource_versions = [
        ("contour-b".into(), "v2".into()),
        ("contour-a".into(), "v1".into()),
    ]
    .into();
    assert_eq!(
        scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 0), reordered)
            .unwrap()
            .work_id(),
        first.work_id()
    );
    let changes: [fn(&mut EvaluationKey); 7] = [
        |key| key.quality = "draft".into(),
        |key| key.output_spec.kind = "tile".into(),
        |key| key.output_spec.format = "rgba16f".into(),
        |key| key.output_spec.width = 640,
        |key| key.output_spec.color_interpretation = "display-p3".into(),
        |key| key.output_spec.alpha_mode = "premultiplied".into(),
        |key| {
            key.declared_resource_versions
                .insert("contour-a".into(), "v3".into());
        },
    ];
    for change in changes {
        let mut variant = key(&snapshot, 0);
        change(&mut variant);
        let next = scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 0), variant)
            .unwrap();
        assert_ne!(next.work_id(), first.work_id());
        assert_eq!(next.admission(), ScheduleAdmission::Scheduled);
    }
    assert_eq!(scheduler.lease_counters().live(), 24);
    let last_valid = schedule(&mut scheduler, &snapshot, 20);
    assert_eq!(last_valid.key().frame(), 20);
    assert_refused(
        &mut scheduler,
        &snapshot,
        pins(&snapshot, 21),
        key(&snapshot, 21),
    );
    scheduler.replace_document("cleanup-opacity-owner").unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
    assert_eq!(
        scheduler.lease_counters().acquired(),
        scheduler.lease_counters().released()
    );
}

#[test]
fn every_pin_key_scope_and_output_refusal_is_atomic() {
    let snapshot = owner("object-negative").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let changes: [fn(&mut ObjectSchedulePins); 11] = [
        |p| p.instance_id.clear(),
        |p| p.instance_id = "foreign".into(),
        |p| p.document_id.clear(),
        |p| p.document_id = "foreign".into(),
        |p| p.content_revision = 1,
        |p| p.document_snapshot_id.clear(),
        |p| p.document_snapshot_id = "foreign".into(),
        |p| p.context_id.clear(),
        |p| p.context_id = "component".into(),
        |p| p.frame = 1,
        |p| p.scope = ObjectScheduleScope::Unsupported,
    ];
    for change in changes {
        let mut invalid = pins(&snapshot, 0);
        change(&mut invalid);
        assert_refused(&mut scheduler, &snapshot, invalid, key(&snapshot, 0));
    }
    let key_changes: [fn(&mut EvaluationKey); 10] = [
        |k| k.document_snapshot_id.clear(),
        |k| k.document_snapshot_id = "foreign".into(),
        |k| k.context_id.clear(),
        |k| k.context_id = "component".into(),
        |k| k.frame = 1,
        |k| k.quality.clear(),
        |k| k.output_spec.width = 0,
        |k| k.output_spec.height = 0,
        |k| k.output_spec.format.clear(),
        |k| {
            k.declared_resource_versions
                .insert("bad".into(), String::new());
        },
    ];
    for change in key_changes {
        let mut invalid = key(&snapshot, 0);
        change(&mut invalid);
        assert_refused(&mut scheduler, &snapshot, pins(&snapshot, 0), invalid);
    }
    assert_refused(
        &mut scheduler,
        &snapshot,
        pins(&snapshot, u32::MAX),
        key(&snapshot, u32::MAX),
    );
    assert_eq!(scheduler.lease_counters().acquired(), 0);
    let accepted = schedule(&mut scheduler, &snapshot, 0);
    for instance in ["object-negative", "another-instance"] {
        let other = owner(instance).acquire_snapshot(0).unwrap();
        assert_refused(&mut scheduler, &other, pins(&other, 0), key(&other, 0));
    }
    assert_eq!(
        scheduler
            .pinned_object_snapshot(accepted.work_id())
            .unwrap()
            .snapshot_id(),
        snapshot.snapshot_id()
    );
}

#[test]
fn real_revisions_remain_pinned_after_edit_undo_and_owner_drop() {
    let mut owner = owner("object-retention");
    let zero = owner.acquire_snapshot(0).unwrap();
    let original = encode_project(zero.document()).unwrap();
    let mut scheduler = FrameScheduler::new();
    let first = schedule(&mut scheduler, &zero, 0);
    let payload = cases()["command"]["request"]["payload"].clone();
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    let one = owner.acquire_snapshot(1).unwrap();
    let changed = encode_project(one.document()).unwrap();
    assert_ne!(original, changed);
    let second = schedule(&mut scheduler, &one, 20);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(
        encode_project(owner.acquire_snapshot(2).unwrap().document()).unwrap(),
        original
    );
    drop(owner);
    assert_eq!(
        encode_project(
            scheduler
                .pinned_object_snapshot(first.work_id())
                .unwrap()
                .document()
        )
        .unwrap(),
        original
    );
    assert_eq!(
        encode_project(
            scheduler
                .pinned_object_snapshot(second.work_id())
                .unwrap()
                .document()
        )
        .unwrap(),
        changed
    );
    let old_receipt = scheduler.succeed(first.work_id()).unwrap();
    assert_eq!(
        old_receipt.publication(),
        &PublicationDisposition::SuppressedStale {
            newest_generation: second.view_generation()
        }
    );
    assert_eq!(old_receipt.document_snapshot_id(), zero.snapshot_id());
    assert_eq!(old_receipt.document_id(), zero.document_id());
    let new_receipt = scheduler.succeed(second.work_id()).unwrap();
    assert_eq!(
        new_receipt.publication(),
        &PublicationDisposition::Published
    );
    assert!(scheduler.pinned_object_snapshot(first.work_id()).is_none());
    assert!(scheduler.pinned_object_snapshot(second.work_id()).is_none());
    assert_eq!(scheduler.succeed(first.work_id()).unwrap(), old_receipt);
    assert_eq!(scheduler.cancel(second.work_id()).unwrap(), new_receipt);
    assert_eq!(scheduler.lease_counters().live(), 0);
    assert_eq!(scheduler.lease_counters().released(), 6);
}

#[test]
fn replacement_checks_full_new_owner_and_late_work_cannot_publish_or_release_twice() {
    let a = owner("object-a").acquire_snapshot(0).unwrap();
    let b = owner("object-b").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let old = schedule(&mut scheduler, &a, 0);
    let before = scheduler.lease_counters();
    assert!(scheduler.replace_object_document(&a, &pins(&a, 0)).is_err());
    let mut wrong = pins(&b, 0);
    wrong.instance_id = a.instance_id().into();
    assert!(scheduler.replace_object_document(&b, &wrong).is_err());
    assert!(scheduler
        .replace_object_document(&b, &pins(&b, 21))
        .is_err());
    assert_eq!(scheduler.lease_counters(), before);
    assert!(scheduler.receipt(old.work_id()).is_none());
    let replaced = scheduler.replace_object_document(&b, &pins(&b, 0)).unwrap();
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0].document_id(), a.document_id());
    assert_eq!(
        replaced[0].terminal_status(),
        &FrameTerminalStatus::Replaced {
            old_document_id: a.document_id().into(),
            new_document_id: b.document_id().into(),
        }
    );
    assert_eq!(
        replaced[0].publication(),
        &PublicationDisposition::NotPublished
    );
    assert_eq!(scheduler.succeed(old.work_id()).unwrap(), replaced[0]);
    assert_refused(&mut scheduler, &a, pins(&a, 0), key(&a, 0));
    let new = schedule(&mut scheduler, &b, 0);
    assert!(new.work_id() > old.work_id());
    assert!(new.view_generation() > old.view_generation());
    let failure = FrameFailure::new(FrameFailureKind::Worker, "test worker failure");
    let failed = scheduler.fail(new.work_id(), failure.clone()).unwrap();
    assert_eq!(
        failed.terminal_status(),
        &FrameTerminalStatus::Failed { error: failure }
    );
    assert_eq!(failed.publication(), &PublicationDisposition::NotPublished);
    let retry = schedule(&mut scheduler, &b, 0);
    assert_ne!(retry.work_id(), new.work_id());
    assert_eq!(scheduler.succeed(new.work_id()).unwrap(), failed);
    assert_eq!(schedule(&mut scheduler, &b, 0).work_id(), retry.work_id());
    let cancelled = scheduler.cancel(retry.work_id()).unwrap();
    assert_eq!(cancelled.terminal_status(), &FrameTerminalStatus::Cancelled);
    assert_eq!(scheduler.succeed(retry.work_id()).unwrap(), cancelled);
    assert_eq!(scheduler.lease_counters().live(), 0);
    assert_eq!(scheduler.lease_counters().acquired(), 9);
    assert_eq!(scheduler.lease_counters().released(), 9);
}

#[test]
fn object_and_opacity_owner_types_do_not_alias_and_share_monotonic_sequences() {
    let object = owner("typed-object").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    scheduler.replace_document(object.document_id()).unwrap();
    assert_refused(&mut scheduler, &object, pins(&object, 0), key(&object, 0));
    scheduler
        .replace_object_document(&object, &pins(&object, 0))
        .unwrap();
    let object_work = schedule(&mut scheduler, &object, 0);
    let opacity = NativeOpacityApplication::new(
        "typed-opacity",
        crate::codec::decode_project(include_bytes!("fixtures/opacity-v2/project.json")).unwrap(),
    )
    .unwrap()
    .acquire_snapshot(0)
    .unwrap();
    let mut opacity_key = key(&object, 0);
    opacity_key.document_snapshot_id = opacity.id().into();
    assert!(scheduler
        .schedule(opacity.clone(), opacity_key.clone())
        .is_err());
    scheduler.replace_document(opacity.document_id()).unwrap();
    let opacity_work = scheduler.schedule(opacity.clone(), opacity_key).unwrap();
    assert!(opacity_work.work_id() > object_work.work_id());
    assert!(opacity_work.view_generation() > object_work.view_generation());
    assert!(scheduler
        .pinned_object_snapshot(opacity_work.work_id())
        .is_none());
    assert_eq!(
        scheduler
            .pinned_snapshot(opacity_work.work_id())
            .unwrap()
            .id(),
        opacity.id()
    );
    assert_refused(&mut scheduler, &object, pins(&object, 0), key(&object, 0));
    scheduler.cancel(opacity_work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().acquired(), 6);
    assert_eq!(scheduler.lease_counters().released(), 6);
}

#[test]
fn overflow_and_partial_acquisition_unwind_through_the_same_owner() {
    let snapshot = owner("object-overflow").acquire_snapshot(0).unwrap();
    for (generation, work, lease) in [
        (u64::MAX, 0, 0),
        (0, u64::MAX, 0),
        (0, 0, u64::MAX),
        (0, 0, u64::MAX - 1),
    ] {
        let mut scheduler = FrameScheduler::with_sequences(generation, work, lease);
        assert!(scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 0), key(&snapshot, 0))
            .is_err());
        assert_eq!(
            (scheduler.next_generation, scheduler.next_work_id),
            (generation, work)
        );
        assert!(scheduler.current_owner.is_none());
        assert!(scheduler.active_by_key.is_empty());
        assert!(scheduler.frames.is_empty());
        assert_eq!(scheduler.lease_counters().live(), 0);
        assert_eq!(
            scheduler.lease_counters().acquired(),
            scheduler.lease_counters().released()
        );
    }
    let mut scheduler = FrameScheduler::new();
    let existing = schedule(&mut scheduler, &snapshot, 0);
    let sequences = (scheduler.next_generation, scheduler.next_work_id);
    scheduler.inject_resource_failure_after(Some(1));
    assert!(scheduler
        .schedule_object(snapshot.clone(), pins(&snapshot, 1), key(&snapshot, 1))
        .is_err());
    assert_eq!(
        (scheduler.next_generation, scheduler.next_work_id),
        sequences
    );
    assert_eq!(scheduler.lease_counters().live(), 3);
    assert_eq!(scheduler.lease_counters().acquired(), 5);
    assert_eq!(scheduler.lease_counters().released(), 2);
    assert_eq!(
        scheduler
            .pinned_object_snapshot(existing.work_id())
            .unwrap()
            .snapshot_id(),
        snapshot.snapshot_id()
    );
    scheduler.inject_resource_failure_after(None);
    let next = schedule(&mut scheduler, &snapshot, 1);
    assert_eq!(next.work_id().value(), existing.work_id().value() + 1);
    scheduler.cancel(existing.work_id()).unwrap();
    scheduler.cancel(next.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
    assert_eq!(
        scheduler.lease_counters().acquired(),
        scheduler.lease_counters().released()
    );
}
