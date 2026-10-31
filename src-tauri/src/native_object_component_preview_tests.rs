//! #2256: the viewport presents an ENTERED component context through the actual owner, scheduler and compositor:
//! a declared component is admitted within its own frame count, anything else is refused as before.
use crate::native_dispatch::object_host::preview::tests::{output, viewport};
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor, desktop_viewport::ViewportStatus, object_codec::decode_storage_project,
    object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector,
};
use serde_json::{json, Value};

/// The resolver's admitted component world (`D`), with the timing every inner layer needs to be presentable.
fn world() -> Value {
    let rows =
        include_str!("../../native-engine/tests/fixtures/object-context-resolver/oracle.txt");
    let line = rows.lines().find(|l| l.starts_with("D\t")).unwrap();
    let mut value: Value = serde_json::from_str(line.split('\t').nth(1).unwrap()).unwrap();
    for component in value["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap()
    {
        for (index, layer) in component["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            let frames: Vec<u32> = if index == 0 {
                (0..12).collect()
            } else {
                vec![0]
            };
            layer["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
                "family":"drawing","contextId":"scene-root",
                "keys":frames.iter().map(|frame|json!({"frame":frame})).collect::<Vec<_>>(),"samples":[]});
        }
    }
    value
}
fn selector<'a>(
    snapshot: &'a native_engine::object_snapshot::ObjectSnapshot,
    context: &'a str,
    frame: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: context,
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

#[test]
fn gpu_the_viewport_presents_a_declared_component_context_within_its_own_frames() {
    let mut gpu = Compositor::new().unwrap();
    let document = decode_storage_project(world().to_string().as_bytes()).unwrap();
    let mut owner = NativeObjectHost::new("component-owner".into(), document).unwrap();
    let (mut view, _trace) = viewport(&gpu, 64);
    let snapshot = owner
        .history
        .acquire_snapshot(owner.history.content_revision())
        .unwrap();
    // The component has 12 frames while the scene has 24: frame 11 is the component's last, frame 12 is outside it.
    for (context, frame) in [
        ("sym_comp", 0),
        ("sym_comp", 7),
        ("sym_comp", 11),
        ("sym_comp_b", 3),
    ] {
        let shown = owner
            .present_object_preview(
                &selector(&snapshot, context, frame),
                output(),
                &mut gpu,
                &mut view,
            )
            .unwrap_or_else(|error| panic!("{context}@{frame}: {error}"));
        assert_eq!(
            shown.status(),
            &ViewportStatus::Presented,
            "{context}@{frame}"
        );
        // The desktop route finalizes the frame's pending Motion effects after it presents (a trusted owner receipt).
        owner
            .finalize_object_motion_effects(&selector(&snapshot, context, frame), &[frame])
            .unwrap_or_else(|error| panic!("{context}@{frame} finalize: {error}"));
        assert_eq!(shown.identity().evaluation_key().context_id(), context);
        assert_eq!(shown.identity().evaluation_key().frame(), frame);
    }
    for (context, frame) in [("sym_comp", 12), ("sym_comp", 23), ("sym_undeclared", 0)] {
        assert!(
            owner
                .present_object_preview(
                    &selector(&snapshot, context, frame),
                    output(),
                    &mut gpu,
                    &mut view
                )
                .is_err(),
            "{context}@{frame} must stay refused"
        );
    }
    // The scene is still presented over its own 24 frames.
    let scene = owner
        .present_object_preview(
            &selector(&snapshot, "scene-root", 23),
            output(),
            &mut gpu,
            &mut view,
        )
        .unwrap();
    assert_eq!(scene.status(), &ViewportStatus::Presented);
}
