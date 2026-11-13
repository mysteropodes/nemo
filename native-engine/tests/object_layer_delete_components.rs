//! Evidence defect 3 (#2112): deleting a component instance layer (and the scene layer next to it) keeps the document
//! admissible: the instance leaves with its layer, the component table the instance placed stays for the undo, one history
//! entry, and the undo restores everything byte for byte.
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

fn base() -> Value {
    let layers = [layer("ly_inst", "Inst", &[]), layer("ly_plain", "Plain", &[(0, "x1")]), layer("ly_dup", "Dup", &[(0, "d1")])];
    let instance = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"once",
        "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0});
    let mut inner = record("xi", "s0", 0);
    inner["target"]["contextId"] = json!("sym_x");
    let component = json!({"symbolId":"sym_x","totalFrames":24,
        "layers":[{"layerUid":"xi","name":"Layer 1","visible":true,"locked":false,"timing":timing(&[0])}],
        "objects":[inner]});
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[component],"instances":[instance]});
    let duplicators = fixture::duplicator_table(&[("ly_dup", json!({"mode":"path","pathLayerUid":"ly_plain"}))]);
    let tweens = json!({"layers":[{"layerUid":"ly_plain","options":{"resamplePoints":50,"step":1,"skipManual":true,"easing":[]},"spans":[]},
        {"layerUid":"ly_dup","options":{"resamplePoints":50,"step":1,"skipManual":true,"easing":[]},"spans":[]}]});
    document(&layers, json!({"componentTables":{"components":components,"duplicators":duplicators},"tweens":tweens}))
}

fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new("delete-components", decode_storage_project(base().to_string().as_bytes()).unwrap()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
fn delete(o: &mut NativeObjectHistory, layer: &str) -> Result<u64, crate::request_receipts::DispatchErrorCode> {
    let payload = json!({"command":"object.layer.delete","selection":{"active":layer,"selected":[layer],"anchor":null}});
    let candidate = o.prepare_layer_delete_json(payload.to_string().as_bytes())?;
    o.commit_layer_lifecycle(candidate)
}

#[test]
fn deleting_a_component_instance_layer_drops_its_instance_with_one_history_entry_and_the_undo_restores_it() {
    let mut o = owner();
    let original = bytes(&o);
    delete(&mut o, "ly_inst").expect("the instance layer deletes");
    assert_eq!(o.history_depths(), (1, 0));
    let after: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    assert_eq!(after["layers"].as_array().unwrap().len(), 2);
    let instances = &after["componentTables"]["components"]["instances"];
    assert!(instances.is_null() || instances.as_array().unwrap().is_empty(), "no instance on a deleted layer: {instances}");
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}

#[test]
fn deleting_the_path_layer_of_a_duplicator_clears_the_reference_and_its_tween_spans_go() {
    let mut o = owner();
    let original = bytes(&o);
    delete(&mut o, "ly_plain").expect("the path layer deletes");
    let after: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    let duplicator = &after["componentTables"]["duplicators"]["duplicators"][0]["duplicator"];
    assert!(duplicator["pathLayerUid"].is_null(), "no reference to a deleted layer: {duplicator}");
    let tweened: Vec<&str> = after["tweens"]["layers"].as_array().unwrap().iter().map(|l| l["layerUid"].as_str().unwrap()).collect();
    assert_eq!(tweened, ["ly_dup"], "the deleted layer's tweens left with it");
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}

#[test]
fn deleting_the_layer_that_hosts_a_duplicator_removes_the_host_record() {
    let mut o = owner();
    let original = bytes(&o);
    delete(&mut o, "ly_dup").expect("the host layer deletes");
    assert_eq!(o.history_depths(), (1, 0));
    let after: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    assert!(after["componentTables"]["duplicators"]["duplicators"].as_array().unwrap().is_empty());
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}
