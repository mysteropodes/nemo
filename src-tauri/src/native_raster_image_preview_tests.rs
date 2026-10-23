//! Actual host CPU preview preparation/key and pre-target controls, authored UNRUN; no GPU creation.
use crate::native_dispatch::NativeObjectHost;
use native_engine::commands::OpacityRequest;
use native_engine::object_document::FrameScopeKind;
use native_engine::{
    object_frame_packet::ObjectFrameSelector, object_snapshot::ObjectSnapshot,
    scheduler::OutputSpec,
};
use serde_json::json;

fn host() -> NativeObjectHost {
    NativeObjectHost::new_blank_project("image-preview".into(), 64, 64, 24.).unwrap()
}
fn snapshot(host: &NativeObjectHost) -> ObjectSnapshot {
    host.history
        .acquire_snapshot(host.history.content_revision())
        .unwrap()
}
fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn import(host: &mut NativeObjectHost) {
    let before = snapshot(host);
    let request = OpacityRequest::history_stage(
        "preview-image",
        before.instance_id(),
        before.document_id(),
        Some(before.content_revision()),
        "command.document.object.asset.attach",
        json!({"documentSnapshotId":before.snapshot_id(),"lifecycleGeneration":1,"name":"still.png",
            "mediaType":"image/png","source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}}),
    );
    let hex="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert!(host.image_import(&request, &bytes).unwrap().is_ok());
}

#[test]
fn actual_preview_key_declares_png_resources_on_every_held_frame_then_undo_removes_them() {
    let mut host = host();
    import(&mut host);
    let original = snapshot(&host);
    let mut declarations = None;
    for frame in [0, 7, 20] {
        let prepared = host
            .prepare_object_preview(&selector(&original, frame), output(), false)
            .unwrap();
        let plan = host
            .preview
            .brush_runtime
            .materialize(&prepared.admission, &mut |_| {
                panic!("plain raster fixture cannot retire bitmap resources")
            })
            .unwrap();
        let key = prepared.materialized_key(&plan).unwrap();
        assert_eq!(plan.commands().len(), 0);
        let expected = plan
            .raster_resource_versions()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(key.declared_resource_versions(), &expected);
        assert_eq!(key.declared_resource_versions().len(), 1);
        if let Some(old) = &declarations {
            assert_eq!(key.declared_resource_versions(), old);
        }
        declarations = Some(key.declared_resource_versions().clone());
    }
    host.history.undo(original.content_revision()).unwrap();
    let after = snapshot(&host);
    let prepared = host
        .prepare_object_preview(&selector(&after, 0), output(), false)
        .unwrap();
    let plan = host
        .preview
        .brush_runtime
        .materialize(&prepared.admission, &mut |_| {
            panic!("plain raster fixture cannot retire bitmap resources")
        })
        .unwrap();
    let key = prepared.materialized_key(&plan).unwrap();
    assert!(key.declared_resource_versions().is_empty());
    host.history.redo(after.content_revision()).unwrap();
    let redone = snapshot(&host);
    let prepared = host
        .prepare_object_preview(&selector(&redone, 20), output(), false)
        .unwrap();
    let plan = host
        .preview
        .brush_runtime
        .materialize(&prepared.admission, &mut |_| {
            panic!("plain raster fixture cannot retire bitmap resources")
        })
        .unwrap();
    let key = prepared.materialized_key(&plan).unwrap();
    assert_eq!(
        key.declared_resource_versions(),
        declarations.as_ref().unwrap()
    );
}

#[test]
fn stale_pin_and_wrong_output_refuse_before_target_callback_and_valid_token_never_escapes() {
    let mut host = host();
    let before = snapshot(&host);
    import(&mut host);
    let mut callbacks = 0;
    assert!(host
        .with_prepared_object_preview(&selector(&before, 0), output(), |_| {
            callbacks += 1;
            Ok(())
        })
        .is_err());
    let current = snapshot(&host);
    let wrong = OutputSpec::new("frame", "rgba8", 64, 64, "linear", "straight").unwrap();
    assert!(host
        .with_prepared_object_preview(&selector(&current, 0), wrong, |_| {
            callbacks += 1;
            Ok(())
        })
        .is_err());
    assert_eq!(callbacks, 0);
    assert!(host.preview.prepared.is_none());
    host.with_prepared_object_preview(&selector(&current, 0), output(), |owner| {
        callbacks += 1;
        let prepared = owner.preview.prepared.as_ref().unwrap();
        let plan = owner
            .preview
            .brush_runtime
            .materialize(&prepared.admission, &mut |_| {
                panic!("plain raster fixture cannot retire bitmap resources")
            })
            .unwrap();
        let key = prepared.materialized_key(&plan).unwrap();
        assert_eq!(key.declared_resource_versions().len(), 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(callbacks, 1);
    assert!(host.preview.prepared.is_none());
    assert!(host.preview.pending.is_none());
}

// Exercise the shared application owner and private UI dispatch rather than
// mutating history directly. CPU preparation is not a desktop/GPU receipt.
mod ordinary_project_undo {
    use super::*;
    use crate::application_mcp::ApplicationMcp;
    use nemo_mcp::contract::NativeApplicationRequest;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::{path::PathBuf, sync::Arc};

    fn info(state: &ApplicationMcp) -> Value {
        let native = state.native_state();
        let guard = native.lock().unwrap();
        let document = guard.active().unwrap().1.document_id().to_owned();
        drop(guard);
        state.native_project_info(&document).unwrap()
    }

    fn save(state: &ApplicationMcp) -> Vec<u8> {
        let pin = info(state);
        state
            .native_project_file_snapshot(
                pin["documentId"].as_str().unwrap(),
                pin["contentRevision"].as_u64().unwrap(),
                pin["documentSnapshotId"].as_str().unwrap(),
            )
            .unwrap()
            .unwrap()
    }

    async fn attach(state: &ApplicationMcp, id: &str, source: &PathBuf) {
        let pin = info(state);
        let request: NativeApplicationRequest = serde_json::from_value(json!({
            "apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
            "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],
            "operation":"command.document.object.asset.attach",
            "payload":{"documentSnapshotId":pin["documentSnapshotId"],
                "lifecycleGeneration":pin["lifecycleGeneration"],"name":"still.png",
                "mediaType":"image/png","source":{"kind":"file","path":source},
                "placement":{"kind":"newLayer"}}
        }))
        .unwrap();
        let expected = source.canonicalize().unwrap();
        let response = state
            .dispatch_native_scoped(request, Arc::new(move |resolved| resolved == expected))
            .await
            .unwrap();
        assert!(response.ok, "actual image attach: {response:?}");
    }

    fn preview(state: &ApplicationMcp, frame: u32) -> BTreeMap<String, String> {
        let pin = info(state);
        assert_eq!(pin["totalFrames"], 120);
        let expected = ObjectFrameSelector {
            instance_id: state.instance_id(),
            document_id: pin["documentId"].as_str().unwrap(),
            content_revision: pin["contentRevision"].as_u64().unwrap(),
            document_snapshot_id: pin["documentSnapshotId"].as_str().unwrap(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        };
        let resources = state
            .with_object_preview_owner(
                pin["lifecycleGeneration"].as_u64().unwrap(),
                &expected,
                |owner| {
                    assert!(state.preview_locks_held_for_test());
                    assert!(owner.matches_project_snapshot(expected.document_snapshot_id));
                    owner.require_project_output((64, 64))?;
                    let resources =
                        owner.with_prepared_object_preview(&expected, output(), |owner| {
                            let prepared = owner.preview.prepared.as_ref().unwrap();
                            let plan = owner
                                .preview
                                .brush_runtime
                                .materialize(&prepared.admission, &mut |_| {
                                    panic!("plain PNG fixture cannot retire bitmap resources")
                                })
                                .map_err(|error| format!("PNG materialization: {error:?}"))?;
                            Ok(prepared
                                .materialized_key(&plan)?
                                .declared_resource_versions()
                                .clone())
                        })?;
                    assert!(owner.preview.prepared.is_none());
                    assert!(owner.preview.pending.is_none());
                    Ok(resources)
                },
            )
            .unwrap_or_else(|error| {
                panic!(
                    "revision {} frame {frame} preview: {error}",
                    pin["contentRevision"]
                )
            });
        assert_eq!(
            info(state),
            pin,
            "preview must preserve the exact owner and revision"
        );
        assert!(state.native_state().try_lock().is_ok());
        assert!(!state.preview_locks_held_for_test());
        resources
    }

    fn history(state: &ApplicationMcp, action: &str, ordinal: u32) {
        let before = info(state);
        let response = state
            .dispatch_object_request_json(
                &json!({
                    "apiVersion":2,"requestId":format!("ordinary-{action}-{ordinal}"),
                    "instanceId":state.instance_id(),"documentId":before["documentId"],
                    "expectedRevision":before["contentRevision"],
                    "operation":format!("command.document.object.{action}"),
                    "payload":{"command":format!("object.{action}")}
                })
                .to_string(),
            )
            .unwrap();
        assert!(response.ok, "ordinary history response: {response:?}");
        let after = info(state);
        assert_eq!(
            after["contentRevision"].as_u64().unwrap(),
            before["contentRevision"].as_u64().unwrap() + 1
        );
        for field in [
            "documentId",
            "instanceId",
            "lifecycleGeneration",
            "totalFrames",
        ] {
            assert_eq!(after[field], before[field], "history changed {field}");
        }
    }

    struct PngFixture(PathBuf);
    impl PngFixture {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("nemo-ordinary-png-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
            let source = Self(directory.join("still.png"));
            let hex = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
            let bytes: Vec<_> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            std::fs::write(&source.0, bytes).unwrap();
            source
        }
    }
    impl Drop for PngFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
        }
    }

    #[tokio::test]
    async fn actual_new_duplicate_png_ordinary_undo_preserves_held_frame_preview_and_resources() {
        let state = ApplicationMcp::default();
        crate::native_object_bootstrap::bootstrap_raw(
            &state,
            &json!({
                "apiVersion":2,"requestId":"ordinary-new","instanceId":state.instance_id(),
                "newProject":{"canvasW":64,"canvasH":64,"fps":24.0}
            })
            .to_string(),
        )
        .unwrap();
        let source = PngFixture::new();
        let blank = save(&state);
        assert!(preview(&state, 0).is_empty());
        attach(&state, "first-png", &source.0).await;
        let first = save(&state);
        let resources = preview(&state, 0);
        assert_eq!(resources.len(), 1);
        assert_eq!(preview(&state, 119), resources);
        attach(&state, "duplicate-png", &source.0).await;
        let duplicate = save(&state);
        assert_ne!(duplicate, first);
        assert_eq!(preview(&state, 0), resources);
        assert_eq!(preview(&state, 119), resources);
        for (ordinal, action, expected, count) in [
            (0, "undo", &first, 1),
            (1, "undo", &blank, 0),
            (2, "redo", &first, 1),
            (3, "redo", &duplicate, 1),
        ] {
            history(&state, action, ordinal);
            assert_eq!(&save(&state), expected, "whole native save after {action}");
            for frame in [119, 0] {
                let actual = preview(&state, frame);
                assert_eq!(actual.len(), count);
                if count == 1 {
                    assert_eq!(actual, resources);
                }
            }
        }
    }
}
