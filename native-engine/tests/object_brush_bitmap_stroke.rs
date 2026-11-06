//! #2361: a bitmap-brush Draw stroke through the real native history: the anchor, its tagged companion directly above
//! it and the baked PNG in the catalog land as one revision and one undo step, and reopen unchanged.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

const GROUP: &str = "bmb_test_1";
fn project() -> Vec<u8> {
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,"layers":[{"layerUid":"L"}],
            "objects":[],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],"scopes":[]}]}},
        "viewportReference":null,"assets":[]}))
    .unwrap()
}
fn anchor(frame: u32) -> Value {
    let point = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":"L","strokeId":"","frameScope":{"kind":"authored","frame":frame}},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[point(10.0, 10.0), point(60.0, 20.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":1,"a":1},"hasRealStroke":true,"strokeWidth":8,"strokeCap":"round",
            "strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":0},
        "vectorExt":{"bitmapBrush":{"bitmapBrushSpec":{"tip":"soft","size":8,"spacing":15,"scatter":20,"opacity":1,
            "color":"#0000ffff","pressure":false,"seed":7},"brushGroupId":GROUP,"preTextureOpacity":1}}})
}
fn request(owner: &NativeObjectHistory, id: &str, record: Value) -> OpacityRequest {
    OpacityRequest::history_stage(
        id,
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "command.document.object.brush.bitmap.stroke".to_string(),
        json!({"documentSnapshotId":current_snapshot_id(owner),"lifecycleGeneration":1,"record":record,
            "companion":{"x":35,"y":15,"width":58,"height":18,"opacity":1},"source":{"kind":"file","path":"/staged/brush.png"}}),
    )
}
fn state(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(
        &owner
            .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn a_bitmap_brush_stroke_lands_as_anchor_companion_and_png_in_one_undo_step() {
    let mut owner = NativeObjectHistory::from_project_file("desktop", &project()).unwrap();
    let before = state(&owner);
    let reply = owner
        .dispatch_bitmap_stroke(
            &request(&owner, "stroke-1", anchor(0)),
            &oracle_bytes("blue-2x1"),
        )
        .unwrap();
    assert!(reply.is_ok(), "{}", serde_json::to_value(&reply).unwrap());
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let after = state(&owner);
    let drawing = &after["drawing"];
    let anchors = drawing["objects"].as_array().unwrap();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0]["target"]["strokeId"], "native-stroke:1:0");
    assert_eq!(
        anchors[0]["vectorExt"]["bitmapBrush"]["brushGroupId"],
        GROUP
    );
    let rasters = drawing["items"]["raster"].as_array().unwrap();
    assert_eq!(rasters.len(), 1);
    let companion = &rasters[0];
    assert_eq!(
        companion["target"]["frameScope"], anchors[0]["target"]["frameScope"],
        "the same cell"
    );
    assert_eq!(
        (
            &companion["item"]["isBitmapBrush"],
            &companion["item"]["isBrushTextureCopy"],
            &companion["item"]["brushGroupId"]
        ),
        (&json!(true), &json!(true), &json!(GROUP))
    );
    assert_eq!(
        (&companion["item"]["width"], &companion["item"]["height"]),
        (&json!(58.0), &json!(18.0))
    );
    let asset = companion["item"]["assetId"].as_str().unwrap();
    assert!(
        after["assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == asset),
        "the PNG joined the catalog"
    );
    // The companion lies directly above its anchor (insertAbove(basePath)).
    let elements = drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap();
    let order: Vec<&Value> = elements.iter().map(|e| &e["strokeId"]).collect();
    assert_eq!(
        order,
        [
            &anchors[0]["target"]["strokeId"],
            &companion["target"]["strokeId"]
        ]
    );
    let reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap())
            .unwrap();
    assert_eq!(state(&reopened), after, "saves and reopens unchanged");
    owner.undo(1).unwrap();
    assert_eq!(
        state(&owner),
        before,
        "one undo removes the stroke, its companion and its PNG"
    );
    owner.redo(2).unwrap();
    assert_eq!(state(&owner), after);
}

#[test]
fn a_stale_snapshot_a_named_anchor_or_a_missing_brush_group_is_refused_before_any_edit() {
    let mut owner = NativeObjectHistory::from_project_file("desktop", &project()).unwrap();
    let png = oracle_bytes("blue-2x1");
    let mut named = anchor(0);
    named["target"]["strokeId"] = json!("chosen");
    let mut plain = anchor(0);
    plain.as_object_mut().unwrap().remove("vectorExt");
    for (id, record) in [("named", named), ("plain", plain)] {
        let reply = owner
            .dispatch_bitmap_stroke(&request(&owner, id, record), &png)
            .unwrap();
        assert!(!reply.is_ok(), "{id}");
    }
    let mut stale = request(&owner, "stale", anchor(0));
    stale.payload["documentSnapshotId"] = json!("native-object:elsewhere:0");
    assert!(!owner.dispatch_bitmap_stroke(&stale, &png).unwrap().is_ok());
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    let _ = DispatchErrorCode::InvalidRequest;
}

#[test]
fn behind_places_the_anchor_under_the_layer_artwork_with_its_companion_directly_above_it() {
    let mut owner = NativeObjectHistory::from_project_file("desktop", &project()).unwrap();
    let png = oracle_bytes("blue-2x1");
    assert!(owner.dispatch_bitmap_stroke(&request(&owner, "front", anchor(0)), &png).unwrap().is_ok());
    let one = state(&owner);
    let mut behind = request(&owner, "behind", anchor(0));
    behind.payload["placement"] = json!("behind");
    let reply = owner.dispatch_bitmap_stroke(&behind, &png).unwrap();
    assert!(reply.is_ok(), "{}", serde_json::to_value(&reply).unwrap());
    let after = state(&owner);
    // The layer's natural stack, bottom to top (naturalPosition, not the element array's order).
    let mut elements: Vec<&Value> = after["drawing"]["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .collect();
    elements.sort_by_key(|e| e["naturalPosition"].as_u64().unwrap());
    let order: Vec<&str> = elements.iter().map(|e| e["strokeId"].as_str().unwrap()).collect();
    // native-stroke/raster :1: is the front stroke, :2: the one drawn behind it.
    assert_eq!(order.len(), 4);
    assert!(order[0].starts_with("native-stroke:2:") && order[1].starts_with("native-raster:2:"), "{order:?}");
    assert!(order[2].starts_with("native-stroke:1:") && order[3].starts_with("native-raster:1:"), "{order:?}");
    owner.undo(2).unwrap();
    assert_eq!(state(&owner), one, "one undo step removes the behind stroke, its companion and its PNG");
    let mut sideways = request(&owner, "sideways", anchor(0));
    sideways.payload["placement"] = json!("front");
    assert!(!owner.dispatch_bitmap_stroke(&sideways, &png).unwrap().is_ok(), "only behind is a placement");
}

/// A scene layer `L1` and a host layer placing component `comp-a` (8 frames, layer `inner`, keyed at 0 only).
fn component_project() -> Vec<u8> {
    let layers = |uids: &[&str]| json!(uids.iter().enumerate().map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}})).collect::<Vec<_>>());
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
        "layers":[{"layerUid":"L1","timing":timing},{"layerUid":"host-a","timing":timing}],"objects":[]});
    drawing["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
        {"contextId":"scene-root","layers":layers(&["L1","host-a"]),"scopes":[]},{"contextId":"comp-a","layers":layers(&["inner"]),"scopes":[]}]});
    drawing["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-a","totalFrames":8,"layers":[{"layerUid":"inner","timing":timing}],"objects":[]}],
        "instances":[{"contextId":"scene-root","layerUid":"host-a","symbolId":"comp-a","symPlayMode":"loop"}]}});
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":drawing,"viewportReference":null,"assets":[]})).unwrap()
}
fn in_component(frame: u32) -> Value {
    let mut record = anchor(frame);
    record["target"]["contextId"] = json!("comp-a");
    record["target"]["layerUid"] = json!("inner");
    record
}

#[test]
fn a_bitmap_brush_stroke_in_a_component_lands_in_that_component_keying_a_held_cell_as_one_undo_step() {
    let png = oracle_bytes("blue-2x1");
    let mut owner = NativeObjectHistory::from_project_file("desktop", &component_project()).unwrap();
    let before = state(&owner);
    // Frame 3 of the component is held: the cell is keyed first, then the anchor and its companion land there, in the same revision.
    let reply = owner.dispatch_bitmap_stroke(&request(&owner, "comp-held", in_component(3)), &png).unwrap();
    assert!(reply.is_ok(), "{}", serde_json::to_value(&reply).unwrap());
    assert_eq!((owner.content_revision(), owner.history_depths()), (1, (1, 0)));
    let after = state(&owner);
    let comp = &after["drawing"]["componentTables"]["components"]["components"][0];
    let anchors = comp["objects"].as_array().unwrap();
    assert_eq!(anchors.len(), 1, "the anchor joined the component");
    assert_eq!((&anchors[0]["target"]["contextId"], &anchors[0]["target"]["frameScope"]["frame"]), (&json!("comp-a"), &json!(3)));
    assert!(after["drawing"]["objects"].as_array().unwrap().is_empty(), "the scene root holds no record");
    let rasters = after["drawing"]["items"]["raster"].as_array().unwrap();
    assert_eq!(rasters.len(), 1);
    assert_eq!((&rasters[0]["target"]["contextId"], &rasters[0]["item"]["isBitmapBrush"], &rasters[0]["item"]["brushGroupId"]),
        (&json!("comp-a"), &json!(true), &json!(GROUP)));
    let keys: Vec<&Value> = comp["layers"][0]["timing"]["keys"].as_array().unwrap().iter().map(|k| &k["frame"]).collect();
    assert_eq!(keys, [&json!(0), &json!(3)], "the held cell became a key");
    let reopened = NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap()).unwrap();
    assert_eq!(state(&reopened), after, "saves and reopens unchanged");
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before, "one undo removes the key, the stroke, its companion and its PNG");
    owner.redo(2).unwrap();
    assert_eq!(state(&owner), after);
    // Past the component's 8 frames, an undeclared component and a layer the component does not hold are refused with nothing written.
    let mut stray = in_component(0);
    stray["target"]["layerUid"] = json!("L1");
    let mut elsewhere = in_component(0);
    elsewhere["target"]["contextId"] = json!("comp-b");
    for (id, record) in [("past", in_component(8)), ("stray", stray), ("elsewhere", elsewhere)] {
        let mut fresh = NativeObjectHistory::from_project_file("desktop", &component_project()).unwrap();
        let refused = fresh.dispatch_bitmap_stroke(&request(&fresh, id, record), &png).unwrap();
        assert!(!refused.is_ok(), "{id}");
        assert_eq!((fresh.content_revision(), fresh.history_depths(), state(&fresh)), (0, (0, 0), state(&NativeObjectHistory::from_project_file("desktop", &component_project()).unwrap())), "{id}");
    }
}

#[test]
fn a_component_bitmap_stroke_is_painted_by_the_component_at_the_frame_it_was_drawn_on() {
    use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
    use crate::object_document::FrameScopeKind;
    use crate::object_render_scene::{ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry};
    let png = oracle_bytes("blue-2x1");
    let mut owner = NativeObjectHistory::from_project_file("desktop", &component_project()).unwrap();
    let painted = |owner: &NativeObjectHistory, frame: u32| {
        let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let expected = ObjectFrameSelector {
            instance_id: s.instance_id(), document_id: s.document_id(), content_revision: s.content_revision(), document_snapshot_id: s.snapshot_id(),
            context_id: "scene-root", scope_kind: FrameScopeKind::Authored, frame,
        };
        let packet = prepare_object_exposure_frame(&s, &expected).unwrap_or_else(|e| panic!("{e:?}"));
        let scene = prepare_object_render_scene(&packet, &expected).unwrap_or_else(|e| panic!("the component scene is admitted: {e:?}"));
        let plan = prepare_object_paint_plan(&scene, &expected).unwrap_or_else(|e| panic!("{e:?}"));
        // (component contours, placed images): a component's image is a Raster entry the component scene appended.
        let entries: Vec<PaintEntry> = ordered_entries(&plan).collect();
        let contours = entries.iter().filter(|e| matches!(e, PaintEntry::Component(c) if c.target.context_id() == "comp-a")).count();
        (contours, entries.iter().filter(|e| matches!(e, PaintEntry::Raster(_))).count())
    };
    assert_eq!(painted(&owner, 3), (0, 0), "nothing drawn yet");
    assert!(owner.dispatch_bitmap_stroke(&request(&owner, "paint", in_component(3)), &png).unwrap().is_ok());
    // The camouflaged anchor is component contour paint and the companion's baked PNG is a placed image at frame 3; frame 0 (the key) shows neither.
    assert_eq!(painted(&owner, 3), (1, 1), "anchor contour and companion image");
    assert_eq!(painted(&owner, 0), (0, 0));
}
