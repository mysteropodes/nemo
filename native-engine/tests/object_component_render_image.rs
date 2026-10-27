//! A placed component's images: a real embedded PNG item held by an inner layer, placed by the
//! baseline's `applyMatrixToStrokeData` raster branch and painted at the host slot in inner order.
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, FrameScopeKind, ProjectEnvelope,
    ProjectMetadata,
};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    object_raster_resource_versions, ordered_entries, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectPaintPlan, ObjectSceneError, PaintEntry,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::sync::Arc;

/// A solid opaque green 2x2 PNG.
const GREEN_PNG: &str = "89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001049444154789c636038c1f01f8c610c003ba4071d8da765460000000049454e44ae426082";

pub(super) fn inventory() -> AssetInventory {
    let bytes: Vec<u8> = (0..GREEN_PNG.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&GREEN_PNG[at..at + 2], 16).unwrap())
        .collect();
    let source =
        AssetSource::admitted_embedded("image/png".into(), Arc::new(bytes), 1 << 20).unwrap();
    let record = AssetRecord::new(
        AssetId::new("img-g".into()).unwrap(),
        "img-g.png".into(),
        AssetKind::Image,
        source,
    )
    .unwrap();
    AssetInventory::new(vec![Arc::new(record)], Vec::new(), 16, 0).unwrap()
}
fn timing(context: &str) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":context,"keys":[{"frame":0}],"samples":[]})
}
fn target(context: &str, layer: &str, stroke: &str) -> Value {
    json!({"contextId":context,"layerUid":layer,"strokeId":stroke,"frameScope":{"kind":"authored","frame":0}})
}
/// A red closed square on `comp`'s inner layer.
fn contour(rect: [i32; 4]) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut r = cases["records"][0].clone();
    r["target"] = target("comp", "inner", "ink");
    let [x0, y0, x1, y1] = rect;
    r["geometry"]["segments"] =
        json!([[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    r["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    r
}
/// One host layer placing `comp` (a red contour under a green image on one inner layer) by `matrix`.
fn drawing(matrix: Option<[f64; 6]>, image: Value) -> Value {
    let mut instance = json!({"contextId":"scene-root","layerUid":"host","symbolId":"comp",
        "symPlayMode":"single","symSingleFrame":0});
    if let Some(m) = matrix {
        instance["symMatrix"] = json!(m);
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
        "layers":[{"layerUid":"host","timing":timing("scene-root")}],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{
            "contextId":"scene-root","layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"comp","totalFrames":4,
                "layers":[{"layerUid":"inner","timing":timing("scene-root")}],"objects":[contour([10,10,22,22])]}],
            "instances":[instance]}},
        "items":{"raster":[{"target":target("comp","inner","img"),"item":image}]}})
}
fn image() -> Value {
    json!({"assetId":"img-g","x":16,"y":16,"width":8,"height":8,"opacity":1})
}
fn owner(drawing: &Value) -> NativeObjectHistory {
    let document = decode_storage_project(drawing.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document)
        .with_asset_inventory(Some(inventory()));
    NativeObjectHistory::from_admitted_project("component-image", project).unwrap()
}
fn selector(snapshot: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}
fn plan(owner: &NativeObjectHistory) -> Result<ObjectPaintPlan, ObjectSceneError> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = selector(&snapshot);
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins)?;
    Ok(prepare_object_paint_plan(&scene, &pins).unwrap())
}
/// The placed rasters of a plan in paint order, with whether a component contour precedes each.
fn placed(plan: &ObjectPaintPlan) -> Vec<(bool, ImageRect, f64)> {
    let mut contour_seen = false;
    let mut out = Vec::new();
    for entry in ordered_entries(plan) {
        match entry {
            PaintEntry::Component(_) => contour_seen = true,
            PaintEntry::Raster(at) => {
                let source = &plan.raster_sources()[at];
                assert!(source.placed, "only the component's image is painted");
                assert_eq!(source.layer_index, 0, "it paints at the host slot");
                out.push((contour_seen, source.source.rect, source.source.opacity));
            }
            _ => panic!("unexpected paint"),
        }
    }
    out
}
fn rect(x: f64, y: f64, width: f64, height: f64, rotation: f64) -> ImageRect {
    ImageRect {
        x,
        y,
        width,
        height,
        rotation,
    }
}

#[test]
fn a_rotated_instance_places_its_image_as_the_baseline_raster_branch_above_the_inner_contour() {
    // symMatrix [0,2,-1,0,40,8]: x' = 40 - y, y' = 2x + 8, column scales 2 and 1. The centre (16,16)
    // maps to (24,40); 8x8 becomes 16x8; the quarter turn is dropped, as `applyMatrixToStrokeData` does.
    let owner = owner(&drawing(Some([0., 2., -1., 0., 40., 8.]), image()));
    let plan = plan(&owner).unwrap();
    assert_eq!(placed(&plan), vec![(true, rect(16., 36., 16., 8., 0.), 1.)]);
    assert_eq!(
        object_raster_resource_versions(&plan).unwrap().len(),
        1,
        "the placed image's pixels are a declared scene resource"
    );
}

#[test]
fn without_a_matrix_the_stored_rect_and_rotation_stay_and_any_matrix_drops_the_rotation() {
    let mut turned = image();
    turned["rotation"] = json!(30);
    turned["opacity"] = json!(0.5);
    let kept = owner(&drawing(None, turned.clone()));
    assert_eq!(
        placed(&plan(&kept).unwrap()),
        vec![(true, rect(12., 12., 8., 8., 30.), 0.5)]
    );
    // `if(ld.symMatrix)`: even an identity matrix goes through the raster branch.
    let identity = owner(&drawing(Some([1., 0., 0., 1., 0., 0.]), turned));
    assert_eq!(
        placed(&plan(&identity).unwrap()),
        vec![(true, rect(12., 12., 8., 8., 0.), 0.5)]
    );
}

#[test]
fn the_placed_image_survives_save_and_reopen() {
    let owner = owner(&drawing(Some([0., 2., -1., 0., 40., 8.]), image()));
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let bytes = owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
    assert_eq!(
        placed(&plan(&reopened).unwrap()),
        vec![(true, rect(16., 36., 16., 8., 0.), 1.)]
    );
}

#[test]
fn component_text_and_unplaced_meshes_stay_unavailable() {
    let refused = |drawing: Value, reason: &str| match plan(&owner(&drawing)) {
        Err(ObjectSceneError::UnsupportedComponent { reason: got, .. }) => assert_eq!(got, reason),
        other => panic!("{reason}: {other:?}"),
    };
    let mut meshed = image();
    meshed["meshId"] = json!("m1");
    let mut unplaced = drawing(None, meshed.clone());
    unplaced["imageMeshes"] = json!({"m1":{
        "outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2}});
    let mut placed_mesh = unplaced.clone();
    refused(unplaced, "component image mesh is unavailable");
    // A matrix drops `meshId` with every other tag: the baseline draws the plain rect.
    placed_mesh["componentTables"]["components"]["instances"][0]["symMatrix"] =
        json!([1, 0, 0, 1, 0, 0]);
    assert_eq!(
        placed(&plan(&owner(&placed_mesh)).unwrap()),
        vec![(true, rect(12., 12., 8., 8., 0.), 1.)]
    );
    let mut text = drawing(Some([1., 0., 0., 1., 0., 0.]), image());
    text["items"]["text"] = json!([{"target":target("comp","inner","words"),"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]);
    refused(text, "component text paint is unavailable");
}

#[test]
fn the_viewport_witness_charges_the_placed_image() {
    let owner = owner(&drawing(Some([0., 2., -1., 0., 40., 8.]), image()));
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let plan = plan(&owner).unwrap();
    let witness = crate::object_render_scene::prepare_object_viewport_composition(
        &plan,
        &selector(&snapshot),
        None,
    )
    .unwrap();
    assert_eq!(witness.raster_images().count(), 1);
}

#[test]
#[ignore = "requires the explicitly coordinated real Metal/GPU validation slot"]
fn gpu_the_placed_image_paints_axis_aligned_over_the_inner_contour_in_the_export() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::{
        prepare_object_composition, prepare_scheduled_object_composition_from_witness,
    };
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let owner = owner(&drawing(Some([0., 2., -1., 0., 40., 8.]), image()));
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let plan = plan(&owner).unwrap();
    let witness = prepare_object_composition(&plan, &selector(&snapshot)).unwrap();
    let output = OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap();
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        0,
        "final",
        output,
        object_raster_resource_versions(&plan).unwrap(),
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: 0,
        scope: ObjectScheduleScope::Authored,
    };
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(snapshot, pins, key).unwrap();
    let live =
        prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness).unwrap();
    let mut gpu = Compositor::new().expect("real Metal required");
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let pixels = gpu
        .readback_scheduled_object_rgba8(&result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec();
    let at = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
    let (green, red, white) = ([0, 200, 0, 255], [255, 0, 0, 255], [255, 255, 255, 255]);
    assert_eq!(
        at(24, 40),
        green,
        "the image paints above the inner contour"
    );
    assert_eq!(at(24, 30), red, "the contour around the image");
    assert_eq!(at(17, 40), green, "the baseline's axis-aligned 16x8 rect");
    assert_eq!(at(24, 33), red, "not the quarter-turned 8x16 footprint");
    assert_eq!(at(8, 8), white);
}
