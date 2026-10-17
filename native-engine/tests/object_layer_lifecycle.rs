//! L02-F: layer delete / duplicate / rename / reorder through the actual owner,
//! history and dispatcher. Expected topologies are hand-derived literals over a
//! fixed three-layer document (A=L0, B=L1, C=L2), read back through plain
//! `serde_json::Value` projections, never through the kernel under test.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

pub(super) fn record(layer: &str, stroke: &str, frame: u32) -> Value {
    let corner =
        |x: u32| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
            "layerUid":layer,"strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(0), corner(10)]},
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1}})
}
pub(super) fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}
fn element(layer: &str, stroke: &str) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":0,"order":{}})
}
fn layer(uid: &str, name: &str, color: &str, visible: bool) -> Value {
    json!({"layerUid":uid,"name":name,"visible":visible,"locked":false,
        "color":color,"parentLayerUid":null,"timing":timing()})
}

/// A=L0 (one stroke, layer + element holders), B=L1 (strokes on frames 0 and 1,
/// authored order), C=L2 (one stroke, layer holder); v2 adds a second,
/// mirrored order context.
pub(super) fn base(version: u32) -> Value {
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,
        "layers":[layer("L0","A","#5fa875",true), layer("L1","B","#b49be6",false),
            layer("L2","C","#f0a585",true)],
        "objects":[record("L0","s0",0), record("L1","s1",0), record("L1","s1",1),
            record("L2","s2",0)],
        "propertyHolders":[
            {"target":{"contextId":"scene-root","layerUid":"L0"},
                "properties":[{"property":"position","static":[0,0]}]},
            {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"s0"},
                "controls":[{"key":"xc_a","name":"Amount","type":"number","default":[3]}],
                "properties":[{"property":"opacity","static":[50]},{"property":"xc_a","static":[7]}]},
            {"target":{"contextId":"scene-root","layerUid":"L2"},
                "properties":[{"property":"position","static":[2,0]}]}]});
    if version == 2 {
        let stack = |slots: [u32; 3]| {
            json!([{"layerUid":"L0","naturalPosition":slots[0],"order":{}},
                {"layerUid":"L1","naturalPosition":slots[1],"order":{"static":[9]}},
                {"layerUid":"L2","naturalPosition":slots[2],"order":{}}])
        };
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":stack([0,1,2]),"scopes":[
                {"frameScope":{"kind":"authored","frame":0},"elements":[
                    element("L0","s0"), element("L1","s1"), element("L2","s2")]},
                {"frameScope":{"kind":"authored","frame":1},"elements":[element("L1","s1")]}]},
                {"contextId":"unused-symbol","layers":stack([2,1,0]),"scopes":[]}]});
    }
    document
}
pub(super) fn owner_of(document: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "lifecycle",
        decode_storage_project(document.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
pub(super) fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
pub(super) fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(current(owner).document()).unwrap()
}
pub(super) fn value(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}
pub(super) fn selection(active: &str, selected: &[&str], anchor: Option<&str>) -> Value {
    json!({"active":active,"selected":selected,"anchor":anchor})
}
pub(super) fn delete(active: &str, selected: &[&str]) -> Value {
    json!({"command":"object.layer.delete","selection":selection(active, selected, None)})
}
pub(super) fn duplicate(uid: &str) -> Value {
    json!({"command":"object.layer.duplicate","layerUid":uid})
}
pub(super) fn rename(uid: &str, name: &str) -> Value {
    json!({"command":"object.layer.rename","layerUid":uid,"name":name})
}
pub(super) fn reorder(uids: &[&str], gap: f64, selection: Value) -> Value {
    json!({"command":"object.layer.reorder","layerUids":uids,"gap":gap,"selection":selection})
}
pub(super) fn request(owner: &NativeObjectHistory, id: &str, op: &str, payload: &Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":format!("command.document.object.layer.{op}"),"payload":payload})
}
pub(super) fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request.clone()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
/// Builds the request against the owner's current revision, then dispatches it.
pub(super) fn send(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: &Value) -> Value {
    let req = request(owner, id, op, payload);
    dispatch(owner, &req)
}
/// Typed preparation + commit; returns the candidate's selection as
/// `(active, selected, anchor)` slots of the new layer array.
pub(super) fn apply(
    owner: &mut NativeObjectHistory,
    op: &str,
    payload: &Value,
) -> Option<(usize, Vec<usize>, Option<usize>)> {
    let raw = payload.to_string();
    let candidate = match op {
        "delete" => owner.prepare_layer_delete_json(raw.as_bytes()),
        "duplicate" => owner.prepare_layer_duplicate_json(raw.as_bytes()),
        "rename" => owner.prepare_layer_rename_json(raw.as_bytes()),
        _ => owner.prepare_layer_reorder_json(raw.as_bytes()),
    }
    .unwrap();
    let outcome = candidate
        .selection()
        .map(|s| (s.active, s.selected.clone(), s.anchor));
    owner.commit_layer_lifecycle(candidate).unwrap();
    outcome
}

fn pairs(items: &Value, keys: &[&str]) -> Value {
    let list = items.as_array().cloned().unwrap_or_default();
    Value::Array(
        list.iter()
            .map(|item| Value::Array(keys.iter().map(|key| item[*key].clone()).collect()))
            .collect(),
    )
}
/// Layers, names, objects, holders and every order context as plain literals.
pub(super) fn topology(document: &Value) -> Value {
    let objects: Vec<Value> = document["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let t = &o["target"];
            json!([t["layerUid"], t["strokeId"], t["frameScope"]["frame"]])
        })
        .collect();
    let holders: Vec<Value> = document["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| json!([h["target"]["layerUid"], h["target"]["strokeId"]]))
        .collect();
    let order = document["compositionOrder"]["contexts"]
        .as_array()
        .map(|contexts| {
            Value::Array(
                contexts
                    .iter()
                    .map(|c| {
                        let scopes: Vec<Value> = c["scopes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|s| pairs(&s["elements"], &["layerUid", "strokeId"]))
                            .collect();
                        json!({"context":c["contextId"],
                    "layers":pairs(&c["layers"], &["layerUid", "naturalPosition", "order"]),
                    "scopes":scopes})
                    })
                    .collect(),
            )
        })
        .unwrap_or(Value::Null);
    json!({"layers":pairs(&document["layers"], &["layerUid", "name"]),
        "objects":objects, "holders":holders, "order":order})
}

#[test]
fn the_fixture_topology_projection_reads_the_literal_base_document() {
    let want = json!({"layers":[["L0","A"],["L1","B"],["L2","C"]],
        "objects":[["L0","s0",0],["L1","s1",0],["L1","s1",1],["L2","s2",0]],
        "holders":[["L0",null],["L0","s0"],["L2",null]],
        "order":[{"context":"scene-root",
            "layers":[["L0",0,{}],["L1",1,{"static":[9]}],["L2",2,{}]],
            "scopes":[[["L0","s0"],["L1","s1"],["L2","s2"]],[["L1","s1"]]]},
            {"context":"unused-symbol",
            "layers":[["L0",2,{}],["L1",1,{"static":[9]}],["L2",0,{}]],"scopes":[]}]});
    let owner = owner_of(&base(2));
    assert_eq!(topology(&value(&owner)), want);
    assert_eq!(topology(&value(&owner_of(&base(1))))["order"], Value::Null);
}
