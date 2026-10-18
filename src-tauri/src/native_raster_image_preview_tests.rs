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
            .prepare_object_preview(&selector(&original, frame), output())
            .unwrap();
        assert_eq!(prepared.plan.commands().len(), 0);
        let expected = prepared
            .plan
            .raster_resource_versions()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(prepared.key.declared_resource_versions(), &expected);
        assert_eq!(prepared.key.declared_resource_versions().len(), 1);
        if let Some(old) = &declarations {
            assert_eq!(prepared.key.declared_resource_versions(), old);
        }
        declarations = Some(prepared.key.declared_resource_versions().clone());
    }
    host.history.undo(original.content_revision()).unwrap();
    let after = snapshot(&host);
    let prepared = host
        .prepare_object_preview(&selector(&after, 0), output())
        .unwrap();
    assert!(prepared.key.declared_resource_versions().is_empty());
    host.history.redo(after.content_revision()).unwrap();
    let redone = snapshot(&host);
    let prepared = host
        .prepare_object_preview(&selector(&redone, 20), output())
        .unwrap();
    assert_eq!(
        prepared.key.declared_resource_versions(),
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
        assert_eq!(prepared.key.declared_resource_versions().len(), 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(callbacks, 1);
    assert!(host.preview.prepared.is_none());
    assert!(host.preview.pending.is_none());
}
