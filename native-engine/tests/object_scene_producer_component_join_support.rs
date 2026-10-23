//! Real document/packet builders, test ports and JSON edit helpers for the component
//! producer join (L07-K). See `rows` for the oracle rows they are driven by.

use super::ComponentPorts;
use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedEntry, ExposedValue,
};
use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_codec::decode_storage_project;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub(crate) fn snapshot_of(document: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "component-join",
        decode_storage_project(&serde_json::to_vec(document).unwrap()).expect("scene document"),
    )
    .expect("snapshot")
}

pub(crate) fn packet_of(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        },
    )
    .expect("packet")
}

pub(crate) fn table_of(value: &Value) -> ComponentSet {
    serde_json::from_value(value.clone()).expect("component set")
}

/// Time Remap and camera providers of the tests; every ask is recorded.
#[derive(Default)]
pub(crate) struct FixturePorts {
    pub(crate) cameras: BTreeMap<String, Vec<CameraKey>>,
    /// `None`: the sampler is not wired.
    pub(crate) remap: Option<Result<Option<f64>, &'static str>>,
    pub(crate) remap_asks: Vec<(String, u32)>,
    pub(crate) camera_asks: Vec<String>,
    /// `Some(reason)`: the camera owner reports unavailable.
    pub(crate) camera_failure: Option<&'static str>,
}

impl ComponentPorts for FixturePorts {
    fn time_remap(&mut self, layer_uid: &str, frame: u32) -> Result<Option<f64>, &'static str> {
        self.remap_asks.push((layer_uid.to_owned(), frame));
        self.remap.unwrap_or(Err("time remap sampler is not wired"))
    }
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        self.camera_asks.push(symbol_id.to_owned());
        if let Some(reason) = self.camera_failure {
            return Err(reason);
        }
        Ok(self.cameras.get(symbol_id).map(Vec::as_slice))
    }
}

/// The property runtime of the exposed properties; `wired: false` is "no SMMotion".
#[derive(Default)]
pub(crate) struct Runtime {
    pub(crate) wired: bool,
    pub(crate) values: HashMap<(String, String), ExposedValue>,
    pub(crate) availability_asks: usize,
    pub(crate) reads: Vec<(String, String, u32)>,
}

impl OverridePort for Runtime {
    fn available(&mut self) -> bool {
        self.availability_asks += 1;
        self.wired
    }
    fn register(&mut self, _prop: &ExposedEntry) {}
    fn read(
        &mut self,
        layer_uid: &str,
        key: &str,
        frame: u32,
    ) -> Result<Vec<ExposedValue>, String> {
        self.reads
            .push((layer_uid.to_owned(), key.to_owned(), frame));
        self.values
            .get(&(layer_uid.to_owned(), key.to_owned()))
            .cloned()
            .map(|value| vec![value])
            .ok_or_else(|| format!("no value for {layer_uid}/{key}"))
    }
}

/// The layer JSON of a scene document, for in-place edits of a copied set.
pub(crate) fn layer_mut<'a>(scene: &'a mut Value, uid: &str) -> &'a mut Value {
    scene["layers"]
        .as_array_mut()
        .expect("layers")
        .iter_mut()
        .find(|layer| layer["layerUid"] == uid)
        .unwrap_or_else(|| panic!("no layer {uid}"))
}

/// The instance JSON of a component set, for in-place edits of a copied set.
pub(crate) fn instance_mut<'a>(components: &'a mut Value, uid: &str) -> &'a mut Value {
    components["instances"]
        .as_array_mut()
        .expect("instances")
        .iter_mut()
        .find(|instance| instance["layerUid"] == uid)
        .unwrap_or_else(|| panic!("no instance on {uid}"))
}

/// The component of a set, for in-place edits.
pub(crate) fn component_mut<'a>(components: &'a mut Value, symbol_id: &str) -> &'a mut Value {
    components["components"]
        .as_array_mut()
        .expect("components")
        .iter_mut()
        .find(|component| component["symbolId"] == symbol_id)
        .unwrap_or_else(|| panic!("no component {symbol_id}"))
}

fn legacy_record(layer: &str, id: &str, frame: u32) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
        {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":0.5,"g":0,"b":0,"a":1}})
}

/// The scene document of a set with an ordinary layer `ly_bg` behind every placing
/// layer and `ly_fg` in front, each holding one record at `frame` (so one real packet
/// carries object records and component slots). `explicit` gives one layer a static
/// Order rank.
pub(crate) fn scene_with_objects(base: &Value, frame: u32, explicit: Option<(&str, f64)>) -> Value {
    let mut doc = base.clone();
    let layers = doc["layers"].as_array_mut().expect("layers");
    layers.insert(0, json!({ "layerUid": "ly_bg" }));
    layers.push(json!({ "layerUid": "ly_fg" }));
    doc["objects"] = json!([
        legacy_record("ly_bg", "bg1", frame),
        legacy_record("ly_fg", "fg1", frame)
    ]);
    let context = &mut doc["compositionOrder"]["contexts"][0];
    let stack = context["layers"].as_array_mut().expect("plan layers");
    for entry in stack.iter_mut() {
        let at = entry["naturalPosition"].as_u64().expect("position");
        entry["naturalPosition"] = json!(at + 1);
    }
    let last = stack.len() + 1;
    stack.insert(
        0,
        json!({"layerUid":"ly_bg","naturalPosition":0,"order":{}}),
    );
    stack.push(json!({"layerUid":"ly_fg","naturalPosition":last,"order":{}}));
    if let Some((uid, rank)) = explicit {
        let entry = stack
            .iter_mut()
            .find(|entry| entry["layerUid"] == uid)
            .expect("explicit layer");
        entry["order"] = json!({ "static": [rank] });
    }
    let scope = context["scopes"]
        .as_array_mut()
        .expect("scopes")
        .iter_mut()
        .find(|scope| scope["frameScope"]["frame"] == frame)
        .expect("scope of the frame");
    scope["elements"] = json!([
        {"layerUid":"ly_bg","strokeId":"bg1","naturalPosition":0,"order":{}},
        {"layerUid":"ly_fg","strokeId":"fg1","naturalPosition":0,"order":{}}
    ]);
    doc
}

/// `doc` (from `scene_with_objects`) with extra legacy records `ids` authored at `frame`
/// on `layer`, in the order plan scope of that frame (natural positions 0.. in the given
/// order): a placing layer that also holds records of its own.
pub(crate) fn with_stray_records(doc: &Value, layer: &str, ids: &[&str], frame: u32) -> Value {
    let mut doc = doc.clone();
    let objects = doc["objects"].as_array_mut().expect("objects");
    for id in ids {
        objects.push(legacy_record(layer, id, frame));
    }
    let scope = doc["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .expect("scopes")
        .iter_mut()
        .find(|scope| scope["frameScope"]["frame"] == frame)
        .expect("scope of the frame");
    let elements = scope["elements"].as_array_mut().expect("elements");
    for (position, id) in ids.iter().enumerate() {
        elements
            .push(json!({"layerUid":layer,"strokeId":id,"naturalPosition":position,"order":{}}));
    }
    doc
}
