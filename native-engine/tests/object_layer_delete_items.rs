//! Evidence defect 3 (#2112): deleting a layer removes EVERY record that lives on it, whatever its kind: the raster
//! placements and text blocks of `items`, with one history entry; an undo restores the layer and all its records, and the
//! assets the records named stay in the project (a deleted placement may be restored, an asset in use cannot be removed).
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;
use support::{bytes, drawing, head, items_of, owner, raster_item, state, target};

fn delete_payload(layer: &str) -> Value {
    json!({"command":"object.layer.delete","selection":{"active":layer,"selected":[layer],"anchor":null}})
}

fn delete(o: &mut NativeObjectHistory, layer: &str) -> Result<u64, DispatchErrorCode> {
    let candidate = o.prepare_layer_delete_json(delete_payload(layer).to_string().as_bytes())?;
    o.commit_layer_lifecycle(candidate)
}

fn layer_uids(o: &NativeObjectHistory) -> Vec<String> {
    serde_json::to_value(head(o).document()).unwrap()["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_owned())
        .collect()
}

/// Layers L0, L1 (locked in `drawing`, so the raster sits on L2), L2; a raster placement of `img-1` on L2.
fn with_raster() -> NativeObjectHistory {
    owner(&drawing(json!({
        "items":{"raster":[{"target":target("L2","r1"),"item":raster_item("img-1", 10, 10, 20, 20)}]}
    })))
}

#[test]
fn deleting_the_layer_that_holds_a_raster_placement_removes_it_with_one_history_entry() {
    let mut o = with_raster();
    let before = state(&o);
    assert_eq!(items_of(&o)["raster"].as_array().unwrap().len(), 1);
    let revision = delete(&mut o, "L2").expect("the layer deletes");
    assert_eq!(revision, before.0 + 1, "one revision");
    assert_eq!(o.history_depths(), (before.1 .0 + 1, 0), "one history entry");
    assert_eq!(layer_uids(&o), ["L0", "L1"]);
    assert!(items_of(&o).is_null(), "the placement left with its layer: {:?}", items_of(&o));
}

#[test]
fn undo_restores_the_layer_and_its_placement_byte_for_byte_and_redo_deletes_again() {
    let mut o = with_raster();
    let original = bytes(&o);
    delete(&mut o, "L2").unwrap();
    let deleted = bytes(&o);
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original, "the layer and its placement are back");
    let revision = o.content_revision();
    o.redo(revision).expect("redo");
    assert_eq!(bytes(&o), deleted);
}

#[test]
fn a_sibling_layer_keeps_its_own_placement_and_the_asset_survives_the_delete() {
    let mut o = owner(&drawing(json!({"items":{"raster":[
        {"target":target("L2","r1"),"item":raster_item("img-1", 10, 10, 20, 20)},
        {"target":target("L0","r0"),"item":raster_item("img-1", 0, 0, 5, 5)}]}})));
    delete(&mut o, "L2").unwrap();
    let raster = items_of(&o)["raster"].clone();
    assert_eq!(raster.as_array().unwrap().len(), 1);
    assert_eq!(raster[0]["target"]["layerUid"], "L0");
    // The asset is still named by the surviving placement, and still in the project for the undo.
    let snapshot = head(&o);
    let inventory = snapshot.project().and_then(|p| p.asset_inventory()).expect("project inventory");
    assert_eq!(inventory.records().len(), 4, "no asset is dropped by a layer delete");
}

#[test]
fn deleting_the_layer_that_holds_a_text_block_removes_it_and_the_undo_restores_it() {
    let oracle = include_str!("fixtures/object-text-layout-commands/oracle.txt");
    let record = support::rows(oracle, Some("EDIT"))[0]["before"].clone();
    let mut o = owner(&drawing(json!({"items":{"text":[{"target":target("L0","t0"),"block":record}]}})));
    let original = bytes(&o);
    delete(&mut o, "L0").expect("the layer deletes");
    assert_eq!(o.history_depths(), (1, 0));
    assert!(items_of(&o).is_null(), "the text block left with its layer: {:?}", items_of(&o));
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}

#[test]
fn a_v2_document_loses_the_placement_and_its_order_element_with_the_layer() {
    use crate::history::object_history::object_layer_lifecycle_tests::base;
    let mut doc = base(2);
    doc["items"] = json!({"raster":[{"target":target("L2","r1"),"item":raster_item("img-1", 10, 10, 20, 20)}]});
    let scopes = &mut doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    scopes.as_array_mut().unwrap().push(json!({"layerUid":"L2","strokeId":"r1","naturalPosition":1,"order":{}}));
    let mut o = owner(&doc);
    let original = bytes(&o);
    delete(&mut o, "L2").expect("the layer deletes");
    assert_eq!(o.history_depths(), (1, 0));
    let after: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    assert!(after["items"].is_null());
    let elements = after["compositionOrder"]["contexts"][0]["scopes"][0]["elements"].as_array().unwrap();
    assert!(elements.iter().all(|e| e["layerUid"] != "L2"), "no order element of the deleted layer: {elements:?}");
    let revision = o.content_revision();
    o.undo(revision).expect("undo");
    assert_eq!(bytes(&o), original);
}
