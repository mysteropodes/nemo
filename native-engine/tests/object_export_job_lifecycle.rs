//! Instrumented port effects, late work and real headless encoder boundaries.
use super::tests::{request, witness, Port, RejectEncoder};
use super::*;
use crate::compositor::{object_test_owner as owner, Compositor};
use crate::object_render_scene::LiveObjectComposition;
use crate::png_output::encode_scheduled_object_png;
use crate::scheduler::ObjectScheduleScope;

#[test]
fn gpu_exact_png_budget_passes_and_short_budgets_never_write_or_publish() {
    let original = owner("object-export-budget");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().expect("D3C2A requires real headless Metal");
    let mut length = 0;
    for case in 0..4 {
        let budget = match case {
            0 => 65536,
            1 => length,
            2 => length - 1,
            _ => 1,
        };
        let mut port = Port::default();
        let mut scheduler = FrameScheduler::new();
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        job.begin(&original, request(&snapshot, budget), witness(&snapshot))
            .unwrap();
        let pending = job.start_frame(&mut gpu).unwrap();
        if case < 2 {
            let result = job.finish_frame(pending.unwrap()).unwrap();
            assert_eq!(result.status, JobStatus::Succeeded);
            length = job.port.bytes.len();
            assert!(length <= budget);
        } else {
            assert!(pending.is_none());
            assert_eq!(job.status().unwrap().status, JobStatus::Failed);
            assert_eq!(job.port.events, ["begin", "cleanup"]);
            assert!(!job.port.published);
        }
        assert_eq!(job.scheduler.lease_counters().live(), 0);
    }
}
struct WrongEncoder<'a> {
    gpu: &'a mut Compositor,
    foreign: &'a Compositor,
    ignore_budget: bool,
}
impl ObjectPngEncoder for WrongEncoder<'_> {
    fn encode_object_png(
        &mut self,
        live: &LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectPng, String> {
        let result = self
            .gpu
            .compose_scheduled_object(live)
            .map_err(|e| e.to_string())?;
        encode_scheduled_object_png(
            if self.ignore_budget {
                self.gpu
            } else {
                self.foreign
            },
            &result,
            live,
            65536,
        )
        .map_err(|e| e.to_string())
    }
}
#[test]
fn gpu_foreign_device_ignored_encoder_budget_and_oversized_output_refuse() {
    let original = owner("object-export-device");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().unwrap();
    let foreign = Compositor::new().unwrap();
    for case in 0..3 {
        let mut port = Port::default();
        let mut scheduler = FrameScheduler::new();
        let mut input = request(&snapshot, if case == 1 { 1 } else { 65536 });
        if case == 2 {
            input.output =
                OutputSpec::new("frame", "rgba8", u32::MAX, 64, "srgb", "straight").unwrap();
        }
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        job.begin(&original, input, witness(&snapshot)).unwrap();
        let mut encoder = WrongEncoder {
            gpu: &mut gpu,
            foreign: &foreign,
            ignore_budget: case != 0,
        };
        assert!(job.start_frame(&mut encoder).unwrap().is_none());
        assert_eq!(job.status().unwrap().status, JobStatus::Failed);
        assert_eq!(job.port.events, ["begin", "cleanup"]);
        assert!(!job.port.published);
        assert_eq!(job.scheduler.lease_counters().live(), 0);
    }
}

#[test]
fn cpu_cleanup_debt_survives_and_retries_without_rearming_work() {
    let original = owner("object-export-debt");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut port = Port {
        fail_cleanup: true,
        ..Port::default()
    };
    let mut scheduler = FrameScheduler::new();
    let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
    job.begin(&original, request(&snapshot, 65536), witness(&snapshot))
        .unwrap();
    assert!(job.start_frame(&mut RejectEncoder).unwrap().is_none());
    let failure = job.status().unwrap();
    assert_eq!(failure.status, JobStatus::Failed);
    assert_eq!(failure.cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        failure.external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert!(!job.reconcile_release().cleanup_complete);
    assert!(job.port.staged);
    job.port.fail_cleanup = false;
    let settled = job.reconcile_release();
    assert!(settled.cleanup_complete);
    assert_eq!(settled.receipts[0].status, JobStatus::Failed);
    assert_eq!(settled.receipts[0].cleanup.status, CleanupStatus::Complete);
    assert_eq!(
        settled.receipts[0].external_effect_disposition,
        ExternalEffectDisposition::None
    );
    assert!(!job.port.staged);
    assert_eq!(job.scheduler.lease_counters().live(), 0);
    assert!(job.start_frame(&mut RejectEncoder).is_err());
}

#[test]
fn gpu_port_failure_unwind_and_bad_manifest_never_fabricate_success() {
    let original = owner("object-export-faults");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().unwrap();
    for case in 0..7 {
        let mut port = Port::default();
        let mut scheduler = FrameScheduler::new();
        match case {
            0 => port.fail = Some("begin"),
            1 => port.fail = Some("write"),
            2 => port.fail = Some("publish"),
            3 => port.bad_manifest = true,
            4 => port.panic_at = Some("begin"),
            5 => port.panic_at = Some("write"),
            _ => port.panic_at = Some("publish"),
        }
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        let begun = job
            .begin(&original, request(&snapshot, 65536), witness(&snapshot))
            .unwrap();
        if begun.status == JobStatus::Running {
            let pending = job.start_frame(&mut gpu).unwrap().unwrap();
            job.finish_frame(pending).unwrap();
        }
        let failed = job.status().unwrap();
        assert_eq!(failed.status, JobStatus::Failed);
        assert!(failed.artifact.is_none());
        assert_eq!(failed.cleanup.status, CleanupStatus::Complete);
        assert_eq!(job.scheduler.lease_counters().live(), 0);
        assert!(!job.port.staged);
        let publication = [2, 3, 6].contains(&case);
        assert_eq!(job.port.published, publication);
        assert_eq!(
            failed.external_effect_disposition,
            if publication {
                ExternalEffectDisposition::Indeterminate
            } else {
                ExternalEffectDisposition::None
            }
        );
        // The port's cleanup removes staging only. Its modeled published side
        // effect stays present, including after Err/unwind/mismatched manifest.
        assert_eq!(job.reconcile_release().cleanup_complete, !publication);
        if publication {
            assert_eq!(
                job.status().unwrap().external_effect_disposition,
                ExternalEffectDisposition::Indeterminate
            );
        }
    }
}

#[test]
fn gpu_cancel_replacement_release_and_late_stale_png_cannot_publish() {
    let original = owner("object-export-late");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().unwrap();
    for case in 0..4 {
        let mut port = Port::default();
        let mut scheduler = FrameScheduler::new();
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        job.begin(&original, request(&snapshot, 65536), witness(&snapshot))
            .unwrap();
        let pending = job.start_frame(&mut gpu).unwrap().unwrap();
        let mut newer = None;
        match case {
            0 => {
                job.cancel().unwrap();
            }
            1 => {
                assert!(job.replace_owner(&original).is_err());
                assert!(
                    job.replace_owner(&owner("replacement"))
                        .unwrap()
                        .cleanup_complete
                );
            }
            2 => {
                assert!(job.reconcile_release().cleanup_complete);
            }
            _ => {
                let pins = ObjectSchedulePins {
                    instance_id: snapshot.instance_id().into(),
                    document_id: snapshot.document_id().into(),
                    content_revision: 0,
                    document_snapshot_id: snapshot.snapshot_id().into(),
                    context_id: "scene-root".into(),
                    frame: 0,
                    scope: ObjectScheduleScope::Authored,
                };
                newer = Some(
                    job.scheduler
                        .schedule_object(
                            snapshot.clone(),
                            pins,
                            EvaluationKey::new(
                                snapshot.snapshot_id(),
                                "scene-root",
                                0,
                                "final",
                                request(&snapshot, 65536).output,
                                [],
                            )
                            .unwrap(),
                        )
                        .unwrap(),
                );
            }
        }
        let result = job.finish_frame(pending).unwrap();
        assert_eq!(
            result.status,
            if case == 3 {
                JobStatus::Failed
            } else {
                JobStatus::Cancelled
            }
        );
        assert_eq!(job.port.events, ["begin", "cleanup"]);
        assert!(!job.port.published);
        if let Some(work) = newer {
            job.scheduler.cancel(work.work_id()).unwrap();
        }
        assert_eq!(job.scheduler.lease_counters().live(), 0);
    }
}

#[test]
fn gpu_equal_numeric_foreign_pending_png_is_refused_without_write() {
    let original = owner("object-export-foreign-pending");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut pa = Port::default();
    let mut pb = Port::default();
    let mut sa = FrameScheduler::new();
    let mut sb = FrameScheduler::new();
    let mut a = ObjectExportJob::new(&mut pa, &mut sa);
    let mut b = ObjectExportJob::new(&mut pb, &mut sb);
    a.begin(&original, request(&snapshot, 65536), witness(&snapshot))
        .unwrap();
    b.begin(&original, request(&snapshot, 65536), witness(&snapshot))
        .unwrap();
    let mut gpu = Compositor::new().unwrap();
    let pending = a.start_frame(&mut gpu).unwrap().unwrap();
    let own = b.start_frame(&mut gpu).unwrap().unwrap();
    assert_eq!(pending.job_id, own.job_id);
    assert_eq!(pending.work.work_id(), own.work.work_id());
    assert!(!pending.work.same_work_as(&own.work));
    assert!(b.finish_frame(pending).is_err());
    assert_eq!(b.port.events, ["begin"]);
    a.cancel().unwrap();
    b.cancel().unwrap();
    assert_eq!(
        (
            a.scheduler.lease_counters().live(),
            b.scheduler.lease_counters().live()
        ),
        (0, 0)
    );
}

#[test]
fn cpu_encoder_and_cleanup_unwind_retain_failure_until_real_cleanup_succeeds() {
    struct PanicEncoder;
    impl ObjectPngEncoder for PanicEncoder {
        fn encode_object_png(
            &mut self,
            _: &LiveObjectComposition<'_>,
            _: usize,
        ) -> Result<ScheduledObjectPng, String> {
            panic!("injected encoder unwind")
        }
    }
    let original = owner("object-export-unwind");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let mut port = Port {
        panic_at: Some("cleanup"),
        ..Port::default()
    };
    let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
    job.begin(&original, request(&snapshot, 65536), witness(&snapshot))
        .unwrap();
    assert!(job.start_frame(&mut PanicEncoder).unwrap().is_none());
    assert_eq!(job.status().unwrap().cleanup.status, CleanupStatus::Failed);
    assert!(!job.reconcile_release().cleanup_complete);
    assert_eq!(job.scheduler.lease_counters().live(), 0);
    job.port.panic_at = None;
    assert!(job.reconcile_release().cleanup_complete);
    assert_eq!(job.status().unwrap().status, JobStatus::Failed);
    assert!(!job.port.published);
}
