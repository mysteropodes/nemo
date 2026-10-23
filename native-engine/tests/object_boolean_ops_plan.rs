//! L02-I helpers and the boolean-tool transactions: documents with straight-edged filled records, payload
//! builders, typed prepare/commit and dispatch wrappers, and plain `serde_json::Value` projections (never
//! the code under test) shared by the erase, flatten, stack and refusal files of this leaf.
//!
//! Literal expectations are derived by hand from the geometry (areas, bounds, ring cycles). A result ring
//! has no canonical start vertex, so rings are compared as cycles rotated to their smallest point and
//! rounded to 1e-6; where the baseline refits a result to curves, the checks are invariants (ids, order,
//! family, paint, anchors inside the exact polygon result), never invented coordinates.
use super::*;
use crate::commands::PreparedBooleanEdit;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

pub(super) type Pt = (f64, f64);

pub(super) fn corner(p: Pt) -> Value {
    json!({"point":{"x":p.0,"y":p.1},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}})
}
fn target(layer: &str, stroke: &str, frame: u32) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
        "layerUid":layer,"strokeId":stroke})
}
fn geometry(pts: &[Pt], closed: bool) -> Value {
    json!({"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world",
        "handleSpace":"relative","segments":pts.iter().map(|p| corner(*p)).collect::<Vec<_>>()})
}
pub(super) fn paint(c: [f64; 4]) -> Value {
    json!({"kind":"solid","r":c[0],"g":c[1],"b":c[2],"a":c[3]})
}

/// A closed fill-only record of the legacy family.
pub(super) fn legacy(layer: &str, stroke: &str, frame: u32, pts: &[Pt], fill: [f64; 4]) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":target(layer, stroke, frame),"geometry":geometry(pts, true),"fill":paint(fill)})
}

/// A record of the vector family.
#[derive(Clone)]
pub(super) struct VectorSpec {
    pub(super) fill: Option<[f64; 4]>,
    pub(super) stroke: Option<[f64; 4]>,
    pub(super) width: f64,
    pub(super) opacity: f64,
    pub(super) closed: bool,
    pub(super) box_angle: Option<f64>,
}

impl Default for VectorSpec {
    fn default() -> Self {
        VectorSpec {
            fill: Some([0.9, 0.1, 0.1, 1.0]),
            stroke: None,
            width: 4.0,
            opacity: 1.0,
            closed: true,
            box_angle: None,
        }
    }
}

pub(super) fn vector(
    layer: &str,
    stroke: &str,
    frame: u32,
    pts: &[Pt],
    spec: &VectorSpec,
) -> Value {
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":target(layer, stroke, frame),"geometry":geometry(pts, spec.closed),
        "fill":spec.fill.map_or(Value::Null, paint),
        "vector":{"stroke":spec.stroke.map_or(Value::Null, paint),"hasRealStroke":spec.stroke.is_some(),
            "strokeWidth":spec.width,"strokeCap":"round","strokeJoin":"round","opacity":spec.opacity}});
    if let Some(angle) = spec.box_angle {
        record["vectorExt"] = json!({"boxAngle":angle});
    }
    record
}

pub(super) fn square(x: f64, y: f64, s: f64) -> Vec<Pt> {
    vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s)]
}
pub(super) fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> {
    vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}
pub(super) fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Pt> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            (
                ((cx + r * t.cos()) * 1e6).round() / 1e6,
                ((cy + r * t.sin()) * 1e6).round() / 1e6,
            )
        })
        .collect()
}
pub(super) const RED: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
pub(super) const BLUE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];

fn element(layer: &str, stroke: &str, position: usize) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

/// Layers L0 (optionally with a group table) and L1; version 2 adds an explicit order plan covering every
/// authored record in its natural order, one scope per frame.
pub(super) fn document(
    version: u32,
    objects: Vec<Value>,
    groups: Option<Value>,
    holders: Option<Value>,
) -> Value {
    let mut layer0 = json!({"layerUid":"L0","name":"A","visible":true});
    if let Some(table) = groups {
        layer0["groups"] = table;
    }
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":4,"layers":[layer0, {"layerUid":"L1","name":"B","visible":true}],
        "objects":objects.clone()});
    if let Some(holders) = holders {
        document["propertyHolders"] = holders;
    }
    if version == 2 {
        let mut frames: Vec<u64> = objects
            .iter()
            .map(|o| o["target"]["frameScope"]["frame"].as_u64().unwrap())
            .collect();
        frames.sort_unstable();
        frames.dedup();
        let scopes: Vec<Value> = frames
            .iter()
            .map(|frame| {
                let mut counts = std::collections::HashMap::new();
                let elements: Vec<Value> = objects
                    .iter()
                    .filter(|o| o["target"]["frameScope"]["frame"].as_u64() == Some(*frame))
                    .map(|o| {
                        let layer = o["target"]["layerUid"].as_str().unwrap().to_string();
                        let at = counts.entry(layer.clone()).or_insert(0usize);
                        let e = element(&layer, o["target"]["strokeId"].as_str().unwrap(), *at);
                        *at += 1;
                        e
                    })
                    .collect();
                json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
            })
            .collect();
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required",
            "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},
                {"layerUid":"L1","naturalPosition":1,"order":{}}],"scopes":scopes}]});
    }
    document
}

pub(super) fn owner_of(document: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "boolean-ops",
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
pub(super) fn state(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}

/// Records of one layer on one frame, in `objects` order.
pub(super) fn frame_records(state: &Value, layer: &str, frame: u32) -> Vec<Value> {
    state["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| {
            o["target"]["layerUid"] == layer && o["target"]["frameScope"]["frame"] == json!(frame)
        })
        .cloned()
        .collect()
}
pub(super) fn ids_of(records: &[Value]) -> Vec<String> {
    records
        .iter()
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}
/// The z-order of a frame according to the composition plan (version 2 documents).
pub(super) fn plan_order(state: &Value, layer: &str, frame: u32) -> Vec<String> {
    let scopes = state["compositionOrder"]["contexts"][0]["scopes"]
        .as_array()
        .unwrap();
    let scope = scopes
        .iter()
        .find(|s| s["frameScope"]["frame"] == json!(frame))
        .unwrap();
    let mut elements: Vec<(u64, String)> = scope["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["layerUid"] == layer)
        .map(|e| {
            (
                e["naturalPosition"].as_u64().unwrap(),
                e["strokeId"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    elements.sort();
    elements.into_iter().map(|(_, id)| id).collect()
}
pub(super) fn ring_points(record: &Value) -> Vec<[f64; 2]> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            [
                s["point"]["x"].as_f64().unwrap(),
                s["point"]["y"].as_f64().unwrap(),
            ]
        })
        .collect()
}
/// Rounded to 1e-6 and rotated to start at the smallest (x, then y) point.
pub(super) fn cycle(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let round = |v: f64| (v * 1e6).round() / 1e6 + 0.0;
    let rounded: Vec<[f64; 2]> = points.iter().map(|p| [round(p[0]), round(p[1])]).collect();
    let start = (0..rounded.len())
        .min_by(|&a, &b| {
            rounded[a]
                .partial_cmp(&rounded[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    let mut out = rounded[start..].to_vec();
    out.extend_from_slice(&rounded[..start]);
    out
}
pub(super) fn cycle_of(points: &[Pt]) -> Vec<[f64; 2]> {
    cycle(&points.iter().map(|p| [p.0, p.1]).collect::<Vec<_>>())
}
pub(super) fn shoelace(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (p, q) = (points[i], points[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        / 2.0
}
pub(super) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

pub(super) fn op_payload(mode: &str, layer: &str, frame: u32, ids: &[&str]) -> Value {
    json!({"command":"object.boolean.apply","mode":mode,"layerUid":layer,"contextId":"scene-root",
        "frame":frame,"strokeIds":ids})
}
pub(super) fn erase_payload(layer: &str, frame: u32, id: &str, footprint: &[Pt]) -> Value {
    json!({"command":"object.boolean.erase","layerUid":layer,"contextId":"scene-root","frame":frame,
        "strokeId":id,"footprint":footprint.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>()})
}
pub(super) fn flatten_payload(layer: &str, frame: u32, group: &str) -> Value {
    json!({"command":"object.boolean.flatten","layerUid":layer,"contextId":"scene-root","frame":frame,
        "groupId":group})
}
pub(super) fn operation(op: &str) -> String {
    format!("command.document.object.boolean.{op}")
}
pub(super) fn prepare(
    owner: &NativeObjectHistory,
    op: &str,
    payload: &Value,
) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
    let raw = payload.to_string();
    match op {
        "apply" => owner.prepare_boolean_op_json(raw.as_bytes()),
        "erase" => owner.prepare_boolean_erase_json(raw.as_bytes()),
        _ => owner.prepare_boolean_flatten_json(raw.as_bytes()),
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
    owner.commit_boolean_edit(candidate)?;
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

/// One group table of L0: `gid` over the leaf `ids` with one combine mode.
pub(super) fn group_of(gid: &str, mode: &str, ids: &[&str]) -> Value {
    json!({"contextId":"scene-root",
        "entries":[{"gid":gid,"order":ids,"combineMode":mode}],
        "members":ids.iter().map(|id| json!({"strokeId":id,"groupId":gid})).collect::<Vec<_>>()})
}
/// s1..s4 on L0 frame 0, apart, plus x1 on L1.
pub(super) fn four_apart(version: u32) -> Value {
    document(
        version,
        vec![
            legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
            legacy("L0", "s4", 0, &square(150.0, 0.0, 10.0), BLUE),
            legacy("L1", "x1", 0, &square(0.0, 0.0, 10.0), RED),
        ],
        None,
        None,
    )
}

#[test]
fn the_helper_documents_admit_in_both_versions() {
    for version in [1, 2] {
        let owner = owner_of(&four_apart(version));
        assert_eq!(state(&owner)["objects"].as_array().unwrap().len(), 5);
        assert_eq!(owner.history_depths(), (0, 0));
    }
}
