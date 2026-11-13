//! Evidence defect 3 follow-up (#2112): deleting a layer INSIDE a declared component takes every row that lives on it, as
//! the scene delete does. The component view carries none of the placement, duplicator and effector tables, so the embed
//! must release the removed layer's rows itself; the stale ones used to fail the strict admission (`invalid_request`).
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

#[path = "object_component_convert_fixture.rs"]
mod fixture;
use fixture::{document, layer, record};

fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}

fn inner_layer(uid: &str) -> Value {
    json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"timing":timing(&[0])})
}

fn inner_record(layer: &str, stroke: &str) -> Value {
    let mut inner = record(layer, stroke, 0);
    inner["target"]["contextId"] = json!("sym_x");
    inner
}

/// Component `sym_x` with layers `xi`, `xj`, `xk`: `xj` hosts a duplicator, `xk` is a
/// path-mode duplicator whose path layer is `xj`.
fn base() -> Value {
    let layers = [layer("ly_inst", "Inst", &[])];
    let instances = json!([
        {"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"once","symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}]);
    let sym_x = json!({"symbolId":"sym_x","totalFrames":24,
        "layers":[inner_layer("xi"),inner_layer("xj"),inner_layer("xk")],
        "objects":[inner_record("xi","s0"),inner_record("xj","s1"),inner_record("xk","s2")]});
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[sym_x],"instances":instances});
    let duplicators = json!({"format":"nemo.native-duplicator-set","formatVersion":1,
        "duplicators":[
            {"contextId":"sym_x","layerUid":"xj","duplicator":{"mode":"grid","rows":2,"cols":3}},
            {"contextId":"sym_x","layerUid":"xk","duplicator":{"mode":"path","pathLayerUid":"xj"}}],
        "effectorLayers":[]});
    document(&layers, json!({"componentTables":{"components":components,"duplicators":duplicators}}))
}

fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new("delete-context", decode_storage_project(base().to_string().as_bytes()).unwrap()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}

#[test]
fn deleting_a_component_layer_takes_its_duplicator_row_and_clears_references_to_it() {
    let mut o = owner();
    let original = bytes(&o);
    let payload = json!({"command":"object.layer.delete","contextId":"sym_x","selection":{"active":"xj","selected":["xj"],"anchor":null}});
    let candidate = o.prepare_layer_delete_json(payload.to_string().as_bytes()).expect("a component layer hosting rows deletes");
    o.commit_layer_lifecycle(candidate).expect("commits");
    assert_eq!(o.history_depths(), (1, 0));
    let after: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    let hosts = after["componentTables"]["duplicators"]["duplicators"].as_array().unwrap();
    assert_eq!(hosts.len(), 1, "the host row of the deleted layer is gone: {hosts:?}");
    assert_eq!(hosts[0]["layerUid"], "xk");
    assert!(hosts[0]["duplicator"]["pathLayerUid"].is_null(), "no reference to a deleted layer: {}", hosts[0]);
    let instances = after["componentTables"]["components"]["instances"].as_array().unwrap();
    assert_eq!(instances.len(), 1, "the scene instance stays");
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}
