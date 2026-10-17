//! Cancel, cleanup debt, replacement fences and unwinds of the SVG job. These are the
//! shapes of `object_export_job_lifecycle.rs` driven through the SVG encoder (no GPU).
use super::fixture::{begin, begin_request, owner, step, witness, PanicSvg, Port, RejectSvg};
use super::*;
use crate::export_job::{
    CleanupStatus, ExportJobErrorKind, ExternalEffectDisposition, JobStatus, ObjectExportJob,
};
use crate::scheduler::FrameScheduler;

#[test]
fn cpu_cancel_mid_sequence_removes_staging_and_a_late_frame_cannot_publish() {
    let frames = [5u32, 6, 7];
    let owner = owner("svg-life-cancel", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &frames, 65536);
        assert_eq!(
            step(&mut job, &snapshot, &frames, 0).status,
            JobStatus::Running
        );
        assert_eq!(port.st().staged.len(), 1);
        assert_eq!(
            job.next_frame(Ok(witness(&snapshot, 6))).unwrap().status,
            JobStatus::Running
        );
        let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        let cancelled = job.cancel().unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(cancelled.error.unwrap().code, "cancelled");
        assert_eq!(cancelled.artifact, None);
        assert_eq!(cancelled.cleanup.status, CleanupStatus::Complete);
        assert_eq!(
            cancelled.external_effect_disposition,
            ExternalEffectDisposition::None
        );
        // The frame that was already encoded cannot be written or published afterwards.
        let late = job.finish_frame(pending).unwrap();
        assert_eq!(late.status, JobStatus::Cancelled);
        assert_eq!(job.cancel().unwrap(), late);
        assert!(job.reconcile_release().cleanup_complete);
    }
    assert_eq!(port.st().events, ["begin", "write", "cleanup"]);
    assert!(port.st().published.is_empty() && port.st().staged.is_empty());
    assert_eq!(port.removed(), ["frame_0001.svg"]);
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_cancel_replacement_and_release_fence_a_late_pending_frame() {
    let owner = owner("svg-life-fence", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    for case in 0..3 {
        let port = Port::default();
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        {
            let mut job =
                ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
            begin(&mut job, &owner, &snapshot, &[7], 65536);
            let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
            match case {
                0 => {
                    job.cancel().unwrap();
                }
                1 => {
                    assert!(job.replace_owner(&owner).is_err());
                    let replaced = self::owner("svg-life-replacement", &[7]);
                    assert!(job.replace_owner(&replaced).unwrap().cleanup_complete);
                }
                _ => assert!(job.reconcile_release().cleanup_complete),
            }
            let late = job.finish_frame(pending).unwrap();
            assert_eq!(late.status, JobStatus::Cancelled, "case {case}");
            assert_eq!(late.artifact, None);
        }
        assert_eq!(port.st().events, ["begin", "cleanup"], "case {case}");
        assert!(port.st().published.is_empty() && port.st().written.is_empty());
        assert_eq!(scheduler.lease_counters().live(), 0, "case {case}");
    }
}

#[test]
fn cpu_foreign_pending_frame_with_equal_numbers_is_refused_without_a_write() {
    let owner = owner("svg-life-foreign", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let (port_a, port_b) = (Port::default(), Port::default());
    let (mut pa, mut pb) = (port_a.clone(), port_b.clone());
    let (mut sa, mut sb) = (FrameScheduler::new(), FrameScheduler::new());
    {
        let mut a = ObjectExportJob::with_format(&mut pa, &mut sa, FrameFormat::Svg);
        let mut b = ObjectExportJob::with_format(&mut pb, &mut sb, FrameFormat::Svg);
        begin(&mut a, &owner, &snapshot, &[7], 65536);
        begin(&mut b, &owner, &snapshot, &[7], 65536);
        let pending = a.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        let own = b.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        // Same job id and work number, different schedulers: only identity tells them apart.
        let error = b.finish_frame(pending).unwrap_err();
        assert_eq!(error.kind, ExportJobErrorKind::InvalidRequest);
        assert_eq!(b.finish_frame(own).unwrap().status, JobStatus::Succeeded);
        a.cancel().unwrap();
    }
    assert_eq!(port_b.st().events, ["begin", "write", "publish"]);
    assert_eq!(port_a.st().events, ["begin", "cleanup"]);
    assert!(port_a.st().published.is_empty());
    assert_eq!(
        (sa.lease_counters().live(), sb.lease_counters().live()),
        (0, 0)
    );
}

#[test]
fn cpu_cleanup_debt_stays_visible_and_retries_without_rearming_work() {
    let frames = [5u32, 6];
    let owner = owner("svg-life-debt", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    {
        let mut state = port.set();
        state.fail_write_at = Some(2);
        state.fail_cleanup = true;
    }
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &frames, 65536);
        step(&mut job, &snapshot, &frames, 0);
        let failure = step(&mut job, &snapshot, &frames, 1);
        assert_eq!(failure.status, JobStatus::Failed);
        assert_eq!(failure.cleanup.status, CleanupStatus::Failed);
        assert_eq!(failure.error.unwrap().code, "cleanup_failed");
        assert_eq!(
            failure.external_effect_disposition,
            ExternalEffectDisposition::Indeterminate
        );
        // The debt is real: the staged file is still there and release is refused.
        assert_eq!(port.st().staged.len(), 1);
        assert!(!job.reconcile_release().cleanup_complete);
        assert_eq!(port.st().staged.len(), 1);
        port.set().fail_cleanup = false;
        let settled = job.reconcile_release();
        assert!(settled.cleanup_complete);
        assert_eq!(settled.receipts[0].status, JobStatus::Failed);
        assert_eq!(settled.receipts[0].cleanup.status, CleanupStatus::Complete);
        assert_eq!(
            settled.receipts[0].external_effect_disposition,
            ExternalEffectDisposition::None
        );
        // Retrying cleanup never re-arms work.
        assert!(job.start_svg_frame(&mut NativeSvgEncoder).is_err());
        assert!(job.next_frame(Ok(witness(&snapshot, 6))).is_err());
    }
    assert!(port.st().staged.is_empty() && port.st().published.is_empty());
    assert_eq!(port.removed(), ["frame_0001.svg"]);
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_encoder_and_cleanup_unwinds_are_retained_until_real_cleanup_succeeds() {
    let owner = owner("svg-life-unwind", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    port.set().panic_at = Some("cleanup");
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &[7], 65536);
        assert!(job.start_svg_frame(&mut PanicSvg).unwrap().is_none());
        assert_eq!(job.status().unwrap().cleanup.status, CleanupStatus::Failed);
        assert!(!job.reconcile_release().cleanup_complete);
        port.set().panic_at = None;
        assert!(job.reconcile_release().cleanup_complete);
        assert_eq!(job.status().unwrap().status, JobStatus::Failed);
    }
    assert!(port.st().published.is_empty());
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_publish_failure_and_a_foreign_manifest_never_fabricate_success() {
    let owner = owner("svg-life-publish", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    for case in 0..3 {
        let port = Port::default();
        match case {
            0 => port.set().fail = Some("publish"),
            1 => port.set().bad_manifest = true,
            _ => port.set().wrong_extension = true,
        }
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        {
            let mut job =
                ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
            begin(&mut job, &owner, &snapshot, &[7], 65536);
            let receipt = step(&mut job, &snapshot, &[7], 0);
            assert_eq!(receipt.status, JobStatus::Failed, "case {case}");
            assert_eq!(receipt.error.unwrap().code, "publish_failed", "case {case}");
            assert_eq!(receipt.artifact, None);
            // A publish that may have taken effect is reported as such, not as clean.
            assert_eq!(
                receipt.external_effect_disposition,
                ExternalEffectDisposition::Indeterminate,
                "case {case}"
            );
            assert!(!job.reconcile_release().cleanup_complete);
        }
        assert_eq!(scheduler.lease_counters().live(), 0, "case {case}");
    }
}

#[test]
fn cpu_a_failed_write_removes_the_earlier_staged_frames_and_publishes_nothing() {
    let frames = [5u32, 6, 7];
    let owner = owner("svg-life-write", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    port.set().fail_write_at = Some(2);
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &frames, 65536);
        assert_eq!(step(&mut job, &snapshot, &frames, 0).progress, 1.0 / 3.0);
        let failure = step(&mut job, &snapshot, &frames, 1);
        assert_eq!(failure.status, JobStatus::Failed);
        assert_eq!(failure.error.unwrap().code, "write_failed");
        assert_eq!(failure.progress, 1.0 / 3.0);
        assert_eq!(failure.cleanup.status, CleanupStatus::Complete);
        assert!(job.reconcile_release().cleanup_complete);
    }
    assert_eq!(port.st().events, ["begin", "write", "write", "cleanup"]);
    assert_eq!(port.removed(), ["frame_0001.svg"]);
    assert!(port.st().published.is_empty());
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_encoder_failure_before_any_write_is_terminal_and_cleans_the_empty_staging() {
    let owner = owner("svg-life-reject", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &[7], 65536);
        assert!(job.start_svg_frame(&mut RejectSvg).unwrap().is_none());
        let failed = job.status().unwrap();
        assert_eq!(failed.status, JobStatus::Failed);
        assert_eq!(failed.error.unwrap().code, "frame_failed");
        assert!(job.start_svg_frame(&mut NativeSvgEncoder).is_err());
        assert_eq!(
            job.begin_frames(
                &owner,
                begin_request(&snapshot, 7, 65536),
                witness(&snapshot, 7),
                vec![7]
            )
            .unwrap()
            .status,
            JobStatus::Failed
        );
    }
    assert_eq!(port.st().events, ["begin", "cleanup"]);
    assert_eq!(port.st().cleanups, [Vec::<String>::new()]);
    assert_eq!(scheduler.lease_counters().live(), 0);
}
