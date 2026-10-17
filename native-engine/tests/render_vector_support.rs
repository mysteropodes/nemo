#![allow(dead_code)]
//! Shared fixtures of the L01-I tests: the oracle reader and the document builders. Included (as `support`) by the CPU, Metal
//! and export-parity test files, each of which compiles its own copy.
//!
//! `fixtures/render-vector-stroke/oracle.txt` is produced by `tools/gen.cjs` from the baseline at fbe4b8a3; its row grammar is the
//! header of that script. `scene_doc` builds a real storage-v2 document (numeric composition order) around up to three vector
//! records from the repository's own `paint_test_document`, so the whole path document -> snapshot -> packet -> scene -> paint plan
//! -> compositor is the production path; no part of it is mocked.
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
    ObjectRenderScene, ObjectSceneError,
};
use crate::render_geometry::CubicSegment;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::{PathEl, Point};

pub(crate) const ORACLE: &str = include_str!("fixtures/render-vector-stroke/oracle.txt");
pub(crate) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
/// Canvas of every Metal scene: world coordinates are pixel coordinates.
pub(crate) const SIZE: usize = 64;

fn fields(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}

pub(crate) fn meta(key: &str) -> &'static str {
    fields("meta")
        .into_iter()
        .find(|f| f[1] == key)
        .unwrap_or_else(|| panic!("oracle meta {key}"))[2]
}

/// One certified pixel: `(x, y, straight RGBA8)`.
pub(crate) type Probe = (usize, usize, [u8; 4]);

pub(crate) struct SceneRow {
    pub id: &'static str,
    pub records: Vec<Value>,
    pub probes: Vec<Probe>,
}

pub(crate) fn scene_rows() -> Vec<SceneRow> {
    fields("S")
        .into_iter()
        .map(|f| SceneRow {
            id: f[1],
            records: serde_json::from_str(f[2]).expect("records json"),
            probes: serde_json::from_str::<Vec<Value>>(f[3])
                .expect("probes json")
                .iter()
                .map(|p| {
                    (
                        p[0].as_u64().expect("x") as usize,
                        p[1].as_u64().expect("y") as usize,
                        [0usize, 1, 2, 3].map(|i| p[2][i].as_u64().expect("channel") as u8),
                    )
                })
                .collect(),
        })
        .collect()
}

pub(crate) fn scene_row(id: &str) -> SceneRow {
    scene_rows()
        .into_iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("oracle scene {id}"))
}

pub(crate) struct PathRow {
    pub id: &'static str,
    pub closed: bool,
    pub segments: Vec<CubicSegment>,
    pub elements: Vec<PathEl>,
}

fn pair(value: &Value) -> [f64; 2] {
    [value[0].as_f64().expect("x"), value[1].as_f64().expect("y")]
}

pub(crate) fn path_rows() -> Vec<PathRow> {
    fields("E")
        .into_iter()
        .map(|f| PathRow {
            id: f[1],
            closed: f[2] == "1",
            segments: serde_json::from_str::<Vec<Value>>(f[3])
                .expect("segments json")
                .iter()
                .map(|s| CubicSegment {
                    point: pair(&s["point"]),
                    handle_in: pair(&s["handleIn"]),
                    handle_out: pair(&s["handleOut"]),
                })
                .collect(),
            elements: serde_json::from_str::<Vec<Value>>(f[4])
                .expect("elements json")
                .iter()
                .map(|e| {
                    let n = |i: usize| e[i].as_f64().expect("coordinate");
                    match e[0].as_str().expect("tag") {
                        "M" => PathEl::MoveTo(Point::new(n(1), n(2))),
                        "C" => PathEl::CurveTo(
                            Point::new(n(1), n(2)),
                            Point::new(n(3), n(4)),
                            Point::new(n(5), n(6)),
                        ),
                        "Z" => PathEl::ClosePath,
                        other => panic!("unknown element {other}"),
                    }
                })
                .collect(),
        })
        .collect()
}

pub(crate) struct FallbackRow {
    pub id: &'static str,
    pub has_real_stroke: Option<bool>,
    pub colour_in: Option<String>,
    pub colour_after: Option<String>,
    pub has_real_stroke_after: Option<bool>,
}

pub(crate) fn fallback_rows() -> Vec<FallbackRow> {
    let text = |s: &str| serde_json::from_str::<Option<String>>(s).expect("string or null");
    let flag = |s: &str| serde_json::from_str::<Option<bool>>(s).expect("bool or null");
    fields("F")
        .into_iter()
        .map(|f| FallbackRow {
            id: f[1],
            has_real_stroke: flag(f[2]),
            colour_in: text(f[3]),
            colour_after: text(f[4]),
            has_real_stroke_after: flag(f[5]),
        })
        .collect()
}

/// Shallow merge of `over` onto `base` (both objects).
pub(crate) fn merge(base: Value, over: Value) -> Value {
    let mut out = base;
    for (key, value) in over.as_object().expect("object").iter() {
        out[key.as_str()] = value.clone();
    }
    out
}

pub(crate) fn straight(points: &[[f64; 2]]) -> Vec<Value> {
    points
        .iter()
        .map(|p| {
            json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
        })
        .collect()
}

pub(crate) fn solid(r: f64, g: f64, b: f64, a: f64) -> Value {
    json!({"kind":"solid","r":r,"g":g,"b":b,"a":a})
}

/// A vector body with every optional member absent; `over` replaces members.
pub(crate) fn body(over: Value) -> Value {
    merge(
        json!({"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}),
        over,
    )
}

/// Record parts (geometry, fill, vector) of a vector record.
pub(crate) fn parts(closed: bool, points: &[[f64; 2]], fill: Value, vector: Value) -> Value {
    json!({"geometry":{"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world",
        "handleSpace":"relative","segments":straight(points)},"fill":fill,"vector":vector})
}

pub(crate) const SQUARE: [[f64; 2]; 4] = [[16., 16.], [48., 16.], [48., 48.], [16., 48.]];

/// A storage-v1 document with one vector record (scene-level tests that need no composition order).
pub(crate) fn v1_doc(spec: &Value) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"L1"}],
        "objects":[{"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},
            "geometry":spec["geometry"],"fill":spec["fill"],"vector":spec["vector"]}]})
}

/// A storage-v1 document with one LEGACY `closed-solid-fill-cubic-path` record.
pub(crate) fn legacy_doc(points: &[[f64; 2]], fill: Value) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"L1"}],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":straight(points)},"fill":fill}]})
}

/// A storage-v2 document (numeric composition order) with one layer holding up to three records, painted in the order given.
/// Each record is `{geometry, fill, vector[, vectorExt]}` as in an oracle scene row.
pub(crate) fn scene_doc(records: &[Value]) -> Value {
    assert!((1..=3).contains(&records.len()));
    let ids = ["a", "b", "c"];
    let keep = &ids[..records.len()];
    let mut value = paint_test_document(&[0]);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .retain(|l| l["layerUid"] == "L0");
    value["objects"].as_array_mut().unwrap().retain(|r| {
        r["target"]["layerUid"] == "L0" && keep.contains(&r["target"]["strokeId"].as_str().unwrap())
    });
    let context = &mut value["compositionOrder"]["contexts"][0];
    context["layers"]
        .as_array_mut()
        .unwrap()
        .retain(|l| l["layerUid"] == "L0");
    for scope in context["scopes"].as_array_mut().unwrap() {
        scope["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["layerUid"] == "L0" && keep.contains(&e["strokeId"].as_str().unwrap()));
    }
    for record in value["objects"].as_array_mut().unwrap() {
        let index = ids
            .iter()
            .position(|id| *id == record["target"]["strokeId"].as_str().unwrap())
            .unwrap();
        let spec = &records[index];
        record["family"] = json!("editable-vector-path");
        record["geometry"] = spec["geometry"].clone();
        record["fill"] = spec["fill"].clone();
        record["vector"] = spec["vector"].clone();
        if spec.get("vectorExt").is_some() {
            record["vectorExt"] = spec["vectorExt"].clone();
        }
    }
    value
}

pub(crate) fn snapshot(value: &Value) -> ObjectSnapshot {
    let bytes = serde_json::to_vec(value).expect("document json");
    ObjectSnapshot::new(
        "render-vector",
        decode_storage_project(&bytes).expect("document admitted"),
    )
    .expect("snapshot")
}

pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

pub(crate) fn scene_at(
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> Result<ObjectRenderScene, ObjectSceneError> {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_frame(snapshot, &pins).expect("packet");
    prepare_object_render_scene(&packet, &pins)
}

pub(crate) fn scene_of(snapshot: &ObjectSnapshot) -> Result<ObjectRenderScene, ObjectSceneError> {
    scene_at(snapshot, 0)
}

pub(crate) fn plan_of(snapshot: &ObjectSnapshot) -> ObjectPaintPlan {
    let pins = selector(snapshot, 0);
    let scene = scene_of(snapshot).expect("scene");
    prepare_object_paint_plan(&scene, &pins).expect("paint plan")
}

/// Check certified probes against one 64x64 straight RGBA8 frame; every channel within `tolerance`.
pub(crate) fn probe_failures(bytes: &[u8], probes: &[Probe], tolerance: u8) -> Vec<String> {
    probes
        .iter()
        .filter_map(|&(x, y, want)| {
            let at = (y * SIZE + x) * 4;
            let got: [u8; 4] = bytes[at..at + 4].try_into().unwrap();
            let bad = got.iter().zip(want).any(|(g, w)| g.abs_diff(w) > tolerance);
            bad.then(|| format!("({x},{y}) got {got:?} want {want:?}"))
        })
        .collect()
}

pub(crate) fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    let at = (y * SIZE + x) * 4;
    bytes[at..at + 4].try_into().unwrap()
}
