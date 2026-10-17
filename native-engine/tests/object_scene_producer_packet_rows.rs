//! Real one-layer document, snapshot and packet builders shared by the L07-L producer tests.
//!
//! `document` stores a single layer `L0` with one closed solid-fill record per id; objects and plan
//! elements are written reversed, so array positions never equal the natural paint order
//! `ids[0]` (back) .. `ids[n-1]` (front). `index_of` maps a stroke id to the packet's
//! `source_object_index`, the key every producer claims by.

use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) const FRAME: u32 = 3;

fn record(id: &str, gid: usize) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":FRAME},"layerUid":"L0","strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
        {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":(gid as f64 + 1.0) / 100.0,"g":0,"b":0,"a":0.75}})
}

pub(crate) fn document(ids: &[&str]) -> Value {
    let mut objects: Vec<Value> = ids
        .iter()
        .enumerate()
        .map(|(k, id)| record(id, k))
        .collect();
    let mut elements: Vec<Value> = ids
        .iter()
        .enumerate()
        .map(|(k, id)| json!({"layerUid":"L0","strokeId":id,"naturalPosition":k,"order":{}}))
        .collect();
    objects.reverse();
    elements.reverse();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L0"}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":FRAME},"elements":elements}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}

pub(crate) fn snapshot(ids: &[&str], instance: &str) -> ObjectSnapshot {
    ObjectSnapshot::new(
        instance,
        decode_storage_project(&serde_json::to_vec(&document(ids)).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn packet(snapshot: &ObjectSnapshot) -> ObjectFramePacket {
    prepare_object_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: FRAME,
        },
    )
    .unwrap()
}

/// The packet's `source_object_index` of the record with this stroke id.
pub(crate) fn index_of(packet: &ObjectFramePacket, id: &str) -> usize {
    packet
        .records()
        .find(|entry| entry.record().target().stroke_id() == id)
        .map(|entry| entry.source_object_index())
        .unwrap_or_else(|| panic!("no record {id}"))
}
