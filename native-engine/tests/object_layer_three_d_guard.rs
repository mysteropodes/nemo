//! #2520/#2525 post-merge: the 3D switch never leaves the document unavailable. A layer the frame path cannot place in 3D (a component
//! instance) refuses the switch with no revision and no history; every accepted switch keeps every frame preparing; and the accepted switch is saved with a
//! revision but is no undo step (baseline `toggleLayer3D` pushed no undo snapshot), exactly like the folder fold.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_component_convert_fixture.rs"]
mod fixture;

fn element(layer: &str, stroke: &str, position: u32) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}
fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":stroke})
}
/// One layer of every kind the switch can be offered on, in one document.
fn mixed() -> Value {
    let layers = [
        fixture::layer("ly_v", "Vector", &[(0, "sv")]),
        fixture::layer("ly_empty", "Empty", &[]),
        fixture::layer("ly_null", "Null", &[]).with(json!({"isNullLayer":true})),
        fixture::layer("ly_folder", "Folder", &[]).with(json!({"isFolderLayer":true})),
        fixture::layer("ly_fx", "Effect", &[]).with(json!({"isEffectLayer":true,"effects":[]})),
        fixture::layer("ly_widget", "Widget", &[])
            .with(json!({"widget":{"kind":"slider","x":{"key":"k","min":0,"max":1}}})),
        fixture::layer("ly_img", "Image", &[]),
        fixture::layer("ly_txt", "Text", &[]),
    ];
    let mut text = fixture::text_item("ly_txt");
    text["target"] = target("ly_txt", "t0");
    let raster = json!({"target":target("ly_img", "r0"),"item":{"assetId":"img-1","x":0,"y":0,"width":8,"height":8,"opacity":1}});
    let mut doc = fixture::document(&layers, json!({"items":{"raster":[raster],"text":[text]}}));
    let elements = doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap();
    elements.push(element("ly_img", "r0", 0));
    elements.push(element("ly_txt", "t0", 0));
    doc
}
/// A document whose only layer is a component instance: the one kind the frame path cannot place in 3D yet.
fn with_instance() -> Value {
    let layers = [fixture::layer("ly_inst", "Instance", &[]), fixture::layer("ly_v", "Vector", &[(0, "sv")])];
    fixture::document(
        &layers,
        json!({"componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"sym_c","totalFrames":24,"layers":[{"layerUid":"ci"}],"objects":[]}],
            "instances":[{"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_c","symPlayMode":"once",
                "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}]}}}),
    )
}
fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "three-d-guard",
        decode_storage_project(doc.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn head(o: &NativeObjectHistory) -> crate::revision::ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn set(o: &mut NativeObjectHistory, layer: &str, value: bool) -> Result<u64, DispatchErrorCode> {
    let payload = json!({"command":"object.layer.policy.edit","layerUid":layer,"flag":"threeD","value":value});
    let candidate = o.prepare_layer_policy_json(payload.to_string().as_bytes())?;
    o.commit_layer_policy(candidate)
}
/// The real frame preparation of the head revision at frame 0.
fn prepares(o: &NativeObjectHistory) -> bool {
    let s = head(o);
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    prepare_object_exposure_frame(&s, &selector).is_ok()
}

/// The Motion-capable frame preparation (the plain one refuses any authored property holder).
fn prepares_with_motion(o: &NativeObjectHistory) -> bool {
    let s = head(o);
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let mut effects = None;
    crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner(o, &s, &selector, &mut effects).is_ok()
}
fn key(o: &mut NativeObjectHistory, layer: &str, property: &str, values: Value) {
    let payload = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
        "property":property,"action":{"kind":"setValue","frame":0,"values":values}});
    let candidate = o.prepare_property_edit_json(payload.to_string().as_bytes()).unwrap();
    o.commit_property_edit(candidate).unwrap();
}

#[test]
fn the_three_d_keys_of_every_kind_leave_the_frame_available_with_the_switch_on_or_off() {
    let mut o = owner(&mixed());
    assert!(prepares_with_motion(&o));
    let kinds = ["ly_v", "ly_empty", "ly_null", "ly_folder", "ly_fx", "ly_widget", "ly_img", "ly_txt"];
    // Dormant keys (switch off) first, on every kind including Text, then the same keys with the switch on.
    for layer in kinds {
        key(&mut o, layer, "positionZ", json!([300]));
        key(&mut o, layer, "rotationY", json!([20]));
        assert!(prepares_with_motion(&o), "{layer}: dormant 3D keys");
    }
    for layer in kinds.iter().filter(|l| **l != "ly_txt") {
        assert!(set(&mut o, layer, true).is_ok(), "{layer}");
        assert!(prepares_with_motion(&o), "{layer}: active 3D keys");
    }
}

#[test]
fn a_component_instance_is_the_one_kind_the_frame_path_cannot_place_yet_and_refuses_the_switch_with_no_revision() {
    let mut o = owner(&with_instance());
    let zero = bytes(&o);
    assert!(prepares(&o), "the fixture prepares before any switch");
    assert_eq!(set(&mut o, "ly_inst", true), Err(DispatchErrorCode::Unavailable));
    assert_eq!(
        (o.content_revision(), o.history_depths(), bytes(&o)),
        (0, (0, 0), zero),
        "no revision, no history"
    );
    assert!(prepares(&o), "the document still prepares");
}

#[test]
fn every_other_kind_takes_the_switch_and_every_frame_still_prepares() {
    let mut o = owner(&mixed());
    let kinds = ["ly_v", "ly_empty", "ly_null", "ly_folder", "ly_fx", "ly_widget", "ly_img", "ly_txt"];
    for (i, layer) in kinds.iter().enumerate() {
        assert_eq!(set(&mut o, layer, true), Ok(i as u64 + 1), "{layer}");
        assert!(prepares(&o), "{layer}: the document prepares with the switch on");
        assert!(!head(&o).frame_has_unjoined_3d(), "{layer}");
    }
    // Off again always works, on every kind (a stored 3D flag can be cleared).
    for layer in kinds {
        assert!(set(&mut o, layer, false).is_ok() && prepares(&o), "{layer}");
    }
}

#[test]
fn a_text_layer_takes_the_switch_with_a_saved_revision_and_no_undo_entry() {
    let mut o = owner(&mixed());
    assert_eq!(set(&mut o, "ly_txt", true), Ok(1));
    assert_eq!(o.history_depths(), (0, 0));
    assert!(prepares(&o) && !head(&o).frame_has_unjoined_3d());
}

#[test]
fn a_stored_3d_flag_on_a_component_instance_can_still_be_switched_off() {
    let mut doc = with_instance();
    doc["layers"][0]["threeD"] = json!(true);
    let mut o = owner(&doc);
    assert!(
        !prepares(&o),
        "an imported 3D instance layer is the unavailable state the switch must never create"
    );
    assert_eq!(set(&mut o, "ly_inst", false), Ok(1), "switching off is never refused");
    assert!(prepares(&o));
}

/// #2520: a duplicator host and a layer holding a mask contour both take the switch, and every frame still prepares (their clones and
/// their mask are projected with the layer).
#[test]
fn a_duplicator_host_and_a_mask_layer_take_the_switch_and_the_frame_still_prepares() {
    let host = fixture::document(
        &[fixture::layer("ly_v", "Vector", &[(0, "sv")])],
        json!({"componentTables":{"duplicators": fixture::duplicator_table(&[("ly_v", json!({"mode":"grid","rows":1,"cols":3,"spacingX":10,"timeOffset":{"enabled":true,"offsetFrames":1,"direction":"forward"}}))])}}),
    );
    // A duplicator needs the canvas metadata of a project.
    let document = decode_storage_project(host.to_string().as_bytes()).unwrap();
    let project = crate::object_document::ProjectEnvelope::no_media(
        crate::object_document::ProjectMetadata::new(100, 100, 24.).unwrap(),
        document,
    );
    let mut o = NativeObjectHistory::from_admitted_project("three-d-dup", project).unwrap();
    assert!(prepares(&o));
    assert_eq!(set(&mut o, "ly_v", true), Ok(1));
    assert!(prepares(&o) && !head(&o).frame_has_unjoined_3d());
    assert_eq!(o.history_depths(), (0, 0));
    let mut masked = mixed();
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    masked["objects"][0] = json!({"schemaVersion":1,"family":"editable-vector-path","target":target("ly_v", "sv"),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(0., 0.), corner(100., 0.), corner(100., 60.), corner(0., 60.)]},
        "fill":null,
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1},
        "vectorExt":{"isMask":true,"maskMode":"add","maskFeather":0}});
    let mut o = owner(&masked);
    assert!(prepares(&o));
    assert_eq!(set(&mut o, "ly_v", true), Ok(1));
    assert!(prepares(&o) && !head(&o).frame_has_unjoined_3d());
}

#[test]
fn a_gradient_fill_takes_the_switch_and_the_frame_still_prepares() {
    let mut doc = mixed();
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    doc["objects"][0] = json!({"schemaVersion":1,"family":"editable-vector-path","target":target("ly_v", "sv"),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(0., 0.), corner(100., 0.), corner(100., 60.), corner(0., 60.)]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1},
        "vectorExt":{"fillGradient":{"kind":"linear","from":[0,0],"to":[100,60],"stops":[
            {"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}}});
    let mut o = owner(&doc);
    assert_eq!(set(&mut o, "ly_v", true), Ok(1));
    assert!(prepares(&o));
}

#[test]
fn the_switch_is_saved_with_a_revision_but_adds_no_undo_entry_and_leaves_redo_alone() {
    let mut o = owner(&mixed());
    // A real undo step first: lock a layer, so there is something to undo and then to redo.
    let lock = |o: &mut NativeObjectHistory, value: bool| {
        let payload = json!({"command":"object.layer.policy.edit","layerUid":"ly_v","flag":"locked","value":value});
        let candidate = o
            .prepare_layer_policy_json(payload.to_string().as_bytes())
            .unwrap();
        o.commit_layer_policy(candidate).unwrap()
    };
    assert_eq!(lock(&mut o, true), 1);
    assert_eq!(o.history_depths(), (1, 0));
    // On: a new revision, the stored flag, no new history entry.
    assert_eq!(set(&mut o, "ly_v", true), Ok(2));
    assert_eq!(o.history_depths(), (1, 0), "no undo entry for the switch");
    assert!(
        String::from_utf8(bytes(&o))
            .unwrap()
            .contains("\"threeD\":true"),
        "the flag is saved"
    );
    // Off again: still no entry, a revision each way; a no-op authors nothing.
    assert_eq!(set(&mut o, "ly_v", false), Ok(3));
    assert_eq!(set(&mut o, "ly_v", false), Ok(3));
    assert_eq!(o.history_depths(), (1, 0));
    // Redo is left alone: undo the lock, switch 3D, and the redo stack still holds the lock.
    assert_eq!(o.undo(3), Ok(4));
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(set(&mut o, "ly_v", true), Ok(5));
    assert_eq!(
        o.history_depths(),
        (0, 1),
        "the switch neither clears redo nor adds an entry"
    );
}

#[test]
fn a_3d_layer_with_children_and_a_child_under_a_3d_parent_both_keep_the_frame_preparing() {
    let mut doc = mixed();
    doc["layers"][1]["parentLayerUid"] = json!("ly_v");
    doc["objects"]
        .as_array_mut()
        .unwrap()
        .push(fixture::record("ly_empty", "se", 0));
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(element("ly_empty", "se", 0));
    let mut o = owner(&doc);
    assert!(prepares(&o));
    // The parent takes the switch: its children keep the chain of its 2D Motion, as the baseline `computeMotionMat` did.
    assert_eq!(set(&mut o, "ly_v", true), Ok(1));
    assert!(prepares(&o) && !head(&o).frame_has_unjoined_3d());
    // The child under that 3D parent takes it too.
    assert_eq!(set(&mut o, "ly_empty", true), Ok(2));
    assert!(prepares(&o));
}
