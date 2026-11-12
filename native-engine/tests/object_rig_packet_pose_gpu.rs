//! L03-P (#2252) / L07-L (#2230) mesh part, Metal proof: a stored rig pose visibly deforms a mesh-bound
//! raster, and the viewport pixels equal the exported PNG pixels (both read the one retained witness).
//! Needs the explicitly coordinated Metal slot (`--ignored`).
//!
//! Hand computation (nothing is copied from the output). Canvas 1920x1080, project scale 1. The raster is the
//! 64x64 corpus image (left half red (220,30,30), right half blue (30,60,220)) centred at (320,180), 200x200,
//! so it covers x 220..420, y 80..280 with the red/blue seam at x = 320. Mesh `im_1` is the corpus fan (the
//! outline is the whole square), bound with weight 1 to control point 0 of one bone. The bone moves by
//! (+100, +50), so every vertex lands 100 right and 50 down: the posed image covers x 320..520, y 130..330
//! with its seam at x = 420. Probes (all well away from texel edges):
//!   (270,180): rest red;    posed outside the image -> canvas white
//!   (370,230): rest blue;   posed u = (370-320)/200 = .25 -> red
//!   (470,230): rest outside -> white; posed u = .75 -> blue
//!   (370,100): rest blue;   posed y < 130 -> outside -> white
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, ProjectEnvelope, ProjectMetadata,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::pixel_consumer::{composition, exported, pixels};

const RED: [u8; 3] = [220, 30, 30];
const BLUE: [u8; 3] = [30, 60, 220];
const WHITE: [u8; 3] = [255, 255, 255];

fn rig(dx: f64, dy: f64) -> Value {
    let seg = |x: f64, y: f64| json!({"point":[x,y],"handleIn":[0,0],"handleOut":[0,0]});
    let rest: Vec<[f64; 2]> = fixtures::mesh()["verts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| [220.0 + v[0].as_f64().unwrap() * 200.0, 80.0 + v[1].as_f64().unwrap() * 200.0])
        .collect();
    json!({"bones":{"b1":{
        "segments":[seg(320.0 + dx, 180.0 + dy), seg(420.0 + dx, 180.0 + dy)],
        "restSegments":[seg(320.0, 180.0), seg(420.0, 180.0)], "closed": false}},
        "ikChains":{},"nextId":2,
        "binds":[{"meshId":"im_1","rest":rest,
            "weights":rest.iter().map(|_| json!([{"boneId":"b1","ptIndex":0,"w":1}])).collect::<Vec<_>>(),
            "rotate":false}]})
}

fn owner(rig: Option<Value>) -> NativeObjectHistory {
    let mut layer = json!({"layerUid":"L0"});
    if let Some(rig) = rig {
        layer["rig"] = rig;
    }
    let target = json!({"contextId":"scene-root","layerUid":"L0",
        "frameScope":{"kind":"authored","frame":0},"strokeId":"raster-0"});
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,
        "layers":[layer],"objects":[],
        "items":{"raster":[{"target":target,"item":{"assetId":"img-1","x":320,"y":180,
            "width":200,"height":200,"meshId":"im_1"}}]},
        "imageMeshes":{"im_1": fixtures::mesh()},
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":[{"layerUid":"L0","strokeId":"raster-0","naturalPosition":0,"order":{}}]}]}]}});
    let source = AssetSource::admitted_embedded(
        "image/png".into(),
        Arc::new(include_bytes!("../../tests/fixtures/mesh/assets/image.png").to_vec()),
        1 << 20,
    )
    .unwrap();
    let asset = Arc::new(
        AssetRecord::new(
            AssetId::new("img-1".into()).unwrap(),
            "image.png".into(),
            AssetKind::Image,
            source,
        )
        .unwrap(),
    );
    let inventory = AssetInventory::new(vec![asset], Vec::new(), 16, 0).unwrap();
    let project = ProjectEnvelope::no_media(
        ProjectMetadata::new(1920, 1080, 24.).unwrap(),
        decode_storage_project(drawing.to_string().as_bytes()).unwrap(),
    )
    .with_asset_inventory(Some(inventory));
    NativeObjectHistory::from_admitted_project("rig-mesh-gpu", project).unwrap()
}

fn rgb(bytes: &[u8], x: usize, y: usize, expected: [u8; 3], label: &str) {
    let at = (y * 1920 + x) * 4;
    for c in 0..3 {
        assert!(
            bytes[at + c].abs_diff(expected[c]) <= 2,
            "{label}: pixel {x},{y} channel {c}: {} != {}",
            bytes[at + c],
            expected[c]
        );
    }
}

#[test]
fn the_scene_and_the_export_witnesses_hold_the_same_posed_mesh_payload() {
    let owner = owner(Some(rig(100.0, 50.0)));
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let viewport = composition(&snapshot, 0, true, 1.);
    let export = composition(&snapshot, 0, false, 1.);
    let scene = viewport.rasters.as_ref().unwrap().mesh(0).expect("viewport mesh").1;
    let out = export.rasters.as_ref().unwrap().mesh(0).expect("export mesh").1;
    assert_eq!(scene.payload(), out.payload(), "one posed geometry for both consumers");
    // The input of both consumers is the one evaluation held by the shared plan.
    let posed = viewport.plan().raster_sources()[0].rig_offsets().expect("posed");
    assert_eq!(Some(posed), export.plan().raster_sources()[0].rig_offsets());
    // Hand computation: every vertex moved by (+100,+50) in world pixels = (+.5,+.25) in rect units.
    for o in posed {
        assert!((o[0] - 0.5).abs() < 1e-12 && (o[1] - 0.25).abs() < 1e-12, "{o:?}");
    }
    let verts = &scene.payload().verts;
    assert_eq!(verts[0], [320.0, 130.0]);
    assert_eq!(verts[4], [520.0, 330.0]);
    // And it differs from the rest pose of the same snapshot's stored mesh.
    let rest = owner_rest();
    let rest_scene = rest.0.rasters.as_ref().unwrap().mesh(0).unwrap().1;
    assert_eq!(rest_scene.payload().verts[0], [220.0, 80.0]);
}

fn owner_rest() -> (crate::object_render_scene::ObjectCompositionWitness,) {
    let owner = owner(None);
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    (composition(&snapshot, 0, true, 1.),)
}

#[test]
#[ignore = "requires the explicitly coordinated real Metal/GPU validation slot"]
fn gpu_a_rig_pose_deforms_the_mesh_bound_raster_and_viewport_pixels_equal_export_pixels() {
    let mut gpu = Compositor::new().expect("the pixel criterion requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    // Control: the same document without a rig draws the stored (rest) mesh.
    let plain = owner(None);
    let plain_snapshot = plain.acquire_snapshot(plain.content_revision()).unwrap();
    let rest = pixels(&mut gpu, &composition(&plain_snapshot, 0, true, 1.));
    rgb(&rest, 270, 180, RED, "rest");
    rgb(&rest, 370, 230, BLUE, "rest");
    rgb(&rest, 470, 230, WHITE, "rest");
    rgb(&rest, 370, 100, BLUE, "rest");
    // A rig whose bone did not move poses nothing: pixels equal the rest control.
    let still = owner(Some(rig(0.0, 0.0)));
    let still_snapshot = still.acquire_snapshot(still.content_revision()).unwrap();
    let unmoved = pixels(&mut gpu, &composition(&still_snapshot, 0, true, 1.));
    assert!(rest.iter().zip(&unmoved).all(|(a, b)| a.abs_diff(*b) <= 1));
    // The posed document.
    let posed_owner = owner(Some(rig(100.0, 50.0)));
    let snapshot = posed_owner.acquire_snapshot(posed_owner.content_revision()).unwrap();
    let viewport = composition(&snapshot, 0, true, 1.);
    assert!(viewport.raster_image(0).is_none(), "a native mesh cannot expose the flat draw");
    let shown = pixels(&mut gpu, &viewport);
    rgb(&shown, 270, 180, WHITE, "posed");
    rgb(&shown, 370, 230, RED, "posed");
    rgb(&shown, 470, 230, BLUE, "posed");
    rgb(&shown, 370, 100, WHITE, "posed");
    let changed = rest.chunks_exact(4).zip(shown.chunks_exact(4)).filter(|(a, b)| a != b).count();
    assert!(changed > 20_000, "the pose visibly moved the image: {changed} pixels differ");
    // Viewport pixels equal export pixels (the real export job and PNG encoder).
    assert_eq!(shown, exported(&posed_owner, &mut gpu, &viewport, 0, "rig-mesh"));
}
