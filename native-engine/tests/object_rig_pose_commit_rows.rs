//! Shared harness of the pose transaction tests (L03-Q): documents built from the oracle rows, owners,
//! payloads and dispatch helpers over the REAL `NativeObjectHistory` (no mock owner, no stand-in codec).
//! No test lives here.
use super::object_rig_pose_commit_oracle as oracle;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

/// The layer every harness document is built around.
pub(crate) const LAYER: &str = "L1";
/// A second layer without a rig (the no-op target of a two-layer history).
pub(crate) const OTHER: &str = "L2";
pub(crate) const TOTAL: u32 = 6;

/// A stored stroke record of layer `layer`: closed paths use the legacy family, open ones the vector family.
pub(crate) fn stroke_record(
    layer: &str,
    stroke: &str,
    closed: bool,
    segs: &[[f64; 6]],
    frame: u32,
) -> Value {
    let segments: Vec<Value> = segs
        .iter()
        .map(|s| {
            json!({
                "point": {"x": s[0], "y": s[1]},
                "handleIn": {"x": s[2], "y": s[3]},
                "handleOut": {"x": s[4], "y": s[5]},
            })
        })
        .collect();
    let target = json!({
        "contextId": "scene-root",
        "frameScope": {"kind": "authored", "frame": frame},
        "layerUid": layer,
        "strokeId": stroke,
    });
    let geometry = json!({
        "kind": "cubic-path",
        "closed": closed,
        "coordinateSpace": "document-world",
        "handleSpace": "relative",
        "segments": segments,
    });
    if closed {
        json!({
            "schemaVersion": 1,
            "family": "closed-solid-fill-cubic-path",
            "target": target,
            "geometry": geometry,
            "fill": {"kind": "solid", "r": 0.5, "g": 0.5, "b": 0.5, "a": 1},
        })
    } else {
        json!({
            "schemaVersion": 1,
            "family": "editable-vector-path",
            "target": target,
            "geometry": geometry,
            "fill": null,
            "vector": {
                "stroke": null,
                "hasRealStroke": false,
                "strokeWidth": 1,
                "strokeCap": "round",
                "strokeJoin": "round",
                "opacity": 1,
            },
        })
    }
}

/// The layer timing of a drawing with one key per listed frame.
pub(crate) fn timing_json(keys: &[u32]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|frame| json!({"frame": frame})).collect();
    json!({
        "format": "nemo.native-layer-timing",
        "formatVersion": 1,
        "family": "drawing",
        "contextId": "scene-root",
        "keys": keys,
        "samples": [],
    })
}

/// The layer record: `rig` (omitted for `null`), the timing of `keys` (omitted for `None`) and the lock.
pub(crate) fn layer_json(uid: &str, rig: &Value, keys: Option<&[u32]>, locked: bool) -> Value {
    let mut layer = json!({"layerUid": uid});
    if !rig.is_null() {
        layer["rig"] = rig.clone();
    }
    if let Some(keys) = keys {
        layer["timing"] = timing_json(keys);
    }
    if locked {
        layer["locked"] = json!(true);
    }
    layer
}

/// The records of the row's strokes at frame 0 of `LAYER`.
pub(crate) fn row_records(row: &oracle::Row) -> Vec<Value> {
    row.strokes
        .iter()
        .map(|s| {
            stroke_record(
                LAYER,
                s["id"].as_str().expect("id"),
                s["closed"].as_bool().expect("closed"),
                &oracle::segs(&s["segs"]),
                0,
            )
        })
        .collect()
}

/// The mesh store of the row as the L03-J wire member (`rect` is a raster placement, not store data).
pub(crate) fn mesh_store_json(row: &oracle::Row) -> Value {
    let mut store = serde_json::Map::new();
    for (id, mesh) in row.meshes.as_object().expect("meshes") {
        store.insert(
            id.clone(),
            json!({
                "outline": mesh["outline"],
                "verts": mesh["verts"],
                "tris": mesh["tris"],
                "offsets": mesh["offsets"],
            }),
        );
    }
    Value::Object(store)
}

pub(crate) fn document_json(layers: Vec<Value>, objects: Vec<Value>) -> Value {
    json!({
        "format": "nemo.native-object-document",
        "formatVersion": 1,
        "totalFrames": TOTAL,
        "layers": layers,
        "objects": objects,
    })
}

pub(crate) fn owner_of(doc: &Value) -> NativeObjectHistory {
    let decoded = decode_storage_project(doc.to_string().as_bytes()).expect("admitted document");
    NativeObjectHistory::new("rig-pose", decoded).expect("owner")
}

/// How a harness document is built from a row.
#[derive(Clone, Copy)]
pub(crate) struct Build<'a> {
    pub(crate) keys: Option<&'a [u32]>,
    pub(crate) locked: bool,
    pub(crate) meshes: bool,
}

pub(crate) const PLAIN: Build<'static> = Build {
    keys: None,
    locked: false,
    meshes: false,
};
/// One key at frame 0: every later frame is a hold of it.
pub(crate) const HELD: Build<'static> = Build {
    keys: Some(&[0]),
    locked: false,
    meshes: false,
};

pub(crate) fn doc_for(row: &oracle::Row, build: Build<'_>) -> Value {
    let layer = layer_json(LAYER, &row.rig, build.keys, build.locked);
    let mut doc = document_json(vec![layer], row_records(row));
    if build.meshes {
        doc["imageMeshes"] = mesh_store_json(row);
    }
    doc
}

pub(crate) fn owner_for(row: &oracle::Row, build: Build<'_>) -> NativeObjectHistory {
    owner_of(&doc_for(row, build))
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

/// The rig of `uid` as the export whitelist text (`null` without a rig): the record's own serialization.
pub(crate) fn rig_text_of(owner: &NativeObjectHistory, uid: &str) -> String {
    let snapshot = current(owner);
    match snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == uid)
        .and_then(|layer| layer.rig())
    {
        Some(rig) => serde_json::to_string(rig).unwrap(),
        None => "null".to_owned(),
    }
}

pub(crate) fn rig_text(owner: &NativeObjectHistory) -> String {
    rig_text_of(owner, LAYER)
}

pub(crate) fn rig_value(owner: &NativeObjectHistory) -> Value {
    serde_json::from_str(&rig_text(owner)).unwrap()
}

/// The wire `objects` array.
pub(crate) fn objects_of(owner: &NativeObjectHistory) -> Value {
    wire(owner)["objects"].clone()
}

/// The records of `LAYER` at `frame`, in document order.
pub(crate) fn records_at(owner: &NativeObjectHistory, frame: u32) -> Vec<Value> {
    objects_of(owner)
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| {
            record["target"]["layerUid"] == LAYER
                && record["target"]["frameScope"]["frame"] == frame
        })
        .cloned()
        .collect()
}

/// `[px,py,hix,hiy,hox,hoy]` rows of a wire record.
pub(crate) fn record_segs(record: &Value) -> Vec<[f64; 6]> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let xy = |name: &str| (oracle::num(&s[name]["x"]), oracle::num(&s[name]["y"]));
            let (px, py) = xy("point");
            let (ix, iy) = xy("handleIn");
            let (ox, oy) = xy("handleOut");
            [px, py, ix, iy, ox, oy]
        })
        .collect()
}

pub(crate) fn reset_payload() -> Value {
    json!({"command":"object.rig.pose","layerUid":LAYER,"action":{"kind":"reset"}})
}

pub(crate) fn commit_payload(frame: u32) -> Value {
    json!({"command":"object.rig.pose","layerUid":LAYER,"action":{"kind":"commit","frame":frame}})
}

pub(crate) fn request(owner: &NativeObjectHistory, id: &str, body: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.rig.pose","payload":body})
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

/// Build and dispatch one pose request at the owner's current revision.
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

pub(crate) fn message(reply: &Value) -> &str {
    reply["error"]["message"].as_str().unwrap()
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

pub(crate) fn drop_member(value: &mut Value, name: &str) {
    value.as_object_mut().unwrap().remove(name);
}

/// A refused request: the stable `code`, the message starting with `reason`, nothing published.
pub(crate) fn assert_refused(
    owner: &mut NativeObjectHistory,
    id: &str,
    body: Value,
    want: (&str, &str),
) {
    let before = state(owner);
    let reply = run(owner, id, body);
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(code(&reply), want.0, "{id}");
    assert!(
        message(&reply).starts_with(want.1),
        "{id}: {}",
        message(&reply)
    );
    assert_eq!(state(owner), before, "{id}: a refusal publishes nothing");
}
