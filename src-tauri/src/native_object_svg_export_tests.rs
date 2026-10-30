//! Real original-owner SVG publication; independent literal scene/manifest checks.
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    export_job::{ExportArtifact, ExternalEffectDisposition, JobStatus, StagedArtifactPort},
    object_codec::decode_storage_project,
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-svg-owner-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn port(&self) -> DesktopArtifactPort {
        DesktopArtifactPort::new_svg(
            self.0.join("stage"),
            [("object-output".into(), self.0.join("published"))],
        )
        .unwrap()
    }
    fn assert_empty_initialized_cache(&self) {
        assert!(
            self.0.join("stage").is_dir(),
            "the direct fixture initializes its cache before the owner call"
        );
        assert_eq!(fs::read_dir(self.0.join("stage")).unwrap().count(), 0);
        assert!(!self.0.join("published").exists());
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn host(text: bool) -> NativeObjectHost {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"vector"});
    record["geometry"]["segments"] =
        json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid","r":0,"g":0,"b":1,"a":1});
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":[record],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":[{"frameScope":{"kind":"authored","frame":0},
            "elements":[{"layerUid":"L0","strokeId":"vector","naturalPosition":0,"order":{}}]}]}],"migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    if text {
        document["items"] = json!({"text":[{"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"text"},
            "block":{"text":"A","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]});
        document["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"L0","strokeId":"text","naturalPosition":1,"order":{}}));
    }
    NativeObjectHost::new(
        "svg-owner".into(),
        decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap(),
    )
    .unwrap()
}
fn selector(snapshot: &native_engine::object_snapshot::ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}

#[test]
fn svg_owned_vector_and_real_bundled_text_publish_exact_frames_without_document_or_history_changes()
{
    let mut owner = host(true);
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let expected = selector(&snapshot);
    let document = serde_json::to_value(snapshot.document()).unwrap();
    let history = owner.history.history_depths();
    let first = Scratch::new();
    let result = owner
        .export_pinned_object_svg_frames(
            &expected,
            &[0, 7],
            "svg-first".into(),
            "object-output".into(),
            output(),
            65536,
            &mut first.port(),
        )
        .unwrap();
    assert!(result.release.cleanup_complete);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Committed
    );
    assert_eq!(receipt.document_snapshot_id, snapshot.snapshot_id());
    assert_eq!(receipt.pinned_revision, 0);
    assert_eq!(
        receipt.artifact.unwrap().files,
        ["frame_0001.svg", "frame_0002.svg"]
    );
    let bytes = fs::read(first.0.join("published/frame_0001.svg")).unwrap();
    assert_eq!(
        bytes,
        fs::read(first.0.join("published/frame_0002.svg")).unwrap(),
        "the same authored frame is held at source frames 0 and 7"
    );
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<svg ") && svg.contains("width=\"64\"") && svg.contains("height=\"64\""));
    // Independent font oracle: bundled Roboto A contributes three real paths,
    // in addition to the one explicitly authored blue vector contour.
    assert_eq!(svg.matches("<path ").count(), 4);
    assert!(!svg.contains("<text") && !svg.contains("data:image"));
    assert_eq!(fs::read_dir(first.0.join("published")).unwrap().count(), 2);
    assert_eq!(fs::read_dir(first.0.join("stage")).unwrap().count(), 0);
    assert_eq!(owner.history.history_depths(), history);
    assert_eq!(
        serde_json::to_value(owner.history.acquire_snapshot(0).unwrap().document()).unwrap(),
        document
    );
    assert_eq!(owner.export_test_state().unwrap().0, 0);

    let second = Scratch::new();
    assert!(owner
        .export_pinned_object_svg_frames(
            &expected,
            &[0],
            "svg-first".into(),
            "object-output".into(),
            output(),
            65536,
            &mut second.port()
        )
        .is_err());
    second.assert_empty_initialized_cache();
    let result = owner
        .export_pinned_object_svg_frames(
            &expected,
            &[0],
            "svg-second".into(),
            "object-output".into(),
            output(),
            65536,
            &mut second.port(),
        )
        .unwrap();
    assert_eq!(result.receipt.unwrap().status, JobStatus::Succeeded);
    let released = owner.release_project().unwrap();
    assert!(released.application.exports.cleanup_complete);
    assert_eq!(released.application.exports.receipts.len(), 1);
    assert_eq!(
        released.application.exports.receipts[0]
            .artifact
            .as_ref()
            .unwrap()
            .files,
        ["frame_0001.svg"]
    );
}

#[test]
fn svg_stale_owner_snapshot_and_invalid_range_never_publish_or_rearm_a_request() {
    let mut owner = host(false);
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let base = selector(&snapshot);
    for changed in [
        ObjectFrameSelector {
            instance_id: "foreign",
            ..base.clone()
        },
        ObjectFrameSelector {
            document_id: "foreign",
            ..base.clone()
        },
        ObjectFrameSelector {
            content_revision: 1,
            ..base.clone()
        },
        ObjectFrameSelector {
            document_snapshot_id: "foreign",
            ..base.clone()
        },
    ] {
        let scratch = Scratch::new();
        assert!(owner
            .export_pinned_object_svg_frames(
                &changed,
                &[0],
                "stale-svg".into(),
                "object-output".into(),
                output(),
                65536,
                &mut scratch.port()
            )
            .is_err());
        scratch.assert_empty_initialized_cache();
    }
    // The original request is bounded before any media, guest or artifact work
    // (6e7d9d5d): an invalid range is a pre-staging refusal, never a receipt.
    for frames in [vec![], vec![7, 0], vec![0, 0], (0..33).collect()] {
        let scratch = Scratch::new();
        let error = owner
            .export_pinned_object_svg_frames(
                &base,
                &frames,
                "bad-range-svg".into(),
                "object-output".into(),
                output(),
                65536,
                &mut scratch.port(),
            )
            .err()
            .expect("an invalid frame range is refused before staging");
        assert_eq!(error, "native object export frames are invalid");
        scratch.assert_empty_initialized_cache();
        assert!(!scratch.0.join("published").exists());
    }
    let small = Scratch::new();
    let result = owner
        .export_pinned_object_svg_frames(
            &base,
            &[0],
            "bounded-svg".into(),
            "object-output".into(),
            output(),
            1,
            &mut small.port(),
        )
        .unwrap();
    assert_ne!(result.receipt.unwrap().status, JobStatus::Succeeded);
    assert!(result.release.cleanup_complete);
    assert!(!small.0.join("published").exists());
}

struct FaultPort {
    inner: DesktopArtifactPort,
    cleanup_debt: bool,
    calls: usize,
}
impl StagedArtifactPort for FaultPort {
    fn begin_staging(&mut self, job: &str, target: &str) -> Result<(), String> {
        self.calls += 1;
        self.inner.begin_staging(job, target)
    }
    fn write_frame(&mut self, job: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.calls += 1;
        self.inner.write_frame(job, name, bytes)?;
        if self.cleanup_debt {
            Err("controlled SVG write failure".into())
        } else {
            Ok(())
        }
    }
    fn publish(
        &mut self,
        job: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.calls += 1;
        self.inner.publish(job, target, files)?;
        Err("controlled lost SVG publication acknowledgement".into())
    }
    fn cleanup(&mut self, job: &str) -> Result<(), String> {
        self.calls += 1;
        if self.cleanup_debt {
            Err("controlled SVG cleanup debt".into())
        } else {
            self.inner.cleanup(job)
        }
    }
}
#[test]
fn svg_lost_ack_and_cleanup_debt_remain_in_the_original_owner_and_block_reentry() {
    for cleanup_debt in [false, true] {
        let mut owner = host(false);
        let snapshot = owner.history.acquire_snapshot(0).unwrap();
        let pin = selector(&snapshot);
        let scratch = Scratch::new();
        let mut port = FaultPort {
            inner: scratch.port(),
            cleanup_debt,
            calls: 0,
        };
        let failed = owner
            .export_pinned_object_svg_frames(
                &pin,
                &[0],
                "failed-svg".into(),
                "object-output".into(),
                output(),
                65536,
                &mut port,
            )
            .unwrap();
        assert_eq!(failed.receipt.unwrap().status, JobStatus::Failed);
        assert!(!failed.release.cleanup_complete);
        let calls = port.calls;
        assert!(owner
            .export_pinned_object_svg_frames(
                &pin,
                &[0],
                "later-svg".into(),
                "object-output".into(),
                output(),
                65536,
                &mut port
            )
            .is_err());
        assert_eq!(
            port.calls, calls,
            "a new request must not re-arm unresolved work"
        );
        assert!(
            !owner
                .release_project()
                .unwrap()
                .application
                .exports
                .cleanup_complete
        );
    }
}
