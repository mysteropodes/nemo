//! Shared harness of the rig edit transaction tests (L03-O): documents built from the oracle rows, owners,
//! payloads and dispatch helpers over the REAL `NativeObjectHistory` (no mock owner, no stand-in codec).
use super::object_rig_edit_oracle as oracle;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

/// The only layer of every harness document.
pub(crate) const LAYER: &str = "L0";

fn point(value: &Value) -> Value {
    json!({"x": value[0], "y": value[1]})
}

/// One stroke of an oracle row as an object record: closed paths are the legacy solid-fill family, open
/// paths the editable vector family (the two families a rig can bind).
pub(crate) fn record(stroke: &Value) -> Value {
    let pts = stroke["pts"].as_array().expect("pts");
    let hs = stroke["hs"].as_array().expect("hs");
    let closed = stroke["closed"].as_bool().expect("closed");
    let zero = json!({"x": 0, "y": 0});
    let segments: Vec<Value> = pts
        .iter()
        .zip(hs)
        .map(|(p, h)| {
            let handle_in = if h[0].is_null() {
                zero.clone()
            } else {
                point(&h[0])
            };
            let handle_out = if h[1].is_null() {
                zero.clone()
            } else {
                point(&h[1])
            };
            json!({"point": point(p), "handleIn": handle_in, "handleOut": handle_out})
        })
        .collect();
    let target = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
        "layerUid": LAYER, "strokeId": stroke["id"]});
    let geometry = json!({"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world",
        "handleSpace":"relative","segments":segments});
    if closed {
        json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":target,
            "geometry":geometry,"fill":{"kind":"solid","r":0.5,"g":0.5,"b":0.5,"a":1}})
    } else {
        json!({"schemaVersion":1,"family":"editable-vector-path","target":target,"geometry":geometry,
            "fill":null,"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,
            "strokeCap":"round","strokeJoin":"round","opacity":1}})
    }
}

/// A v1 document text with one layer carrying `rig` (raw JSON text, kept byte for byte so the authored
/// order of the bone table is exactly what the oracle wrote) and the given records.
pub(crate) fn doc_text(rig: &str, records: &[Value]) -> String {
    let layer = if rig == "null" {
        format!("{{\"layerUid\":\"{LAYER}\"}}")
    } else {
        format!("{{\"layerUid\":\"{LAYER}\",\"rig\":{rig}}}")
    };
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":1,\"totalFrames\":3,\"layers\":[{layer}],\"objects\":{}}}",
        Value::Array(records.to_vec())
    )
}

pub(crate) fn owner_of_text(text: &str) -> NativeObjectHistory {
    NativeObjectHistory::new("rig-edit", decode_storage_project(text.as_bytes()).unwrap()).unwrap()
}

/// The document text holding the row's rig and strokes.
pub(crate) fn text_for(row: &oracle::ActRow) -> String {
    let records: Vec<Value> = row
        .strokes
        .as_array()
        .expect("strokes")
        .iter()
        .map(record)
        .collect();
    doc_text(row.before, &records)
}

/// The owner holding the row's rig and strokes.
pub(crate) fn owner_for(row: &oracle::ActRow) -> NativeObjectHistory {
    owner_of_text(&text_for(row))
}

pub(crate) fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}

pub(crate) fn document(owner: &NativeObjectHistory) -> ObjectDocument {
    current(owner).document().clone()
}

pub(crate) fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(current(owner).document()).unwrap()
}

pub(crate) fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}

/// The rig of the layer as the export whitelist text (`null` without a rig): the record's own
/// serialization, whose member order is the baseline export order.
pub(crate) fn rig_text(owner: &NativeObjectHistory) -> String {
    let snapshot = current(owner);
    match snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == LAYER)
        .and_then(|layer| layer.rig())
    {
        Some(rig) => serde_json::to_string(rig).unwrap(),
        None => "null".to_owned(),
    }
}

/// The wire `objects` array: rest geometry of every stroke.
pub(crate) fn objects_of(owner: &NativeObjectHistory) -> Value {
    wire(owner)["objects"].clone()
}

pub(crate) fn payload(action: Value) -> Value {
    json!({"command":"object.rig.edit","layerUid":LAYER,"action":action})
}

pub(crate) fn request(owner: &NativeObjectHistory, id: &str, body: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.rig.edit","payload":body})
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

/// Build and dispatch one rig edit at the owner's current revision.
pub(crate) fn run(owner: &mut NativeObjectHistory, id: &str, body: Value) -> Value {
    let req = request(owner, id, body);
    dispatch(owner, &req)
}

pub(crate) fn run_ok(owner: &mut NativeObjectHistory, id: &str, body: Value) -> Value {
    let reply = run(owner, id, body);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    reply
}

pub(crate) fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap()
}

/// Everything a refused or replayed request must leave alone.
pub(crate) fn state(owner: &NativeObjectHistory) -> (Vec<u8>, u64, (usize, usize)) {
    (
        bytes(owner),
        owner.content_revision(),
        owner.history_depths(),
    )
}

pub(crate) fn with(mut body: Value, change: impl FnOnce(&mut Value)) -> Value {
    change(&mut body);
    body
}

/// Runs the row's action on an owner built from the row and returns the owner and the reply.
pub(crate) fn apply_row(row: &oracle::ActRow) -> (NativeObjectHistory, Value) {
    let mut owner = owner_for(row);
    let reply = run_ok(&mut owner, row.name, payload(row.action.clone()));
    (owner, reply)
}

/// The full contract of one executed-baseline row: the rig after the edit is the baseline's rig byte
/// for byte, the reply says whether anything changed, the edit is one history entry, undo restores the
/// exact prior document bytes and redo the exact edited ones; no-ops keep bytes, revision and history.
pub(crate) fn check_row(row: &oracle::ActRow) {
    let mut owner = owner_for(row);
    let initial = state(&owner);
    let objects = objects_of(&owner);
    let reply = run_ok(&mut owner, row.name, payload(row.action.clone()));
    assert_eq!(
        rig_text(&owner),
        row.after,
        "{}: rig after the edit",
        row.name
    );
    assert_eq!(reply["result"]["applied"], row.changes(), "{}", row.name);
    assert_eq!(
        reply["result"]["historyEntriesAdded"],
        u64::from(row.changes()),
        "{}",
        row.name
    );
    assert_eq!(objects_of(&owner), objects, "{}: rest geometry", row.name);
    if !row.changes() {
        assert_eq!(
            state(&owner),
            initial,
            "{}: a no-op changes nothing",
            row.name
        );
        return;
    }
    let edited = state(&owner);
    assert_eq!(edited.1, initial.1 + 1, "{}", row.name);
    assert_eq!(edited.2, (1, 0), "{}: one history entry", row.name);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        bytes(&owner),
        initial.0,
        "{}: undo restores the bytes",
        row.name
    );
    assert_eq!(rig_text(&owner), row.before, "{}", row.name);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), edited.0, "{}: redo reapplies", row.name);
    assert_eq!(rig_text(&owner), row.after, "{}", row.name);
}

/// An edit the baseline refused (it returned `false`): the native edit is refused too, with the given
/// code, and nothing is published.
pub(crate) fn check_refused_row(row: &oracle::ActRow, want: &str) {
    assert_eq!(row.ret, json!(false), "{}", row.name);
    assert_eq!(
        row.before, row.after,
        "{}: the baseline stored nothing",
        row.name
    );
    let mut owner = owner_for(row);
    let initial = state(&owner);
    let reply = run(&mut owner, row.name, payload(row.action.clone()));
    assert_eq!(reply["ok"], false, "{}", row.name);
    assert_eq!(code(&reply), want, "{}", row.name);
    assert_eq!(state(&owner), initial, "{}", row.name);
}

pub(crate) fn move_twin() -> Value {
    json!({"kind":"moveSegment","boneId":"bone1","vi":1,"part":"point","to":[110,30]})
}

pub(crate) fn drop_member(value: &mut Value, name: &str) {
    value.as_object_mut().unwrap().remove(name);
}

/// Every flaw is refused as `invalid_request` and leaves the owner untouched.
pub(crate) fn assert_refused(row: &str, flaws: Vec<(&str, Value)>) {
    let row = oracle::act(row);
    for (name, action) in flaws {
        let mut owner = owner_for(&row);
        let before = state(&owner);
        let reply = run(&mut owner, name, payload(action));
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), "invalid_request", "{name}");
        assert_eq!(state(&owner), before, "{name}");
    }
}

/// The unflawed twin is accepted on the same owner shape.
pub(crate) fn assert_accepted(row: &str, twins: Vec<Value>) {
    let row = oracle::act(row);
    for (index, action) in twins.into_iter().enumerate() {
        let mut owner = owner_for(&row);
        let reply = run(&mut owner, &format!("twin{index}"), payload(action));
        assert_eq!(reply["ok"], true, "twin {index}: {reply}");
    }
}
