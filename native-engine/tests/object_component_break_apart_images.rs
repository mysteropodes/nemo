//! Render A/B of breaking a component apart when it holds images (#2256 D-comp-break-apart): the layer baked from an instance
//! paints the very same image rect, opacity and paint order as the instance did, with a rotated instance matrix, the component
//! camera and an inner opacity track in play. Both sides come from the one object scene.
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, FrameScopeKind, ProjectEnvelope,
    ProjectMetadata,
};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry,
};
use serde_json::{json, Value};
use std::sync::Arc;

const OP: &str = "command.document.object.component.breakApart";
/// A solid opaque green 2x2 PNG.
const GREEN_PNG: &str = "89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001049444154789c636038c1f01f8c610c003ba4071d8da765460000000049454e44ae426082";

fn inventory() -> AssetInventory {
    let bytes: Vec<u8> = (0..GREEN_PNG.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&GREEN_PNG[at..at + 2], 16).unwrap())
        .collect();
    let source = AssetSource::admitted_embedded("image/png".into(), Arc::new(bytes), 1 << 20).unwrap();
    let record = AssetRecord::new(AssetId::new("img-g".into()).unwrap(), "img-g.png".into(), AssetKind::Image, source).unwrap();
    AssetInventory::new(vec![Arc::new(record)], Vec::new(), 16, 0).unwrap()
}
fn timing(context: &str, keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":context,
        "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
fn target(layer: &str, stroke: &str, frame: u32) -> Value {
    json!({"contextId":"comp","layerUid":layer,"strokeId":stroke,"frameScope":{"kind":"authored","frame":frame}})
}
fn contour(stroke: &str, frame: u32, rect: [i32; 4]) -> Value {
    let cases: Value = serde_json::from_str(include_str!("../../engineering/application/examples/native-object-v1/cases.json")).unwrap();
    let mut r = cases["records"][0].clone();
    r["target"] = target("inner", stroke, frame);
    let [x0, y0, x1, y1] = rect;
    r["geometry"]["segments"] = json!([[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x, y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    r["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    r
}
/// The inner layer holds, on frame 0, a red square UNDER a green image and, on frame 2, the image alone further right.
fn drawing(instance: Value, component_extra: Value) -> Value {
    let mut host = json!({"contextId":"scene-root","layerUid":"host","symbolId":"comp","symPlayMode":"once","symSpeed":1,"symPlacedAt":0});
    for (k, v) in instance.as_object().unwrap() {
        host[k] = v.clone();
    }
    let mut component = json!({"symbolId":"comp","totalFrames":4,
        "layers":[{"layerUid":"inner","timing":timing("scene-root", &[0, 2])}],
        "objects":[contour("ink", 0, [10, 10, 22, 22])]});
    for (k, v) in component_extra.as_object().unwrap() {
        component[k] = v.clone();
    }
    let element = |layer: &str, stroke: &str, at: u32| json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":at,"order":{}});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
        "layers":[{"layerUid":"host","timing":timing("scene-root", &[0])}],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
                {"contextId":"scene-root","layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],"scopes":[]},
                {"contextId":"comp","layers":[{"layerUid":"inner","naturalPosition":0,"order":{}}],"scopes":[
                    {"frameScope":{"kind":"authored","frame":0},"elements":[element("inner","ink",0),element("inner","img",1)]},
                    {"frameScope":{"kind":"authored","frame":2},"elements":[element("inner","img2",0)]}]}]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[component],"instances":[host]}},
        "items":{"raster":[
            {"target":target("inner","img",0),"item":{"assetId":"img-g","x":16,"y":16,"width":8,"height":8,"opacity":0.8}},
            {"target":target("inner","img2",2),"item":{"assetId":"img-g","x":30,"y":16,"width":8,"height":8,"opacity":1}}]}})
}
fn owner(drawing: &Value) -> NativeObjectHistory {
    let document = decode_storage_project(drawing.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document).with_asset_inventory(Some(inventory()));
    NativeObjectHistory::from_admitted_project("break-images", project).unwrap()
}

/// What one frame paints in order: a path (component or contour) or an image rect with its opacity.
#[derive(Debug, Clone, PartialEq)]
enum Paint {
    Path(vello::kurbo::BezPath),
    Image(ImageRect, f64),
}
fn painted(owner: &NativeObjectHistory, frame: u32) -> Vec<Paint> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Component(c) => Paint::Path(c.path.clone()),
            PaintEntry::Contour(c) => Paint::Path(c.path().clone()),
            PaintEntry::Raster(at) => {
                let source = &plan.raster_sources()[at];
                Paint::Image(source.source.rect, source.source.opacity)
            }
            _ => panic!("unexpected paint entry"),
        })
        .collect()
}
fn bake(owner: &mut NativeObjectHistory) -> Value {
    let request = json!({"apiVersion":2,"requestId":"break-images","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":OP,
        "payload":{"command":"object.component.breakApart","contextId":"scene-root","layerUid":"host"}});
    serde_json::to_value(owner.try_dispatch_object_command(&serde_json::from_value(request).unwrap()).unwrap().unwrap()).unwrap()
}
fn assert_baked_equals_instance(drawing: Value) {
    let mut owner = owner(&drawing);
    let before: Vec<Vec<Paint>> = (0..4).map(|f| painted(&owner, f)).collect();
    assert!(before[0].iter().any(|p| matches!(p, Paint::Image(..))), "the instance paints its image");
    let reply = bake(&mut owner);
    assert_eq!(reply["ok"], true, "{reply}");
    for frame in 0..4u32 {
        let after = painted(&owner, frame);
        assert_eq!(after.len(), before[frame as usize].len(), "frame {frame}");
        for (a, b) in before[frame as usize].iter().zip(&after) {
            match (a, b) {
                (Paint::Path(x), Paint::Path(y)) => assert_eq!(x, y, "frame {frame}"),
                (Paint::Image(rx, ox), Paint::Image(ry, oy)) => {
                    assert_eq!(rx, ry, "frame {frame}");
                    assert!((ox - oy).abs() < 1e-9, "frame {frame}: {ox} vs {oy}");
                }
                _ => panic!("frame {frame}: the paint order changed: {a:?} then {b:?}"),
            }
        }
    }
    // The undo brings the instance paint back.
    owner.undo(owner.content_revision()).unwrap();
    for frame in 0..4u32 {
        assert_eq!(painted(&owner, frame), before[frame as usize], "undo, frame {frame}");
    }
}

#[test]
fn an_unplaced_instance_bakes_its_image_and_contour_in_the_same_paint_order() {
    assert_baked_equals_instance(drawing(json!({}), json!({})));
}

#[test]
fn a_rotated_instance_matrix_and_the_component_camera_place_the_baked_image_as_they_placed_the_instance_image() {
    let instance = json!({"symMatrix":[0.0, 2.0, -1.0, 0.0, 40.0, 8.0]});
    let camera = json!({"cameraKeys":[{"frame":0,"x":32,"y":32,"w":32},{"frame":3,"x":32,"y":32,"w":64}]});
    assert_baked_equals_instance(drawing(instance, camera));
}

#[test]
fn an_inner_opacity_track_changes_the_baked_image_opacity_on_every_frame() {
    let mut doc = drawing(json!({}), json!({}));
    doc["propertyHolders"] = json!([{"target":{"contextId":"comp","layerUid":"inner"},"properties":[{"property":"opacity","track":{"keys":[
        {"frame":0,"v":[100],"curvePoints":[]},{"frame":3,"v":[40]}]}}]}]);
    assert_baked_equals_instance(doc);
}

#[test]
fn a_brush_image_inside_the_component_is_refused_whole_and_publishes_nothing() {
    let mut doc = drawing(json!({}), json!({}));
    doc["items"]["raster"][0]["item"]["meshId"] = json!("mesh_1");
    let document = decode_storage_project(doc.to_string().as_bytes()).expect("a mesh-bound image is admissible");
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document).with_asset_inventory(Some(inventory()));
    let mut owner = NativeObjectHistory::from_admitted_project("break-images", project).unwrap();
    let before = owner.content_revision();
    let reply = bake(&mut owner);
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(owner.content_revision(), before);
}
