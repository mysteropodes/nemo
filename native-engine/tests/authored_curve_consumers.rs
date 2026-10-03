use crate::codec::{decode_project, encode_project};
use crate::compositor::{CompositionResult, Compositor};
use crate::export_job::{ExportCompositor, ExportJobManager, ExportReadback, JobStatus};
use crate::export_job_tests::{decode, edit_to_revision_one, geometry, request, MemoryPort};
use crate::history::NativeOpacityHistory;
use crate::render_scene::{prepare, RenderScene, RenderSceneErrorKind};
use crate::revision::DocumentSnapshot;
use crate::scheduler::{EvaluationKey, FrameScheduler, OutputSpec};
use serde_json::{json, Value};

const FRAMES: [u32; 5] = [0, 5, 10, 15, 20];
// Independent polynomial: x=t, y=4t-3t², opacity=20+40y.
const OPACITY: [f64; 5] = [20.0, 52.5, 70.0, 72.5, 60.0];
// Opaque red over white: green/blue = round(255 * (1-opacity/100)).
const GREEN: [i16; 5] = [204, 121, 77, 70, 102];

/// Observe identity at the real scene/compositor boundary; every pixel still
/// comes from the production GPU compositor and its production readback.
struct PinnedCompositor {
    gpu: Compositor,
    snapshot: DocumentSnapshot,
}

impl ExportCompositor for PinnedCompositor {
    type Composition = CompositionResult;

    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        assert_eq!(scene.document_snapshot_id(), self.snapshot.id());
        assert_eq!(scene.document_id(), self.snapshot.document_id());
        assert_eq!(scene.content_revision(), 0);
        assert_eq!(scene.context_id(), "scene-root");
        assert_eq!(scene.quality(), "final");
        let index = FRAMES
            .iter()
            .position(|frame| *frame == scene.frame())
            .unwrap();
        assert!((scene.layers[0].opacity_percent - OPACITY[index]).abs() < 1e-12);
        ExportCompositor::compose(&mut self.gpu, scene)
    }

    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        let pixels = ExportCompositor::readback_rgba8(&self.gpu, result)?;
        assert_eq!(pixels.document_snapshot_id, self.snapshot.id());
        assert_eq!(pixels.document_id, self.snapshot.document_id());
        assert_eq!(pixels.content_revision, 0);
        Ok(pixels)
    }
}

#[test]
fn authored_overshoot_exports_real_pixels_from_the_pinned_revision() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/animation-curves/authored.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "overshoot")
        .unwrap();
    assert_eq!(case["opacity"], json!([20, 52.5, 70, 72.5, 60]));
    let mut project: Value =
        serde_json::from_slice(include_bytes!("fixtures/opacity-v2/project.json")).unwrap();
    project["layers"][0]["motion"]["opacity"]["keys"][0]["curvePoints"] = case["points"].clone();
    project["layers"][0]["motion"]["opacity"]["keys"][1]["v"] = json!([60]);
    let document = decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    let mut history = NativeOpacityHistory::new("n22b-authored-export", document).unwrap();
    let pinned = history.acquire_snapshot(0).unwrap();
    let before = encode_project(pinned.document()).unwrap();
    let gpu = Compositor::new().expect("a real native GPU adapter is required for N22B");
    let compositor = PinnedCompositor {
        gpu,
        snapshot: pinned.clone(),
    };
    let mut manager = ExportJobManager::new(MemoryPort::default(), compositor);
    let begun = manager
        .begin(&history, request("authored-pin", 0, &FRAMES))
        .unwrap();
    let first = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    manager.finish_frame(first).unwrap();

    // Only the stored static field changes; the authored track stays intact.
    edit_to_revision_one(&mut history);
    let current = history.acquire_snapshot(1).unwrap();
    assert_ne!(current.id(), pinned.id());
    assert_eq!(current.static_opacity("r08_curve_layer"), Some(40.0));
    assert_eq!(pinned.static_opacity("r08_curve_layer"), Some(25.0));
    assert_eq!(
        current.document().layers()[0].opacity_keys(),
        pinned.document().layers()[0].opacity_keys()
    );
    let receipt = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(receipt.pinned_revision, 0);
    assert_eq!(receipt.document_snapshot_id, pinned.id());
    assert_eq!(receipt.artifact.as_ref().unwrap().files.len(), FRAMES.len());
    assert_eq!(manager.port().write_log.len(), FRAMES.len());
    for (index, (name, png)) in manager.port().write_log.iter().enumerate() {
        assert_eq!(*name, format!("frame_{:04}.png", index + 1));
        let (width, height, rgba) = decode(png);
        assert_eq!((width, height, rgba.len()), (320, 180, 320 * 180 * 4));
        for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
            let (x, y) = (pixel_index % 320, pixel_index / 320);
            if (20..40).contains(&x) && (60..80).contains(&y) {
                assert_eq!((pixel[0], pixel[3]), (255, 255));
                assert!(
                    (i16::from(pixel[1]) - GREEN[index]).abs() <= 1,
                    "frame {}: {pixel:?}",
                    FRAMES[index]
                );
                assert_eq!(pixel[1], pixel[2]);
            } else {
                assert_eq!(pixel, [255, 255, 255, 255]);
            }
        }
    }
    assert_eq!(encode_project(pinned.document()).unwrap(), before);
    assert_eq!(history.content_revision(), 1);
    let leases = manager.lease_counters();
    assert_eq!(
        (leases.acquired(), leases.released(), leases.live()),
        (10, 10, 0)
    );
}

#[test]
fn finite_native_opacity_outside_gpu_alpha_range_fails_before_scene_publication() {
    let mut project: Value =
        serde_json::from_slice(include_bytes!("fixtures/opacity-v2/project.json")).unwrap();
    project["layers"][0]["motion"]["opacity"]["keys"][0]["curvePoints"] =
        json!([{"x":0,"y":0},{"x":1,"y":1e40}]);
    project["layers"][0]["motion"]["opacity"]["keys"][1]["v"] = json!([60]);
    let document = decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    let history = NativeOpacityHistory::new("n22b-alpha-boundary", document).unwrap();
    let snapshot = history.acquire_snapshot(0).unwrap();
    let before = encode_project(snapshot.document()).unwrap();
    let value = crate::evaluation::evaluate(&snapshot, "scene-root", 10)
        .unwrap()
        .layers()[0]
        .value();
    assert!(value.is_finite());
    assert!(!((value / 100.0) as f32).is_finite());
    let key = EvaluationKey::new(
        snapshot.id(),
        "scene-root",
        10,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        [("geometry/r08".to_owned(), "v1".to_owned())],
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule(snapshot.clone(), key).unwrap();
    let error = prepare(&scheduler, &work, &geometry(0.0)).unwrap_err();
    assert_eq!(error.kind(), RenderSceneErrorKind::InvalidInput);
    assert_eq!(history.content_revision(), 0);
    assert_eq!(encode_project(snapshot.document()).unwrap(), before);
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}
