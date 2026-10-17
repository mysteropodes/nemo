//! SVG through the existing pinned export job: one frame, a sequence, the host
//! drivers, byte budgets and the shared evaluation key. Expected documents are
//! rebuilt from literals (`expected_svg`), never from the serializer under test.
use super::fixture::{
    begin, expected_svg, media_plan, owner, plan, recolor_layer_two, staged_files, step, utf8, Port,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_plan::OutputKind;
use crate::export_job::native_media_export::native_media_export_verify::verify_output;
use crate::export_job::{
    CleanupStatus, ExportArtifact, ExternalEffectDisposition, JobStatus, ObjectExportJob,
};
use crate::scheduler::FrameScheduler;

#[test]
fn cpu_still_svg_job_stages_one_file_and_publishes_the_pinned_frame() {
    let owner = owner("svg-job-still", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        let begun = begin(&mut job, &owner, &snapshot, &[7], 65536);
        assert_eq!(begun.status, JobStatus::Running);
        assert_eq!(
            begun.external_effect_disposition,
            ExternalEffectDisposition::Contained
        );
        let done = step(&mut job, &snapshot, &[7], 0);
        assert_eq!(done.status, JobStatus::Succeeded);
        assert_eq!(done.pinned_revision, 0);
        assert_eq!(done.document_snapshot_id, snapshot.snapshot_id());
        assert_eq!(done.progress, 1.0);
        assert_eq!(done.cleanup.status, CleanupStatus::Complete);
        assert_eq!(
            done.external_effect_disposition,
            ExternalEffectDisposition::Committed
        );
        assert_eq!(
            done.artifact.unwrap(),
            ExportArtifact {
                target: "/published/object".into(),
                files: vec!["frame_0001.svg".into()]
            }
        );
        assert!(job.reconcile_release().cleanup_complete);
    }
    assert_eq!(port.st().events, ["begin", "write", "publish"]);
    let state = port.st();
    assert_eq!(state.published.len(), 1);
    let (name, bytes) = &state.published[0];
    assert_eq!(name, "frame_0001.svg");
    let expected = expected_svg(&plan(&snapshot, 7));
    assert_eq!(utf8(bytes), expected);
    assert_eq!(expected.matches("<path ").count(), 3);
    assert!(expected.starts_with(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"64\" height=\"64\" viewBox=\"0 0 64 64\">\n<path d=\"M11,4 C11,4 35,4 35,4 "
    ));
    assert!(expected.ends_with("fill-rule=\"nonzero\"/>\n</svg>"));
    assert_eq!(
        (
            scheduler.lease_counters().acquired(),
            scheduler.lease_counters().live()
        ),
        (1, 0)
    );
    let verified = verify_output(
        &media_plan(&snapshot, OutputKind::Still, 7, 7),
        &staged_files(&state.published),
    )
    .unwrap();
    assert_eq!(verified.files.len(), 1);
    assert_eq!(
        (verified.files[0].width, verified.files[0].height),
        (64, 64)
    );
    assert_eq!(verified.total_bytes, expected.len() as u64);
}

#[test]
fn cpu_svg_sequence_stages_ordered_names_each_from_its_own_frame() {
    let frames = [5u32, 6, 7];
    let owner = owner("svg-job-sequence", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &frames, 65536);
        let progress: Vec<f64> = (0..3)
            .map(|index| step(&mut job, &snapshot, &frames, index))
            .map(|receipt| receipt.progress)
            .collect();
        assert_eq!(progress, [1.0 / 3.0, 2.0 / 3.0, 1.0]);
        let last = job.status().unwrap();
        assert_eq!(last.status, JobStatus::Succeeded);
        assert_eq!(
            last.artifact.unwrap().files,
            ["frame_0001.svg", "frame_0002.svg", "frame_0003.svg"]
        );
        assert!(job.reconcile_release().cleanup_complete);
    }
    let state = port.st();
    assert_eq!(
        state.events,
        ["begin", "write", "write", "write", "publish"]
    );
    let names: Vec<&str> = state.published.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["frame_0001.svg", "frame_0002.svg", "frame_0003.svg"]
    );
    for ((_, bytes), frame) in state.published.iter().zip(frames) {
        assert_eq!(
            utf8(bytes),
            expected_svg(&plan(&snapshot, frame)),
            "frame {frame}"
        );
    }
    assert!(state.published[0].1 != state.published[1].1);
    assert!(state.published[1].1 != state.published[2].1);
    assert_eq!(
        (
            scheduler.lease_counters().acquired(),
            scheduler.lease_counters().live()
        ),
        (3, 0)
    );
    let verified = verify_output(
        &media_plan(&snapshot, OutputKind::Sequence, 5, 7),
        &staged_files(&state.published),
    )
    .unwrap();
    let total: usize = state.published.iter().map(|(_, b)| b.len()).sum();
    assert_eq!(verified.total_bytes, total as u64);
}

#[test]
fn cpu_svg_byte_budget_is_exact_and_a_short_budget_never_writes() {
    let owner = owner("svg-job-budget", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let length = expected_svg(&plan(&snapshot, 7)).len();
    for (budget, fits) in [
        (65536, true),
        (length, true),
        (length - 1, false),
        (1, false),
    ] {
        let port = Port::default();
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &[7], budget);
        let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap();
        if fits {
            let done = job.finish_frame(pending.unwrap()).unwrap();
            assert_eq!(done.status, JobStatus::Succeeded);
            assert!(port.st().published[0].1.len() <= budget);
        } else {
            assert!(pending.is_none());
            let failed = job.status().unwrap();
            assert_eq!(failed.status, JobStatus::Failed);
            let error = failed.error.unwrap();
            assert_eq!(error.code, "frame_failed");
            assert!(
                error.message.contains("SVG encoded byte budget exceeded"),
                "{error:?}"
            );
            assert_eq!(port.st().events, ["begin", "cleanup"]);
            assert!(port.st().published.is_empty());
        }
    }
    // An SVG can never exceed the serializer ceiling: a huge request budget is clamped, not honored.
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
    begin(&mut job, &owner, &snapshot, &[7], usize::MAX);
    assert_eq!(
        step(&mut job, &snapshot, &[7], 0).status,
        JobStatus::Succeeded
    );
}

#[test]
fn cpu_svg_job_pins_the_original_revision_across_a_later_owner_edit() {
    let frames = [6u32, 7];
    let mut owner = owner("svg-job-pinned", &frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = expected_svg(&plan(&snapshot, 7));
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
    begin(&mut job, &owner, &snapshot, &frames, 65536);
    step(&mut job, &snapshot, &frames, 0);
    recolor_layer_two(&mut owner);
    let done = step(&mut job, &snapshot, &frames, 1);
    assert_eq!(
        (done.status, done.pinned_revision),
        (JobStatus::Succeeded, 0)
    );
    assert_eq!(utf8(&port.st().published[1].1), before);
    let edited = owner.acquire_snapshot(1).unwrap();
    let spec = SvgFrameSpec::new(64, 64, 65536).unwrap();
    let after = serialize_svg_frame(&plan(&edited, 7), spec).unwrap();
    let after = utf8(&after.bytes);
    assert!(before != after);
    assert!(after.contains("fill=\"rgb(100%,100%,0%)\" fill-opacity=\"1\""));
}
