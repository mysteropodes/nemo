//! Publication matrix on one scheduled fill frame: cancel, supersede, replace,
//! fail and success, occlusion/resume, foreign scheduler/GPU/extent. Every
//! refusal keeps lease counters balanced and never reaches the surface.
use super::viewport_scene_support::{self as support, Step};
use super::ObjectViewportFrame;
use crate::compositor::{
    object_test_plan as plan, object_test_selector as selector, schedule_test_object as schedule,
    Compositor,
};
use crate::desktop_viewport::{
    DeferredAction, DesktopViewportHost, PhysicalExtent, PresentationReceipt, ViewportError,
    ViewportStatus,
};
use crate::object_render_scene::{
    prepare_scheduled_object_composition as prepare, ScheduledObjectSceneError,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    FrameFailure, FrameFailureKind, FrameScheduler, FrameTerminalStatus, PublicationDisposition,
    ScheduledFrame,
};

#[derive(Debug, Clone, Copy)]
enum Cause {
    Cancel,
    Supersede,
    Replace,
    Fail,
    Succeed,
}
const CAUSES: [Cause; 5] = [
    Cause::Cancel,
    Cause::Supersede,
    Cause::Replace,
    Cause::Fail,
    Cause::Succeed,
];

/// Apply one cause to `old` (frame 7), assert the exact scheduler receipt, and
/// schedule the next current frame 0 on whichever document is now current.
fn apply(
    cause: Cause,
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    replacement: &ObjectSnapshot,
    old: &ScheduledFrame,
) -> (ScheduledFrame, ObjectSnapshot) {
    let id = old.work_id();
    match cause {
        Cause::Cancel => {
            let receipt = scheduler.cancel(id).unwrap();
            assert_eq!(receipt.terminal_status(), &FrameTerminalStatus::Cancelled);
            assert_eq!(receipt.publication(), &PublicationDisposition::NotPublished);
            (schedule(scheduler, snapshot, 0), snapshot.clone())
        }
        Cause::Supersede => {
            let next = schedule(scheduler, snapshot, 0);
            assert!(next.view_generation() > old.view_generation());
            assert!(
                scheduler.receipt(id).is_none(),
                "superseded work is not terminal yet"
            );
            let receipt = scheduler.succeed(id).unwrap();
            assert_eq!(receipt.terminal_status(), &FrameTerminalStatus::Succeeded);
            assert_eq!(
                receipt.publication(),
                &PublicationDisposition::SuppressedStale {
                    newest_generation: next.view_generation()
                }
            );
            (next, snapshot.clone())
        }
        Cause::Replace => {
            let receipts = scheduler
                .replace_object_document(replacement, &support::pins(replacement, 0))
                .unwrap();
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].work_id(), id);
            assert_eq!(
                receipts[0].terminal_status(),
                &FrameTerminalStatus::Replaced {
                    old_document_id: snapshot.document_id().into(),
                    new_document_id: replacement.document_id().into(),
                }
            );
            assert_eq!(
                receipts[0].publication(),
                &PublicationDisposition::NotPublished
            );
            (schedule(scheduler, replacement, 0), replacement.clone())
        }
        Cause::Fail => {
            let error = FrameFailure::new(FrameFailureKind::Worker, "refused");
            let receipt = scheduler.fail(id, error.clone()).unwrap();
            assert_eq!(
                receipt.terminal_status(),
                &FrameTerminalStatus::Failed { error }
            );
            assert_eq!(receipt.publication(), &PublicationDisposition::NotPublished);
            (schedule(scheduler, snapshot, 0), snapshot.clone())
        }
        Cause::Succeed => {
            let receipt = scheduler.succeed(id).unwrap();
            assert_eq!(receipt.publication(), &PublicationDisposition::Published);
            (schedule(scheduler, snapshot, 0), snapshot.clone())
        }
    }
}

fn assert_balanced(scheduler: &FrameScheduler, works: u64, label: &str) {
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (works, works, 0),
        "{label}"
    );
}

#[test]
fn cpu_viewport_scene_each_stale_cause_refuses_the_permit_with_exact_receipts_and_balanced_leases()
{
    for cause in CAUSES {
        let snapshot = support::scene_owner("viewport-scene-cpu", 0.5)
            .acquire_snapshot(0)
            .unwrap();
        let replacement = support::scene_owner("viewport-scene-cpu-new", 0.5)
            .acquire_snapshot(0)
            .unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let old = schedule(&mut scheduler, &snapshot, 7);
        assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_ok());
        let (next, current) = apply(cause, &mut scheduler, &snapshot, &replacement, &old);
        assert!(
            matches!(
                prepare(&scheduler, &old, &input, &selector(&snapshot, 7)),
                Err(ScheduledObjectSceneError::Inactive)
            ),
            "{cause:?}"
        );
        let live = prepare(
            &scheduler,
            &next,
            &plan(&current, 0),
            &selector(&current, 0),
        )
        .unwrap();
        assert_ne!(
            live.identity(),
            &ScheduledFrameIdentity::from_scheduled(&old)
        );
        drop(live);
        scheduler.cancel(next.work_id()).unwrap();
        assert_balanced(&scheduler, 2, &format!("{cause:?}"));
    }
}

#[test]
fn gpu_viewport_scene_occluded_frame_is_refused_by_every_stale_cause_without_reaching_the_surface()
{
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    for cause in CAUSES {
        let snapshot = support::scene_owner("viewport-scene-stale", 0.5)
            .acquire_snapshot(0)
            .unwrap();
        let replacement = support::scene_owner("viewport-scene-stale-new", 0.5)
            .acquire_snapshot(0)
            .unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let old = schedule(&mut scheduler, &snapshot, 7);
        let (frame, mut host) = {
            let live = prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).unwrap();
            let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
            let mut host = support::host_for(&gpu, &frame, &live, &[Step::Occluded]);
            let first = frame.present(&mut host, &gpu, &live).unwrap();
            assert_eq!(
                first.status(),
                &ViewportStatus::Deferred(DeferredAction::Occluded)
            );
            (frame, host)
        };
        assert_eq!(host.pending_work_ids(), vec![old.work_id()]);
        let (next, current) = apply(cause, &mut scheduler, &snapshot, &replacement, &old);
        assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_err());
        let live = prepare(
            &scheduler,
            &next,
            &plan(&current, 0),
            &selector(&current, 0),
        )
        .unwrap();
        // A permit for the current work cannot retire the retained old frame either.
        assert_eq!(
            frame.present(&mut host, &gpu, &live),
            Err(ViewportError::IdentityMismatch(old.work_id())),
            "{cause:?}"
        );
        let port = host.test_port();
        support::assert_no_surface_call(1, port.calls, &format!("{cause:?}"));
        assert_eq!(
            (port.frames, port.settlements, port.captured.len()),
            (0, 0, 0)
        );
        // No receipt was ever produced for the old identity: it is still pending
        // until the host is told, and only then retired.
        assert_eq!(host.pending_work_ids(), vec![old.work_id()]);
        assert_eq!(
            host.reconcile_replaced(&[old.work_id()]),
            vec![old.work_id()]
        );
        assert!(host.pending_work_ids().is_empty());
        drop(live);
        scheduler.cancel(next.work_id()).unwrap();
        assert_balanced(&scheduler, 2, &format!("{cause:?}"));
    }
}

#[test]
fn gpu_viewport_scene_deferred_frame_presents_on_retry_while_current_then_is_idempotent() {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let snapshot = support::scene_owner("viewport-scene-resume", 0.5)
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    let bytes = gpu
        .readback_scheduled_object_rgba8(&frame.result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec();
    let mut host = support::host_for(&gpu, &frame, &live, &[Step::Timeout, Step::Occluded]);
    for expected in [DeferredAction::Timeout, DeferredAction::Occluded] {
        let receipt = frame.present(&mut host, &gpu, &live).unwrap();
        assert_eq!(receipt.status(), &ViewportStatus::Deferred(expected));
        assert_eq!(host.pending_work_ids(), vec![work.work_id()]);
        assert_eq!(host.test_port().captured.len(), 0);
    }
    let presented = frame.present(&mut host, &gpu, &live).unwrap();
    assert_eq!(presented.status(), &ViewportStatus::Presented);
    assert_eq!(presented.identity(), frame.identity());
    assert_eq!(frame.present(&mut host, &gpu, &live).unwrap(), presented);
    let port = host.test_port();
    assert_eq!((port.calls, port.frames, port.settlements), (3, 1, 1));
    support::assert_same_bytes("resumed frame", &port.captured[0], &bytes);
    assert!(host.pending_work_ids().is_empty());
    drop(live);
    let receipt = scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(receipt.publication(), &PublicationDisposition::Published);
    assert_balanced(&scheduler, 1, "resume");
}

#[test]
fn gpu_viewport_scene_host_generation_fence_discards_a_deferred_frame_without_reading_its_texture()
{
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let snapshot = support::scene_owner("viewport-scene-fence", 0.5)
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let old = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).unwrap();
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    let mut host = support::host_for(&gpu, &frame, &live, &[Step::Occluded]);
    let first = frame.present(&mut host, &gpu, &live).unwrap();
    assert_eq!(
        first.status(),
        &ViewportStatus::Deferred(DeferredAction::Occluded)
    );
    // The view moved on: the host learns a newer generation (from a separate
    // scheduler sequence, since one scheduler would already refuse the old permit).
    let mut newer = FrameScheduler::with_sequences(5, 50, 500);
    let newest = schedule(&mut newer, &snapshot, 0);
    assert!(newest.view_generation() > old.view_generation());
    assert_ne!(newest.work_id(), old.work_id());
    host.register(ScheduledFrameIdentity::from_scheduled(&newest))
        .unwrap();
    let discarded = frame.present(&mut host, &gpu, &live).unwrap();
    assert_eq!(discarded.status(), &ViewportStatus::StaleDiscarded);
    assert_eq!(discarded.identity(), frame.identity());
    assert_eq!(frame.present(&mut host, &gpu, &live).unwrap(), discarded);
    let port = host.test_port();
    support::assert_no_surface_call(1, port.calls, "generation fence");
    assert_eq!(
        (port.frames, port.settlements, port.captured.len()),
        (0, 0, 0)
    );
    assert_eq!(host.pending_work_ids(), vec![newest.work_id()]);
    drop(live);
    scheduler.succeed(old.work_id()).unwrap();
    newer.cancel(newest.work_id()).unwrap();
    assert_balanced(&scheduler, 1, "fence old");
    assert_balanced(&newer, 1, "fence newer");
}

#[test]
fn gpu_viewport_scene_foreign_scheduler_gpu_and_extent_are_refused_before_the_surface() {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let other_gpu = Compositor::new().unwrap();
    let snapshot = support::scene_owner("viewport-scene-foreign", 0.5)
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    let mut host = support::host_for(&gpu, &frame, &live, &[]);
    let mut foreign_scheduler = FrameScheduler::new();
    let foreign_work = schedule(&mut foreign_scheduler, &snapshot, 7);
    assert_eq!(foreign_work.work_id(), work.work_id());
    assert_eq!(foreign_work.view_generation(), work.view_generation());
    let foreign = prepare(
        &foreign_scheduler,
        &foreign_work,
        &input,
        &selector(&snapshot, 7),
    )
    .unwrap();
    let mismatch =
        || Err::<PresentationReceipt, _>(ViewportError::IdentityMismatch(work.work_id()));
    assert_eq!(frame.present(&mut host, &gpu, &foreign), mismatch());
    assert_eq!(frame.present(&mut host, &other_gpu, &live), mismatch());
    let mut narrow = DesktopViewportHost::new(
        support::CapturePort::new(&gpu, &frame, &live),
        support::mapping(32),
    );
    narrow.register(frame.identity().clone()).unwrap();
    assert_eq!(
        frame.present(&mut narrow, &gpu, &live),
        Err(ViewportError::CompositionExtentMismatch {
            expected: PhysicalExtent {
                width: 32,
                height: 32
            },
            actual: PhysicalExtent {
                width: 64,
                height: 64
            },
        })
    );
    support::assert_no_surface_call(0, host.test_port().calls, "foreign scheduler or gpu");
    support::assert_no_surface_call(0, narrow.test_port().calls, "wrong extent");
    // The refusals did not poison the host: the matching permit still presents.
    let receipt = frame.present(&mut host, &gpu, &live).unwrap();
    assert_eq!(receipt.status(), &ViewportStatus::Presented);
    assert_eq!(host.test_port().calls, 1);
    drop(live);
    drop(foreign);
    scheduler.succeed(work.work_id()).unwrap();
    foreign_scheduler.cancel(foreign_work.work_id()).unwrap();
    assert_balanced(&scheduler, 1, "foreign primary");
    assert_balanced(&foreign_scheduler, 1, "foreign secondary");
}
