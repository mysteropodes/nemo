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
fn the_switch_is_one_saved_revision_each_way_with_no_history_entry_and_a_no_op_publishes_nothing() {
    let mut o = owner();
    let zero = bytes(&o);
    // Off and absent read the same: switching an absent flag off authors nothing.
    assert_eq!(set(&mut o, "A", json!(false)), Ok(0));
    assert_eq!((o.history_depths(), bytes(&o)), ((0, 0), zero.clone()));
    // #2520: the baseline `toggleLayer3D` pushed no undo snapshot, so the switch is saved but is no step of its own.
    assert_eq!(set(&mut o, "A", json!(true)), Ok(1));
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(true));
    assert!(wire(&o)["layers"][1].get("threeD").is_none(), "only the toggled layer carries the flag");
    assert_eq!(set(&mut o, "A", json!(true)), Ok(1), "the same value again is a no-op");
    assert_eq!(set(&mut o, "A", json!(false)), Ok(2));
    assert_eq!(wire(&o)["layers"][0]["threeD"], json!(false));
    assert_eq!(o.history_depths(), (0, 0));
    assert!(o.undo(2).is_err(), "nothing to undo: the switch adds no entry");
}
#[test]
fn an_undo_step_taken_before_the_switch_carries_it_so_redo_brings_it_back() {
    let mut o = owner();
    let lock = |o: &mut NativeObjectHistory| {
        let payload = json!({"command":"object.layer.policy.edit","layerUid":"B","flag":"locked","value":true});
        let candidate = o.prepare_layer_policy_json(payload.to_string().as_bytes()).unwrap();
        o.commit_layer_policy(candidate).unwrap()
    };
    assert_eq!(lock(&mut o), 1);
    assert_eq!(set(&mut o, "A", json!(true)), Ok(2));
    assert_eq!(o.history_depths(), (1, 0));
    // Undo takes the lock and the switch back together (they rode one step); redo restores both.
    assert_eq!(o.undo(2), Ok(3));
    assert!(wire(&o)["layers"][0].get("threeD").is_none() || wire(&o)["layers"][0]["threeD"] == json!(false));
    assert_eq!(o.redo(3), Ok(4));
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
#[cfg(feature = "evaluation")]
#[test]
fn a_3d_layer_with_no_geometry_at_the_playhead_is_boxed_at_the_origin_and_a_flat_empty_layer_stays_refused() {
    use crate::request_receipts::DispatchErrorCode::Unavailable;
    let mut value = fixture();
    let blank = |uid: &str| json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875",
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    value["layers"].as_array_mut().unwrap().push(blank("C"));
    value["compositionOrder"]["contexts"][0]["layers"].as_array_mut().unwrap().push(json!({"layerUid":"C","naturalPosition":2,"order":{}}));
    let mut o = NativeObjectHistory::new("three-d-empty", decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap();
    let bounds = |o: &NativeObjectHistory| {
        let pin = head(o);
        let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
            "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"layerBounds","target":{"contextId":"scene-root","layerUid":"C"},"frame":0}});
        pin.query_property_read_json(selector.to_string().as_bytes()).map(|r| (r["bounds"].clone(), r["parentAffine"].clone()))
    };
    assert_eq!(bounds(&o), Err(Unavailable), "a flat layer with nothing to box is still a typed refusal");
    set(&mut o, "C", json!(true)).unwrap();
    assert_eq!(bounds(&o).unwrap(), (json!([0.0, 0.0, 0.0, 0.0]), json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])));
    set(&mut o, "C", json!(false)).unwrap();
    assert_eq!(bounds(&o), Err(Unavailable));
}
#[test]
fn a_duplicator_host_lists_its_eight_per_copy_offsets_after_the_transform_rows_and_they_edit_and_read_back() {
    let document = decode_storage_project(crate::object_frame_packet::duplicator_sources::tests::duplicated_document(1).to_string().as_bytes()).unwrap();
    let mut o = NativeObjectHistory::new("dup-rows", document).unwrap();
    let dup = ["dupOffsetPos", "dupOffsetRot", "dupOffsetScale", "dupOffsetOpacity", "dupOffsetHue", "dupOffsetPosZ", "dupOffsetRotX", "dupOffsetRotY"];
    let base = ["position", "anchor", "rotation", "scale", "opacity", "order"];
    let host = names(&o, "L0");
    assert_eq!(host, base.iter().chain(dup.iter()).map(|s| (*s).to_owned()).collect::<Vec<_>>());
    let mut items = vec![];
    let mut offset = 0;
    loop {
        let page = properties(&o, "L0", offset);
        items.extend(page["items"].as_array().unwrap().clone());
        match page["nextOffset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    let z = items.iter().find(|i| i["property"] == "dupOffsetPosZ").unwrap();
    assert_eq!((z["dimension"].clone(), z["unit"].clone(), z["defaultValue"].clone()), (json!(1), json!("px"), json!([0])));
    // The 3D switch adds its own three rows in front; the duplicator rows follow, and a layer without a duplicator lists none.
    set(&mut o, "L0", json!(true)).unwrap();
    let three = names(&o, "L0");
    assert_eq!(&three[..9], ["position", "positionZ", "anchor", "rotation", "rotationX", "rotationY", "scale", "opacity", "order"]);
    assert_eq!(&three[9..], dup);
    let others: Vec<String> = head(&o).document().layers().iter().map(|l| l.layer_uid().to_owned()).filter(|u| u != "L0").collect();
    for uid in others {
        assert!(names(&o, &uid).iter().all(|n| !n.starts_with("dupOffset")), "{uid}");
    }
    // The three 3D offsets edit and read back like the others, as one revision each.
    let before = o.content_revision();
    assert_eq!(edit(&mut o, "L0", "dupOffsetPosZ", json!([-40])), before + 1);
    assert_eq!(edit(&mut o, "L0", "dupOffsetRotX", json!([12])), before + 2);
    assert_eq!(edit(&mut o, "L0", "dupOffsetRotY", json!([-7.5])), before + 3);
    let holders = wire(&o)["propertyHolders"].clone();
    let value = |property: &str| holders.as_array().unwrap().iter().flat_map(|h| h["properties"].as_array().unwrap().iter())
        .find(|p| p["property"] == property).unwrap()["static"].clone();
    assert_eq!((value("dupOffsetPosZ"), value("dupOffsetRotX"), value("dupOffsetRotY")), (json!([-40]), json!([12]), json!([-7.5])));
    assert_eq!(o.undo(before + 3), Ok(before + 4));
    assert_eq!(wire(&o)["propertyHolders"].as_array().unwrap().iter().flat_map(|h| h["properties"].as_array().unwrap().iter())
        .find(|p| p["property"] == "dupOffsetRotY").map(|p| p["static"].clone()), None);
}
#[test]
fn the_conditional_layer_rows_appear_with_their_prerequisite_in_the_baseline_order() {
    let mut value = crate::object_frame_packet::duplicator_sources::tests::duplicated_document(1);
    let mut other = value["layers"][0].clone();
    other["layerUid"] = json!("L1");
    value["layers"].as_array_mut().unwrap().push(other);
    value["compositionOrder"]["contexts"][0]["layers"].as_array_mut().unwrap().push(json!({"layerUid":"L1","naturalPosition":1,"order":{}}));
    value["componentTables"]["duplicators"]["effectorLayers"] = json!([
        {"contextId":"scene-root","layerUid":"L1","effector":{"targetLayerUid":"L0","falloff":"linear"}}]);
    let base = ["position", "anchor", "rotation", "scale", "opacity", "order"];
    let dup = ["dupOffsetPos", "dupOffsetRot", "dupOffsetScale", "dupOffsetOpacity", "dupOffsetHue", "dupOffsetPosZ", "dupOffsetRotX", "dupOffsetRotY"];
    let listed = |value: &Value, layer: &str| {
        let o = NativeObjectHistory::new("rows", decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap();
        names(&o, layer)
    };
    let strings = |rows: &[&str]| rows.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    // An effector layer lists its radius, strength, the angle of a linear falloff and one offset per channel; a radial one lists no angle.
    let channels = ["effOffPosition", "effOffRotation", "effOffScale", "effOffOpacity", "effOffPositionZ", "effOffRotationX", "effOffRotationY"];
    let mut want = base.to_vec();
    want.extend(["effRadius", "effStrength", "effAngle"]);
    want.extend(channels);
    assert_eq!(listed(&value, "L1"), strings(&want));
    let mut radial = value.clone();
    radial["componentTables"]["duplicators"]["effectorLayers"][0]["effector"]["falloff"] = json!("radial");
    want.retain(|r| *r != "effAngle");
    assert_eq!(listed(&radial, "L1"), strings(&want));
    // A matte, a followed path (Influence only with Align) and a Parent in Time link each add their rows after the duplicator rows.
    assert_eq!(listed(&value, "L0"), strings(&[&base[..], &dup[..]].concat()));
    value["layers"][0]["matteMode"] = json!("alpha");
    value["layers"][0]["matteSourceLayerUid"] = json!("L1");
    let mut with_matte = base.to_vec();
    with_matte.extend(dup);
    with_matte.push("matteOn");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
    value["layers"][0]["followPath"] = json!({"targetLayerUid":"L1"});
    with_matte.push("pathPercent");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
    value["layers"][0]["followPath"] = json!({"targetLayerUid":"L1","align":true});
    with_matte.push("pathInfluence");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1","mode":"in"}});
    with_matte.push("timeLinkInOffset");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1"}});
    with_matte.push("timeLinkOutOffset");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1","mode":"out"}});
    with_matte.retain(|r| *r != "timeLinkInOffset");
    assert_eq!(listed(&value, "L0"), strings(&with_matte));
}
fn gizmo_bounds(value: &Value, layer: &str) -> Result<Value, crate::request_receipts::DispatchErrorCode> {
    let o = NativeObjectHistory::new("gizmo-bounds", decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap();
    let pin = head(&o);
    let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"layerBounds","target":{"contextId":"scene-root","layerUid":layer},"frame":0}});
    pin.query_property_read_json(selector.to_string().as_bytes()).map(|r| r["bounds"].clone())
}
#[test]
fn an_effect_layer_is_boxed_at_the_origin_and_adds_nothing_to_a_null_centre() {
    let mut value = fixture();
    let effect = json!({"layerUid":"FX","name":"fx","visible":true,"locked":false,"color":"#5fa875","isEffectLayer":true,"effects":[],
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    value["layers"].as_array_mut().unwrap().push(effect);
    value["compositionOrder"]["contexts"][0]["layers"].as_array_mut().unwrap().push(json!({"layerUid":"FX","naturalPosition":2,"order":{}}));
    assert_eq!(gizmo_bounds(&value, "FX").unwrap(), json!([0.0, 0.0, 0.0, 0.0]), "the baseline boxes an empty layer at the origin");
    assert_eq!(gizmo_bounds(&value, "A").unwrap(), json!([20.0, 30.0, 120.0, 90.0]), "a vector layer is unchanged");
    let o = NativeObjectHistory::new("fx-union", decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap();
    let union = crate::object_snapshot::object_parent_pivot_read::layers_world_union(&head(&o), "scene-root", &["A", "FX"], 0);
    assert_eq!(union, Some([20.0, 30.0, 120.0, 90.0]), "the effect layer's zero bounds do not pull the centre to the origin");
}
#[test]
fn an_expression_driven_document_still_boxes_its_layers_on_their_rest_geometry() {
    use crate::object_frame_packet::{prepare_object_exposure_frame, FrameScopeKind, ObjectFrameSelector};
    let mut value = fixture();
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"A"},"properties":[
        {"property":"position","expression":{"code":"[1, 2]","enabled":true}}]}]);
    let o = NativeObjectHistory::new("expr-bounds", decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap();
    let pin = head(&o);
    let selector = ObjectFrameSelector { instance_id: pin.instance_id(), document_id: pin.document_id(), content_revision: pin.content_revision(),
        document_snapshot_id: pin.snapshot_id(), context_id: "scene-root", scope_kind: FrameScopeKind::Authored, frame: 0 };
    assert!(prepare_object_exposure_frame(&pin, &selector).is_err(), "premise: the strict packet needs the expression owner");
    let p = crate::object_snapshot::object_parent_pivot_read::layer_gizmo_bounds(&pin, "scene-root", "A", 0);
    let q = crate::object_snapshot::object_parent_pivot_read::layer_ancestor_affine(&pin, "scene-root", "A", 0);
    assert!(p.is_ok() && q.is_ok(), "{p:?} {q:?}");
    assert_eq!(gizmo_bounds(&value, "A").unwrap(), json!([20.0, 30.0, 120.0, 90.0]));
    assert_eq!(gizmo_bounds(&value, "B").unwrap(), json!([0.0, 0.0, 10.0, 10.0]));
}
