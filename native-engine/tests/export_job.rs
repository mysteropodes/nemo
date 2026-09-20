use crate::codec::decode_project;
use crate::commands::OpacityRequest;
use crate::compositor::Compositor;
use crate::export_job::{
    CleanupStatus, ExportBegin, ExportCompositor, ExportFrameInput, ExportJobErrorKind,
    ExportJobManager, ExportReadback, ExternalEffectDisposition, JobStatus, StagedArtifactPort,
};
use crate::history::NativeOpacityHistory;
use crate::png_output::{encode_rgba8, ExportArtifact};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use serde_json::json;
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");

fn history(label: &str) -> NativeOpacityHistory {
    NativeOpacityHistory::new(label, decode_project(PROJECT).unwrap()).unwrap()
}

fn geometry(offset: f64) -> GeometryPaintInput {
    GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::new(
            "r08_curve_layer",
            [20.0, 60.0, 40.0, 80.0],
            [1.0, 0.0, 0.0, 1.0, offset, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap()
}

fn request(id: &str, revision: u64, frames: &[u32]) -> ExportBegin {
    ExportBegin {
        request_id: id.into(),
        expected_revision: revision,
        context_id: "scene-root".into(),
        quality: "final".into(),
        target: "/published/sequence".into(),
        frames: frames
            .iter()
            .map(|frame| ExportFrameInput::new(*frame, geometry(0.0)))
            .collect(),
    }
}

#[derive(Default)]
struct MemoryPort {
    events: Vec<String>,
    staged: Vec<(String, Vec<u8>)>,
    write_log: Vec<(String, Vec<u8>)>,
    published: Vec<ExportArtifact>,
    fail_write: Option<usize>,
    fail_cleanup: bool,
}

impl StagedArtifactPort for MemoryPort {
    fn begin_staging(&mut self, job_id: &str, target: &str) -> Result<(), String> {
        self.events.push(format!("begin:{job_id}:{target}"));
        Ok(())
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        let ordinal = self.write_log.len() + 1;
        self.events.push(format!("write:{name}"));
        if self.fail_write == Some(ordinal) {
            return Err(format!("injected write {ordinal}"));
        }
        let entry = (name.into(), bytes.to_vec());
        self.staged.push(entry.clone());
        self.write_log.push(entry);
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.events.push("publish".into());
        let artifact = ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        };
        self.staged.clear();
        self.published.push(artifact.clone());
        Ok(artifact)
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.events.push("cleanup".into());
        if self.fail_cleanup {
            Err("injected cleanup".into())
        } else {
            self.staged.clear();
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
enum PixelMode {
    Frame,
    Revision,
}

struct FakeCompositor {
    mode: PixelMode,
    calls: RefCell<Vec<String>>,
    readbacks: Rc<RefCell<Vec<ExportReadback>>>,
}

impl FakeCompositor {
    fn new(mode: PixelMode) -> Self {
        Self {
            mode,
            calls: RefCell::new(Vec::new()),
            readbacks: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl ExportCompositor for FakeCompositor {
    type Composition = ExportReadback;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        self.calls
            .borrow_mut()
            .push(format!("compose:{}", scene.frame()));
        let value = match self.mode {
            PixelMode::Frame => scene.frame() as u8,
            PixelMode::Revision => scene.content_revision() as u8,
        };
        Ok(ExportReadback {
            width: 320,
            height: 180,
            bytes: [value, 20, 30, 255].repeat(320 * 180),
            document_snapshot_id: scene.document_snapshot_id().into(),
            document_id: scene.document_id().into(),
            content_revision: scene.content_revision(),
            context_id: scene.context_id().into(),
            source_frame: scene.frame(),
            quality: scene.quality().into(),
        })
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        self.calls
            .borrow_mut()
            .push(format!("readback:{}", result.source_frame));
        self.readbacks.borrow_mut().push(result.clone());
        Ok(result.clone())
    }
}

struct ExtremeReadbackCompositor;

impl ExportCompositor for ExtremeReadbackCompositor {
    type Composition = ExportReadback;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        Ok(ExportReadback {
            width: u32::MAX,
            height: u32::MAX,
            bytes: Vec::new(),
            document_snapshot_id: scene.document_snapshot_id().into(),
            document_id: scene.document_id().into(),
            content_revision: scene.content_revision(),
            context_id: scene.context_id().into(),
            source_frame: scene.frame(),
            quality: scene.quality().into(),
        })
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        Ok(result.clone())
    }
}

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    (info.width, info.height, rgba)
}

fn edit_to_revision_one(history: &mut NativeOpacityHistory) {
    let command = OpacityRequest::command(
        "n14-edit-r1",
        history.instance_id(),
        history.document_id(),
        0,
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": "r08_curve_layer" },
            "value": 40
        }),
    );
    assert!(history.handle(command).is_ok());
    assert_eq!(history.content_revision(), 1);
}

#[test]
fn publishes_one_complete_artifact_with_ordinal_names_and_decoded_rgba8() {
    let history = history("n14-success");
    let mut manager =
        ExportJobManager::new(MemoryPort::default(), FakeCompositor::new(PixelMode::Frame));
    let begun = manager
        .begin(&history, request("success", 0, &[10, 12, 20]))
        .unwrap();
    assert_eq!(begun.artifact, None);
    assert_eq!(begun.cleanup.status, CleanupStatus::Pending);
    assert_eq!(
        begun.external_effect_disposition,
        ExternalEffectDisposition::None
    );
    let receipt = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(receipt.progress, 1.0);
    let names = ["frame_0001.png", "frame_0002.png", "frame_0003.png"];
    assert_eq!(receipt.artifact.as_ref().unwrap().files, names);
    assert_eq!(manager.port().published.len(), 1);
    assert!(manager.port().staged.is_empty());
    assert_eq!(
        manager
            .port()
            .write_log
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<Vec<_>>(),
        names
    );
    for ((_, png), value) in manager.port().write_log.iter().zip([10, 12, 20]) {
        let (width, height, rgba) = decode(png);
        assert_eq!((width, height, rgba.len()), (320, 180, 320 * 180 * 4));
        assert_eq!(&rgba[..4], &[value, 20, 30, 255]);
        assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }
    let counters = manager.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (6, 6, 0)
    );
}

#[test]
fn production_compositor_exports_the_frozen_frame_zero_pixel_oracle() {
    let history = history("n14-real-compositor");
    let compositor = Compositor::new().expect("a real native GPU adapter is required for N14");
    let mut manager = ExportJobManager::new(MemoryPort::default(), compositor);
    let begun = manager
        .begin(&history, request("real-compositor", 0, &[0]))
        .unwrap();
    let receipt = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    let (_, _, rgba) = decode(&manager.port().write_log[0].1);
    let mut colored = 0;
    let mut bounds = [320, 180, 0, 0];
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let x = (index % 320) as u32;
        let y = (index / 320) as u32;
        assert_eq!(pixel[3], 255);
        if pixel == [255, 255, 255, 255] {
            continue;
        }
        colored += 1;
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x + 1),
            bounds[3].max(y + 1),
        ];
        assert_eq!(pixel[0], 255);
        assert!((i16::from(pixel[1]) - 204).abs() <= 1);
        assert_eq!(pixel[1], pixel[2]);
    }
    assert_eq!(colored, 400);
    assert_eq!(bounds, [20, 60, 40, 80]);
}

#[test]
fn pinned_revision_survives_edit_with_stable_encoded_and_decoded_bytes() {
    let mut history = history("n14-pin");
    let compositor = FakeCompositor::new(PixelMode::Revision);
    let observed = Rc::clone(&compositor.readbacks);
    let mut manager = ExportJobManager::new(MemoryPort::default(), compositor);
    let begun = manager
        .begin(&history, request("pin", 0, &[10, 11]))
        .unwrap();
    let first = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    manager.finish_frame(first).unwrap();
    edit_to_revision_one(&mut history);
    let receipt = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(receipt.pinned_revision, 0);
    assert_eq!(receipt.document_snapshot_id, begun.document_snapshot_id);
    let writes = &manager.port().write_log;
    assert_eq!(
        writes[0].1, writes[1].1,
        "fixed encoder bytes are stable within N14"
    );
    let first = decode(&writes[0].1);
    let second = decode(&writes[1].1);
    assert_eq!(first, second);
    assert_eq!((first.0, first.1, first.2.len()), (320, 180, 320 * 180 * 4));
    assert_eq!(&first.2[..4], &[0, 20, 30, 255]);
    let readbacks = observed.borrow();
    assert_eq!(readbacks.len(), 2);
    assert_eq!(readbacks[0].bytes, readbacks[1].bytes);
    for (pixels, source_frame) in readbacks.iter().zip([10, 11]) {
        assert_eq!(
            (pixels.width, pixels.height, pixels.bytes.len()),
            (320, 180, 320 * 180 * 4)
        );
        assert_eq!(pixels.document_snapshot_id, begun.document_snapshot_id);
        assert_eq!(pixels.content_revision, 0);
        assert_eq!(pixels.context_id, "scene-root");
        assert_eq!(pixels.source_frame, source_frame);
        assert_eq!(pixels.quality, "final");
    }
}

#[test]
fn cancel_is_exact_idempotent_and_releases_in_flight_work_once() {
    let history = history("n14-cancel");
    let mut manager =
        ExportJobManager::new(MemoryPort::default(), FakeCompositor::new(PixelMode::Frame));
    let begun = manager
        .begin(&history, request("cancel", 0, &[1, 2]))
        .unwrap();
    let staged = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    manager.finish_frame(staged).unwrap();
    assert_eq!(manager.port().staged.len(), 1);
    let _late = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    assert_eq!(manager.lease_counters().live(), 2);
    let cancelled = manager.cancel(&begun.job_id).unwrap();
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert_eq!(cancelled.artifact, None);
    assert_eq!(cancelled.cleanup.status, CleanupStatus::Complete);
    assert_eq!(cancelled.error, None);
    assert!(manager.port().staged.is_empty());
    assert_eq!(manager.cancel(&begun.job_id).unwrap(), cancelled);
    assert_eq!(manager.status(&begun.job_id).unwrap(), cancelled);
    let counters = manager.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (4, 4, 0)
    );
    assert_eq!(
        manager
            .port()
            .events
            .iter()
            .filter(|event| *event == "cleanup")
            .count(),
        1
    );
}

#[test]
fn replacement_reconciles_old_job_and_late_completion_cannot_publish() {
    let mut history = history("n14-replace");
    let mut manager =
        ExportJobManager::new(MemoryPort::default(), FakeCompositor::new(PixelMode::Frame));
    let begun = manager
        .begin(&history, request("replace", 0, &[7]))
        .unwrap();
    let late = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    history
        .replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    let replacements = manager.replace_document(history.document_id()).unwrap();
    assert_eq!(replacements.len(), 1);
    assert_eq!(replacements[0].status, JobStatus::Cancelled);
    assert_eq!(
        replacements[0].error.as_ref().unwrap().code,
        "document_replaced"
    );
    let next = manager
        .begin(&history, request("replace", 0, &[8]))
        .expect("replacement ends the old document requestId lifetime");
    assert_ne!(next.job_id, begun.job_id);
    assert_eq!(manager.finish_frame(late).unwrap(), replacements[0]);
    assert!(manager.port().staged.is_empty());
    assert!(manager.port().write_log.is_empty());
    assert!(manager.port().published.is_empty());
    assert_eq!(manager.lease_counters().live(), 0);
    manager.cancel(&next.job_id).unwrap();
}

#[test]
fn write_and_cleanup_failures_never_publish_partial_success() {
    let history = history("n14-write-failure");
    let port = MemoryPort {
        fail_write: Some(2),
        ..MemoryPort::default()
    };
    let mut manager = ExportJobManager::new(port, FakeCompositor::new(PixelMode::Frame));
    let begun = manager
        .begin(&history, request("write-failure", 0, &[1, 2, 3]))
        .unwrap();
    let failed = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.progress, 1.0 / 3.0);
    assert_eq!(failed.artifact, None);
    assert_eq!(failed.cleanup.status, CleanupStatus::Complete);
    assert!(manager.port().staged.is_empty());
    assert_eq!(manager.port().write_log.len(), 1);
    assert!(manager.port().published.is_empty());
    assert_eq!(manager.lease_counters().live(), 0);

    let port = MemoryPort {
        fail_write: Some(2),
        fail_cleanup: true,
        ..MemoryPort::default()
    };
    let mut manager = ExportJobManager::new(port, FakeCompositor::new(PixelMode::Frame));
    let begun = manager
        .begin(&history, request("cleanup-failure", 0, &[1, 2]))
        .unwrap();
    let failed = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        failed.cleanup.error.as_ref().unwrap().code,
        "cleanup_failed"
    );
    assert_eq!(failed.error.as_ref().unwrap().code, "cleanup_failed");
    assert_eq!(
        failed.external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert_eq!(failed.artifact, None);
    assert_eq!(manager.port().staged.len(), 1);
}

#[test]
fn begin_retry_reuses_the_exact_job_and_changed_body_is_rejected() {
    let history = history("n14-retry");
    let mut manager =
        ExportJobManager::new(MemoryPort::default(), FakeCompositor::new(PixelMode::Frame));
    let body = request("retry", 0, &[3]);
    let begun = manager.begin(&history, body.clone()).unwrap();
    let terminal = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(manager.begin(&history, body).unwrap(), terminal);
    assert_eq!(
        manager
            .port()
            .events
            .iter()
            .filter(|event| event.starts_with("begin:"))
            .count(),
        1
    );
    assert_eq!(manager.port().write_log.len(), 1);
    let changed = manager
        .begin(&history, request("retry", 0, &[4]))
        .unwrap_err();
    assert_eq!(changed.kind, ExportJobErrorKind::ChangedRequestId);
}

#[test]
fn png_encoder_rejects_invalid_shape_before_output() {
    assert!(encode_rgba8(0, 180, &[]).is_err());
    assert!(encode_rgba8(320, 180, &[0; 4]).is_err());
    let pixels = [9, 8, 7, 255].repeat(320 * 180);
    let first = encode_rgba8(320, 180, &pixels).unwrap();
    let second = encode_rgba8(320, 180, &pixels).unwrap();
    assert_eq!(first, second);
    assert_eq!(decode(&first), (320, 180, pixels));
}

#[test]
fn extreme_readback_dimensions_fail_without_panicking_or_publishing() {
    let history = history("n14-extreme-readback");
    let mut manager = ExportJobManager::new(MemoryPort::default(), ExtremeReadbackCompositor);
    let begun = manager
        .begin(&history, request("extreme-readback", 0, &[0]))
        .unwrap();
    let failed = manager.run_to_completion(&begun.job_id).unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.artifact, None);
    assert_eq!(failed.cleanup.status, CleanupStatus::Complete);
    assert!(manager.port().published.is_empty());
    assert!(manager.port().staged.is_empty());
    assert_eq!(manager.lease_counters().live(), 0);
}
