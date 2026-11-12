//! #2520: a duplicator host under the 3D switch. Every clone goes through the layer projector around the union bounds of ALL its
//! clones, and a clone carrying a `dup3D` delta (positionZ offset) gets its own projector (baseline `dup3DProjectorCache`).
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::duplicators::tests::fixture;
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;

fn owner(value: &serde_json::Value) -> NativeObjectHistory {
    let document = decode_storage_project(value.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.).unwrap(), document);
    NativeObjectHistory::from_admitted_project("dup-3d", project).unwrap()
}
fn doc(three_d: bool, step_z: f64, z: f64) -> serde_json::Value {
    let mut value = fixture(1);
    value["layers"][0]["threeD"] = json!(three_d);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
        {"property":"dupOffsetPosZ","static":[step_z]},
        {"property":"positionZ","static":[z]},{"property":"rotationX","static":[0]},{"property":"rotationY","static":[0]}]}]);
    value
}
fn starts(o: &NativeObjectHistory) -> Vec<[f64; 2]> {
    let s = o.acquire_snapshot(0).unwrap();
    let pin = selector(&s, 2);
    let mut effects = None;
    let packet = prepare_object_motion_exposure_frame_for_owner(o, &s, &pin, &mut effects).unwrap();
    let plan = prepare_object_paint_plan(&prepare_object_render_scene(&packet, &pin).unwrap(), &pin).unwrap();
    ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => match copy.path.elements()[0] {
                PathEl::MoveTo(p) => [p.x, p.y],
                _ => panic!("path start"),
            },
            _ => panic!("only clones paint"),
        })
        .collect()
}

#[test]
fn each_clone_of_a_3d_host_takes_its_own_depth_delta_around_the_canvas_centre() {
    let flat = starts(&owner(&doc(false, 500.0, 0.0)));
    assert_eq!(flat.len(), 3);
    let projected = starts(&owner(&doc(true, 500.0, 0.0)));
    assert_eq!(projected.len(), 3);
    for (k, (from, to)) in flat.iter().zip(&projected).enumerate() {
        // Clone k sits k * 500 further back: depth (2000 + 500 k) / 2000, rotation and Position zero, canvas centre (50, 50).
        let depth = (2000.0 + 500.0 * k as f64) / 2000.0;
        let want = [50.0 + (from[0] - 50.0) / depth, 50.0 + (from[1] - 50.0) / depth];
        assert!((want[0] - to[0]).abs() < 1e-9 && (want[1] - to[1]).abs() < 1e-9, "clone {k}: {to:?} != {want:?}");
    }
    // The layer's own depth adds to every clone's delta.
    let pushed = starts(&owner(&doc(true, 500.0, 2000.0)));
    for (k, (from, to)) in flat.iter().zip(&pushed).enumerate() {
        let depth = (4000.0 + 500.0 * k as f64) / 2000.0;
        let want = [50.0 + (from[0] - 50.0) / depth, 50.0 + (from[1] - 50.0) / depth];
        assert!((want[0] - to[0]).abs() < 1e-9 && (want[1] - to[1]).abs() < 1e-9, "clone {k}: {to:?} != {want:?}");
    }
}

#[test]
fn a_3d_host_with_no_depth_offsets_projects_every_clone_alike_and_keeps_the_frame() {
    let flat = starts(&owner(&doc(false, 0.0, 0.0)));
    let level = starts(&owner(&doc(true, 0.0, 0.0)));
    assert_eq!(flat, level, "depth zero is the flat picture");
}
