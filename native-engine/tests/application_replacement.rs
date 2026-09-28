//! Independent core replacement oracles against the committed opacity-v2 fixture.
use crate::application::{
    ExportResourceResolver, NativeApplication, ReplacementFailureKind, ReplacementPhase,
    ResourceResolutionError,
};
use crate::codec::decode_project;
use crate::commands::{DispatchErrorCode, OpacityRequest};
use crate::export_job::{
    CleanupStatus, ExportArtifact, ExportCompositor, ExportReadback, ExternalEffectDisposition,
    JobStatus, StagedArtifactPort,
};
use crate::protocol::{OpaqueResourceHandle, OP_JOB_EXPORT_PNG_BEGIN};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use crate::transaction::OP_HISTORY_UNDO;
use serde_json::json;

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";

#[derive(Default)]
struct Port {
    cleanup_calls: usize,
    fail_cleanup: bool,
    panic_cleanup: bool,
    fail_write: bool,
    published: usize,
}

impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        if self.fail_write {
            Err("injected write failure".into())
        } else {
            Ok(())
        }
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        self.published += 1;
        Ok(ExportArtifact {
            target: "out".into(),
            files: Vec::new(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.cleanup_calls += 1;
        if self.panic_cleanup {
            panic!("injected cleanup unwind");
        }
        if self.fail_cleanup {
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
            bytes: [1, 2, 3, 255].repeat(320 * 180),
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

struct Resolver;
impl ExportResourceResolver for Resolver {
    fn resolve_geometry(
        &mut self,
        handle: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        Ok(GeometryPaintInput::new(
            handle.resource_id(),
            handle.resource_version(),
            vec![LayerGeometry::new(
                LAYER,
                [20.0, 60.0, 40.0, 80.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .unwrap())
    }
}

type App = NativeApplication<Port, Compositor, Resolver>;
fn app(port: Port) -> App {
    NativeApplication::new(
        "replacement-fixture",
        decode_project(PROJECT).unwrap(),
        port,
        Compositor,
        Resolver,
    )
    .unwrap()
}
fn set(app: &App, id: &str, value: u32) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":value}),
    )
}
fn begin(app: &App) -> OpacityRequest {
    OpacityRequest::history_stage(
        "begin-export",
        app.instance_id(),
        app.document_id(),
        Some(app.content_revision()),
        OP_JOB_EXPORT_PNG_BEGIN,
        json!({"contextId":"scene-root", "quality":"final", "outputHandle":"output/fixture",
            "frames":[{"sourceFrame":0,"geometryHandle":{"resourceId":"geometry/r08","resourceVersion":"v1"}}]}),
    )
}

#[test]
fn failed_cleanup_cannot_admit_b_or_resume_a() {
    let mut app = app(Port {
        fail_cleanup: true,
        ..Port::default()
    });
    assert!(app.dispatch(set(&app, "set-40", 40)).is_ok());
    assert!(app.dispatch(set(&app, "set-60", 60)).is_ok());
    let undo = OpacityRequest::history_stage(
        "undo",
        app.instance_id(),
        app.document_id(),
        Some(app.content_revision()),
        OP_HISTORY_UNDO,
        json!({}),
    );
    assert!(app.dispatch(undo).is_ok());
    assert_eq!(app.content_revision(), 3);
    let old_id = app.document_id().to_owned();
    let old_snapshot = app.acquire_snapshot(3).unwrap();
    assert_eq!(old_snapshot.static_opacity(LAYER), Some(40.0));
    let begun = app.dispatch(begin(&app));
    assert!(begun.is_ok());
    let job_id = begun.result().unwrap()["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let held = app.start_next_export_frame(&job_id).unwrap().unwrap();

    assert!(app
        .replace_document(decode_project(PROJECT).unwrap())
        .is_err());
    assert_eq!(app.document_id(), old_id);
    assert_eq!(app.content_revision(), 3);
    let progress = app.replacement_progress().unwrap();
    assert_eq!(
        progress.phase,
        ReplacementPhase::Fenced(ReplacementFailureKind::UnresolvedCleanup)
    );
    assert_eq!(progress.old_document_id, old_id);
    assert_ne!(progress.prepared_document_id, old_id);
    assert_eq!(progress.old_revision, 3);
    assert_eq!((progress.old_undo_depth, progress.old_redo_depth), (1, 1));
    assert_eq!(progress.export_receipts.len(), 1);
    assert_eq!(progress.live_leases, 0);
    let receipt = app.export_receipt(&job_id).unwrap();
    assert_eq!(receipt.status, JobStatus::Failed);
    assert_eq!(receipt.cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert_eq!(app.artifact_port().cleanup_calls, 1);
    assert_eq!(app.artifact_port().published, 0);
    let denied = app.dispatch(set(&app, "after-failure", 80));
    assert_eq!(
        denied.error().unwrap().code(),
        DispatchErrorCode::Unavailable
    );
    assert!(app.acquire_snapshot(3).is_none());
    assert!(app.start_next_export_frame(&job_id).is_err());
    assert!(app.run_export_to_completion(&job_id).is_err());
    assert!(app
        .replace_document(decode_project(PROJECT).unwrap())
        .is_err());
    assert!(app.finish_export_frame(held).is_err());
    assert_eq!(app.artifact_port().published, 0);
}

#[test]
fn previously_failed_cleanup_is_not_hidden_by_an_empty_running_result() {
    let mut app = app(Port {
        fail_cleanup: true,
        fail_write: true,
        ..Port::default()
    });
    let old_id = app.document_id().to_owned();
    let begun = app.dispatch(begin(&app));
    let job_id = begun.result().unwrap()["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let failed = app.run_export_to_completion(&job_id).unwrap();
    assert_eq!(failed.cleanup.status, CleanupStatus::Failed);
    assert_eq!(
        failed.external_effect_disposition,
        ExternalEffectDisposition::Indeterminate
    );
    assert!(app
        .replace_document(decode_project(PROJECT).unwrap())
        .is_err());
    assert_eq!(app.document_id(), old_id);
    let progress = app.replacement_progress().unwrap();
    assert_eq!(
        progress.phase,
        ReplacementPhase::Fenced(ReplacementFailureKind::UnresolvedCleanup)
    );
    assert_eq!(progress.export_receipts, vec![failed]);
    assert_eq!(app.artifact_port().cleanup_calls, 1);
    assert_eq!(app.artifact_port().published, 0);
}

#[test]
fn cleanup_unwind_is_contained_with_a_installed_and_no_late_publish() {
    let mut app = app(Port {
        panic_cleanup: true,
        ..Port::default()
    });
    let old_id = app.document_id().to_owned();
    let begun = app.dispatch(begin(&app));
    let job_id = begun.result().unwrap()["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let held = app.start_next_export_frame(&job_id).unwrap().unwrap();
    assert!(app
        .replace_document(decode_project(PROJECT).unwrap())
        .is_err());
    assert_eq!(app.document_id(), old_id);
    let progress = app.replacement_progress().unwrap();
    assert_eq!(
        progress.phase,
        ReplacementPhase::Fenced(ReplacementFailureKind::Unwind)
    );
    assert_eq!(progress.export_receipts.len(), 1);
    assert!(app.finish_export_frame(held).is_err());
    assert_eq!(app.artifact_port().published, 0);
}

#[test]
fn clean_replacement_installs_fresh_b_and_contains_late_a_work() {
    let mut app = app(Port::default());
    assert!(app.dispatch(set(&app, "set-40", 40)).is_ok());
    let old_id = app.document_id().to_owned();
    let begun = app.dispatch(begin(&app));
    let job_id = begun.result().unwrap()["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let held = app.start_next_export_frame(&job_id).unwrap().unwrap();
    let reconciled = app
        .replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    assert_ne!(app.document_id(), old_id);
    assert_eq!(app.content_revision(), 0);
    assert!(app.replacement_progress().is_none());
    assert_eq!(
        app.acquire_snapshot(0).unwrap().static_opacity(LAYER),
        Some(25.0)
    );
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0].status, JobStatus::Cancelled);
    assert_eq!(reconciled[0].cleanup.status, CleanupStatus::Complete);
    assert_eq!(app.finish_export_frame(held).unwrap(), reconciled[0]);
    assert_eq!(app.artifact_port().published, 0);
    assert!(app.dispatch(set(&app, "b-set", 60)).is_ok());
}
