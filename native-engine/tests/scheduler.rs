use crate::codec::decode_project;
use crate::commands::{NativeOpacityApplication, OpacityRequest};
use crate::resource_leases::{LeaseError, ResourceLeaseManager};
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, FrameTerminalStatus, OutputSpec,
    PublicationDisposition, ScheduleAdmission, ScheduleError,
};
use serde_json::json;
use std::collections::BTreeSet;

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");

fn application(instance_id: &str) -> NativeOpacityApplication {
    NativeOpacityApplication::new(instance_id, decode_project(PROJECT).unwrap()).unwrap()
}

fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap()
}

fn key(
    snapshot: &crate::revision::DocumentSnapshot,
    context: &str,
    frame: u32,
    quality: &str,
    output_spec: OutputSpec,
    resources: &[(&str, &str)],
) -> EvaluationKey {
    EvaluationKey::new(
        snapshot.id(),
        context,
        frame,
        quality,
        output_spec,
        resources
            .iter()
            .map(|(id, version)| ((*id).to_owned(), (*version).to_owned())),
    )
    .unwrap()
}

#[test]
fn complete_key_deduplicates_only_identical_active_work_and_is_structural() {
    let mut app = application("scheduler-key");
    let revision_zero = app.acquire_snapshot(0).unwrap();
    let baseline = key(
        &revision_zero,
        "scene-root",
        10,
        "final",
        output(),
        &[("image-a", "v1"), ("font-a", "v3")],
    );
    let mut scheduler = FrameScheduler::new();
    let first = scheduler
        .schedule(revision_zero.clone(), baseline.clone())
        .unwrap();
    let duplicate = scheduler
        .schedule(revision_zero.clone(), baseline.clone())
        .unwrap();
    assert_eq!(first.work_id(), duplicate.work_id());
    assert_eq!(first.view_generation(), duplicate.view_generation());
    assert_eq!(duplicate.admission(), ScheduleAdmission::Deduplicated);
    assert_eq!(scheduler.lease_counters().acquired(), 3);

    let same_resources_reversed = key(
        &revision_zero,
        "scene-root",
        10,
        "final",
        output(),
        &[("font-a", "v3"), ("image-a", "v1")],
    );
    let canonical_duplicate = scheduler
        .schedule(revision_zero.clone(), same_resources_reversed)
        .unwrap();
    assert_eq!(
        canonical_duplicate.admission(),
        ScheduleAdmission::Deduplicated
    );
    assert_eq!(scheduler.lease_counters().acquired(), 3);

    let command = OpacityRequest::command(
        "scheduler-rev-one",
        app.instance_id(),
        app.document_id(),
        0,
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": "r08_curve_layer" },
            "value": 40
        }),
    );
    assert!(app.handle(command).is_ok());
    let revision_one = app.acquire_snapshot(1).unwrap();
    let variants = [
        key(
            &revision_zero,
            "component-a",
            10,
            "final",
            output(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            11,
            "final",
            output(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "draft",
            output(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("frame", "rgba16f", 320, 180, "srgb", "straight").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("frame", "rgba8", 320, 360, "srgb", "straight").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("tile", "rgba8", 320, 180, "srgb", "straight").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("frame", "rgba8", 640, 180, "srgb", "straight").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("frame", "rgba8", 320, 180, "display-p3", "straight").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "premultiplied").unwrap(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
        key(
            &revision_zero,
            "scene-root",
            10,
            "final",
            output(),
            &[("image-a", "v2"), ("font-a", "v3")],
        ),
        key(
            &revision_one,
            "scene-root",
            10,
            "final",
            output(),
            &[("image-a", "v1"), ("font-a", "v3")],
        ),
    ];
    for variant in variants {
        let scheduled = scheduler.schedule(revision_zero.clone(), variant.clone());
        if variant.document_snapshot_id() == revision_one.id() {
            assert!(matches!(
                scheduled,
                Err(ScheduleError::SnapshotKeyMismatch { .. })
            ));
            let scheduled = scheduler.schedule(revision_one.clone(), variant).unwrap();
            assert_eq!(scheduled.admission(), ScheduleAdmission::Scheduled);
        } else {
            let scheduled = scheduled.unwrap();
            assert_eq!(scheduled.admission(), ScheduleAdmission::Scheduled);
            assert_ne!(scheduled.work_id(), first.work_id());
        }
    }
    let counters = scheduler.lease_counters();
    assert_eq!(counters.live(), 3 * 12);
    scheduler.replace_document("scheduler-key-cleanup").unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (36, 36, 0)
    );
}

#[test]
fn retains_the_actual_old_snapshot_after_a_later_revision() {
    let mut app = application("scheduler-pin");
    let old = app.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let frame = scheduler
        .schedule(
            old.clone(),
            key(&old, "scene-root", 10, "final", output(), &[]),
        )
        .unwrap();
    let command = OpacityRequest::command(
        "scheduler-pin-commit",
        app.instance_id(),
        app.document_id(),
        0,
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": "r08_curve_layer" },
            "value": 40
        }),
    );
    assert!(app.handle(command).is_ok());
    assert_eq!(
        app.acquire_snapshot(1)
            .unwrap()
            .static_opacity("r08_curve_layer"),
        Some(40.0)
    );
    let pinned = scheduler.pinned_snapshot(frame.work_id()).unwrap();
    assert_eq!(pinned.id(), old.id());
    assert_eq!(pinned.static_opacity("r08_curve_layer"), Some(25.0));
    scheduler.succeed(frame.work_id()).unwrap();
    assert!(scheduler.pinned_snapshot(frame.work_id()).is_none());
}

#[test]
fn stale_success_is_suppressed_and_terminal_receipts_are_idempotent() {
    let app = application("scheduler-stale");
    let snapshot = app.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let older = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 0, "final", output(), &[]),
        )
        .unwrap();
    let newer = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 1, "final", output(), &[]),
        )
        .unwrap();
    assert!(newer.view_generation() > older.view_generation());
    let old_receipt = scheduler.succeed(older.work_id()).unwrap();
    assert_eq!(
        old_receipt.terminal_status(),
        &FrameTerminalStatus::Succeeded
    );
    assert_eq!(
        old_receipt.publication(),
        &PublicationDisposition::SuppressedStale {
            newest_generation: newer.view_generation()
        }
    );
    assert_eq!(scheduler.succeed(older.work_id()).unwrap(), old_receipt);
    let new_receipt = scheduler.succeed(newer.work_id()).unwrap();
    assert_eq!(
        new_receipt.terminal_status(),
        &FrameTerminalStatus::Succeeded
    );
    assert_eq!(
        new_receipt.publication(),
        &PublicationDisposition::Published
    );
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}

#[test]
fn failure_and_cancellation_release_once_without_publication() {
    let app = application("scheduler-terminal");
    let snapshot = app.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let failed = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 0, "final", output(), &[]),
        )
        .unwrap();
    let failure = FrameFailure::new(FrameFailureKind::Evaluation, "fixture evaluation failed");
    let failed_receipt = scheduler.fail(failed.work_id(), failure.clone()).unwrap();
    assert_eq!(
        failed_receipt.terminal_status(),
        &FrameTerminalStatus::Failed { error: failure }
    );
    assert_eq!(scheduler.cancel(failed.work_id()).unwrap(), failed_receipt);

    let cancelled = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 1, "final", output(), &[]),
        )
        .unwrap();
    let cancelled_receipt = scheduler.cancel(cancelled.work_id()).unwrap();
    assert_eq!(
        cancelled_receipt.terminal_status(),
        &FrameTerminalStatus::Cancelled
    );
    assert_eq!(
        cancelled_receipt.publication(),
        &PublicationDisposition::NotPublished
    );
    assert_eq!(
        scheduler.cancel(cancelled.work_id()).unwrap(),
        cancelled_receipt
    );
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}

#[test]
fn replacement_is_identity_typed_clears_dedup_and_rejects_late_old_publication() {
    let old_app = application("scheduler-old");
    let old_snapshot = old_app.acquire_snapshot(0).unwrap();
    let new_app = application("scheduler-new");
    let new_snapshot = new_app.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let old_work = scheduler
        .schedule(
            old_snapshot.clone(),
            key(
                &old_snapshot,
                "scene-root",
                0,
                "final",
                output(),
                &[("old", "1")],
            ),
        )
        .unwrap();
    assert!(scheduler
        .replace_document(old_snapshot.document_id())
        .is_err());
    let replacements = scheduler
        .replace_document(new_snapshot.document_id())
        .unwrap();
    assert_eq!(
        replacements[0].terminal_status(),
        &FrameTerminalStatus::Replaced {
            old_document_id: old_snapshot.document_id().to_owned(),
            new_document_id: new_snapshot.document_id().to_owned(),
        }
    );
    assert_eq!(
        replacements[0].publication(),
        &PublicationDisposition::NotPublished
    );
    assert_eq!(
        scheduler.succeed(old_work.work_id()).unwrap(),
        replacements[0]
    );
    assert!(matches!(
        scheduler.schedule(
            old_snapshot.clone(),
            key(
                &old_snapshot,
                "scene-root",
                0,
                "final",
                output(),
                &[("old", "1")]
            )
        ),
        Err(ScheduleError::WrongDocument { .. })
    ));
    let new_work = scheduler
        .schedule(
            new_snapshot.clone(),
            key(
                &new_snapshot,
                "scene-root",
                0,
                "final",
                output(),
                &[("old", "1")],
            ),
        )
        .unwrap();
    assert_ne!(new_work.work_id(), old_work.work_id());
    assert_eq!(
        scheduler.succeed(new_work.work_id()).unwrap().publication(),
        &PublicationDisposition::Published
    );
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (4, 4, 0)
    );
}

#[test]
fn invalid_keys_overflow_and_partial_acquisition_never_leak() {
    assert!(OutputSpec::new("", "rgba8", 1, 1, "srgb", "straight").is_err());
    let app = application("scheduler-negative");
    let snapshot = app.acquire_snapshot(0).unwrap();
    assert!(matches!(
        EvaluationKey::new(
            snapshot.id(),
            "scene-root",
            0,
            "final",
            output(),
            [
                ("same".to_owned(), "1".to_owned()),
                ("same".to_owned(), "2".to_owned())
            ]
        ),
        Err(ScheduleError::InvalidKey(
            "declared resource IDs must be unique"
        ))
    ));
    let key = key(
        &snapshot,
        "scene-root",
        0,
        "final",
        output(),
        &[("a", "1"), ("b", "2")],
    );
    let mut partial = FrameScheduler::new();
    partial.inject_resource_failure_after(Some(1));
    assert!(matches!(
        partial.schedule(snapshot.clone(), key.clone()),
        Err(ScheduleError::Lease(
            LeaseError::InjectedResourceFailure { .. }
        ))
    ));
    let counters = partial.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );

    let mut generation_overflow = FrameScheduler::with_sequences(u64::MAX, 0, 0);
    assert!(matches!(
        generation_overflow.schedule(snapshot.clone(), key.clone()),
        Err(ScheduleError::GenerationExhausted)
    ));
    assert_eq!(generation_overflow.lease_counters().live(), 0);
    let mut work_overflow = FrameScheduler::with_sequences(0, u64::MAX, 0);
    assert!(matches!(
        work_overflow.schedule(snapshot.clone(), key.clone()),
        Err(ScheduleError::WorkIdExhausted)
    ));
    assert_eq!(work_overflow.lease_counters().live(), 0);
    let mut lease_overflow = FrameScheduler::with_sequences(0, 0, u64::MAX);
    assert!(matches!(
        lease_overflow.schedule(snapshot, key),
        Err(ScheduleError::Lease(LeaseError::IdExhausted))
    ));
    assert_eq!(lease_overflow.lease_counters().live(), 0);
}

#[test]
fn lease_ids_are_never_reused_and_direct_double_release_is_detected() {
    let app = application("scheduler-lease-ids");
    let snapshot = app.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let first = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 0, "final", output(), &[("a", "1")]),
        )
        .unwrap();
    let first_ids = scheduler.lease_ids(first.work_id()).unwrap();
    scheduler.cancel(first.work_id()).unwrap();
    let second = scheduler
        .schedule(
            snapshot.clone(),
            key(&snapshot, "scene-root", 1, "final", output(), &[("a", "1")]),
        )
        .unwrap();
    let second_ids = scheduler.lease_ids(second.work_id()).unwrap();
    let all: BTreeSet<_> = [first_ids.0, first_ids.1[0], second_ids.0, second_ids.1[0]]
        .into_iter()
        .collect();
    assert_eq!(all.len(), 4);
    scheduler.cancel(second.work_id()).unwrap();

    let mut manager = ResourceLeaseManager::new();
    let bundle = manager.acquire(snapshot, Vec::<String>::new()).unwrap();
    let id = bundle.snapshot_lease_id();
    assert!(manager.release_id_for_test(id).is_ok());
    assert_eq!(
        manager.release_id_for_test(id),
        Err(LeaseError::UnknownLease(id))
    );
}
