//! The host drivers, the shared evaluation key, the format guard and replay of the
//! SVG job. Expected documents are rebuilt from literals (`expected_svg`).
use super::fixture::{
    begin, begin_request, expected_svg, host_request, owner, plan, step, utf8, witness, PngKeySpy,
    Port, SvgKeySpy,
};
use super::*;
use crate::export_job::{ExportJobErrorKind, JobStatus, ObjectExportJob};
use crate::scheduler::FrameScheduler;

#[test]
fn cpu_host_svg_drivers_run_the_same_lifecycle_and_reconcile_before_returning() {
    let frames = [5u32, 6, 7];
    let owner = owner("svg-job-host", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut scheduler = FrameScheduler::new();
    let outcome = export_object_svg_frames(
        &owner,
        &mut scheduler,
        &mut port.clone(),
        host_request(&snapshot, 5, 65536),
        &frames,
        |frame| Ok(witness(&snapshot, frame)),
    );
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(outcome.release.receipts, [receipt]);
    assert_eq!(
        port.st().events,
        ["begin", "write", "write", "write", "publish"]
    );
    for ((_, bytes), frame) in port.st().published.iter().zip(frames) {
        assert_eq!(utf8(bytes), expected_svg(&plan(&snapshot, frame)));
    }
    assert_eq!(scheduler.lease_counters().live(), 0);

    // The one-frame driver names its file like the first frame of a sequence.
    let single = Port::default();
    let outcome = export_one_object_svg_frame(
        &owner,
        &mut FrameScheduler::new(),
        &mut single.clone(),
        host_request(&snapshot, 7, 65536),
        witness(&snapshot, 7),
    );
    assert_eq!(
        outcome.receipt.unwrap().artifact.unwrap().files,
        ["frame_0001.svg"]
    );
    assert_eq!(
        utf8(&single.st().published[0].1),
        expected_svg(&plan(&snapshot, 7))
    );

    // A producer that fails for frame 6 leaves nothing published and staging removed.
    let failing = Port::default();
    let mut scheduler = FrameScheduler::new();
    let outcome = export_object_svg_frames(
        &owner,
        &mut scheduler,
        &mut failing.clone(),
        host_request(&snapshot, 5, 65536),
        &frames,
        |frame| {
            if frame == 6 {
                Err("producer failed".into())
            } else {
                Ok(witness(&snapshot, frame))
            }
        },
    );
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Failed);
    assert_eq!(receipt.error.unwrap().code, "frame_failed");
    assert!(outcome.release.cleanup_complete);
    assert!(failing.st().published.is_empty());
    assert_eq!(failing.removed(), ["frame_0001.svg"]);
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_svg_and_png_encoders_see_the_same_evaluation_key_and_dimensions() {
    let owner = owner("svg-job-key", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let (mut png_spy, mut svg_spy) = (PngKeySpy::default(), SvgKeySpy::default());
    for svg in [false, true] {
        let port = Port::default();
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        let format = if svg {
            FrameFormat::Svg
        } else {
            FrameFormat::Png
        };
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, format);
        job.begin(
            &owner,
            begin_request(&snapshot, 7, 65536),
            witness(&snapshot, 7),
        )
        .unwrap();
        if svg {
            let pending = job.start_svg_frame(&mut svg_spy).unwrap().unwrap();
            assert_eq!(
                job.finish_frame(pending).unwrap().status,
                JobStatus::Succeeded
            );
        } else {
            assert!(job.start_frame(&mut png_spy).unwrap().is_none());
        }
    }
    assert_eq!(png_spy.keys.len(), 1);
    assert_eq!(png_spy.keys, svg_spy.keys);
    assert_eq!(png_spy.dimensions, svg_spy.dimensions);
    assert_eq!(svg_spy.dimensions, [(64, 64)]);
    let key = &svg_spy.keys[0];
    assert_eq!(
        (
            key.document_snapshot_id(),
            key.context_id(),
            key.frame(),
            key.quality()
        ),
        (snapshot.snapshot_id(), "scene-root", 7, "final")
    );
    let spec = key.output_spec();
    assert_eq!(
        (
            spec.kind(),
            spec.format(),
            spec.color_interpretation(),
            spec.alpha_mode()
        ),
        ("frame", "rgba8", "srgb", "straight")
    );
}

#[test]
fn cpu_format_mismatch_is_refused_before_any_effect_or_state_change() {
    let owner = owner("svg-job-mismatch", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    for svg_job in [false, true] {
        let port = Port::default();
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        let format = if svg_job {
            FrameFormat::Svg
        } else {
            FrameFormat::Png
        };
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, format);
        job.begin(
            &owner,
            begin_request(&snapshot, 7, 65536),
            witness(&snapshot, 7),
        )
        .unwrap();
        let before = job.status().unwrap();
        let refused = if svg_job {
            job.start_frame(&mut PngKeySpy::default()).err()
        } else {
            job.start_svg_frame(&mut SvgKeySpy::default()).err()
        };
        assert_eq!(refused.unwrap().kind, ExportJobErrorKind::InvalidRequest);
        assert_eq!(job.status().unwrap(), before);
        // The refused start did not take the in-flight slot: the right encoder still works.
        if svg_job {
            let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
            assert_eq!(
                job.finish_frame(pending).unwrap().status,
                JobStatus::Succeeded
            );
        } else {
            assert!(job
                .start_frame(&mut PngKeySpy::default())
                .unwrap()
                .is_none());
        }
        assert_eq!(port.st().events.first(), Some(&"begin"));
    }
}

#[test]
fn cpu_svg_replay_returns_the_retained_receipt_and_never_restages() {
    let frames = [6u32, 7];
    let owner = owner("svg-job-replay", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
    let begun = begin(&mut job, &owner, &snapshot, &frames, 65536);
    let again = job
        .begin_frames(
            &owner,
            begin_request(&snapshot, 6, 65536),
            witness(&snapshot, 6),
            frames.to_vec(),
        )
        .unwrap();
    assert_eq!(again, begun);
    let other = job.begin_frames(
        &owner,
        begin_request(&snapshot, 6, 65536),
        witness(&snapshot, 6),
        vec![6],
    );
    assert_eq!(
        other.unwrap_err().kind,
        ExportJobErrorKind::ChangedRequestId
    );
    assert_eq!(port.st().events, ["begin"]);
    step(&mut job, &snapshot, &frames, 0);
    step(&mut job, &snapshot, &frames, 1);
    let replay = job
        .begin_frames(
            &owner,
            begin_request(&snapshot, 6, 65536),
            witness(&snapshot, 6),
            frames.to_vec(),
        )
        .unwrap();
    assert_eq!(replay.status, JobStatus::Succeeded);
    assert_eq!(port.st().events, ["begin", "write", "write", "publish"]);
}
