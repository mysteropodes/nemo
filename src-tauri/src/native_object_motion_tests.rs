//! Actual owner-held CPU preview and SVG export hooks. No GPU. ALL UNRUN.
use crate::native_dispatch::object_host::preview::tests::{
    host as base_host, output, selector, snapshot,
};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    export_job::{ExportArtifact, JobStatus, StagedArtifactPort},
    object_codec::encode_storage_project,
};
use serde_json::{json, Value};

fn host(code: &str) -> NativeObjectHost {
    let base = base_host();
    let snapshot = snapshot(&base);
    let mut document = serde_json::to_value(snapshot.document()).unwrap();
    document["componentTables"] = json!({"duplicators":{"format":"nemo.native-duplicator-set",
        "formatVersion":1,"duplicators":[{"contextId":"scene-root","layerUid":"L0",
        "duplicator":{"mode":"grid","rows":1,"cols":3,"spacingX":10}}],"effectorLayers":[]}});
    document["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[0,0],"expression":{"enabled":true,"code":code}}]}]);
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,
        "assets":[],"drawing":document});
    NativeObjectHost::from_project_file(
        "motion-owner".into(),
        &serde_json::to_vec(&project).unwrap(),
    )
    .unwrap()
}

#[derive(Default)]
struct MemoryPort {
    started: usize,
    frames: Vec<Vec<u8>>,
    published: usize,
}
impl StagedArtifactPort for MemoryPort {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.started += 1;
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, bytes: &[u8]) -> Result<(), String> {
        self.frames.push(bytes.to_vec());
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.published += 1;
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn actual_preview_preparation_paints_expression_values_without_touching_history() {
    let mut owner = host("[frame,0]");
    let snapshot = snapshot(&owner);
    let selector = selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    owner
        .with_prepared_object_preview(&selector, output(), |h| {
            let prepared = h.preview.prepared.as_ref().unwrap();
            let plan = h.preview.brush_runtime.materialize(
                &prepared.admission,
                &mut |_| panic!("non-brush motion fixture cannot retire bitmap resources"),
            )
            .unwrap();
            assert_eq!(plan.packet().frame(), 1);
            assert_eq!(plan.packet().records().len(), 1); // Only authored provenance.
            assert_eq!(plan.commands().len(), 0); // The seed is replaced by clone paint.
            assert!(!h.preview.motion_control.requires_revision());
            Ok(())
        })
        .unwrap();
    assert!(
        owner.preview.prepared.is_none(),
        "the one-shot preparation retires"
    );
    assert_eq!(owner.history.history_depths(), (0, 0));
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn actual_svg_export_uses_the_same_evaluated_frame_and_existing_job_lifecycle() {
    let mut owner = host("[frame,0]");
    let snapshot = snapshot(&owner);
    let selector = selector(&snapshot, 1);
    let mut port = MemoryPort::default();
    let result = owner
        .export_pinned_object_svg_frames(
            &selector,
            &[1],
            "motion-svg".into(),
            "output".into(),
            output(),
            1024 * 1024,
            &mut port,
        )
        .unwrap();
    assert_eq!(result.receipt.unwrap().status, JobStatus::Succeeded);
    assert!(result.release.cleanup_complete);
    assert_eq!((port.started, port.frames.len(), port.published), (1, 1, 1));
    let svg = std::str::from_utf8(&port.frames[0]).unwrap();
    assert!(svg.contains("<svg"));
    for x in [4, 15, 26] {
        assert!(
            svg.contains(&format!("<path d=\"M{x},4")),
            "original expression placement missing: {svg}"
        );
    }
    assert!(!owner.preview.motion_control.requires_revision());
    assert_eq!(owner.history.history_depths(), (0, 0));
}

#[test]
fn actual_timeout_is_retained_at_owner_and_prevents_both_preview_and_export_publication() {
    let mut owner = host("while(true){}");
    let snapshot = snapshot(&owner);
    let selector = selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    let error = owner
        .with_prepared_object_preview::<()>(&selector, output(), |_| {
            panic!("a non-clean frame must never reach the real preview operation")
        })
        .unwrap_err();
    assert!(error.contains("Timeout"));
    assert!(error.contains("dupOffsetPos"));
    let retained = format!("{:?}", owner.preview.motion_control);
    let mut port = MemoryPort::default();
    assert!(owner
        .export_pinned_object_svg_frames(
            &selector,
            &[1],
            "motion-failed-svg".into(),
            "output".into(),
            output(),
            1024 * 1024,
            &mut port
        )
        .is_err());
    assert_eq!((port.started, port.frames.len(), port.published), (0, 0, 0));
    assert_eq!(format!("{:?}", owner.preview.motion_control), retained);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    let saved: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        saved["propertyHolders"][0]["properties"][0]["expression"]["enabled"],
        true
    );
    assert_eq!(
        (owner.content_revision(), owner.history.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn dropping_the_actual_cpu_preparation_on_a_caller_error_keeps_the_owner_clean() {
    let mut owner = host("[frame,0]");
    let snapshot = snapshot(&owner);
    let selector = selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    assert_eq!(
        owner
            .with_prepared_object_preview::<()>(&selector, output(), |_| {
                Err("caller stopped before surface acquisition".into())
            })
            .unwrap_err(),
        "caller stopped before surface acquisition"
    );
    assert!(owner.preview.prepared.is_none());
    assert!(!owner.preview.motion_control.requires_revision());
    owner
        .with_prepared_object_preview(&selector, output(), |_| Ok(()))
        .unwrap();
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(owner.history.history_depths(), (0, 0));
}

#[test]
fn pending_original_effects_follow_the_real_retained_document_across_tab_activation() {
    let mut owner = host("while(true){}");
    let origin = snapshot(&owner);
    let expected = selector(&origin, 1);
    assert!(owner
        .with_prepared_object_preview::<()>(&expected, output(), |_| Ok(()))
        .is_err());
    let receipt = format!("{:?}", owner.preview.motion_control);
    let incoming =
        NativeObjectHost::new_blank_project("motion-incoming".into(), 64, 64, 24.).unwrap();
    let incoming_snapshot = snapshot(&incoming);
    let bytes = incoming
        .history
        .serialize_project_file(0, incoming_snapshot.snapshot_id())
        .unwrap();
    let info = owner
        .prepare_inactive_project(
            origin.instance_id(),
            origin.document_id(),
            0,
            origin.snapshot_id(),
            &bytes,
        )
        .unwrap();
    let plan = owner
        .prepare_retained_activation(
            origin.instance_id(),
            origin.document_id(),
            0,
            origin.snapshot_id(),
            info["preparationToken"].as_str().unwrap(),
            "motion-park",
        )
        .unwrap();
    owner.activate_retained_project(plan, || Ok(())).unwrap();
    assert!(
        !owner.preview.motion_control.has_pending(),
        "the incoming document has no effects"
    );
    let current = snapshot(&owner);
    let token = owner.retained_project_token().unwrap().to_owned();
    let back = owner
        .prepare_retained_activation(
            current.instance_id(),
            current.document_id(),
            current.content_revision(),
            current.snapshot_id(),
            &token,
            "motion-unpark",
        )
        .unwrap();
    owner.activate_retained_project(back, || Ok(())).unwrap();
    assert_eq!(format!("{:?}", owner.preview.motion_control), receipt);
    assert_eq!(snapshot(&owner).snapshot_id(), origin.snapshot_id());
    assert_eq!(owner.history.history_depths(), (0, 0));
    let returned = snapshot(&owner);
    let commit = owner
        .finalize_object_motion_effects(&selector(&returned, 1), &[1])
        .unwrap()
        .unwrap();
    assert!(commit.changed());
    assert_eq!(
        (owner.content_revision(), owner.history.history_depths()),
        (1, (1, 0))
    );
    assert!(owner
        .finalize_object_motion_effects(&selector(&returned, 1), &[1])
        .unwrap()
        .is_none());
}

// Same actual owner fixture lent to private RevisionSync lifecycle controls.
pub(crate) fn finalizer_fixture(
    code: &str,
) -> (
    NativeObjectHost,
    native_engine::object_snapshot::ObjectSnapshot,
    native_engine::scheduler::OutputSpec,
) {
    let owner = host(code);
    let origin = snapshot(&owner);
    (owner, origin, output())
}

pub(crate) fn finalizer_svg(
    owner: &mut NativeObjectHost,
    origin: &native_engine::object_snapshot::ObjectSnapshot,
) -> String {
    let mut port = MemoryPort::default();
    let outcome = owner
        .export_pinned_object_svg_frames(
            &selector(origin, 1),
            &[1],
            format!("callback-svg/{}", uuid::Uuid::new_v4()),
            "output".into(),
            output(),
            1024 * 1024,
            &mut port,
        )
        .unwrap();
    assert_eq!(outcome.receipt.unwrap().status, JobStatus::Succeeded);
    assert!(outcome.release.cleanup_complete);
    assert_eq!((port.started, port.published), (1, 1));
    String::from_utf8(port.frames.remove(0)).unwrap()
}

#[test]
fn diagnostic_batch_finishes_before_any_export_staging_and_replays_actual_fallback_without_history()
{
    let mut owner = host("'wrong dimension'");
    let origin = snapshot(&owner);
    let expected = selector(&origin, 0);
    let mut port = MemoryPort::default();
    let before = encode_storage_project(origin.document()).unwrap();
    assert!(owner
        .export_pinned_object_svg_frames(
            &expected,
            &[0, 1],
            "batch-before-seal".into(),
            "output".into(),
            output(),
            1024 * 1024,
            &mut port
        )
        .is_err());
    assert_eq!((port.started, port.published), (0, 0));
    assert_eq!(owner.pending_object_motion_frames(), [0, 1]);
    let receipt = owner
        .finalize_object_motion_effects(&expected, &[0, 1])
        .unwrap()
        .unwrap();
    assert!(!receipt.changed());
    assert!(receipt.frame_ready());
    let done = owner
        .export_pinned_object_svg_frames(
            &expected,
            &[0, 1],
            "batch-after-seal".into(),
            "output".into(),
            output(),
            1024 * 1024,
            &mut port,
        )
        .unwrap();
    assert_eq!(done.receipt.unwrap().status, JobStatus::Succeeded);
    assert_eq!((port.started, port.frames.len(), port.published), (1, 2, 1));
    for bytes in &port.frames {
        let svg = std::str::from_utf8(bytes).unwrap();
        for x in [4, 14, 24] {
            assert!(svg.contains(&format!("<path d=\"M{x},4")));
        }
    }
    assert_eq!(
        encode_storage_project(snapshot(&owner).document()).unwrap(),
        before
    );
    assert_eq!(
        (owner.content_revision(), owner.history.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn actual_owner_teardown_keeps_unsealed_receipts_and_refuses_late_finalization() {
    let mut owner = host("while(true){}");
    let origin = snapshot(&owner);
    let expected = selector(&origin, 1);
    assert!(owner
        .with_prepared_object_preview(&expected, output(), |_| Ok(()))
        .is_err());
    let retained = format!("{:?}", owner.preview.motion_control);
    let before = encode_storage_project(origin.document()).unwrap();
    let released = owner.release_project().unwrap();
    assert!(released.reconciliation_complete());
    assert_eq!(format!("{:?}", owner.preview.motion_control), retained);
    assert!(owner
        .finalize_object_motion_effects(&expected, &[1])
        .is_err());
    assert_eq!(format!("{:?}", owner.preview.motion_control), retained);
    assert_eq!(owner.history.history_depths(), (0, 0));
    assert_eq!(
        encode_storage_project(snapshot(&owner).document()).unwrap(),
        before
    );
}
