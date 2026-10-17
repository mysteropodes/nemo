//! Cancel, replacement and release fences with decoded frames in flight. The
//! job is driven step by step (as the accepted job lifecycle tests do) so a frame
//! that was encoded but never consumed can outlive the job: the gate must keep its
//! resources leased until that last holder is gone, then release each exactly once.
use super::support::{
    begin_request, exactly_once, isolated, owner, used, witness, Decoder, Scene, Stage,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_plan::FrameFormat;
use crate::export_job::native_vector_export::native_vector_export_job::NativeSvgEncoder;
use crate::export_job::{CleanupStatus, ExportJobErrorKind, JobStatus, ObjectExportJob};
use crate::revision::ObjectSnapshot;
use crate::scheduler::FrameScheduler;

fn media_witness(
    gate: &mut MediaExportGate<'_, Decoder>,
    scene: &Scene,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> ObjectCompositionWitness {
    let specs = scene.specs(frame).unwrap();
    gate.frame_witness(&specs, |_| Ok(witness(snapshot, frame)))
        .unwrap()
}

#[test]
fn cancel_mid_sequence_keeps_an_unconsumed_frame_leased_until_its_holder_is_gone() {
    let frames = [4u32, 5, 6];
    let history = owner("media-life-cancel", &frames);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let scene = Scene::video(3, 20.0);
    let (decoder, budget, stage) = (Decoder::sized(4, 2), isolated(1 << 20), Stage::default());
    let mut scheduler = FrameScheduler::new();
    let (mut port, mut sink) = (decoder.clone(), stage.clone());
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    let pending;
    {
        let mut job = ObjectExportJob::with_format(&mut sink, &mut scheduler, FrameFormat::Svg);
        let first = media_witness(&mut gate, &scene, &snapshot, 4);
        job.begin_frames(
            &history,
            begin_request(&snapshot, 4),
            first,
            frames.to_vec(),
        )
        .unwrap();
        let done = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        assert_eq!(job.finish_frame(done).unwrap().status, JobStatus::Running);
        let second = media_witness(&mut gate, &scene, &snapshot, 5);
        assert_eq!(
            job.next_frame(Ok(second)).unwrap().status,
            JobStatus::Running
        );
        pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        let cancelled = job.cancel().unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(cancelled.error.unwrap().code, "cancelled");
        assert_eq!(cancelled.cleanup.status, CleanupStatus::Complete);
        assert!(job.reconcile_release().cleanup_complete);
    }
    // The job and its scheduler permits are gone. Frame 1 (source of timeline frame 4)
    // is released; the encoded-but-unconsumed frame still pins source frame 2.
    assert_eq!(scheduler.lease_counters().live(), 0);
    let held = gate.finish();
    assert!(!held.complete());
    assert_eq!((held.frames.live(), held.sessions_open), (1, 1));
    assert_eq!(decoder.st().released, [1]);
    assert!(decoder.st().closed.is_empty());
    assert_eq!(used(&budget), 32);
    assert_eq!(stage.st().events, ["begin", "write", "cleanup"]);
    assert!(stage.st().published.is_empty());
    drop(pending);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(decoder.st().released, [1, 2]);
    assert_eq!(decoder.st().closed, [1]);
    assert_eq!(used(&budget), 0);
    // Two works, each one snapshot lease and one media lease, all returned.
    let counters = scheduler.lease_counters();
    assert_eq!((counters.acquired(), counters.released()), (4, 4));
}

fn fence_case(case: u8, escape: bool) {
    let label = format!("case {case} escape {escape}");
    let history = owner("media-life-fence", &[4]);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let scene = Scene::video(0, 20.0);
    let (decoder, budget, stage) = (Decoder::sized(4, 2), isolated(1 << 20), Stage::default());
    let mut scheduler = FrameScheduler::new();
    let (mut port, mut sink) = (decoder.clone(), stage.clone());
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    let mut escaped = None;
    {
        let mut job = ObjectExportJob::with_format(&mut sink, &mut scheduler, FrameFormat::Svg);
        let first = media_witness(&mut gate, &scene, &snapshot, 4);
        job.begin_frames(&history, begin_request(&snapshot, 4), first, vec![4])
            .unwrap();
        let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        match case {
            0 => {
                job.cancel().unwrap();
            }
            1 => {
                assert!(job.replace_owner(&history).is_err(), "{label}");
                let replaced = owner("media-life-replacement", &[4]);
                assert!(job.replace_owner(&replaced).unwrap().cleanup_complete);
            }
            _ => assert!(job.reconcile_release().cleanup_complete, "{label}"),
        }
        if escape {
            escaped = Some(pending);
        } else {
            let late = job.finish_frame(pending).unwrap();
            assert_eq!(late.status, JobStatus::Cancelled, "{label}");
            assert_eq!(late.artifact, None);
        }
    }
    // Whatever fenced the job, the late frame wrote and published nothing.
    assert_eq!(stage.st().events, ["begin", "cleanup"], "{label}");
    assert!(stage.st().published.is_empty(), "{label}");
    assert_eq!(scheduler.lease_counters().live(), 0, "{label}");
    if escape {
        let held = gate.finish();
        assert!(!held.complete(), "{label}");
        assert_eq!(held.frames.live(), 1, "{label}");
        assert!(decoder.st().released.is_empty() && decoder.st().closed.is_empty());
        drop(escaped);
    }
    let done = gate.finish();
    assert!(done.complete(), "{label}");
    assert_eq!(decoder.st().released, [1], "{label}");
    assert_eq!(decoder.st().closed, [1], "{label}");
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(used(&budget), 0, "{label}");
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released()),
        (2, 2),
        "{label}"
    );
}

#[test]
fn cancel_replacement_and_release_each_release_every_decoded_frame_exactly_once() {
    for case in 0..3 {
        fence_case(case, false);
        fence_case(case, true);
    }
}

#[test]
fn a_refused_begin_still_returns_every_decoded_frame() {
    let history = owner("media-life-refused", &[4]);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let scene = Scene::video(0, 20.0);
    let (decoder, budget, stage) = (Decoder::sized(4, 2), isolated(1 << 20), Stage::default());
    let mut scheduler = FrameScheduler::new();
    let (mut port, mut sink) = (decoder.clone(), stage.clone());
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    {
        let mut job = ObjectExportJob::with_format(&mut sink, &mut scheduler, FrameFormat::Svg);
        let first = media_witness(&mut gate, &scene, &snapshot, 4);
        // An empty frame list is refused before any effect: the witness is dropped.
        let error = job
            .begin_frames(&history, begin_request(&snapshot, 4), first, vec![])
            .unwrap_err();
        assert_eq!(error.kind, ExportJobErrorKind::InvalidRequest);
    }
    assert!(stage.st().events.is_empty());
    assert_eq!(scheduler.lease_counters().acquired(), 0);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(used(&budget), 0);
}
