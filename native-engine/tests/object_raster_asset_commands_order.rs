//! v2 documents: an item gains and loses exactly one scoped order element, the plan keeps covering every authored
//! record (storage admission refuses a plan that does not), and undo / save / reopen restore plan and items together.
use super::super::support::{
    bytes, drawing, head, inventory, items_of, project, raster_item, run, state, target,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode as E;
use serde_json::{json, Value};

fn plan(contexts: Value) -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":contexts})
}

fn v2(contexts: Value) -> Value {
    let mut doc = drawing(json!({"compositionOrder":plan(contexts)}));
    doc["formatVersion"] = json!(2);
    doc["layers"] = json!([{"layerUid":"L0"},{"layerUid":"L1"}]);
    doc
}

fn empty_plan() -> Value {
    json!([{"contextId":"scene-root","layers":[
        {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L1","naturalPosition":1,"order":{}}],
        "scopes":[]}])
}

fn owner_v2(contexts: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project("ordered", project(&v2(contexts), inventory()))
        .unwrap()
}

fn scopes(o: &NativeObjectHistory) -> Value {
    serde_json::to_value(head(o).document()).unwrap()["compositionOrder"]["contexts"][0]["scopes"]
        .clone()
}

fn element(layer: &str, stroke: &str, position: u32) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

fn create_raster(layer: &str) -> Value {
    json!({"command":"object.raster.create","target":target(layer,""),"item":raster_item("img-1", 1, 2, 3, 4)})
}

fn create_text(layer: &str) -> Value {
    let record = json!({"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}});
    json!({"command":"object.text.create","target":target(layer,""),"block":record})
}

#[test]
fn creating_and_deleting_items_keeps_the_order_plan_covering_every_authored_record() {
    let mut o = owner_v2(empty_plan());
    let start = bytes(&o);
    run(&mut o, &create_raster("L0")).unwrap();
    run(&mut o, &create_text("L0")).unwrap();
    run(&mut o, &create_raster("L1")).unwrap();
    assert_eq!(
        scopes(&o),
        json!([{"frameScope":{"kind":"authored","frame":0},"elements":[
            element("L0", "native-raster:1:0", 0),
            element("L0", "native-text:2:0", 1),
            element("L1", "native-raster:3:0", 0)]}])
    );
    let full = bytes(&o);
    assert_eq!(
        decode_storage_project(&full).unwrap(),
        *head(&o).document(),
        "the ordered file round-trips"
    );
    run(
        &mut o,
        &json!({"command":"object.raster.delete","stableTarget":target("L0","native-raster:1:0")}),
    )
    .unwrap();
    assert_eq!(
        scopes(&o),
        json!([{"frameScope":{"kind":"authored","frame":0},"elements":[
            element("L0", "native-text:2:0", 0),
            element("L1", "native-raster:3:0", 0)]}]),
        "the later element of the layer closes the gap"
    );
    assert_eq!(items_of(&o)["raster"].as_array().unwrap().len(), 1);
    assert_eq!(o.undo(4), Ok(5));
    assert_eq!(bytes(&o), full, "undo restores plan and items together");
    for _ in 0..3 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(bytes(&o), start);
}

#[test]
fn a_plan_that_does_not_declare_the_context_refuses_the_creation_and_publishes_nothing() {
    let mut o = owner_v2(json!([]));
    let before = state(&o);
    assert_eq!(run(&mut o, &create_raster("L0")), Err(E::NotFound));
    assert_eq!(run(&mut o, &create_text("L0")), Err(E::NotFound));
    assert_eq!(state(&o), before);
}

#[test]
fn storage_admission_refuses_a_plan_that_does_not_cover_the_items() {
    let mut o = owner_v2(empty_plan());
    run(&mut o, &create_raster("L0")).unwrap();
    let good: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    assert!(decode_storage_project(good.to_string().as_bytes()).is_ok());
    let mut uncovered = good.clone();
    uncovered["compositionOrder"]["contexts"][0]["scopes"] = json!([]);
    assert!(
        decode_storage_project(uncovered.to_string().as_bytes()).is_err(),
        "an item without its order element"
    );
    let mut phantom = good.clone();
    phantom["items"]["raster"][0]["target"]["strokeId"] = json!("other");
    assert!(
        decode_storage_project(phantom.to_string().as_bytes()).is_err(),
        "an order element without its item"
    );
    let document = decode_storage_project(good.to_string().as_bytes()).unwrap();
    assert_eq!(encode_storage_project(&document).unwrap(), bytes(&o));
}
