//! Widget layers never render (L03-S, acceptance bullet 4), through the real document and scene path.
//!
//! The baseline claim (CLAUDE.md section 13): `getEffectiveStrokes` returns `[]` for a widget layer,
//! `buildSceneJson` pushes an empty slot, and the pad and puck exist only as a viewport overlay, so a
//! 1920x1080 frame holds the same pixels with and without the widget layer. Natively the document
//! admits no drawing object on a widget layer (L03-K), so the real scene producers see no record
//! there. These tests decode real documents (widget layer last, first and absent), build the real
//! render scenes of two frames and compare every entry. The layer-level property holder that
//! `addWidgetLayer` leaves behind (the widget's two controls) is a holder the evaluation join does not
//! read as a paint input: a widget layer never paints and its controls are read by expressions elsewhere, so
//! the packet gate admits that holder and the scenes compared are those of the same document with and
//! without its holder table, which must be identical. The slot rule itself is unit-tested in
//! `render_image_mesh_scene_policy`.

use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_policy::widget_layers_with_content;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::widget_document;
use crate::object_codec::decode_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_frame, ObjectFrameError, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_render_scene::{prepare_object_render_scene, ObjectRenderScene};
use crate::revision::ObjectSnapshot;
use serde_json::Value;

fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "widget-scene",
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

fn try_packet(
    snapshot: &ObjectSnapshot,
    kind: FrameScopeKind,
    frame: u32,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    prepare_object_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: kind,
            frame,
        },
    )
}

fn packet(snapshot: &ObjectSnapshot, kind: FrameScopeKind, frame: u32) -> ObjectFramePacket {
    try_packet(snapshot, kind, frame).unwrap()
}

fn scene(packet: &ObjectFramePacket) -> ObjectRenderScene {
    let selector = ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    };
    prepare_object_render_scene(packet, &selector).unwrap()
}

/// (layer position, object position, record JSON, bounds, contour) of every scene entry.
type Entry = (usize, usize, String, [f64; 4], String);

fn entries(scene: &ObjectRenderScene) -> Vec<Entry> {
    scene
        .entries()
        .map(|entry| {
            (
                entry.source().source_layer_index(),
                entry.source().source_object_index(),
                serde_json::to_string(entry.source().record()).unwrap(),
                entry.envelope(),
                format!("{:?}", entry.contour().commands()),
            )
        })
        .collect()
}

fn frames() -> Vec<(FrameScopeKind, u32)> {
    vec![
        (FrameScopeKind::Authored, 7),
        (FrameScopeKind::Reference, 12),
    ]
}

fn frame_entries(value: &Value) -> Vec<Vec<Entry>> {
    let snapshot = snapshot(value);
    frames()
        .into_iter()
        .map(|(kind, frame)| entries(&scene(&packet(&snapshot, kind, frame))))
        .collect()
}

/// The widget document of `widget_at` with its control holders: the holder table is the ONLY difference from the
/// document without them, the packet over it is admitted (the widget never paints, its controls are read by
/// expressions elsewhere) and its scene entries are exactly those of the document without the table.
fn assert_holders_do_not_change_the_scene(widget_at: usize) {
    let with_holders = widget_document(Some(widget_at), true);
    let mut stripped = with_holders.clone();
    stripped
        .as_object_mut()
        .unwrap()
        .remove("propertyHolders")
        .expect("the widget document declares its holders");
    assert_eq!(
        stripped,
        widget_document(Some(widget_at), false),
        "the holder table is the only difference (widget at {widget_at})"
    );
    let (held, bare) = (snapshot(&with_holders), snapshot(&stripped));
    for (kind, frame) in frames() {
        assert_eq!(
            entries(&scene(&packet(&held, kind.clone(), frame))),
            entries(&scene(&packet(&bare, kind.clone(), frame))),
            "widget at {widget_at}, {kind:?} frame {frame}"
        );
    }
}

#[test]
fn a_widget_layer_adds_no_scene_entry_and_changes_no_other() {
    let without = frame_entries(&widget_document(None, false));
    assert!(
        without.iter().all(|frame| frame.len() == 1),
        "each sampled frame paints one shipped record"
    );
    let last = frame_entries(&widget_document(Some(2), false));
    assert_eq!(last, without, "widget layer last");
    let first = frame_entries(&widget_document(Some(0), false));
    let shifted: Vec<Vec<Entry>> = without
        .iter()
        .map(|frame| {
            frame
                .iter()
                .map(|e| (e.0 + 1, e.1, e.2.clone(), e.3, e.4.clone()))
                .collect()
        })
        .collect();
    assert_eq!(
        first, shifted,
        "a leading widget layer only shifts layer positions"
    );
    // With its control holders the same documents paint the same entries.
    for widget_at in [2, 0] {
        assert_holders_do_not_change_the_scene(widget_at);
    }
}

#[test]
fn no_scene_entry_is_owned_by_a_widget_layer() {
    for widget_at in [0, 2] {
        assert_holders_do_not_change_the_scene(widget_at);
        let value = widget_document(Some(widget_at), false);
        let snapshot = snapshot(&value);
        let layers = snapshot.document().layers();
        for (kind, frame) in frames() {
            let prepared = scene(&packet(&snapshot, kind, frame));
            let owners = prepared.entries().map(|e| e.source().source_layer_index());
            assert_eq!(
                widget_layers_with_content(layers, owners),
                Vec::<usize>::new()
            );
        }
    }
}
