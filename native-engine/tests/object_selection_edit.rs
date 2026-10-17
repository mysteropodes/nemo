//! L02-H helpers: documents (v1 and v2 with a derived order plan), payload
//! builders, typed prepare / apply and dispatch wrappers, and plain
//! `serde_json::Value` projections (never the plan under test) shared by the
//! delete, duplicate, split, clipboard and control files of this leaf.
use super::*;
use crate::commands::PreparedSelectionEdit;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn corner(x: i64, y: i64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
fn target(layer: &str, stroke: &str, frame: u32) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
        "layerUid":layer,"strokeId":stroke})
}
/// A filled shape (legacy family) with points `(x, 0)` and `(x + 10, 5)`.
pub(super) fn shape(layer: &str, stroke: &str, frame: u32, x: i64) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":target(layer, stroke, frame),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(x, 0), corner(x + 10, 5)]},
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1}})
}
/// An unfilled open stroke (vector family, explicit null fill).
pub(super) fn line(layer: &str, stroke: &str, frame: u32, x: i64) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":target(layer, stroke, frame),
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(x, 0), corner(x + 10, 5)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,
            "strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
pub(super) fn layer(uid: &str, name: &str) -> Value {
    json!({"layerUid":uid,"name":name,"visible":true})
}
pub(super) fn timing(keys: &[u32], samples: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root",
        "keys":keys.iter().map(|frame| json!({"frame":frame})).collect::<Vec<_>>(),
        "samples":samples.iter().map(|frame| json!({"frame":frame})).collect::<Vec<_>>()})
}
pub(super) fn timed(uid: &str, name: &str, keys: &[u32], samples: &[u32]) -> Value {
    let mut value = layer(uid, name);
    value["timing"] = timing(keys, samples);
    value
}

/// Order plan covering every authored scene-root record: one element per
/// record, the natural slot being its rank inside its layer and frame.
fn plan(layers: &[Value], objects: &[Value]) -> Value {
    let mut frames: Vec<u64> = objects
        .iter()
        .filter(|record| record["target"]["frameScope"]["kind"] == "authored")
        .map(|record| record["target"]["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    frames.sort_unstable();
    frames.dedup();
    let scopes: Vec<Value> = frames
        .iter()
        .map(|frame| {
            let mut ranks: Vec<(String, u64)> = Vec::new();
            let mut elements: Vec<Value> = Vec::new();
            for record in objects {
                let at = &record["target"];
                if at["frameScope"]["frame"].as_u64() != Some(*frame) {
                    continue;
                }
                let uid = at["layerUid"].as_str().unwrap().to_string();
                let rank = match ranks.iter_mut().find(|entry| entry.0 == uid) {
                    Some(entry) => {
                        entry.1 += 1;
                        entry.1
                    }
                    None => {
                        ranks.push((uid.clone(), 0));
                        0
                    }
                };
                elements.push(json!({"layerUid":uid,"strokeId":at["strokeId"],
                    "naturalPosition":rank,"order":{}}));
            }
            json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
        })
        .collect();
    let stack: Vec<Value> = layers
        .iter()
        .enumerate()
        .map(
            |(slot, layer)| json!({"layerUid":layer["layerUid"],"naturalPosition":slot,"order":{}}),
        )
        .collect();
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required",
        "referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":stack,"scopes":scopes}]})
}
/// A document of six frames; version 2 adds the derived order plan.
pub(super) fn document(version: u32, layers: Vec<Value>, objects: Vec<Value>) -> Value {
    let order = if version == 2 {
        Some(plan(&layers, &objects))
    } else {
        None
    };
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":6,"layers":layers,"objects":objects});
    if let Some(order) = order {
        document["compositionOrder"] = order;
    }
    document
}
pub(super) fn owner_of(document: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "selection-edit",
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
pub(super) fn frozen(owner: &NativeObjectHistory) -> (Vec<u8>, (usize, usize), u64) {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}

/// Authored scene-root stroke ids of one layer cell, in document order.
pub(super) fn strokes_at(document: &Value, layer: &str, frame: u32) -> Vec<String> {
    document["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| {
            record["target"]["layerUid"] == layer
                && record["target"]["frameScope"]["frame"] == frame
                && record["target"]["frameScope"]["kind"] == "authored"
        })
        .map(|record| record["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}
/// The record of one cell by stroke id.
pub(super) fn record_at(document: &Value, layer: &str, frame: u32, stroke: &str) -> Value {
    document["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| {
            record["target"]["layerUid"] == layer
                && record["target"]["frameScope"]["frame"] == frame
                && record["target"]["strokeId"] == stroke
        })
        .cloned()
        .unwrap_or(Value::Null)
}
/// `(strokeId, naturalPosition)` of one layer in one order scope, scene-root.
pub(super) fn elements_at(document: &Value, layer: &str, frame: u32) -> Vec<(String, u64)> {
    let scopes = &document["compositionOrder"]["contexts"][0]["scopes"];
    scopes
        .as_array()
        .unwrap()
        .iter()
        .filter(|scope| scope["frameScope"]["frame"] == frame)
        .flat_map(|scope| scope["elements"].as_array().unwrap().iter())
        .filter(|element| element["layerUid"] == layer)
        .map(|element| {
            (
                element["strokeId"].as_str().unwrap().to_string(),
                element["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect()
}
/// The `points` of a record's two anchors as `[x0, y0, x1, y1]`.
pub(super) fn corners(record: &Value) -> Vec<f64> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|segment| {
            [
                segment["point"]["x"].as_f64().unwrap(),
                segment["point"]["y"].as_f64().unwrap(),
            ]
        })
        .collect()
}

pub(super) fn selection(command: &str, layer: &str, frame: u32, mode: &str, ids: &[&str]) -> Value {
    json!({"command":command,"contextId":"scene-root","layerUid":layer,"frame":frame,
        "mode":mode,"strokeIds":ids})
}
pub(super) fn delete(layer: &str, frame: u32, mode: &str, ids: &[&str]) -> Value {
    selection("object.selection.delete", layer, frame, mode, ids)
}
pub(super) fn duplicate(layer: &str, frame: u32, mode: &str, ids: &[&str]) -> Value {
    selection("object.selection.duplicate", layer, frame, mode, ids)
}
pub(super) fn split(layer: &str, frame: u32) -> Value {
    json!({"command":"object.layer.split","contextId":"scene-root","layerUid":layer,
        "frame":frame,"labels":{"shape":"Forme","stroke":"Trait"}})
}
/// The wire operation of a short op name.
pub(super) fn operation(op: &str) -> String {
    let (family, tail) = match op {
        "delete" | "duplicate" => ("selection", op),
        "split" => ("layer", "split"),
        _ => ("clipboard", "paste"),
    };
    format!("command.document.object.{family}.{tail}")
}
pub(super) fn prepare(
    owner: &NativeObjectHistory,
    op: &str,
    payload: &Value,
) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
    let raw = payload.to_string();
    match op {
        "delete" => owner.prepare_selection_delete_json(raw.as_bytes()),
        "duplicate" => owner.prepare_selection_duplicate_json(raw.as_bytes()),
        "split" => owner.prepare_layer_split_json(raw.as_bytes()),
        _ => owner.prepare_clipboard_paste_json(raw.as_bytes()),
    }
}
/// Typed preparation + commit; `Ok(changed)` or the typed refusal.
pub(super) fn apply(
    owner: &mut NativeObjectHistory,
    op: &str,
    payload: &Value,
) -> Result<bool, DispatchErrorCode> {
    let candidate = prepare(owner, op, payload)?;
    let changed = candidate.changed();
    owner.commit_selection_edit(candidate)?;
    Ok(changed)
}
pub(super) fn request(owner: &NativeObjectHistory, id: &str, op: &str, payload: &Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":operation(op),"payload":payload})
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
pub(super) fn send(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: &Value) -> Value {
    let req = request(owner, id, op, payload);
    dispatch(owner, &req)
}

/// L0 "Hero" (keys 0 and 3) holds `a b c` on frame 0 and `a d` on frame 3 (so
/// `a` is one stroke across two frames); L1 "Side" holds `x` on frame 0.
pub(super) fn base(version: u32) -> Value {
    document(
        version,
        vec![timed("L0", "Hero", &[0, 3], &[]), layer("L1", "Side")],
        vec![
            shape("L0", "a", 0, 0),
            shape("L0", "b", 0, 20),
            shape("L0", "c", 0, 40),
            shape("L0", "a", 3, 0),
            shape("L0", "d", 3, 60),
            shape("L1", "x", 0, 80),
        ],
    )
}

#[test]
fn the_helper_documents_admit_in_both_versions_and_the_plan_derives_ranks() {
    for version in [1, 2] {
        let owner = owner_of(&base(version));
        let document = value(&owner);
        assert_eq!(strokes_at(&document, "L0", 0), ["a", "b", "c"]);
        assert_eq!(strokes_at(&document, "L0", 3), ["a", "d"]);
        assert_eq!(strokes_at(&document, "L1", 0), ["x"]);
        assert_eq!(
            corners(&record_at(&document, "L0", 0, "b")),
            [20.0, 0.0, 30.0, 5.0]
        );
        if version == 2 {
            assert_eq!(
                elements_at(&document, "L0", 0),
                [
                    ("a".to_string(), 0),
                    ("b".to_string(), 1),
                    ("c".to_string(), 2)
                ]
            );
            assert_eq!(elements_at(&document, "L1", 0), [("x".to_string(), 0)]);
        } else {
            assert!(document.get("compositionOrder").is_none());
        }
    }
}
