//! Real document / packet / pinned-scene builders shared by the L08-K tests (no mock stands in for an owner).
//!
//! A document is the accepted paint-plan document (`paint_test_document`: layers L2, L0, L1 stored in that order,
//! painted L0, L1, L2 back to front, five objects) edited through its JSON; it is decoded by the real storage codec,
//! pinned in a real `ObjectSnapshot`, selected into a real `ObjectFramePacket`, compiled by the real contour producer
//! into a real `ObjectRenderScene` and `ObjectPaintPlan`, and joined onto the packet by the real L07-C pin registry.
//! This file is included by the render-scene tests and by the compositor-layer tests, so it names only crate paths.

use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinnedScene, ProducerOutput, ScenePin,
};
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
    ObjectRenderScene,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) const FRAME: u32 = 7;

/// The paint-plan document at `FRAME`.
pub(crate) fn base_document() -> Value {
    paint_test_document(&[FRAME])
}

/// The paint-plan document with a composition-order scope (and its objects) at each of `frames`: a v2 document
/// has no inferred scope, so a packet exists only at a frame the plan names.
pub(crate) fn document_at(frames: &[u32]) -> Value {
    paint_test_document(frames)
}

/// Merge `members` into the existing layer `uid`.
pub(crate) fn set_layer(document: &mut Value, uid: &str, members: Value) {
    let layer = document["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["layerUid"] == uid)
        .expect("layer exists");
    for (key, value) in members.as_object().unwrap() {
        layer[key.as_str()] = value.clone();
    }
}

/// Append an object-less layer painted on top of the existing ones (`natural` is its natural position).
pub(crate) fn push_layer(document: &mut Value, uid: &str, natural: u32, members: Value) {
    let mut layer = json!({ "layerUid": uid });
    for (key, value) in members.as_object().unwrap() {
        layer[key.as_str()] = value.clone();
    }
    document["layers"].as_array_mut().unwrap().push(layer);
    document["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "layerUid": uid, "naturalPosition": natural, "order": {} }));
}

pub(crate) fn snapshot(document: &Value, instance: &str) -> ObjectSnapshot {
    ObjectSnapshot::new(
        instance,
        decode_storage_project(&serde_json::to_vec(document).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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

pub(crate) fn packet(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(snapshot, &selector(snapshot, frame)).unwrap()
}

pub(crate) fn render_scene(packet: &ObjectFramePacket) -> ObjectRenderScene {
    let pins = ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    };
    prepare_object_render_scene(packet, &pins).unwrap()
}

pub(crate) fn paint_plan(scene: &ObjectRenderScene) -> ObjectPaintPlan {
    let packet = scene.packet();
    let pins = ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    };
    prepare_object_paint_plan(scene, &pins).unwrap()
}

/// The contour producer's claims over the retained scene, read like `command_sequence` reads them; the records
/// named in `skip` (source object indices) are deliberately left to no producer.
pub(crate) fn contour_claims(
    scene: &ObjectRenderScene,
    producer: &str,
    skip: &[usize],
) -> ProducerOutput<usize> {
    let mut output = ProducerOutput::new(producer, ScenePin::of_packet(scene.packet()));
    for (position, entry) in scene.entries().enumerate() {
        let index = entry.source().source_object_index();
        if !skip.contains(&index) {
            output = output.with_item(index, position);
        }
    }
    output
}

/// Join outputs onto the packet through the real registry.
pub(crate) fn pinned(
    packet: &ObjectFramePacket,
    outputs: Vec<ProducerOutput<usize>>,
) -> PinnedScene<usize> {
    join_scene(packet, &ScenePin::of_packet(packet), outputs).unwrap()
}
