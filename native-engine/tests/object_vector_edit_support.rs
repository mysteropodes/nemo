//! L01-F test support: the baseline oracle rows (fixtures/object-vector-edit/oracle.txt), the cross-check that
//! re-derives every native value from the baseline column, document builders and the real dispatch caller.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

pub(super) const ORACLE: &str = include_str!("fixtures/object-vector-edit/oracle.txt");
pub(super) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(super) const SERP_SLICE: &str =
    "c71e9d4fbae1837b3f50bec603808a701ed629412eb050c64b9a62b8710e635e";
pub(super) const DESP_SLICE: &str =
    "e14636767dccba47d429b7f7097bef50d9682658942b9a8373a5b6f403a42044";
pub(super) const OP: &str = "command.document.object.vector.edit";

pub(super) struct Rec {
    pub(super) label: String,
    pub(super) baseline: Value,
    pub(super) native: Value,
}
pub(super) struct Step {
    pub(super) what: String,
    pub(super) order: Value,
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle cell is JSON")
}
pub(super) fn oracle() -> (Vec<(String, String)>, Vec<Rec>, Vec<Step>) {
    let (mut meta, mut recs, mut steps) = (Vec::new(), Vec::new(), Vec::new());
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols[0] {
            "meta" => meta.push((cols[1].to_string(), cols[2].to_string())),
            "rec" => {
                assert_eq!(cols.len(), 5, "{line}");
                parse(cols[2]);
                recs.push(Rec {
                    label: cols[1].into(),
                    baseline: parse(cols[3]),
                    native: parse(cols[4]),
                });
            }
            "seq" => {
                assert_eq!(cols.len(), 4, "{line}");
                assert_eq!(cols[1].parse::<usize>().unwrap(), steps.len());
                steps.push(Step {
                    what: cols[2].into(),
                    order: parse(cols[3]),
                });
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    (meta, recs, steps)
}
pub(super) fn rec(label: &str) -> Rec {
    let (_, recs, _) = oracle();
    recs.into_iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("no record {label}"))
}

fn channels(hex: &str) -> [f64; 4] {
    let byte =
        |i: usize| f64::from(u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap()) / 255.0;
    [
        byte(0),
        byte(1),
        byte(2),
        if hex.len() == 9 { byte(3) } else { 1.0 },
    ]
}
fn same_paint(native: &Value, hex: &str) -> bool {
    let want = channels(hex);
    ["r", "g", "b", "a"]
        .iter()
        .zip(want)
        .all(|(key, want)| native[*key].as_f64() == Some(want))
        && native["kind"] == "solid"
}
/// Re-derive every native value from the baseline serP column of the same row; Err names the first mismatch.
pub(super) fn check_record(rec: &Rec) -> Result<(), String> {
    let (b, n) = (&rec.baseline, &rec.native);
    let bad = |what: &str| Err(format!("{}: {what}", rec.label));
    if n["family"] != "editable-vector-path" || n["schemaVersion"] != 1 {
        return bad("family");
    }
    if n["geometry"]["closed"] != b["closed"] {
        return bad("closed");
    }
    let (bs, ns) = (
        b["segments"].as_array().unwrap(),
        n["geometry"]["segments"].as_array().unwrap(),
    );
    if bs.len() != ns.len() {
        return bad("anchor count");
    }
    for (bs, ns) in bs.iter().zip(ns) {
        for (key, bkey) in [
            ("point", "point"),
            ("handleIn", "handleIn"),
            ("handleOut", "handleOut"),
        ] {
            if ns[key]["x"] != bs[bkey][0] || ns[key]["y"] != bs[bkey][1] {
                return bad(key);
            }
        }
    }
    match (b["fillColor"].as_str(), &n["fill"]) {
        (None, Value::Null) => {}
        (Some(hex), fill) if same_paint(fill, hex) => {}
        _ => return bad("fill"),
    }
    let v = &n["vector"];
    let real = b["hasRealStroke"].as_bool().unwrap();
    if v["hasRealStroke"] != json!(real) {
        return bad("hasRealStroke");
    }
    match (real, b["strokeColor"].as_str(), &v["stroke"]) {
        (false, _, Value::Null) => {}
        (true, Some(hex), stroke) if same_paint(stroke, hex) => {}
        _ => return bad("stroke"),
    }
    for key in ["strokeWidth", "strokeCap", "strokeJoin", "opacity"] {
        if v[key] != b[key] {
            return bad(key);
        }
    }
    for key in ["miterLimit", "dashArray", "dashOffset", "paintOrder"] {
        if v.get(key) != b.get(key) {
            return bad(key);
        }
    }
    Ok(())
}

pub(super) fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
        "layerUid":layer,"strokeId":stroke})
}
/// The oracle's native record placed at a target (stroke "" is the create sentinel).
pub(super) fn record_at(label: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    let mut record = rec(label).native;
    record["target"] = target(layer, frame, stroke);
    record
}

/// Scene-root authored plan derived from the objects: one scope per frame (ascending), per-layer positions
/// in object order; a second declared context stays empty.
fn plan(objects: &[Value]) -> Value {
    let mut frames: Vec<u64> = objects
        .iter()
        .filter(|o| o["target"]["frameScope"]["kind"] == "authored")
        .map(|o| o["target"]["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    frames.sort_unstable();
    frames.dedup();
    let scopes: Vec<Value> = frames
        .iter()
        .map(|frame| {
            let mut seen: Vec<(String, u64)> = Vec::new();
            let mut elements = Vec::new();
            for o in objects.iter().filter(|o| {
                o["target"]["frameScope"]["kind"] == "authored"
                    && o["target"]["frameScope"]["frame"] == json!(frame)
            }) {
                let layer = o["target"]["layerUid"].as_str().unwrap().to_string();
                let position = seen.iter().filter(|(l, _)| *l == layer).count() as u64;
                seen.push((layer.clone(), position));
                elements.push(json!({"layerUid":layer,"strokeId":o["target"]["strokeId"],
                    "naturalPosition":position,"order":{}}));
            }
            json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
        })
        .collect();
    let layers = json!([{"layerUid":"L1","naturalPosition":0,"order":{}},
        {"layerUid":"L2","naturalPosition":1,"order":{}}]);
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":layers,"scopes":scopes},
            {"contextId":"unused-symbol","layers":layers,"scopes":[]}]})
}
pub(super) fn document(version: u32, objects: Vec<Value>, holders: Option<Value>) -> Value {
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":24,
        "layers":[{"layerUid":"L1"},{"layerUid":"L2"}],"objects":objects});
    if version == 2 {
        doc["compositionOrder"] = plan(doc["objects"].as_array().unwrap());
    }
    if let Some(holders) = holders {
        doc["propertyHolders"] = holders;
    }
    doc
}
/// Three admitted vector records on two layers and two frames (fill/stroke variety from the oracle).
pub(super) fn seeds() -> Vec<Value> {
    vec![
        record_at("A", "L1", 0, "s-a"),
        record_at("B", "L2", 0, "s-b"),
        record_at("C", "L1", 5, "s-c"),
    ]
}
pub(super) fn owner_of(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "vector-edit",
        decode_storage_project(doc.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
pub(super) fn state(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(snapshot.document()).unwrap()).unwrap()
}
pub(super) fn objects(state: &Value) -> &Vec<Value> {
    state["objects"].as_array().unwrap()
}
pub(super) fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
pub(super) fn request(owner: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":OP,"payload":payload})
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
pub(super) fn create(record: Value) -> Value {
    json!({"command":"object.vector.create","record":record})
}
pub(super) fn replace(record: Value) -> Value {
    json!({"command":"object.vector.replace","record":record})
}
pub(super) fn delete(target: Value) -> Value {
    json!({"command":"object.vector.delete","stableTarget":target})
}
/// Send one payload with a fresh request id; returns the response.
pub(super) fn send(owner: &mut NativeObjectHistory, id: &str, payload: Value) -> Value {
    let request = request(owner, id, payload);
    dispatch(owner, &request)
}
pub(super) fn created_id(response: &Value) -> String {
    assert_eq!(response["ok"], true, "{response}");
    response["result"]["strokeId"].as_str().unwrap().to_string()
}

/// A real unlocked edit then undo leaves a live redo entry, so a refusal cannot
/// pass by clearing an already empty history. Return the exact content redo owes.
pub(super) fn seed_redo(owner: &mut NativeObjectHistory) -> Vec<u8> {
    let before = bytes(owner);
    let revision = owner.content_revision();
    let response = send(owner, "redo-seed", replace(record_at("D2", "L2", 0, "s-b")));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["applied"], true);
    let owed = bytes(owner);
    assert_ne!(owed, before);
    assert_eq!(owner.undo(revision + 1), Ok(revision + 2));
    assert_eq!(bytes(owner), before);
    assert_eq!(owner.history_depths(), (0, 1));
    owed
}

/// The caller's immutable source, current snapshot identity, revision and both
/// history stacks must survive the exact typed refusal and its replay.
fn assert_dispatch_error(owner: &mut NativeObjectHistory, id: &str, payload: Value, code: &str) {
    let revision = owner.content_revision();
    let pinned = owner.acquire_snapshot(revision).unwrap();
    let snapshot_id = pinned.snapshot_id().to_owned();
    let source = serde_json::to_value(pinned.document()).unwrap();
    let before = bytes(owner);
    let depths = owner.history_depths();
    let request = request(owner, id, payload);
    let response = dispatch(owner, &request);
    assert_eq!(response["requestId"], id);
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error"]["code"], code, "{response}");
    assert_eq!(response["contentRevision"], revision);
    assert_eq!(dispatch(owner, &request), response, "exact refusal replay");
    assert_eq!(owner.content_revision(), revision);
    assert_eq!(owner.history_depths(), depths);
    assert_eq!(bytes(owner), before);
    let current = owner.acquire_snapshot(revision).unwrap();
    assert_eq!(current.snapshot_id(), snapshot_id);
    assert_eq!(serde_json::to_value(current.document()).unwrap(), source);
    assert_eq!(serde_json::to_value(pinned.document()).unwrap(), source);
}

pub(super) fn assert_unavailable(owner: &mut NativeObjectHistory, id: &str, payload: Value) {
    assert_eq!(
        owner
            .prepare_vector_edit_json(payload.to_string().as_bytes())
            .unwrap_err(),
        crate::request_receipts::DispatchErrorCode::Unavailable,
        "{id}"
    );
    assert_dispatch_unavailable(owner, id, payload);
}

/// Unsupported Drawing scope is an exact Unavailable refusal.
pub(super) fn assert_dispatch_unavailable(
    owner: &mut NativeObjectHistory,
    id: &str,
    payload: Value,
) {
    assert_dispatch_error(owner, id, payload, "unavailable");
}
/// The command/reply cannot fit its admitted envelope: exact InvalidRequest.
pub(super) fn assert_dispatch_invalid_request(
    owner: &mut NativeObjectHistory,
    id: &str,
    payload: Value,
) {
    assert_dispatch_error(owner, id, payload, "invalid_request");
}
