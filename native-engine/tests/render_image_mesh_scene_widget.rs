//! Widget layers never render (L03-S, acceptance bullet 4), through the real document and scene path.
//!
//! The baseline claim (CLAUDE.md section 13): `getEffectiveStrokes` returns `[]` for a widget layer,
//! `buildSceneJson` pushes an empty slot, and the pad and puck exist only as a viewport overlay, so a
//! 1920x1080 frame holds the same pixels with and without the widget layer. Natively the document
//! admits no drawing object on a widget layer (L03-K), so the real scene producers see no record
//! there. These tests decode real documents (widget layer last, first and absent, with and without
//! its control holders), build the real render scenes of two frames and compare every entry. The slot
//! rule itself is unit-tested in `render_image_mesh_scene_policy`.

use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_policy::widget_layers_with_content;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::widget_document;
use crate::object_codec::decode_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
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

fn packet(snapshot: &ObjectSnapshot, kind: FrameScopeKind, frame: u32) -> ObjectFramePacket {
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
    .unwrap()
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

#[test]
fn a_widget_layer_adds_no_scene_entry_and_changes_no_other() {
    let without = frame_entries(&widget_document(None, false));
    assert!(
        without.iter().all(|frame| frame.len() == 1),
        "each sampled frame paints one shipped record"
    );
    for with_holders in [false, true] {
        let last = frame_entries(&widget_document(Some(2), with_holders));
        assert_eq!(last, without, "widget layer last (holders {with_holders})");
        let first = frame_entries(&widget_document(Some(0), with_holders));
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
            "a leading widget layer only shifts layer positions (holders {with_holders})"
        );
    }
}

#[test]
fn no_scene_entry_is_owned_by_a_widget_layer() {
    for widget_at in [0, 2] {
        let value = widget_document(Some(widget_at), true);
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
