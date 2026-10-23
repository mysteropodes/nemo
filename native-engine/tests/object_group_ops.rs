//! L02-G helpers: documents with an optional per-layer group table, payload
//! builders, typed prepare/commit and dispatch wrappers, and plain
//! `serde_json::Value` projections (never the kernel under test) shared by the
//! chain, topology, join and control files of this leaf.
use super::*;
use crate::commands::PreparedGroupEdit;
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
fn element(layer: &str, stroke: &str, position: usize) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

/// L0 holds the strokes of each `(frame, ids)` entry, L1 holds the single
/// stroke `x1` on frame 0. `groups` is the optional table of L0. Version 2
/// adds an explicit order plan that covers every authored record.
pub(super) fn document_frames(
    version: u32,
    frames: &[(u32, &[&str])],
    groups: Option<Value>,
) -> Value {
    let mut layer0 = json!({"layerUid":"L0","name":"A","visible":true});
    if let Some(table) = groups {
        layer0["groups"] = table;
    }
    let mut objects: Vec<Value> = Vec::new();
    let mut scopes: Vec<Value> = Vec::new();
    for (frame, ids) in frames {
        let mut elements: Vec<Value> = Vec::new();
        for (position, id) in ids.iter().enumerate() {
            objects.push(record("L0", id, *frame));
            elements.push(element("L0", id, position));
        }
        if *frame == 0 {
            objects.push(record("L1", "x1", 0));
            elements.push(element("L1", "x1", 0));
        }
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
    }
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":4,
        "layers":[layer0, {"layerUid":"L1","name":"B","visible":true}],"objects":objects});
    if version == 2 {
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},
                {"layerUid":"L1","naturalPosition":1,"order":{}}],"scopes":scopes}]});
    }
    document
}
pub(super) fn document(version: u32, items: &[&str], groups: Option<Value>) -> Value {
    document_frames(version, &[(0, items)], groups)
}
pub(super) fn table(entries: Value, members: Value) -> Value {
    json!({"contextId":"scene-root","entries":entries,"members":members})
}
pub(super) fn owner_of(document: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "group-ops",
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
/// `(entries, members)` of L0's table; `([], [])` when the layer has none.
pub(super) fn table_of(owner: &NativeObjectHistory) -> (Value, Value) {
    let document = value(owner);
    let groups = &document["layers"][0]["groups"];
    if groups.is_null() {
        (json!([]), json!([]))
    } else {
        (groups["entries"].clone(), groups["members"].clone())
    }
}
pub(super) fn has_table(owner: &NativeObjectHistory) -> bool {
    !value(owner)["layers"][0]["groups"].is_null()
}

pub(super) fn group(strokes: &[&str], gid: &str, name: &str) -> Value {
    json!({"command":"object.group.create","layerUid":"L0","contextId":"scene-root",
        "groupId":gid,"name":name,"strokeIds":strokes})
}
pub(super) fn ungroup(strokes: &[&str]) -> Value {
    json!({"command":"object.group.ungroup","layerUid":"L0","contextId":"scene-root",
        "strokeIds":strokes})
}
pub(super) fn combine(strokes: &[&str], gid: &str, mode: &str) -> Value {
    json!({"command":"object.group.combine","layerUid":"L0","contextId":"scene-root",
        "groupId":gid,"mode":mode,"strokeIds":strokes})
}
pub(super) fn set_mode(gid: &str, mode: &str) -> Value {
    json!({"command":"object.group.mode.set","layerUid":"L0","contextId":"scene-root",
        "groupId":gid,"mode":mode})
}
pub(super) fn rename(gid: &str, name: &str) -> Value {
    json!({"command":"object.group.rename","layerUid":"L0","contextId":"scene-root",
        "groupId":gid,"name":name})
}
pub(super) fn exit_group(stroke: &str) -> Value {
    json!({"command":"object.group.member.remove","layerUid":"L0","contextId":"scene-root",
        "strokeId":stroke})
}
/// The wire operation of a short op name.
pub(super) fn operation(op: &str) -> String {
    let tail = match op {
        "group" => "create",
        "mode" => "mode.set",
        "exit" => "member.remove",
        other => other,
    };
    format!("command.document.object.group.{tail}")
}
pub(super) fn prepare(
    owner: &NativeObjectHistory,
    op: &str,
    payload: &Value,
) -> Result<PreparedGroupEdit, DispatchErrorCode> {
    let raw = payload.to_string();
    match op {
        "group" => owner.prepare_group_create_json(raw.as_bytes()),
        "ungroup" => owner.prepare_group_ungroup_json(raw.as_bytes()),
        "combine" => owner.prepare_group_combine_json(raw.as_bytes()),
        "mode" => owner.prepare_group_mode_json(raw.as_bytes()),
        "exit" => owner.prepare_group_exit_json(raw.as_bytes()),
        _ => owner.prepare_group_rename_json(raw.as_bytes()),
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
    owner.commit_group_edit(candidate)?;
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
pub(super) fn entry(gid: &str, extra: Value, order: &[&str]) -> Value {
    let mut entry = json!({"gid":gid,"order":order});
    for (key, value) in extra.as_object().unwrap() {
        entry[key] = value.clone();
    }
    entry
}
pub(super) fn tags(pairs: &[(&str, &str)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(stroke, gid)| json!({"strokeId":stroke,"groupId":gid}))
            .collect(),
    )
}

/// Strokes s1..s6 on L0 with two flat groups: `g1` (named "one", unite) over
/// s1,s2 and `g2` (no name, none) over s3,s4; s5 and s6 are loose.
pub(super) fn base(version: u32) -> Value {
    let groups = table(
        json!([
            entry(
                "g1",
                json!({"name":"one","combineMode":"unite"}),
                &["s1", "s2"]
            ),
            entry("g2", json!({"combineMode":"none"}), &["s3", "s4"])
        ]),
        tags(&[("s1", "g1"), ("s2", "g1"), ("s3", "g2"), ("s4", "g2")]),
    );
    document(version, &["s1", "s2", "s3", "s4", "s5", "s6"], Some(groups))
}

#[test]
fn the_helper_documents_admit_with_and_without_a_table_in_both_versions() {
    for version in [1, 2] {
        let plain = owner_of(&document(version, &["s1", "s2"], None));
        assert!(!has_table(&plain));
        let empty = owner_of(&document(
            version,
            &["s1", "s2"],
            Some(table(json!([]), json!([]))),
        ));
        assert!(has_table(&empty));
        assert_eq!(table_of(&empty), (json!([]), json!([])));
        assert_ne!(
            bytes(&plain),
            bytes(&empty),
            "absent and empty tables are different bytes"
        );
    }
}
