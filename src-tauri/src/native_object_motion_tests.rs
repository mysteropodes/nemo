//! Actual owner-held CPU preview and SVG export hooks. No GPU. ALL UNRUN.
use super::*;
use crate::native_dispatch::NativeDispatch;
use native_engine::{
    export_job::{ExportArtifact, JobStatus, StagedArtifactPort},
    object_codec::encode_storage_project,
};
use serde_json::{json, Value};

fn host(code: &str) -> NativeObjectHost {
    let base = super::tests::host();
    let snapshot = super::tests::snapshot(&base);
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
    let snapshot = super::tests::snapshot(&owner);
    let selector = super::tests::selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    owner
        .with_prepared_object_preview(&selector, super::tests::output(), |h| {
            let plan = &h.preview.prepared.as_ref().unwrap().plan;
            assert_eq!(plan.packet().frame(), 1);
            assert_eq!(plan.packet().records().len(), 1); // Only authored provenance.
            assert_eq!(plan.commands().len(), 0); // The seed is replaced by clone paint.
            assert!(h.preview.motion_failure.is_none());
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
    let snapshot = super::tests::snapshot(&owner);
    let selector = super::tests::selector(&snapshot, 1);
    let mut port = MemoryPort::default();
    let result = owner
        .export_pinned_object_svg_frames(
            &selector,
            &[1],
            "motion-svg".into(),
            "output".into(),
            super::tests::output(),
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
    assert!(owner.preview.motion_failure.is_none());
    assert_eq!(owner.history.history_depths(), (0, 0));
}

#[test]
fn actual_timeout_is_retained_at_owner_and_prevents_both_preview_and_export_publication() {
    let mut owner = host("while(true){}");
    let snapshot = super::tests::snapshot(&owner);
    let selector = super::tests::selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    let error = owner
        .with_prepared_object_preview::<()>(&selector, super::tests::output(), |_| {
            panic!("a non-clean frame must never reach the real preview operation")
        })
        .unwrap_err();
    assert!(error.contains("Timeout"));
    assert!(error.contains("dupOffsetPos"));
    let retained = format!("{:?}", owner.preview.motion_failure.as_ref().unwrap());
    let mut port = MemoryPort::default();
    assert!(owner
        .export_pinned_object_svg_frames(
            &selector,
            &[1],
            "motion-failed-svg".into(),
            "output".into(),
            super::tests::output(),
            1024 * 1024,
            &mut port
        )
        .is_err());
    assert_eq!((port.started, port.frames.len(), port.published), (0, 0, 0));
    assert_eq!(
        format!("{:?}", owner.preview.motion_failure.as_ref().unwrap()),
        retained
    );
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
    let snapshot = super::tests::snapshot(&owner);
    let selector = super::tests::selector(&snapshot, 1);
    let before = encode_storage_project(snapshot.document()).unwrap();
    assert_eq!(
        owner
            .with_prepared_object_preview::<()>(&selector, super::tests::output(), |_| {
                Err("caller stopped before surface acquisition".into())
            })
            .unwrap_err(),
        "caller stopped before surface acquisition"
    );
    assert!(owner.preview.prepared.is_none());
    assert!(owner.preview.motion_failure.is_none());
    owner
        .with_prepared_object_preview(&selector, super::tests::output(), |_| Ok(()))
        .unwrap();
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(owner.history.history_depths(), (0, 0));
}

#[test]
fn pending_original_effects_follow_the_real_retained_document_across_tab_activation() {
    let mut owner = host("while(true){}");
    let origin = super::tests::snapshot(&owner);
    let selector = super::tests::selector(&origin, 1);
    assert!(owner
        .with_prepared_object_preview::<()>(&selector, super::tests::output(), |_| Ok(()))
        .is_err());
    let receipt = owner.preview.motion_failure.as_ref().unwrap().to_string();
    let incoming =
        NativeObjectHost::new_blank_project("motion-incoming".into(), 64, 64, 24.).unwrap();
    let incoming_snapshot = super::tests::snapshot(&incoming);
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
        owner.preview.motion_failure.is_none(),
        "the incoming document has no effects"
    );
    let current = super::tests::snapshot(&owner);
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
    assert_eq!(
        owner.preview.motion_failure.as_ref().unwrap().to_string(),
        receipt
    );
    assert_eq!(
        super::tests::snapshot(&owner).snapshot_id(),
        origin.snapshot_id()
    );
    assert_eq!(owner.history.history_depths(), (0, 0));
}
