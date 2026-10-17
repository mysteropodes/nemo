//! Oracle row parsing, state building and operation replay for the image-mesh binding kernel
//! (L03-R, F17/F18).
//!
//! `fixtures/object-image-mesh-binding/oracle.txt` is produced by `tools/gen.cjs` (kept outside the
//! repo; sha256 in the leaf HANDOFF) from `src/js/image-mesh.js` at fbe4b8a3 run in a Node `vm` with
//! the vendored Delaunator, a fresh vm per scenario (the baseline counter `_seq` is module state).
//! Every state change in a row is made by the real baseline functions and the `remapMeshes` block of
//! `duplicateLayer` cut verbatim out of `src/js/timeline.js`.
//!
//! Tab separated rows: `scn name storeJSON setsJSON opsJSON resultsJSON snapsJSON`. A layer is
//! `{nv, vm, fr:[[slot]]}` and a slot `{k:"r", m:tag|null, x:int}` (a raster) or `{k:"p"}` (any other
//! stroke). `snaps[0]` is the state right after `load`, `snaps[i + 1]` the state after op `i`.
use super::{BindingLayer, BindingSlot, MeshBindingState, SlotTarget};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_binding_ops::VideoFamily;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{ImageMesh, ImageMeshStore};
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-binding/oracle.txt");

pub(crate) struct Scenario {
    pub(crate) name: String,
    pub(crate) store_text: String,
    pub(crate) sets: Value,
    pub(crate) ops: Vec<Value>,
    pub(crate) results: Vec<Value>,
    pub(crate) snaps: Vec<Value>,
}

pub(crate) fn scenarios() -> Vec<Scenario> {
    let list = |text: &str| -> Vec<Value> { serde_json::from_str(text).expect("json list") };
    ORACLE
        .lines()
        .filter(|line| line.starts_with("scn\t"))
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 7, "scn row has seven fields: {}", f[1]);
            Scenario {
                name: f[1].into(),
                store_text: f[2].into(),
                sets: serde_json::from_str(f[3]).expect("sets"),
                ops: list(f[4]),
                results: list(f[5]),
                snaps: list(f[6]),
            }
        })
        .collect()
}

/// A raster record at `x` carrying `mesh` (or no tag), built through the strict L06-C codec.
pub(crate) fn raster(x: i64, mesh: Option<&str>) -> BindingSlot {
    let tag = mesh
        .map(|id| format!(",\"meshId\":\"{id}\""))
        .unwrap_or_default();
    let text = format!("{{\"assetId\":\"a1\",\"x\":{x},\"y\":20,\"width\":64,\"height\":32{tag}}}");
    BindingSlot::Raster(RasterItem::decode(&text).expect("raster item"))
}

fn slot_from(value: &Value) -> BindingSlot {
    match value["k"].as_str() {
        Some("r") => raster(value["x"].as_i64().expect("x"), value["m"].as_str()),
        _ => BindingSlot::NonRaster,
    }
}

pub(crate) fn layer_from(value: &Value) -> BindingLayer {
    let frames = value["fr"].as_array().expect("fr");
    BindingLayer {
        frames: frames
            .iter()
            .map(|frame| {
                frame
                    .as_array()
                    .expect("frame")
                    .iter()
                    .map(slot_from)
                    .collect()
            })
            .collect(),
        native_video: value["nv"].as_i64() == Some(1),
        video_mesh_id: value["vm"].as_str().map(str::to_string),
    }
}

pub(crate) fn initial_state(scenario: &Scenario) -> MeshBindingState {
    let store: ImageMeshStore = serde_json::from_str(&scenario.store_text).expect("store");
    store.validate().expect("the baseline store is admitted");
    let sets = scenario
        .sets
        .as_array()
        .expect("sets")
        .iter()
        .map(|set| {
            set.as_array()
                .expect("set")
                .iter()
                .map(layer_from)
                .collect()
        })
        .collect();
    MeshBindingState::new(store, sets).expect("state")
}

fn slot_json(slot: &BindingSlot) -> Value {
    match slot {
        BindingSlot::Raster(item) => {
            json!({"k": "r", "m": item.mesh_id(), "x": item.center().0 as i64})
        }
        BindingSlot::NonRaster => json!({"k": "p"}),
    }
}

fn layer_json(layer: &BindingLayer) -> Value {
    let frames: Vec<Vec<Value>> = layer
        .frames
        .iter()
        .map(|frame| frame.iter().map(slot_json).collect())
        .collect();
    json!({
        "nv": i64::from(layer.native_video),
        "vm": layer.video_mesh_id,
        "fr": frames,
    })
}

/// The oracle's snapshot shape: store key order, store content, every set of layers.
pub(crate) fn snapshot(state: &MeshBindingState) -> Value {
    let ids: Vec<&str> = state
        .store
        .entries()
        .iter()
        .map(|entry| entry.0.as_str())
        .collect();
    let sets: Vec<Vec<Value>> = state
        .sets
        .iter()
        .map(|set| set.iter().map(layer_json).collect())
        .collect();
    json!({
        "ids": ids,
        "store": serde_json::to_value(&state.store).expect("store value"),
        "sets": sets,
    })
}

fn index(op: &Value, key: &str) -> usize {
    op[key].as_u64().expect(key) as usize
}

pub(crate) fn target_of(op: &Value) -> SlotTarget {
    SlotTarget {
        set: 0,
        layer: index(op, "layer"),
        frame: index(op, "frame"),
        slot: index(op, "slot"),
    }
}

pub(crate) fn mesh_of(op: &Value) -> ImageMesh {
    serde_json::from_value(op["mesh"].clone()).expect("op mesh")
}

fn mesh_arg(op: &Value) -> &str {
    op["mesh"].as_str().expect("mesh id")
}

/// Runs one oracle op through production and returns its result in the oracle's shape.
pub(crate) fn run_op(state: &mut MeshBindingState, op: &Value) -> Value {
    match op["op"].as_str().expect("op name") {
        "propagate" => {
            let done = state
                .propagate(target_of(op), op["mesh"].as_str())
                .expect("propagate");
            json!({"n": done.written})
        }
        "attach" => {
            let done = state.attach(target_of(op), mesh_of(op)).expect("attach");
            json!({"id": done.mesh_id})
        }
        "detach" => {
            let done = state.detach(target_of(op)).expect("detach");
            json!({"id": done.mesh_id, "released": done.released, "cleared": done.propagate.written})
        }
        "release" => json!(state.release_if_unused(mesh_arg(op)).expect("release")),
        "isReferenced" => json!(state.is_referenced(mesh_arg(op))),
        "duplicate" => json!(state.duplicate_mesh(mesh_arg(op)).expect("duplicate")),
        "attachVideo" => json!(state
            .attach_video(VideoFamily::Provided, 0, index(op, "layer"), mesh_of(op))
            .expect("attachVideo")),
        "detachVideo" => json!(state
            .detach_video(VideoFamily::Provided, 0, index(op, "layer"))
            .expect("detachVideo")),
        "remap" => {
            let done = state
                .remap_duplicated_layer(0, index(op, "layer"))
                .expect("remap");
            json!({"pairs": done.pairs, "dangling": done.dangling})
        }
        other => panic!("unknown oracle op {other}"),
    }
}
