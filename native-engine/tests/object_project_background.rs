//! Literal background/color oracles; actual GPU output is not installed acceptance.
use crate::compositor::{object_compositor::ObjectOutputSpec, Compositor};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene,
};
use crate::revision::ObjectSnapshot;
use serde_json::json;

fn owner(color: Option<&str>) -> NativeObjectHistory {
    let mut drawing = paint_test_document(&[0]);
    drawing["objects"] = json!([]);
    for context in drawing["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
    {
        for scope in context["scopes"].as_array_mut().unwrap() {
            scope["elements"] = json!([]);
        }
    }
    let metadata = ProjectMetadata::new(64, 64, 24.)
        .unwrap()
        .with_canvas_background(color)
        .unwrap();
    let drawing = decode_storage_project(&serde_json::to_vec(&drawing).unwrap()).unwrap();
    NativeObjectHistory::from_admitted_project(
        "settings-background",
        ProjectEnvelope::no_media(metadata, drawing),
    )
    .unwrap()
}
fn pixels(gpu: &mut Compositor, snapshot: &ObjectSnapshot, mode: &str) -> [u8; 4] {
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let witness = prepare_object_composition(&plan, &pins).unwrap();
    let result = gpu
        .compose_object(&witness, ObjectOutputSpec::new(64, 64, mode).unwrap())
        .unwrap();
    assert!(result.identity().same_identity_as(&witness));
    let result = gpu.readback_object_rgba8(&result).unwrap();
    let pixel: [u8; 4] = result.bytes()[0..4].try_into().unwrap();
    assert!(result.bytes().chunks_exact(4).all(|actual| actual == pixel));
    pixel
}
fn close(actual: [u8; 4], expected: [u8; 4]) {
    assert_eq!(actual[3], expected[3], "alpha must match exactly");
    if expected[3] == 0 {
        assert_eq!(actual, [0; 4], "zero alpha must have canonical zero RGB");
    } else {
        assert!(
            actual[..3]
                .iter()
                .zip(&expected[..3])
                .all(|(a, b)| a.abs_diff(*b) <= 1),
            "actual={actual:?}, expected={expected:?}"
        );
    }
}

#[test]
fn gpu_literal_project_colors_preserve_straight_alpha_and_explicit_white_mode() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    for (color, expected) in [
        (None, [255, 255, 255, 255]),
        (Some("#aBc"), [170, 187, 204, 255]),
        (Some("A03080"), [160, 48, 128, 255]),
        (Some("#A0308080"), [160, 48, 128, 128]),
        (Some("aBcD09eF"), [171, 205, 9, 239]),
        (Some("#12345600"), [0, 0, 0, 0]),
    ] {
        let owner = owner(color);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        close(
            pixels(&mut gpu, &snapshot, "project-srgb-normal-rgba8-area"),
            expected,
        );
        close(
            pixels(&mut gpu, &snapshot, "white-srgb-normal-rgba8-area"),
            [255; 4],
        );
    }
}
#[test]
fn gpu_settings_revision_undo_redo_and_old_pins_keep_their_own_background() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let mut owner = owner(Some("#aBc"));
    let old = owner.acquire_snapshot(0).unwrap();
    let candidate=owner.prepare_project_settings_json(br##"{"command":"object.project.settings.edit","action":{"kind":"setCanvasBackground","color":"#A0308080"}}"##).unwrap();
    let changed = owner.commit_project_settings(candidate).unwrap();
    let new = owner.acquire_snapshot(changed).unwrap();
    assert_eq!(owner.history_depths(), (1, 0));
    close(
        pixels(&mut gpu, &new, "project-srgb-normal-rgba8-area"),
        [160, 48, 128, 128],
    );
    close(
        pixels(&mut gpu, &old, "project-srgb-normal-rgba8-area"),
        [170, 187, 204, 255],
    );
    let undo = owner.undo(changed).unwrap();
    close(
        pixels(
            &mut gpu,
            &owner.acquire_snapshot(undo).unwrap(),
            "project-srgb-normal-rgba8-area",
        ),
        [170, 187, 204, 255],
    );
    let redo = owner.redo(undo).unwrap();
    let redone = owner.acquire_snapshot(redo).unwrap();
    close(
        pixels(&mut gpu, &redone, "project-srgb-normal-rgba8-area"),
        [160, 48, 128, 128],
    );
    let bytes = owner
        .serialize_project_file(redo, redone.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened-background", &bytes).unwrap();
    close(
        pixels(
            &mut gpu,
            &reopened.acquire_snapshot(0).unwrap(),
            "project-srgb-normal-rgba8-area",
        ),
        [160, 48, 128, 128],
    );
}
