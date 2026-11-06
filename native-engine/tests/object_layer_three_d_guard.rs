//! #2520/#2525 post-merge: the 3D switch never leaves the document unavailable. A layer the frame path cannot place in 3D refuses the
//! switch with no revision and no history; every accepted switch keeps every frame preparing; and the accepted switch is saved with a
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

#[test]
fn a_layer_the_frame_path_cannot_place_refuses_the_switch_with_no_revision_and_the_document_keeps_preparing(
) {
    let mut o = owner(&mixed());
    let zero = bytes(&o);
    assert!(prepares(&o), "the fixture prepares before any switch");
    for layer in ["ly_img", "ly_txt", "ly_folder", "ly_fx", "ly_widget"] {
        assert_eq!(
            set(&mut o, layer, true),
            Err(DispatchErrorCode::Unavailable),
            "{layer}"
        );
        assert_eq!(
            (o.content_revision(), o.history_depths(), bytes(&o)),
            (0, (0, 0), zero.clone()),
            "{layer}: no revision, no history"
        );
        assert!(prepares(&o), "{layer}: the document still prepares");
    }
}

#[test]
fn a_vector_layer_a_null_and_an_empty_layer_take_the_switch_and_every_frame_still_prepares() {
    let mut o = owner(&mixed());
    for (revision, layer) in [(1, "ly_v"), (2, "ly_empty")] {
        assert_eq!(set(&mut o, layer, true), Ok(revision), "{layer}");
        assert!(
            prepares(&o),
            "{layer}: the document prepares with the switch on"
        );
        assert!(!head(&o).frame_has_unjoined_3d(), "{layer}");
    }
    // A Null is either taken with the frame still preparing, or refused with no revision: never a silent unavailable document.
    let before = o.content_revision();
    match set(&mut o, "ly_null", true) {
        Ok(_) => assert!(prepares(&o) && !head(&o).frame_has_unjoined_3d()),
        Err(code) => assert_eq!(
            (code, o.content_revision()),
            (DispatchErrorCode::Unavailable, before)
        ),
    }
    // Off again always works, on every kind (a stored 3D flag can be cleared).
    assert!(set(&mut o, "ly_v", false).is_ok() && prepares(&o));
}

#[test]
fn a_stored_3d_flag_on_a_non_placeable_layer_can_still_be_switched_off() {
    let mut doc = mixed();
    doc["layers"][6]["threeD"] = json!(true);
    let mut o = owner(&doc);
    assert!(
        !prepares(&o),
        "an imported 3D image layer is the unavailable state the switch must never create"
    );
    assert_eq!(
        set(&mut o, "ly_img", false),
        Ok(1),
        "switching off is never refused"
    );
    assert!(prepares(&o));
}

#[test]
fn a_gradient_fill_under_3d_is_refused_instead_of_failing_the_scene() {
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
    let zero = bytes(&o);
    assert_eq!(
        set(&mut o, "ly_v", true),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(bytes(&o), zero);
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
fn a_layer_with_children_refuses_the_switch_and_a_child_under_a_flat_parent_takes_it() {
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
    let zero = bytes(&o);
    // The ancestor stays unjoined for its children, so the switch is refused on the parent and the document keeps preparing.
    assert_eq!(
        set(&mut o, "ly_v", true),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!((o.content_revision(), bytes(&o)), (0, zero));
    assert!(prepares(&o));
    // The child itself, under a flat parent, takes the switch.
    assert_eq!(set(&mut o, "ly_empty", true), Ok(1));
    assert!(prepares(&o));
}
