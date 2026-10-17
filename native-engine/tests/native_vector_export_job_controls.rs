//! Negative and mutation controls. Each splices ONE mistake into real job output (or
//! drives one real mutant encoder) and must be caught; an unmutated pass-through
//! first proves the comparison is not vacuous.
use super::fixture::{
    begin, expected_svg, media_plan, output, owner, pins, plan, staged_files, step, witness,
    IgnoreBudget, Port,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_plan::{
    ExportPins, FrameRange, MediaExportPlan, OutputKind,
};
use crate::export_job::native_media_export::native_media_export_verify::{
    verify_output, VerifiedOutput, VerifyFailure,
};
use crate::export_job::{JobStatus, ObjectExportJob};
use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{EvaluationKey, FrameScheduler};

type Files = Vec<(String, Vec<u8>)>;

/// The three files a real SVG job publishes for frames 5, 6, 7.
fn published_sequence() -> (ObjectSnapshot, Files) {
    let frames = [5u32, 6, 7];
    let history = owner("svg-controls", &frames);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let port = Port::default();
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &history, &snapshot, &frames, 65536);
        for index in 0..3 {
            step(&mut job, &snapshot, &frames, index);
        }
    }
    let files = port.st().published.clone();
    (snapshot, files)
}

fn verdict(snapshot: &ObjectSnapshot, files: &Files) -> Result<VerifiedOutput, VerifyFailure> {
    verify_output(
        &media_plan(snapshot, OutputKind::Sequence, 5, 7),
        &staged_files(files),
    )
}

fn edited(files: &Files, index: usize, change: impl Fn(&str) -> String) -> Files {
    let mut out = files.clone();
    out[index].1 = change(std::str::from_utf8(&files[index].1).unwrap()).into_bytes();
    out
}

#[test]
fn unmutated_pass_through_is_clean() {
    let (snapshot, files) = published_sequence();
    let verified = verdict(&snapshot, &files).unwrap();
    assert_eq!(verified.files.len(), 3);
    for ((_, bytes), frame) in files.iter().zip([5u32, 6, 7]) {
        assert_eq!(
            std::str::from_utf8(bytes).unwrap(),
            expected_svg(&plan(&snapshot, frame))
        );
    }
}

#[test]
fn mutation_structure_name_and_order_mistakes_are_caught_by_the_verifier() {
    let (snapshot, files) = published_sequence();
    let mut swapped = files.clone();
    swapped.swap(0, 1);
    assert!(matches!(
        verdict(&snapshot, &swapped),
        Err(VerifyFailure::OutOfOrder { index: 1 })
    ));
    let mut dropped = files.clone();
    dropped.pop();
    assert!(matches!(
        verdict(&snapshot, &dropped),
        Err(VerifyFailure::CountMismatch {
            expected: 3,
            actual: 2
        })
    ));
    let mut duplicated = files.clone();
    duplicated[2].0 = duplicated[1].0.clone();
    assert!(matches!(
        verdict(&snapshot, &duplicated),
        Err(VerifyFailure::OutOfOrder { index: 2 })
    ));
    let mut renamed = files.clone();
    renamed[1].0 = "frame_0002.png".into();
    assert!(matches!(
        verdict(&snapshot, &renamed),
        Err(VerifyFailure::NameMismatch { index: 1, .. })
    ));
    let mut emptied = files.clone();
    emptied[0].1.clear();
    assert!(matches!(
        verdict(&snapshot, &emptied),
        Err(VerifyFailure::EmptyFile { .. })
    ));
    // The same bytes judged as PNG files are refused: the verifier is format sensitive.
    let as_png = MediaExportPlan::new(
        OutputKind::Sequence,
        FrameFormat::Png,
        ExportPins::from_schedule(&pins(&snapshot, 5)),
        output(),
        FrameRange::new(5, 7).unwrap(),
        32,
    )
    .unwrap();
    assert!(verify_output(&as_png, &staged_files(&files)).is_err());
}

#[test]
fn mutation_document_framing_and_size_mistakes_are_caught_by_the_verifier() {
    let (snapshot, files) = published_sequence();
    let no_tail = edited(&files, 0, |text| {
        text.trim_end_matches("\n</svg>").to_owned()
    });
    assert!(matches!(
        verdict(&snapshot, &no_tail),
        Err(VerifyFailure::Malformed { .. })
    ));
    let no_preamble = edited(&files, 0, |text| {
        text.trim_start_matches("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")
            .to_owned()
    });
    assert!(matches!(
        verdict(&snapshot, &no_preamble),
        Err(VerifyFailure::Malformed { .. })
    ));
    let torn = edited(&files, 0, |text| {
        text.replacen("width=\"64\"", "width=\"63\"", 1)
    });
    assert!(matches!(
        verdict(&snapshot, &torn),
        Err(VerifyFailure::Malformed { .. })
    ));
    let resized = edited(&files, 0, |text| {
        text.replacen("width=\"64\"", "width=\"63\"", 1).replacen(
            "viewBox=\"0 0 64 64\"",
            "viewBox=\"0 0 63 64\"",
            1,
        )
    });
    assert!(matches!(
        verdict(&snapshot, &resized),
        Err(VerifyFailure::DimensionMismatch {
            expected: (64, 64),
            actual: (63, 64),
            ..
        })
    ));
}

#[test]
fn mutation_content_mistakes_pass_the_verifier_and_only_the_exact_compare_catches_them() {
    let (snapshot, files) = published_sequence();
    let frame7 = expected_svg(&plan(&snapshot, 7));
    let moved = edited(&files, 2, |text| text.replace("35,4", "36,4"));
    assert!(verdict(&snapshot, &moved).is_ok());
    assert_ne!(std::str::from_utf8(&moved[2].1).unwrap(), frame7);
    // Bytes of one frame under another frame's name.
    let mut crossed = files.clone();
    let first = crossed[0].1.clone();
    crossed[0].1 = crossed[1].1.clone();
    crossed[1].1 = first;
    assert!(verdict(&snapshot, &crossed).is_ok());
    for (index, frame) in [5u32, 6].into_iter().enumerate() {
        assert_ne!(
            std::str::from_utf8(&crossed[index].1).unwrap(),
            expected_svg(&plan(&snapshot, frame))
        );
    }
}

#[test]
fn mutation_encoder_that_ignores_the_job_budget_is_caught_by_the_job() {
    let owner = owner("svg-controls-budget", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let length = expected_svg(&plan(&snapshot, 7)).len();
    for (encoder_is_mutant, budget) in [(false, length - 1), (true, length - 1), (true, length)] {
        let port = Port::default();
        let mut job_port = port.clone();
        let mut scheduler = FrameScheduler::new();
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &owner, &snapshot, &[7], budget);
        let pending = if encoder_is_mutant {
            job.start_svg_frame(&mut IgnoreBudget)
        } else {
            job.start_svg_frame(&mut NativeSvgEncoder)
        }
        .unwrap();
        if budget < length {
            assert!(pending.is_none(), "mutant={encoder_is_mutant}");
            assert_eq!(job.status().unwrap().status, JobStatus::Failed);
            assert!(port.st().written.is_empty() && port.st().published.is_empty());
        } else {
            assert_eq!(
                job.finish_frame(pending.unwrap()).unwrap().status,
                JobStatus::Succeeded
            );
        }
    }
}

#[test]
fn mutation_plan_of_another_frame_or_owner_is_refused_and_stale_work_cannot_be_consumed() {
    let owner = owner("svg-controls-pins", &[6, 7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let foreign = self::owner("svg-controls-foreign", &[6, 7])
        .acquire_snapshot(0)
        .unwrap();
    let schedule = |scheduler: &mut FrameScheduler| {
        scheduler
            .schedule_object(
                snapshot.clone(),
                pins(&snapshot, 7),
                EvaluationKey::new(
                    snapshot.snapshot_id(),
                    "scene-root",
                    7,
                    "final",
                    output(),
                    [],
                )
                .unwrap(),
            )
            .unwrap()
    };
    let (mut first, mut second) = (FrameScheduler::new(), FrameScheduler::new());
    let (work_a, work_b) = (schedule(&mut first), schedule(&mut second));
    let witness_seven = witness(&snapshot, 7);
    let live_a =
        prepare_scheduled_object_composition_from_witness(&first, &work_a, &witness_seven).unwrap();
    let live_b =
        prepare_scheduled_object_composition_from_witness(&second, &work_b, &witness_seven)
            .unwrap();
    let good = encode_scheduled_object_svg(&plan(&snapshot, 7), &live_a, 65536).unwrap();
    let expected = expected_svg(&plan(&snapshot, 7));
    assert_eq!(
        std::str::from_utf8(good.bytes(&live_a).unwrap()).unwrap(),
        expected
    );
    assert_eq!((good.dimensions(), good.path_count()), ((64, 64), 3));
    assert_eq!(good.pins().frame, 7);
    assert_eq!(
        good.identity().evaluation_key(),
        live_a.identity().evaluation_key()
    );
    // The same bytes through another scheduler's live permit are foreign work.
    let stale = good.bytes(&live_b).unwrap_err();
    assert!(stale.contains("not consumable"), "{stale}");
    // A plan of another frame, or of another owner at the same frame, never serializes.
    for (label, other) in [
        ("frame 6", plan(&snapshot, 6)),
        ("foreign owner", plan(&foreign, 7)),
    ] {
        let error = encode_scheduled_object_svg(&other, &live_a, 65536).err();
        assert_eq!(
            error.as_deref(),
            Some("SVG plan pins differ from the scheduled frame"),
            "{label}"
        );
    }
    // A zero budget is not a budget.
    assert!(encode_scheduled_object_svg(&plan(&snapshot, 7), &live_a, 0).is_err());
    drop((live_a, live_b));
    first.cancel(work_a.work_id()).unwrap();
    second.cancel(work_b.work_id()).unwrap();
    assert_eq!(
        (
            first.lease_counters().live(),
            second.lease_counters().live()
        ),
        (0, 0)
    );
}
