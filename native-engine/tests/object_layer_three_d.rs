//! #2520/#2525: the baseline `toggleLayer3D` switch as a native layer-policy flag. One toggle is one revision and one history
//! entry, the row reports it, a 3D layer lists Position Z / Rotation X / Rotation Y in the baseline places, the gizmo bounds
//! read answers it, and its three extra properties edit like any other (stored whether or not the switch is on).
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn corner(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
fn rect(layer: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":format!("s{layer}")},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x, y), corner(x + w, y), corner(x + w, y + h), corner(x, y + h)]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn fixture() -> Value {
    let layer = |uid: &str| json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875",
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    let element = |uid: &str| json!({"layerUid":uid,"strokeId":format!("s{uid}"),"naturalPosition":0,"order":{}});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[layer("A"), layer("B")],"objects":[rect("A", 20., 30., 100., 60.), rect("B", 0., 0., 10., 10.)],
        "propertyHolders":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"A","naturalPosition":0,"order":{}},{"layerUid":"B","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[element("A"), element("B")]}]}]}})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new("three-d", decode_storage_project(fixture().to_string().as_bytes()).unwrap()).unwrap()
}
fn head(o: &NativeObjectHistory) -> crate::revision::ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(o)).unwrap()
}
fn set(o: &mut NativeObjectHistory, layer: &str, value: Value) -> Result<u64, crate::request_receipts::DispatchErrorCode> {
    let payload = json!({"command":"object.layer.policy.edit","layerUid":layer,"flag":"threeD","value":value});
    let candidate = o.prepare_layer_policy_json(payload.to_string().as_bytes())?;
    o.commit_layer_policy(candidate)
}
fn edit(o: &mut NativeObjectHistory, layer: &str, property: &str, values: Value) -> u64 {
    let payload = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
        "property":property,"action":{"kind":"setValue","frame":0,"values":values}});
    let candidate = o.owner.prepare_property_edit_json(payload.to_string().as_bytes()).unwrap();
    o.commit_property_edit(candidate).unwrap()
}

#[test]
fn the_switch_is_one_revision_and_one_history_entry_each_way_and_a_no_op_publishes_nothing() {
    let mut o = owner();
    let zero = bytes(&o);
    // Off and absent read the same: switching an absent flag off authors nothing and adds no history.
    assert_eq!(set(&mut o, "A", json!(false)), Ok(0));
    assert_eq!((o.history_depths(), bytes(&o)), ((0, 0), zero.clone()));
    assert_eq!(set(&mut o, "A", json!(true)), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(true));
    assert!(wire(&o)["layers"][1].get("threeD").is_none(), "only the toggled layer carries the flag");
    assert_eq!(set(&mut o, "A", json!(true)), Ok(1), "the same value again is a no-op");
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(set(&mut o, "A", json!(false)), Ok(2));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(false));
    assert_eq!(o.history_depths(), (2, 0));
    // Undo restores the exact bytes of the previous revision, redo the exact switch.
    assert_eq!(o.undo(2), Ok(3));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(true));
    assert_eq!(o.undo(3), Ok(4));
    assert_eq!(bytes(&o), zero);
    assert_eq!(o.redo(4), Ok(5));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(true));
}
#[test]
fn the_switch_refuses_a_non_boolean_value_and_an_unknown_layer_without_publishing() {
    use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound};
    let mut o = owner();
    let zero = bytes(&o);
    for value in [json!(null), json!("in"), json!(1), json!([true])] {
        assert_eq!(set(&mut o, "A", value.clone()), Err(InvalidRequest), "{value}");
    }
    assert_eq!(set(&mut o, "missing", json!(true)), Err(NotFound));
    assert_eq!((o.content_revision(), o.history_depths(), bytes(&o)), (0, (0, 0), zero));
}
fn properties(o: &NativeObjectHistory, layer: &str, offset: u64) -> Value {
    let pin = head(o);
    let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"properties","target":{"contextId":"scene-root","layerUid":layer},"offset":offset,"limit":8}});
    pin.query_property_metadata_json(selector.to_string().as_bytes()).unwrap()
}
fn names(o: &NativeObjectHistory, layer: &str) -> Vec<String> {
    // The 3072-byte page bound cuts a page early: follow the cursor exactly as returned.
    let (mut out, mut offset) = (vec![], 0);
    loop {
        let page = properties(o, layer, offset);
        out.extend(page["items"].as_array().unwrap().iter().map(|i| i["property"].as_str().unwrap().to_owned()));
        match page["nextOffset"].as_u64() {
            Some(next) => offset = next,
            None => return out,
        }
    }
}
#[test]
fn a_3d_layer_lists_position_z_and_the_two_rotations_in_the_baseline_places() {
    let mut o = owner();
    let flat = ["position", "anchor", "rotation", "scale", "opacity", "order"];
    assert_eq!(names(&o, "A"), flat);
    set(&mut o, "A", json!(true)).unwrap();
    assert_eq!(names(&o, "A"), ["position", "positionZ", "anchor", "rotation", "rotationX", "rotationY", "scale", "opacity", "order"]);
    let first = properties(&o, "A", 0);
    let z = first["items"].as_array().unwrap().iter().find(|i| i["property"] == "positionZ").unwrap();
    assert_eq!((z["dimension"].clone(), z["unit"].clone(), z["defaultValue"].clone()), (json!(1), json!("px"), json!([0])));
    let x = first["items"].as_array().unwrap().iter().find(|i| i["property"] == "rotationX").unwrap();
    assert_eq!(x["unit"], json!("°"));
    // The neighbour layer keeps the 2D list, and switching off restores it.
    assert_eq!(names(&o, "B"), flat);
    set(&mut o, "A", json!(false)).unwrap();
    assert_eq!(names(&o, "A"), flat);
}
#[test]
fn the_three_extra_properties_edit_and_read_back_whether_or_not_the_switch_is_on() {
    let mut o = owner();
    assert_eq!(edit(&mut o, "A", "positionZ", json!([-250])), 1);
    assert_eq!(edit(&mut o, "A", "rotationX", json!([30])), 2);
    assert_eq!(edit(&mut o, "A", "rotationY", json!([-15.5])), 3);
    let holders = wire(&o)["propertyHolders"].clone();
    let value = |property: &str| holders[0]["properties"].as_array().unwrap().iter()
        .find(|p| p["property"] == property).unwrap()["static"].clone();
    assert_eq!((value("positionZ"), value("rotationX"), value("rotationY")), (json!([-250]), json!([30]), json!([-15.5])));
    assert!(wire(&o)["layers"][0].get("threeD").is_none(), "an edit never switches 3D on");
    let pin = head(&o);
    let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"rawSample","target":{"contextId":"scene-root","layerUid":"A"},
            "property":"rotationX","frame":0}});
    let read = pin.query_property_read_json(selector.to_string().as_bytes()).unwrap();
    assert_eq!(read["value"][0].as_f64(), Some(30.0));
}
#[cfg(feature = "evaluation")]
#[test]
fn the_gizmo_bounds_read_answers_a_3d_layer_and_still_refuses_nothing_it_answered_in_2d() {
    let mut o = owner();
    let bounds = |o: &NativeObjectHistory| {
        let pin = head(o);
        let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
            "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"layerBounds","target":{"contextId":"scene-root","layerUid":"A"},"frame":0}});
        pin.query_property_read_json(selector.to_string().as_bytes()).map(|r| (r["bounds"].clone(), r["parentAffine"].clone()))
    };
    let flat = bounds(&o).unwrap();
    assert_eq!(flat.0, json!([20.0, 30.0, 120.0, 90.0]));
    set(&mut o, "A", json!(true)).unwrap();
    assert_eq!(bounds(&o).unwrap(), flat, "the 3D gizmo pivots about the same rest bounds the 2D box did");
}
