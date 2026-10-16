//! Plan-level behavior of the native media export plan (G44): ranges, the
//! manifest, batches, the pins, the ffmpeg argument vector. Literal expected
//! values; the baseline oracle rows live in `native_media_export_plan_oracle.rs`.
use super::*;
use crate::export_job::native_media_export::native_media_export_video::js_number_text;

pub(super) fn pins() -> ExportPins {
    ExportPins {
        instance_id: "instance-1".into(),
        document_id: "document-1".into(),
        content_revision: 7,
        document_snapshot_id: "snapshot-7".into(),
        context_id: "scene-root".into(),
    }
}

pub(super) fn output(width: u32, height: u32) -> OutputSpec {
    OutputSpec::new("frame", "rgba8", width, height, "srgb", "straight").unwrap()
}

pub(super) fn plan_with(
    kind: OutputKind,
    format: FrameFormat,
    start: u32,
    end: u32,
    limit: usize,
) -> Result<MediaExportPlan, PlanError> {
    MediaExportPlan::new(
        kind,
        format,
        pins(),
        output(320, 180),
        FrameRange::new(start, end).unwrap(),
        limit,
    )
}

pub(super) fn sequence(start: u32, end: u32) -> MediaExportPlan {
    plan_with(
        OutputKind::Sequence,
        FrameFormat::Png,
        start,
        end,
        DEFAULT_BATCH_FRAMES,
    )
    .unwrap()
}

pub(super) fn video(fps: Option<f64>, project: f64, quality: Option<&str>) -> MediaExportPlan {
    let spec = VideoSpec::resolve(fps, project, quality).unwrap();
    plan_with(
        OutputKind::Video(spec),
        FrameFormat::Png,
        0,
        47,
        DEFAULT_BATCH_FRAMES,
    )
    .unwrap()
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn ranges_follow_export_frame_range_and_an_explicit_zero_is_not_a_fallback() {
    let area = WorkArea { start: 5, end: 9 };
    let none = RangeRequest::default();
    assert_eq!(frame_range_raw(none, area), (5, 9));
    let zero = RangeRequest {
        start: Some(0),
        end: None,
    };
    assert_eq!(frame_range_raw(zero, area), (0, 9));
    let both = RangeRequest {
        start: Some(2),
        end: Some(0),
    };
    assert_eq!(frame_range_raw(both, area), (2, 0));
    assert_eq!(FrameRange::resolve(zero, area).unwrap().count(), 10);
    assert_eq!(
        FrameRange::resolve(both, area),
        Err(PlanError::EmptyRange { start: 2, end: 0 })
    );
    assert_eq!(FrameRange::new(4, 4).unwrap().count(), 1);
}

#[test]
fn manifest_names_are_one_based_padded_and_map_source_frames() {
    let plan = sequence(10, 14);
    assert_eq!(plan.frame_count(), 5);
    let entry = plan.entry(1).unwrap();
    assert_eq!(
        (entry.ordinal, entry.source_frame, entry.name.as_str()),
        (1, 10, "frame_0001.png")
    );
    let last = plan.entry(5).unwrap();
    assert_eq!(
        (last.ordinal, last.source_frame, last.name.as_str()),
        (5, 14, "frame_0005.png")
    );
    assert_eq!(plan.entry(0), None);
    assert_eq!(plan.entry(6), None);
    let svg = plan_with(OutputKind::Sequence, FrameFormat::Svg, 10, 14, 32).unwrap();
    assert_eq!(svg.entry(3).unwrap().name, "frame_0003.svg");
}

#[test]
fn names_grow_past_four_digits_and_are_never_truncated() {
    let plan = sequence(0, 10_000);
    assert_eq!(plan.frame_count(), 10_001);
    assert_eq!(plan.entry(9_999).unwrap().name, "frame_9999.png");
    assert_eq!(plan.entry(10_000).unwrap().name, "frame_10000.png");
    let last = plan.entry(10_001).unwrap();
    assert_eq!(
        (last.source_frame, last.name.as_str()),
        (10_000, "frame_10001.png")
    );
}

#[test]
fn a_still_is_one_frame_named_like_the_first_sequence_frame() {
    let still = plan_with(OutputKind::Still, FrameFormat::Png, 5, 5, 32).unwrap();
    let entry = still.entry(1).unwrap();
    assert_eq!(
        (entry.source_frame, entry.name.as_str()),
        (5, "frame_0001.png")
    );
    assert_eq!(
        plan_with(OutputKind::Still, FrameFormat::Png, 5, 6, 32),
        Err(PlanError::StillNeedsOneFrame { count: 2 })
    );
}

#[test]
fn ranges_beyond_the_host_bound_are_a_typed_multi_batch_plan() {
    assert_eq!(DEFAULT_BATCH_FRAMES, 32);
    let plan = sequence(0, 69);
    assert_eq!(plan.batch_count(), 3);
    let sizes: Vec<usize> = (0..3)
        .map(|i| plan.batch(i).unwrap().frames.len())
        .collect();
    assert_eq!(sizes, vec![32, 32, 6]);
    assert_eq!(plan.batch(3), None);
    let second = plan.batch(1).unwrap();
    assert_eq!(second.first_ordinal, 33);
    assert_eq!(second.frames.first(), Some(&32));
    assert_eq!(second.staged_names.first().unwrap(), "frame_0001.png");
    assert_eq!(second.final_names.first().unwrap(), "frame_0033.png");
    assert_eq!(second.final_names.last().unwrap(), "frame_0064.png");
    let mut covered: Vec<u32> = Vec::new();
    for index in 0..plan.batch_count() {
        covered.extend(plan.batch(index).unwrap().frames);
    }
    assert_eq!(covered, (0..=69).collect::<Vec<u32>>());
    // Exact multiples, one over, a single frame, and a smaller configured limit.
    assert_eq!(sequence(0, 63).batch_count(), 2);
    assert_eq!(sequence(0, 32).batch_count(), 2);
    assert_eq!(sequence(0, 0).batch_count(), 1);
    let small = plan_with(OutputKind::Sequence, FrameFormat::Png, 0, 11, 5).unwrap();
    let small_sizes: Vec<usize> = (0..small.batch_count())
        .map(|i| small.batch(i).unwrap().frames.len())
        .collect();
    assert_eq!(small_sizes, vec![5, 5, 2]);
}

#[test]
fn the_whole_u32_range_is_lazy_and_exact_at_both_ends() {
    let plan = sequence(0, u32::MAX);
    assert_eq!(plan.frame_count(), 4_294_967_296);
    let last = plan.entry(4_294_967_296).unwrap();
    assert_eq!(
        (last.source_frame, last.name.as_str()),
        (4_294_967_295, "frame_4294967296.png")
    );
    assert_eq!(plan.batch_count(), 134_217_728);
    let tail = plan.batch(134_217_727).unwrap();
    assert_eq!(tail.frames.len(), 32);
    assert_eq!(tail.frames.first(), Some(&4_294_967_264));
    assert_eq!(tail.frames.last(), Some(&4_294_967_295));
    assert_eq!(tail.first_ordinal, 4_294_967_265);
}

#[test]
fn invalid_batch_limits_and_empty_pins_are_refused() {
    for limit in [0usize, 33, 1000] {
        assert_eq!(
            plan_with(OutputKind::Sequence, FrameFormat::Png, 0, 3, limit),
            Err(PlanError::InvalidBatchLimit { limit, maximum: 32 })
        );
    }
    let mut blank = pins();
    blank.document_snapshot_id.clear();
    let refused = MediaExportPlan::new(
        OutputKind::Sequence,
        FrameFormat::Png,
        blank,
        output(8, 8),
        FrameRange::new(0, 1).unwrap(),
        32,
    );
    assert_eq!(refused, Err(PlanError::EmptyPin("document_snapshot_id")));
}

#[test]
fn pins_output_and_schedule_pins_travel_with_the_plan_and_are_detached() {
    let mut source = ObjectSchedulePins {
        instance_id: "instance-1".into(),
        document_id: "document-1".into(),
        content_revision: 7,
        document_snapshot_id: "snapshot-7".into(),
        context_id: "scene-root".into(),
        frame: 99,
        scope: ObjectScheduleScope::Authored,
    };
    let copied = ExportPins::from_schedule(&source);
    assert_eq!(copied, pins());
    source.instance_id.push_str("-mutated");
    source.content_revision = 8;
    let plan = MediaExportPlan::new(
        OutputKind::Sequence,
        FrameFormat::Png,
        copied,
        output(320, 180),
        FrameRange::new(3, 4).unwrap(),
        32,
    )
    .unwrap();
    assert_eq!(plan.pins(), &pins());
    assert_eq!(plan.output().dimensions(), (320, 180));
    let for_frame = plan.schedule_pins(4);
    assert_eq!(
        for_frame,
        ObjectSchedulePins {
            instance_id: "instance-1".into(),
            document_id: "document-1".into(),
            content_revision: 7,
            document_snapshot_id: "snapshot-7".into(),
            context_id: "scene-root".into(),
            frame: 4,
            scope: ObjectScheduleScope::Authored,
        }
    );
}

#[test]
fn video_plans_need_png_frames_and_build_the_baseline_ffmpeg_argv() {
    let spec = VideoSpec::resolve(Some(24.0), 30.0, None).unwrap();
    assert_eq!(
        plan_with(OutputKind::Video(spec), FrameFormat::Svg, 0, 3, 32),
        Err(PlanError::VideoNeedsPngFrames)
    );
    let plan = video(Some(24.0), 30.0, Some("best"));
    assert_eq!(
        plan.mp4_ffmpeg_args("/tmp/sm-export-1", "/tmp/o.mp4")
            .unwrap(),
        strings(&[
            "-y",
            "-framerate",
            "24",
            "-i",
            "/tmp/sm-export-1/frame_%04d.png",
            "-c:v",
            "h264_videotoolbox",
            "-pix_fmt",
            "yuv420p",
            "-q:v",
            "80",
            "-profile:v",
            "high",
            "/tmp/o.mp4",
        ])
    );
    // The directory is joined verbatim, exactly as the baseline concatenation does.
    let doubled = plan.mp4_ffmpeg_args("/tmp/x/", "/o.mp4").unwrap();
    assert_eq!(doubled[4], "/tmp/x//frame_%04d.png");
}

#[test]
fn the_argv_input_pattern_expands_to_exactly_the_planned_names() {
    let plan = video(None, 24.0, None);
    let args = plan.mp4_ffmpeg_args("/d", "/o.mp4").unwrap();
    let pattern = args[4].strip_prefix("/d/").unwrap().to_owned();
    assert_eq!(pattern, "frame_%04d.png");
    for ordinal in 1..=plan.frame_count() {
        let digits = ordinal.to_string();
        let expanded =
            pattern.replace("%04d", &format!("{}{digits}", "0".repeat(4 - digits.len())));
        assert_eq!(expanded, plan.entry(ordinal).unwrap().name);
    }
}

#[test]
fn argv_is_video_only_and_refuses_empty_paths() {
    assert_eq!(
        sequence(0, 1).mp4_ffmpeg_args("/d", "/o.mp4"),
        Err(PlanError::NotVideo)
    );
    let plan = video(None, 24.0, None);
    assert_eq!(
        plan.mp4_ffmpeg_args("", "/o.mp4"),
        Err(PlanError::EmptyPath("staged directory"))
    );
    assert_eq!(
        plan.mp4_ffmpeg_args("/d", ""),
        Err(PlanError::EmptyPath("output path"))
    );
}

#[test]
fn frame_rate_and_quality_use_the_baseline_fallbacks() {
    let text = |fps: Option<f64>, project: f64| {
        VideoSpec::resolve(fps, project, None).map(|spec| spec.fps_text().to_owned())
    };
    assert_eq!(text(None, 30.0), Ok("30".to_owned()));
    assert_eq!(text(Some(0.0), 30.0), Ok("30".to_owned()));
    assert_eq!(text(Some(-0.0), 30.0), Ok("30".to_owned()));
    assert_eq!(text(Some(f64::NAN), 30.0), Ok("30".to_owned()));
    assert_eq!(text(Some(29.97), 30.0), Ok("29.97".to_owned()));
    assert_eq!(text(Some(-5.0), 30.0), Err(PlanError::InvalidFps));
    assert_eq!(text(Some(f64::INFINITY), 30.0), Err(PlanError::InvalidFps));
    assert_eq!(text(None, 0.0), Err(PlanError::InvalidFps));
    assert_eq!(text(None, f64::NAN), Err(PlanError::InvalidFps));
    let qv = |quality: Option<&str>| {
        VideoSpec::resolve(None, 24.0, quality)
            .unwrap()
            .quality_qv()
    };
    assert_eq!(qv(None), "65");
    assert_eq!(qv(Some("draft")), "35");
    assert_eq!(qv(Some("medium")), "50");
    assert_eq!(qv(Some("high")), "65");
    assert_eq!(qv(Some("best")), "80");
    assert_eq!(qv(Some("")), "65");
    assert_eq!(qv(Some("bogus")), "65");
    assert_eq!(qv(Some("HIGH")), "65");
    assert_eq!(qv(Some("constructor")), "65");
}

#[test]
fn js_number_text_matches_known_javascript_strings_and_refuses_non_finite() {
    for (value, expected) in [
        (24.0, "24"),
        (-0.0, "0"),
        (0.5, "0.5"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1.5e-7, "1.5e-7"),
        (1e21, "1e+21"),
        (123456789012345680000.0, "123456789012345680000"),
        (-1e21, "-1e+21"),
    ] {
        assert_eq!(js_number_text(value).as_deref(), Some(expected));
    }
    assert_eq!(js_number_text(f64::NAN), None);
    assert_eq!(js_number_text(f64::INFINITY), None);
}
