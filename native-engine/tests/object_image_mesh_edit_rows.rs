//! Shared harness of the image-mesh edit transaction tests: documents, owners, payloads and dispatch
//! helpers over the REAL `NativeObjectHistory` (no mock owner, no stand-in codec).
use super::object_image_mesh_edit_oracle as oracle;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ImageMesh;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

/// The mesh id every oracle start mesh is stored under.
pub(crate) const ID: &str = "m";

/// The fixture corpus mesh (`tests/fixtures/lib/corpus.cjs` `mesh()`), outline 8, verts 9, a fan of 8
/// triangles, `offsets[8]` zero here: the pose the editor starts from before the vertex drag.
pub(crate) const CORPUS_ZERO: &str = r#"{"im_1":{"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],"verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],"tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2}}"#;
/// The same mesh with the corpus drag already applied (`offsets[8] = [0.1, 0]`).
pub(crate) const CORPUS_DRAGGED: &str = r#"{"im_1":{"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],"verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],"tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0.1,0]],"cols":2,"rows":2}}"#;

/// A document with one layer and no objects: the mesh store is the only thing an edit can touch.
/// Version 2 adds the order plan with a single context.
pub(crate) fn doc(version: u32, meshes: Value) -> Value {
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,"layers":[{"layerUid":"L0"}],"objects":[]});
    if version == 2 {
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[]}]});
    }
    if !meshes.is_null() {
        value["imageMeshes"] = meshes;
    }
    value
}

pub(crate) fn owner_of(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "mesh-edit",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

/// A v1 owner holding one oracle start mesh under [`ID`].
pub(crate) fn owner_with(mesh: &oracle::Mesh) -> NativeObjectHistory {
    owner_of(&doc(1, store_of(&[(ID, mesh)])))
}

/// An `imageMeshes` table with the entries in the given authored order.
pub(crate) fn store_of(entries: &[(&str, &oracle::Mesh)]) -> Value {
    let mut table = serde_json::Map::new();
    for (id, mesh) in entries {
        table.insert((*id).to_owned(), mesh.entry());
    }
    Value::Object(table)
}

/// A v1 document as text, so the authored order of the mesh table is exactly what the caller wrote.
pub(crate) fn doc_text(meshes_json: &str) -> String {
    format!(
        r#"{{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,"layers":[{{"layerUid":"L0"}}],"objects":[],"imageMeshes":{meshes_json}}}"#
    )
}

pub(crate) fn owner_of_text(text: &str) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "mesh-edit",
        decode_storage_project(text.as_bytes()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}

pub(crate) fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(current(owner).document()).unwrap()
}

pub(crate) fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}

/// The stored record `id` of the owner's current revision.
pub(crate) fn stored(owner: &NativeObjectHistory, id: &str) -> ImageMesh {
    current(owner)
        .document()
        .image_meshes()
        .unwrap()
        .get(id)
        .unwrap()
        .clone()
}

/// A request number spelled like `JSON.stringify` of the same JavaScript value (`-0` stays `-0.0`).
pub(crate) fn jnum(value: f64) -> Value {
    if value.to_bits() == (-0.0f64).to_bits() {
        return json!(-0.0);
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

pub(crate) fn payload(id: &str, action: Value) -> Value {
    json!({"command":"object.image.mesh.edit","meshId":id,"action":action})
}

pub(crate) fn set_offset(index: i64, du: f64, dv: f64) -> Value {
    json!({"kind":"setOffset","index":index,"du":jnum(du),"dv":jnum(dv)})
}

pub(crate) fn reset_offsets() -> Value {
    json!({"kind":"resetOffsets"})
}

pub(crate) fn set_outline(poly: &[[f64; 2]], cols: Option<u32>, rows: Option<u32>) -> Value {
    let points: Vec<Value> = poly
        .iter()
        .map(|p| json!([jnum(p[0]), jnum(p[1])]))
        .collect();
    let mut action = json!({"kind":"setOutline","outline":points});
    if let Some(cols) = cols {
        action["cols"] = json!(cols);
    }
    if let Some(rows) = rows {
        action["rows"] = json!(rows);
    }
    action
}

pub(crate) fn set_density(cols: u32, rows: u32) -> Value {
    json!({"kind":"setDensity","cols":cols,"rows":rows})
}

pub(crate) fn request(owner: &NativeObjectHistory, id: &str, body: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.image.mesh.edit","payload":body})
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

/// Build and dispatch one mesh edit at the owner's current revision.
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

/// The wire document without its mesh store: what an edit must never touch.
pub(crate) fn without_meshes(owner: &NativeObjectHistory) -> Value {
    let mut value = wire(owner);
    value.as_object_mut().unwrap().remove("imageMeshes");
    value
}

/// The effective offsets of a stored record as f64 pairs.
pub(crate) fn offsets_of(mesh: &ImageMesh) -> Vec<[f64; 2]> {
    mesh.effective_offsets()
}

/// A stored record as the plain f64 mesh the oracle rows speak.
pub(crate) fn plain_of(mesh: &ImageMesh) -> oracle::Mesh {
    oracle::Mesh {
        outline: mesh.outline_points(),
        verts: mesh.rest_points(),
        tris: mesh.triangles().to_vec(),
        offsets: mesh.effective_offsets(),
        cols: mesh.effective_cols(),
        rows: mesh.effective_rows(),
    }
}

/// The owner's stored record `id` as a plain mesh.
pub(crate) fn plain(owner: &NativeObjectHistory, id: &str) -> oracle::Mesh {
    plain_of(&stored(owner, id))
}

/// The invariants every mesh must keep: the outline is the vertex prefix, offsets align with the
/// vertices one to one and every triangle index is a vertex.
pub(crate) fn assert_invariants(mesh: &oracle::Mesh, what: &str) {
    assert!(mesh.outline.len() <= mesh.verts.len(), "{what}");
    assert!(
        oracle::same_pts(&mesh.verts[..mesh.outline.len()], &mesh.outline),
        "{what}: outline prefix"
    );
    assert_eq!(
        mesh.offsets.len(),
        mesh.verts.len(),
        "{what}: offsets align with verts"
    );
    assert_eq!(mesh.tris.len() % 3, 0, "{what}");
    assert!(
        mesh.tris.iter().all(|i| (*i as usize) < mesh.verts.len()),
        "{what}: triangle indices"
    );
}

/// A real offset edit on `g4` (the accepted twin of the offset flaws).
pub(crate) fn twin_offset() -> Value {
    payload(ID, set_offset(8, 0.25, -0.5))
}

pub(crate) fn square() -> Vec<[f64; 2]> {
    vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]]
}

pub(crate) fn twin_outline() -> Value {
    payload(ID, set_outline(&square(), Some(4), Some(4)))
}

pub(crate) fn twin_density() -> Value {
    payload(ID, set_density(6, 3))
}

pub(crate) fn action_with(body: Value, change: impl FnOnce(&mut Value)) -> Value {
    with(body, |p| change(&mut p["action"]))
}
