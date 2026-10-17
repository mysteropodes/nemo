//! Shared harness of the widget transaction tests: oracle rows, documents, dispatch helpers.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-widget-edit/oracle.txt");
/// Layer id the first creation receives in `base`.
pub(crate) const UID: &str = "native-layer:1:0";

pub(crate) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| line.split('\t').next() == Some(tag))
        .map(|line| line.split('\t').collect())
        .collect()
}

/// An oracle f64: 16 hex digits of the big-endian bit pattern.
pub(crate) fn f(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).unwrap())
}

/// A request number spelled like JSON.stringify of the same JavaScript value.
pub(crate) fn jnum(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// One-layer document; v2 adds an order plan with two contexts, both listing the layer.
pub(crate) fn base(version: u32) -> Value {
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,"layers":[{"layerUid":"L0"}],"objects":[]});
    if version == 2 {
        let layers = json!([{"layerUid":"L0","naturalPosition":0,"order":{}}]);
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":layers,"scopes":[]},
                {"contextId":"unused-symbol","layers":layers,"scopes":[]}]});
    }
    value
}

pub(crate) fn owner_of(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "widget-edit",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}

pub(crate) fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&encode_storage_project(current(owner).document()).unwrap()).unwrap()
}

pub(crate) fn request(owner: &NativeObjectHistory, id: &str, op: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":format!("command.document.object.widget.{op}"),"payload":payload})
}

pub(crate) fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request.clone()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

/// Build and dispatch one widget request at the owner's current revision.
pub(crate) fn run(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: Value) -> Value {
    let req = request(owner, id, op, payload);
    dispatch(owner, &req)
}

pub(crate) fn run_ok(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: Value) -> Value {
    let reply = run(owner, id, op, payload);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    reply
}

pub(crate) fn create_payload(kind: &str, name: &str, pos: [f64; 2]) -> Value {
    json!({"command":"object.widget.create","kind":kind,"name":name,"pos":[jnum(pos[0]),jnum(pos[1])]})
}

pub(crate) fn settings_payload(uid: &str, set: Value) -> Value {
    json!({"command":"object.widget.settings","layerUid":uid,"set":set})
}

pub(crate) fn write_payload(uid: &str, frame: Value, to: Value) -> Value {
    json!({"command":"object.widget.write","layerUid":uid,"frame":frame,"to":to})
}

pub(crate) fn local(point: [f64; 2]) -> Value {
    json!({"kind":"local","point":[jnum(point[0]),jnum(point[1])]})
}

/// Control key the creation scheme gives the axis `suffix` (`x`, `y`, `value`) of layer `uid`.
pub(crate) fn key(uid: &str, suffix: &str) -> String {
    let stem: String = uid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("xc_{stem}_{suffix}")
}

/// The layer with `uid` in a wire document.
pub(crate) fn layer(wire: &Value, uid: &str) -> Value {
    wire["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layerUid"] == uid)
        .unwrap()
        .clone()
}

/// The property record of control `key` in the first layer-level holder.
pub(crate) fn record(wire: &Value, key: &str) -> Value {
    wire["propertyHolders"][0]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["property"] == key)
        .unwrap_or_else(|| panic!("no record {key} in {wire}"))
        .clone()
}

/// `static[0]` of control `key`.
pub(crate) fn static_of(wire: &Value, key: &str) -> f64 {
    record(wire, key)["static"][0].as_f64().unwrap()
}

/// A widget JSON without its (scheme-defined) control keys, the form the oracle stores.
pub(crate) fn stripped(widget: &Value) -> Value {
    let mut widget = widget.clone();
    widget["x"].as_object_mut().unwrap().remove("key");
    if let Some(y) = widget.get_mut("y") {
        y.as_object_mut().unwrap().remove("key");
    }
    widget
}

/// Controls and seeds as the oracle stores them (keys removed, seeds in control order).
pub(crate) fn controls_and_seeds(wire: &Value) -> (Value, Value) {
    let holder = &wire["propertyHolders"][0];
    let controls: Vec<Value> = holder["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|control| {
            let mut control = control.clone();
            control.as_object_mut().unwrap().remove("key");
            control
        })
        .collect();
    let seeds: Vec<Value> = holder["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|control| record(wire, control["key"].as_str().unwrap())["static"].clone())
        .collect();
    (json!(controls), json!(seeds))
}

/// Everything a refused or replayed request must leave alone.
pub(crate) fn state(owner: &NativeObjectHistory) -> (Value, u64, (usize, usize)) {
    (
        wire(owner),
        owner.content_revision(),
        owner.history_depths(),
    )
}

/// A joystick created at `pos` with the given settings applied; returns the owner.
pub(crate) fn joystick(set: Option<Value>) -> NativeObjectHistory {
    let mut owner = owner_of(&base(1));
    run_ok(
        &mut owner,
        "c",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    if let Some(set) = set {
        run_ok(&mut owner, "s", "settings", settings_payload(UID, set));
    }
    owner
}

/// One generic property edit on control `key` of layer `UID` (the existing owner, used to key a control).
pub(crate) fn property_edit(owner: &mut NativeObjectHistory, id: &str, key: &str, action: Value) {
    let payload = json!({"command":"object.property.edit",
        "target":{"contextId":"scene-root","layerUid":UID},"property":key,"action":action});
    let req = json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.property.edit","payload":payload});
    let reply = dispatch(owner, &req);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
}

pub(crate) fn with(mut payload: Value, change: impl FnOnce(&mut Value)) -> Value {
    change(&mut payload);
    payload
}

pub(crate) fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap()
}

/// A v1 document whose widget layer `W` has the given layer-level controls (keys `kx`, `ky`).
pub(crate) fn broken_document(controls: Value) -> Value {
    let widget = json!({"kind":"joystick","x":{"key":"kx","min":-100,"max":100},
        "y":{"key":"ky","min":-100,"max":100}});
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"W","widget":widget}],"objects":[]});
    if !controls.as_array().unwrap().is_empty() {
        doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"W"},
            "controls":controls,"properties":[]}]);
    }
    doc
}

pub(crate) fn number_control(key: &str, name: &str) -> Value {
    json!({"key":key,"name":name,"type":"number","default":[0]})
}
