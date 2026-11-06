//! Render A/B of breaking a component apart, camera included (#2256 C-comp-owned-state + D-comp-break-apart): the component's
//! own camera keys (`sym.cameraKeys`) ride with the placed instance, and the layer baked from that instance paints the very
//! same geometry on every frame as the instance did. Both sides come from the one object scene, so this is the
//! viewport/export pixel contract, not a second evaluation.
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
    PaintEntry,
};
use serde_json::{json, Value};
use vello::kurbo::BezPath;

const OP: &str = "command.document.object.component.breakApart";
const TOTAL: u32 = 8;

fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
fn contour(
    context: &str,
    layer: &str,
    stroke: &str,
    frame: u32,
    rect: [i32; 4],
    rgb: [u8; 3],
) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut r = cases["records"][0].clone();
    r["target"] = json!({"contextId":context,"layerUid":layer,"strokeId":stroke,"frameScope":{"kind":"authored","frame":frame}});
    let [x0, y0, x1, y1] = rect;
    r["geometry"]["segments"] =
        json!([[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    let [red, green, blue] = rgb.map(|c| f64::from(c) / 255.0);
    r["fill"] = json!({"kind":"solid","r":red,"g":green,"b":blue,"a":1});
    r
}
fn drawing(instance_extra: Value, component_extra: Value) -> Value {
    let mut instance = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"loop",
        "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0});
    for (k, v) in instance_extra.as_object().unwrap() {
        instance[k] = v.clone();
    }
    let mut component = json!({"symbolId":"sym_x","totalFrames":TOTAL,
        "layers":[{"layerUid":"xi","name":"Layer 1","visible":true,"locked":false,"timing":timing(&[0, 4])}],
        "objects":[contour("sym_x","xi","a",0,[10,10,30,20],[255,0,0]), contour("sym_x","xi","b",4,[40,40,60,70],[0,255,0])]});
    for (k, v) in component_extra.as_object().unwrap() {
        component[k] = v.clone();
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":TOTAL,
        "layers":[{"layerUid":"ly_inst","name":"Inst","visible":true,"locked":true,"timing":timing(&[0])}],
        "objects":[],
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[component],"instances":[instance]}},
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
                {"contextId":"scene-root","layers":[{"layerUid":"ly_inst","naturalPosition":0,"order":{}}],"scopes":[]},
                {"contextId":"sym_x","layers":[{"layerUid":"xi","naturalPosition":0,"order":{}}],"scopes":[
                    {"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"xi","strokeId":"a","naturalPosition":0,"order":{}}]},
                    {"frameScope":{"kind":"authored","frame":4},"elements":[{"layerUid":"xi","strokeId":"b","naturalPosition":0,"order":{}}]}]}]}})
}
fn owner(drawing: Value) -> NativeObjectHistory {
    let document = decode_storage_project(drawing.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), document);
    NativeObjectHistory::from_admitted_project("break-render", project).unwrap()
}
fn plan_at(owner: &NativeObjectHistory, frame: u32) -> ObjectPaintPlan {
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
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
/// Every painted path of the frame in paint order, whether a placed component or a contour of the layer.
fn paths(owner: &NativeObjectHistory, frame: u32) -> Vec<BezPath> {
    let plan = plan_at(owner, frame);
    ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Component(c) => c.path.clone(),
            PaintEntry::Contour(c) => c.path().clone(),
            _ => panic!("unexpected paint entry"),
        })
        .collect()
}
fn bake(owner: &mut NativeObjectHistory) {
    let request = json!({"apiVersion":2,"requestId":"break-1","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":OP,
        "payload":{"command":"object.component.breakApart","contextId":"scene-root","layerUid":"ly_inst"}});
    let reply = serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
}
fn assert_same(a: &BezPath, b: &BezPath) {
    assert_eq!(a.elements().len(), b.elements().len());
    assert_eq!(a, b);
}

fn camera() -> Value {
    // 320x180 canvas: frame 0 a 2x zoom on the centre, frame 6 back to 1x; every frame between is a different matrix.
    json!({"cameraKeys":[{"frame":0,"x":160,"y":90,"w":160},{"frame":6,"x":160,"y":90,"w":320}]})
}

#[test]
fn the_component_camera_moves_the_placed_geometry_and_the_baked_layer_paints_the_same_on_every_frame(
) {
    let plain = owner(drawing(json!({}), json!({})));
    let cammed = owner(drawing(json!({}), camera()));
    let still = paths(&plain, 0);
    let zoomed = paths(&cammed, 0);
    assert_eq!(still.len(), 1);
    assert_ne!(
        still[0], zoomed[0],
        "the stored component camera is applied to the instance"
    );
    let mut cammed = cammed;
    let before: Vec<Vec<BezPath>> = (0..TOTAL).map(|f| paths(&cammed, f)).collect();
    bake(&mut cammed);
    for frame in 0..TOTAL {
        let after = paths(&cammed, frame);
        assert_eq!(after.len(), before[frame as usize].len(), "frame {frame}");
        for (a, b) in before[frame as usize].iter().zip(&after) {
            assert_same(a, b);
        }
    }
    // The camera moves on every frame up to its last key (6); frames 6 and 7 are identical, so 7 is a hold of 6.
    let snapshot = cammed.acquire_snapshot(cammed.content_revision()).unwrap();
    let stored: Value = serde_json::from_slice(
        &crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        stored["layers"][0]["timing"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[test]
fn the_instance_matrix_and_the_camera_compose_camera_first() {
    let mut both = owner(drawing(json!({"symMatrix":[1,0,0,1,5,7]}), camera()));
    let before: Vec<Vec<BezPath>> = (0..TOTAL).map(|f| paths(&both, f)).collect();
    bake(&mut both);
    for frame in 0..TOTAL {
        let after = paths(&both, frame);
        for (a, b) in before[frame as usize].iter().zip(&after) {
            assert_same(a, b);
        }
    }
}

#[test]
fn undo_restores_the_instance_paint() {
    let mut cammed = owner(drawing(json!({}), camera()));
    let before: Vec<Vec<BezPath>> = (0..TOTAL).map(|f| paths(&cammed, f)).collect();
    bake(&mut cammed);
    cammed.undo(cammed.content_revision()).unwrap();
    for frame in 0..TOTAL {
        let again = paths(&cammed, frame);
        for (a, b) in before[frame as usize].iter().zip(&again) {
            assert_same(a, b);
        }
    }
}
