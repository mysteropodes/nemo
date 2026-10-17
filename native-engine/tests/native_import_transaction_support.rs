//! L10-H test support: documents, vector records and batches built through the REAL owners
//! (`decode_storage_project`, `NativeObjectHistory`, the L01-D record codec), never a test-local
//! record type; plus the readers every transaction test compares through.
use super::{ImportBatch, ImportRecord, NewLayer};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectRecord;
use serde_json::{json, Value};

pub(super) const ORACLE: &str = include_str!("fixtures/native-import-transaction/oracle.txt");
pub(super) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(super) const OP: &str = "command.document.object.import.batch";

/// One admitted `editable-vector-path` record as JSON: a two-anchor open line from (x,0) to (x+10,5).
pub(super) fn vector_json(layer: &str, frame: u32, stroke: &str, x: i64) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
            "layerUid":layer,"strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[
                {"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
                {"point":{"x":x + 10,"y":5},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,
            "strokeWidth":2,"strokeCap":"butt","strokeJoin":"miter","opacity":1}})
}

pub(super) fn record(layer: &str, frame: u32, x: i64) -> ObjectRecord {
    serde_json::from_value(vector_json(layer, frame, "", x)).expect("admitted record shape")
}
pub(super) fn item(source: &str, layer: &str, frame: u32, x: i64) -> ImportRecord {
    ImportRecord {
        source_id: source.into(),
        record: record(layer, frame, x),
    }
}
pub(super) fn layer(source: &str, name: &str) -> NewLayer {
    NewLayer {
        source_id: source.into(),
        name: name.into(),
    }
}

fn plan_of(objects: &[Value]) -> Value {
    let elements: Vec<Value> = objects
        .iter()
        .map(|o| {
            json!({"layerUid":o["target"]["layerUid"],"strokeId":o["target"]["strokeId"],
                "naturalPosition":0,"order":{}})
        })
        .collect();
    let layers = json!([{"layerUid":"L0","naturalPosition":0,"order":{}},
        {"layerUid":"L1","naturalPosition":1,"order":{}}]);
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":layers,"scopes":[
            {"frameScope":{"kind":"authored","frame":0},"elements":elements}]},
            {"contextId":"unused-symbol","layers":layers,"scopes":[]}]})
}

/// Two layers (no timing, like earlier native files) and one record `s0` on `L0` at frame 0.
/// Version 2 adds the explicit composition-order plan covering it.
pub(super) fn base_doc(version: u32) -> Value {
    let objects = vec![vector_json("L0", 0, "s0", 100)];
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,"layers":[{"layerUid":"L0"},{"layerUid":"L1"}],"objects":objects});
    if version == 2 {
        doc["compositionOrder"] = plan_of(doc["objects"].as_array().unwrap());
    }
    doc
}
pub(super) fn history_of(doc: &Value, instance: &str) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_storage_project(doc.to_string().as_bytes()).expect("admitted base document"),
    )
    .expect("history")
}
pub(super) fn history(version: u32) -> NativeObjectHistory {
    history_of(&base_doc(version), "import-tx")
}
pub(super) fn bytes(h: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = h.acquire_snapshot(h.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
pub(super) fn state(h: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(h)).unwrap()
}

/// Everything a refused or no-op operation must leave exactly as it was.
#[derive(Debug, PartialEq)]
pub(super) struct Frozen {
    pub(super) revision: u64,
    pub(super) bytes: Vec<u8>,
    pub(super) depths: (usize, usize),
}
pub(super) fn frozen(h: &NativeObjectHistory) -> Frozen {
    Frozen {
        revision: h.content_revision(),
        bytes: bytes(h),
        depths: h.history_depths(),
    }
}

/// A real visibility edit then undo: leaves a live redo entry, so "redo survives" cannot pass by
/// clearing an already empty stack. Returns the head revision afterwards.
pub(super) fn prime_redo(h: &mut NativeObjectHistory) -> u64 {
    let payload = json!({"command":"object.layer.visibility.set","layerUid":"L0","visible":false});
    let prepared = h
        .prepare_layer_visibility_json(payload.to_string().as_bytes())
        .unwrap();
    let after = h.commit_layer_visibility(prepared).unwrap();
    let head = h.undo(after).unwrap();
    assert_eq!(h.history_depths(), (0, 1));
    head
}

/// Two new layers and four records: frame 0 and frame 3 of the first, frame 0 of the second, and
/// one into the existing layer `L0`.
pub(super) fn mixed_batch() -> ImportBatch {
    ImportBatch {
        layers: vec![layer("fig:a", "Frame A"), layer("fig:b", "Frame B")],
        records: vec![
            item("fig:a:0", "fig:a", 0, 1),
            item("fig:a:1", "fig:a", 3, 2),
            item("fig:b:0", "fig:b", 0, 3),
            item("e:0", "L0", 0, 9),
        ],
        skipped: vec![],
        unavailable: vec![],
    }
}

/// The wire form of a batch (layers and records only).
pub(super) fn wire_batch(layers: Vec<Value>, records: Vec<Value>) -> Value {
    json!({"command":"object.import.batch","layers":layers,"records":records})
}
pub(super) fn request(h: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":h.instance_id(),
        "documentId":h.document_id(),"expectedRevision":h.content_revision(),
        "operation":OP,"payload":payload})
}
pub(super) fn dispatch(h: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        h.try_dispatch_object_command(&serde_json::from_value(request.clone()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
