//! #2520: Text under the 3D switch. Every glyph vertex goes through the layer projector (baseline `project3DSegments` over the
//! text segments, around the union bounds of the layer), the 2D layer opacity does not apply, an edge-on turn drops the ink
//! instead of refusing the frame, and the picture survives a save and a reload.
use super::*;
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;

fn text_only(three_d: bool, z: f64, rotation_y: f64) -> Value {
    let mut doc = drawing();
    doc["objects"] = json!([]);
    doc["items"].as_object_mut().unwrap().remove("raster");
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] =
        json!([{"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
    doc["layers"][0]["threeD"] = json!(three_d);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
        {"property":"opacity","static":[40]},
        {"property":"positionZ","static":[z]},{"property":"rotationX","static":[0]},{"property":"rotationY","static":[rotation_y]}]}]);
    doc
}
fn motion_plan(o: &NativeObjectHistory) -> ObjectPaintPlan {
    let s = support::head(o);
    let pins = selector(&s, 0);
    let mut effects = None;
    let packet = prepare_object_motion_exposure_frame_for_owner(o, &s, &pins, &mut effects).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &pins).unwrap(), &pins).unwrap()
}
/// Per contour: every point row `[x, y, hix, hiy, hox, hoy]`, and the alpha of the first fill.
fn rows(plan: &ObjectPaintPlan) -> Vec<(Vec<[f64; 6]>, u8)> {
    ordered_entries(plan)
        .filter_map(|e| match e {
            PaintEntry::Text(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| {
            t.item
                .painted()
                .map(|(c, rgba)| {
                    let points = c
                        .contour
                        .segments()
                        .iter()
                        .map(|s| [s.point[0], s.point[1], s.handle_in[0], s.handle_in[1], s.handle_out[0], s.handle_out[1]])
                        .collect();
                    (points, rgba[3])
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_3d_text_block_at_depth_two_halves_every_vertex_toward_the_canvas_centre_and_ignores_the_2d_opacity() {
    let flat = rows(&motion_plan(&owner(&text_only(false, 0.0, 0.0))));
    assert_eq!(flat.len(), 3, "the three paths of the bundled A");
    assert!(flat.iter().all(|(_, alpha)| *alpha == 102), "2D: layer opacity 40% on the 255 fill");
    let level = rows(&motion_plan(&owner(&text_only(true, 0.0, 0.0))));
    // Depth zero: the 3D picture is the 2D picture (at full opacity: 3D replaces the layer's 2D Motion opacity).
    for ((a, _), (b, alpha)) in flat.iter().zip(&level) {
        assert!(a.iter().zip(b).all(|(p, q)| p.iter().zip(q).all(|(x, y)| (x - y).abs() < 1e-9)));
        assert_eq!(*alpha, 255);
    }
    // Pushed 2000 back: every point halves its distance to the 320x180 canvas centre, every handle halves.
    let far = rows(&motion_plan(&owner(&text_only(true, 2000.0, 0.0))));
    assert_eq!(far.len(), flat.len());
    for ((a, _), (b, _)) in flat.iter().zip(&far) {
        for (from, to) in a.iter().zip(b) {
            let want = [160.0 + (from[0] - 160.0) / 2.0, 90.0 + (from[1] - 90.0) / 2.0, from[2] / 2.0, from[3] / 2.0, from[4] / 2.0, from[5] / 2.0];
            assert!(want.iter().zip(to).all(|(w, g)| (w - g).abs() < 1e-6), "{to:?} != {want:?}");
        }
    }
}

#[test]
fn a_3d_text_block_turned_about_y_keeps_the_frame_and_survives_save_and_reload() {
    let o = owner(&text_only(true, 0.0, 40.0));
    let turned = rows(&motion_plan(&o));
    assert_eq!(turned.len(), 3, "a 40 degree turn keeps every contour");
    let flat = rows(&motion_plan(&owner(&text_only(true, 0.0, 0.0))));
    assert_ne!(turned[0].0, flat[0].0, "the turn moved the vertices");
    // Save and reload keep the projected block.
    let envelope = support::project(&text_only(true, 0.0, 40.0), support::images_only());
    let reopened = NativeObjectHistory::from_admitted_project("reopened-3d-text", envelope).unwrap();
    assert_eq!(rows(&motion_plan(&reopened)), turned);
    assert_eq!(support::bytes(&o), support::bytes(&reopened));
    // Edge on (a quarter turn about Y): the ink is a hair-thin line; the frame still builds and never refuses.
    let edge = rows(&motion_plan(&owner(&text_only(true, 0.0, 90.0))));
    assert!(edge.len() <= turned.len(), "{} contours at 90 degrees", edge.len());
}

#[test]
fn text_sharing_a_3d_layer_with_a_vector_and_an_image_projects_around_the_union() {
    // The shared fixture draws a vector, an image and the text on L0.
    let mut doc = drawing();
    doc["layers"][0]["threeD"] = json!(true);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
        {"property":"positionZ","static":[2000]},{"property":"rotationX","static":[0]},{"property":"rotationY","static":[0]}]}]);
    let mut flat_doc = doc.clone();
    flat_doc["layers"][0]["threeD"] = json!(false);
    flat_doc["propertyHolders"] = json!([]);
    let flat = rows(&plan(&support::head(&owner(&flat_doc)), 0));
    let far = rows(&motion_plan(&owner(&doc)));
    assert_eq!(far.len(), flat.len());
    for ((a, _), (b, _)) in flat.iter().zip(&far) {
        for (from, to) in a.iter().zip(b) {
            assert!((160.0 + (from[0] - 160.0) / 2.0 - to[0]).abs() < 1e-6 && (90.0 + (from[1] - 90.0) / 2.0 - to[1]).abs() < 1e-6);
        }
    }
}
