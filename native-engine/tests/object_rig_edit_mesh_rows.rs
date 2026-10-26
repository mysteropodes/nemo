//! Harness of the mesh half of the rig edit transaction tests (L03-O, #2251): the executed-baseline rows
//! of `fixtures/object-rig-edit-mesh/oracle.txt` (produced by `gen.cjs` from the verbatim baseline
//! `rigBindMesh` / `rigAutoAssignLayer`), and documents carrying rasters, an image-mesh store and a rig.
//! One tab-separated row per case: `mesh  name  scenario  action  ret  before  after`; `scenario` is
//! `{children: [raster | path], meshes}` exactly as the generator ran it.
use super::object_rig_edit_rows::*;
use super::*;
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-rig-edit-mesh/oracle.txt");

pub(crate) struct MeshRow {
    pub(crate) name: &'static str,
    pub(crate) scenario: Value,
    pub(crate) action: Value,
    pub(crate) ret: Value,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
}

impl MeshRow {
    pub(crate) fn changes(&self) -> bool {
        self.before != self.after
    }
}

/// The baseline calls take the current frame implicitly; the native `autoAssign` names it.
fn native_action(mut action: Value) -> Value {
    if action["kind"] == "autoAssign" && action.get("frame").is_none() {
        action["frame"] = json!(0);
    }
    action
}

pub(crate) fn mesh_rows() -> Vec<MeshRow> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .map(|f| {
            assert_eq!(f.len(), 7, "mesh oracle row shape: {}", f[1]);
            assert_eq!(f[0], "mesh");
            MeshRow {
                name: f[1],
                scenario: serde_json::from_str(f[2]).expect("scenario json"),
                action: native_action(serde_json::from_str(f[3]).expect("action json")),
                ret: serde_json::from_str(f[4]).expect("ret json"),
                before: f[5],
                after: f[6],
            }
        })
        .collect()
}

pub(crate) fn mesh_row(name: &str) -> MeshRow {
    mesh_rows()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("mesh oracle has no row {name}"))
}

/// The raster item of a generator rect `{x, y, w, h, rot}` (`x`, `y` is the top-left; the record stores the
/// centre, the display size and, when it is not zero, the rotation).
pub(crate) fn raster_item(mesh_id: Option<&str>, rect: &Value) -> Value {
    let (x, y) = (rect["x"].as_f64().unwrap(), rect["y"].as_f64().unwrap());
    let (w, h) = (rect["w"].as_f64().unwrap(), rect["h"].as_f64().unwrap());
    let mut item = json!({"assetId":"image:source","x":x + w / 2.0,"y":y + h / 2.0,"width":w,"height":h});
    if let Some(rot) = rect["rot"].as_f64().filter(|rot| *rot != 0.0) {
        item["rotation"] = json!(rot);
    }
    if let Some(id) = mesh_id {
        item["meshId"] = json!(id);
    }
    item
}

/// One raster entry on `layer` at an authored `frame`.
pub(crate) fn raster_entry(layer: &str, stroke_id: &str, frame: u32, item: Value) -> Value {
    json!({"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
        "layerUid":layer,"strokeId":stroke_id},"item":item})
}

/// A path child of a scenario as the stroke the stroke records are built from.
fn stroke_of(child: &Value) -> Value {
    let pts = child["pts"].clone();
    let hs: Vec<Value> = pts
        .as_array()
        .unwrap()
        .iter()
        .map(|_| json!([null, null]))
        .collect();
    json!({"id":child["id"],"closed":child["closed"],"pts":pts,"hs":hs})
}

/// A v1 document text: layer `L0` carrying `rig` (raw JSON text), a bare layer `L1`, the stroke records,
/// the raster entries and the image-mesh store.
pub(crate) fn mesh_doc_text(rig: &str, records: &[Value], rasters: &[Value], meshes: &Value) -> String {
    let head = format!("{{\"layerUid\":\"{LAYER}\",\"rig\":{rig}}}");
    let mut text = format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":1,\"totalFrames\":3,\"layers\":[{head},{{\"layerUid\":\"L1\"}}],\"objects\":{}",
        Value::Array(records.to_vec())
    );
    if !rasters.is_empty() {
        text.push_str(&format!(",\"items\":{{\"raster\":{}}}", Value::Array(rasters.to_vec())));
    }
    if meshes.as_object().is_some_and(|m| !m.is_empty()) {
        text.push_str(&format!(",\"imageMeshes\":{meshes}"));
    }
    text.push('}');
    text
}

/// The document text holding the row's rig, its path strokes, its rasters (on `L0`, frame 0) and meshes.
pub(crate) fn text_for_mesh_row(row: &MeshRow) -> String {
    let mut records = Vec::new();
    let mut rasters = Vec::new();
    for (index, child) in row.scenario["children"].as_array().unwrap().iter().enumerate() {
        if child["t"] == "raster" {
            let id = format!("raster:{index}");
            rasters.push(raster_entry(LAYER, &id, 0, raster_item(child["meshId"].as_str(), &child["rect"])));
        } else {
            records.push(record(&stroke_of(child)));
        }
    }
    mesh_doc_text(row.before, &records, &rasters, &row.scenario["meshes"])
}

pub(crate) fn owner_for_mesh_row(row: &MeshRow) -> NativeObjectHistory {
    owner_of_text(&text_for_mesh_row(row))
}

/// Two JSON values equal, every number within `1e-9` (the baseline `Math.cos` and the native `cos` of a
/// rotated placement may differ in the last bits; structure, ids and counts are compared exactly).
pub(crate) fn json_close(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| json_close(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && json_close(va, vb))
        }
        _ => left == right,
    }
}

/// The parts of the document a rig edit must never touch: stroke geometry, raster items and the mesh store.
pub(crate) fn rest_geometry(owner: &NativeObjectHistory) -> Value {
    let wire = wire(owner);
    json!({"objects": wire["objects"], "items": wire["items"], "imageMeshes": wire["imageMeshes"]})
}
