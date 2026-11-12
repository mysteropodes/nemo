//! nemo-test-harness: test-export_job => export_job_lifecycle_tests
use crate::export_job::{
    CleanupReceipt, CleanupStatus, ExportBegin, ExportCompositor, ExportFrameInput,
    ExportJobManager, ExportReadback, ExternalEffectDisposition, JobError, JobReceipt, JobStatus,
    StagedArtifactPort,
};
use crate::export_job_lifecycle::{cleanup_allows_release, retry_failed_cleanup, terminalize};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use crate::{codec::decode_project, history::NativeOpacityHistory};

#[derive(Default)]
struct Port {
    cleanup_calls: usize,
    fail_cleanup: bool,
    cleanup_failures_remaining: usize,
    fail_write: bool,
    writes: usize,
    publishes: usize,
}

impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        if self.fail_write {
            return Err("injected write failure".into());
        }
        self.writes += 1;
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<crate::export_job::ExportArtifact, String> {
        self.publishes += 1;
        Ok(crate::export_job::ExportArtifact {
            target: "out".into(),
            files: Vec::new(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.cleanup_calls += 1;
        if self.fail_cleanup || self.cleanup_failures_remaining > 0 {
            self.cleanup_failures_remaining = self.cleanup_failures_remaining.saturating_sub(1);
            Err("injected cleanup failure".into())
        } else {
            Ok(())
        }
    }
}

struct Compositor;
impl ExportCompositor for Compositor {
    type Composition = ExportReadback;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        Ok(ExportReadback {
            width: 320,
            height: 180,
            bytes: [1, 2, 3, 255].repeat(320 * 180).try_into().unwrap(),
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

fn manager_fixture() -> (NativeOpacityHistory, ExportBegin) {
    let history = NativeOpacityHistory::new(
        "release-lifecycle",
        decode_project(include_bytes!("fixtures/opacity-v2/project.json")).unwrap(),
    )
    .unwrap();
    let geometry = GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::new(
            "r08_curve_layer",
            [20.0, 60.0, 40.0, 80.0],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap();
    let request = ExportBegin {
        request_id: "release".into(),
        expected_revision: 0,
        context_id: "scene-root".into(),
        quality: "final".into(),
        target: "out".into(),
        frames: [0, 1]
            .into_iter()
            .map(|frame| ExportFrameInput::new(frame, geometry.clone()))
            .collect(),
    };
    (history, request)
}

fn running() -> JobReceipt {
    JobReceipt {
        job_id: "job-1".into(),
        status: JobStatus::Running,
        pinned_revision: 0,
        document_snapshot_id: "snapshot-1".into(),
        progress: 0.0,
        artifact: None,
        cleanup: CleanupReceipt {
            status: CleanupStatus::Pending,
            error: None,
        },
        external_effect_disposition: ExternalEffectDisposition::None,
        error: None,
    }
}

#[test]
fn terminal_cleanup_failure_is_indeterminate_and_retry_contains_it() {
    let mut port = Port {
        fail_cleanup: true,
        ..Port::default()
    };
    let mut receipt = running();
    terminalize(
        &mut port,
        &mut receipt,
        JobStatus::Cancelled,
        Some(JobError {
            code: "authority_released",
            message: "released".into(),
            details: None,
        }),
    );
    assert_eq!(receipt.status, JobStatus::Failed);
    assert_eq!(receipt.cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert_eq!(port.cleanup_calls, 1);

    port.fail_cleanup = false;
    retry_failed_cleanup(&mut port, &mut receipt);
    assert_eq!(receipt.status, JobStatus::Failed);
    assert_eq!(receipt.cleanup.status, CleanupStatus::Complete);
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::None
    );
    assert_eq!(port.cleanup_calls, 2);
}

#[test]
fn successful_committed_receipt_is_not_cleaned_again() {
    let mut port = Port::default();
    let mut receipt = running();
    receipt.status = JobStatus::Succeeded;
    receipt.cleanup.status = CleanupStatus::Complete;
    receipt.external_effect_disposition = ExternalEffectDisposition::Committed;
    retry_failed_cleanup(&mut port, &mut receipt);
    assert_eq!(port.cleanup_calls, 0);
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Committed
    );
}

#[test]
fn pending_cleanup_never_authorizes_release() {
    assert!(!cleanup_allows_release(&running()));
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
#[test]
fn cpu_pending_png_resource_survives_terminal_clone_and_manager_drop() {
    const KEY: &str = "NEMO_PENDING_PNG_RESOURCE_TEST";
    let name = concat!(
        module_path!(),
        "::cpu_pending_png_resource_survives_terminal_clone_and_manager_drop"
    )
    .split_once("::")
    .unwrap()
    .1;
    if std::env::var(KEY).as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(KEY, name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    let budget = crate::revision::shared_image_resource_budget();
    for finish in [false, true] {
        assert_eq!(budget.used().unwrap(), 0);
        let (history, mut request) = manager_fixture();
        request.frames.truncate(1);
        let mut manager = ExportJobManager::new(Port::default(), Compositor);
        let begun = manager.begin(&history, request).unwrap();
        let pending = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
        let retained = budget.used().unwrap();
        assert!(
            retained > 0,
            "pending real PNG output must retain its charge"
        );
        let clone = pending.clone();
        assert_eq!(budget.used().unwrap(), retained);
        let terminal = if finish {
            manager.finish_frame(pending.clone()).unwrap()
        } else {
            let released = manager.reconcile_release();
            assert!(released.cleanup_complete);
            released.receipts[0].clone()
        };
        assert_eq!(
            terminal.status,
            if finish {
                JobStatus::Succeeded
            } else {
                JobStatus::Cancelled
            }
        );
        assert_eq!(manager.finish_frame(pending.clone()).unwrap(), terminal);
        let effects = if finish { (1, 1) } else { (0, 0) };
        assert_eq!((manager.port().writes, manager.port().publishes), effects);
        assert_eq!(manager.lease_counters().live(), 0);
        assert_eq!(budget.used().unwrap(), retained);
        drop(manager);
        drop(history);
        drop(pending);
        assert_eq!(budget.used().unwrap(), retained);
        drop(clone);
        assert_eq!(budget.used().unwrap(), 0);
    }
}

#[test]
fn manager_release_fences_a_captured_frame_and_cleans_exactly_once() {
    let (history, request) = manager_fixture();
    let mut manager = ExportJobManager::new(Port::default(), Compositor);
    let begun = manager.begin(&history, request).unwrap();
    let late = manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    let released = manager.reconcile_release();
    let cancelled = released.receipts[0].clone();
    assert!(released.cleanup_complete);
    assert_eq!(manager.finish_frame(late).unwrap(), cancelled);
    assert_eq!(manager.port().cleanup_calls, 1);
    assert_eq!((manager.port().writes, manager.port().publishes), (0, 0));
    assert_eq!(manager.lease_counters().live(), 0);
}

#[test]
fn manager_release_retries_terminal_failed_cleanup_and_remains_indeterminate() {
    let (history, mut request) = manager_fixture();
    request.frames.truncate(1);
    let port = Port {
        fail_write: true,
        fail_cleanup: true,
        ..Port::default()
    };
    let mut manager = ExportJobManager::new(port, Compositor);
    let begun = manager.begin(&history, request).unwrap();
    assert_eq!(
        manager
            .run_to_completion(&begun.job_id)
            .unwrap()
            .cleanup
            .status,
        CleanupStatus::Failed
    );
    let released = manager.reconcile_release();
    assert!(!released.cleanup_complete);
    assert_eq!(released.receipts[0].cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        released.receipts[0].external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert_eq!(manager.port().cleanup_calls, 2);
    assert_eq!(manager.port().publishes, 0);
}

#[test]
fn fresh_release_cleanup_failure_is_not_retried_in_the_same_release() {
    let (history, request) = manager_fixture();
    let port = Port {
        cleanup_failures_remaining: 1,
        ..Port::default()
    };
    let mut manager = ExportJobManager::new(port, Compositor);
    manager.begin(&history, request).unwrap();
    let released = manager.reconcile_release();
    assert!(!released.cleanup_complete);
    assert_eq!(manager.port().cleanup_calls, 1);
    assert_eq!(released.receipts[0].cleanup.status, CleanupStatus::Failed);
}

#[test]
fn scheduler_cancel_failure_keeps_release_indeterminate_with_a_live_lease() {
    let (history, request) = manager_fixture();
    let mut manager = ExportJobManager::new(Port::default(), Compositor);
    let begun = manager.begin(&history, request).unwrap();
    manager.start_next_frame(&begun.job_id).unwrap().unwrap();
    manager.inject_release_cancel_failure(true);
    let released = manager.reconcile_release();
    assert!(!released.cleanup_complete);
    assert!(manager.lease_counters().live() > 0);
    assert_eq!(released.receipts[0].cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        released.receipts[0].external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
#[test]
fn cpu_public_png_resource_retains_shared_output_and_cleans_partial_refusals() {
    const KEY: &str = "NEMO_PUBLIC_PNG_RESOURCE_TEST";
    let name = concat!(
        module_path!(),
        "::cpu_public_png_resource_retains_shared_output_and_cleans_partial_refusals"
    )
    .split_once("::")
    .unwrap()
    .1;
    if std::env::var(KEY).as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(KEY, name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    let budget = crate::revision::shared_image_resource_budget();
    use crate::export_job_tests::decode;
    use crate::png_output::encode_rgba8;
    let mut seed = 0x1234_5678u32;
    let mut pixels = vec![0; 256 * 64 * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        pixel.copy_from_slice(&seed.to_le_bytes());
    }
    pixels[..12].copy_from_slice(&[17, 34, 51, 0, 17, 34, 51, 127, 99, 88, 77, 255]);
    assert_eq!(budget.used().unwrap(), 0);
    let encoded = encode_rgba8(256, 64, &pixels).unwrap();
    let retained = budget.used().unwrap();
    assert!(
        retained > encoded.len(),
        "actual encoded storage, including spare capacity, must stay charged"
    );
    assert_eq!(decode(&encoded), (256, 64, pixels.clone()));
    let clone = encoded.clone();
    assert_eq!(encoded.as_ptr(), clone.as_ptr());
    assert_eq!(budget.used().unwrap(), retained);
    drop(encoded);
    assert_eq!(budget.used().unwrap(), retained);
    // Reserve scalars, never a hundreds-of-MiB test allocation.
    let maximum = 384 * 1024 * 1024;
    let pressure = budget.reserve(maximum - retained).unwrap();
    assert!(encode_rgba8(1, 1, &[1, 2, 3, 4]).is_err());
    assert_eq!(budget.used().unwrap(), maximum);
    drop(pressure);
    assert_eq!(decode(&clone), (256, 64, pixels.clone()));
    drop(clone);
    assert_eq!(budget.used().unwrap(), 0);
    for available in [7, 40, 4096] {
        let pressure = budget.reserve(maximum - available).unwrap();
        assert!(encode_rgba8(256, 64, &pixels).is_err());
        assert_eq!(budget.used().unwrap(), maximum - available);
        drop(pressure);
        assert_eq!(budget.used().unwrap(), 0);
    }
    let recovered = encode_rgba8(256, 64, &pixels).unwrap();
    assert_eq!(decode(&recovered), (256, 64, pixels));
    drop(recovered);
    assert_eq!(budget.used().unwrap(), 0);
}
