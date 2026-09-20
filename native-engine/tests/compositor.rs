use crate::codec::decode_project;
use crate::commands::NativeOpacityApplication;
use crate::compositor::Compositor;
use crate::render_scene::{
    prepare, GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderSceneErrorKind,
    ScheduledFrameIdentity,
};
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, OutputSpec, ScheduledFrame,
};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");

fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap()
}

fn input(offset_x: f64) -> GeometryPaintInput {
    GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::new(
            "r08_curve_layer",
            [20.0, 60.0, 40.0, 80.0],
            [1.0, 0.0, 0.0, 1.0, offset_x, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap()
}

fn scheduled(frame: u32) -> (FrameScheduler, crate::scheduler::ScheduledFrame) {
    scheduled_with(frame, "final", output())
}

fn scheduled_with(
    frame: u32,
    quality: &str,
    output_spec: OutputSpec,
) -> (FrameScheduler, ScheduledFrame) {
    let app =
        NativeOpacityApplication::new("n12-compositor", decode_project(PROJECT).unwrap()).unwrap();
    let snapshot = app.acquire_snapshot(0).unwrap();
    let key = EvaluationKey::new(
        snapshot.id(),
        "scene-root",
        frame,
        quality,
        output_spec,
        [("geometry/r08".to_owned(), "v1".to_owned())],
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let scheduled = scheduler.schedule(snapshot, key).unwrap();
    (scheduler, scheduled)
}

#[test]
fn one_native_compositor_matches_the_frozen_pixel_oracle_and_keeps_leases_live() {
    let mut compositor = Compositor::new().expect("a real native GPU adapter is required for N12");
    for (frame, opacity, offset_x) in [(0, 20, 0.0), (10, 50, 64.0), (20, 80, 128.0)] {
        let (mut scheduler, scheduled) = scheduled(frame);
        let identity = ScheduledFrameIdentity::from_scheduled(&scheduled);
        let before = scheduler.lease_counters();
        let pinned = scheduler.pinned_snapshot(scheduled.work_id()).unwrap();
        let stored_bytes = serde_json::to_vec(pinned.document()).unwrap();
        let snapshot_id = pinned.id().to_owned();
        let document_id = pinned.document_id().to_owned();
        let revision = pinned.content_revision();
        let geometry = input(offset_x);
        let geometry_before = geometry.clone();
        let scene = prepare(&scheduler, &scheduled, &geometry).unwrap();
        assert_eq!(scene.scheduled_identity(), &identity);
        assert_eq!(scene.document_id(), document_id);
        assert_eq!(scene.content_revision(), revision);
        let first = compositor.compose(&scene).unwrap();
        assert_eq!(first.scheduled_identity(), &identity);
        assert_eq!(first.document_id(), document_id);
        assert_eq!(first.content_revision(), revision);
        let _native_preview_view = first.texture_view();
        let first_bytes = compositor.readback_rgba8(&first).unwrap();
        assert_eq!(first_bytes.scheduled_identity(), &identity);
        assert_eq!(first_bytes.document_id(), document_id);
        let repeated_result = compositor.compose(&scene).unwrap();
        let repeat = compositor.readback_rgba8(&repeated_result).unwrap();
        assert_eq!(first_bytes, repeat, "frame {frame} must be byte-identical");
        assert_eq!(
            first_bytes.document_snapshot_id(),
            scheduled.key().document_snapshot_id()
        );
        assert_eq!(first_bytes.frame(), frame);
        assert_eq!(first_bytes.content_revision(), revision);
        assert_eq!(
            geometry, geometry_before,
            "composition cannot mutate immutable geometry input"
        );
        let pinned_after = scheduler.pinned_snapshot(scheduled.work_id()).unwrap();
        assert_eq!(pinned_after.id(), snapshot_id);
        assert_eq!(pinned_after.content_revision(), revision);
        assert_eq!(
            serde_json::to_vec(pinned_after.document()).unwrap(),
            stored_bytes
        );
        assert_eq!(
            scheduler.lease_counters(),
            before,
            "composition must not terminalize work"
        );
        scan(
            &first_bytes,
            opacity,
            [20 + offset_x as u32, 60, 40 + offset_x as u32, 80],
        );
        scheduler.succeed(scheduled.work_id()).unwrap();
        let after = scheduler.lease_counters();
        assert_eq!(
            (after.acquired(), after.released(), after.live()),
            (2, 2, 0)
        );
    }
}

#[test]
fn malformed_or_mismatched_input_fails_before_any_partial_publish() {
    let (mut scheduler, first_scheduled) = scheduled(0);
    let wrong_resource = GeometryPaintInput::new("geometry/r08", "v2", vec![]).unwrap();
    assert!(
        matches!(prepare(&scheduler, &first_scheduled, &wrong_resource), Err(error) if error.kind() == RenderSceneErrorKind::ResourceMismatch)
    );
    assert_rejection_is_live(&scheduler, &first_scheduled);
    scheduler
        .fail(
            first_scheduled.work_id(),
            FrameFailure::new(FrameFailureKind::Evaluation, "reject mismatched resource"),
        )
        .unwrap();
    assert_released(&scheduler);

    let (mut scheduler, scheduled) = scheduled(0);
    let unmatched = GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::new(
            "wrong",
            [20.0, 60.0, 40.0, 80.0],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap();
    assert!(
        matches!(prepare(&scheduler, &scheduled, &unmatched), Err(error) if error.kind() == RenderSceneErrorKind::LayerJoin)
    );
    assert_rejection_is_live(&scheduler, &scheduled);
    scheduler.cancel(scheduled.work_id()).unwrap();
    assert_released(&scheduler);
}

#[test]
fn only_the_characterized_final_output_contract_is_admitted() {
    let rejected = [
        ("draft", output()),
        (
            "final",
            OutputSpec::new("frame", "rgba8", 321, 180, "srgb", "straight").unwrap(),
        ),
        (
            "final",
            OutputSpec::new("tile", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        ),
        (
            "final",
            OutputSpec::new("frame", "rgba16f", 320, 180, "srgb", "straight").unwrap(),
        ),
        (
            "final",
            OutputSpec::new("frame", "rgba8", 320, 180, "display-p3", "straight").unwrap(),
        ),
        (
            "final",
            OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "premultiplied").unwrap(),
        ),
    ];
    for (quality, output_spec) in rejected {
        let (mut scheduler, scheduled) = scheduled_with(0, quality, output_spec);
        assert!(
            matches!(prepare(&scheduler, &scheduled, &input(0.0)), Err(error) if error.kind() == RenderSceneErrorKind::Unsupported)
        );
        assert_rejection_is_live(&scheduler, &scheduled);
        scheduler.cancel(scheduled.work_id()).unwrap();
        assert_released(&scheduler);
    }
}

fn assert_rejection_is_live(scheduler: &FrameScheduler, scheduled: &ScheduledFrame) {
    assert!(scheduler.receipt(scheduled.work_id()).is_none());
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 0, 2)
    );
}

fn assert_released(scheduler: &FrameScheduler) {
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}

fn scan(readback: &crate::compositor::NativeRgba8Readback, opacity: u8, bounds: [u32; 4]) {
    assert_eq!(readback.dimensions(), (320, 180));
    assert_eq!(readback.bytes().len(), 320 * 180 * 4);
    let mut colored = 0;
    let mut found = [320, 180, 0, 0];
    let expected = 255.0 * (1.0 - f64::from(opacity) / 100.0);
    for (index, pixel) in readback.bytes().chunks_exact(4).enumerate() {
        let x = (index % 320) as u32;
        let y = (index / 320) as u32;
        assert_eq!(pixel[3], 255, "all output pixels are opaque");
        if pixel == [255, 255, 255, 255] {
            continue;
        }
        colored += 1;
        found = [
            found[0].min(x),
            found[1].min(y),
            found[2].max(x + 1),
            found[3].max(y + 1),
        ];
        assert_eq!(pixel[0], 255);
        assert!((f64::from(pixel[1]) - expected).abs() <= 1.0);
        assert_eq!(pixel[1], pixel[2]);
    }
    assert_eq!(colored, 400);
    assert_eq!(320 * 180 - colored, 57_200);
    assert_eq!(found, bounds);
}
