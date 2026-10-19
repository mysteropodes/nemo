//! Actual native owner/packet -> parent producer -> Text/raster -> viewport/export.
//! Source-only candidate: every new CPU/GPU test is UNRUN until a named booking.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, prepare_object_viewport_composition, ObjectCompositionWitness,
    PaintEntry,
};
use serde_json::{json, Value};
#[path = "object_raster_asset_commands_support.rs"]
mod support;

fn drawing() -> Value {
    let mut doc = super::tests::drawing();
    let mut parent = doc["objects"][0].clone();
    parent["target"]["layerUid"] = json!("P");
    parent["target"]["strokeId"] = json!("parent-rect");
    parent["geometry"]["segments"] = json!([
        {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
        {"point":{"x":40,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
        {"point":{"x":40,"y":20},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
        {"point":{"x":0,"y":20},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]);
    let mut layer = doc["layers"][0].clone();
    layer["layerUid"] = json!("P");
    layer["visible"] = json!(false);
    doc["layers"].as_array_mut().unwrap().push(layer);
    doc["layers"][0]["parentLayerUid"] = json!("P");
    doc["objects"] = json!([parent]);
    let context = &mut doc["compositionOrder"]["contexts"][0];
    context["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P","naturalPosition":1,"order":{}}));
    context["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}},
        {"layerUid":"L0","strokeId":"image","naturalPosition":1,"order":{}},
        {"layerUid":"P","strokeId":"parent-rect","naturalPosition":0,"order":{}}]);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"P"},"properties":[
        {"property":"position","static":[40,0]}]}]);
    doc
}
fn owner(doc: &Value) -> NativeObjectHistory {
    if doc["formatVersion"].as_u64() == Some(2) {
        let bytes = serde_json::to_vec(doc).unwrap();
        crate::object_codec::decode_ordered_project(&bytes).unwrap_or_else(|error| {
            panic!("ordered parent fixture failed before storage fallback: {error:?}")
        });
    }
    super::tests::owner(doc)
}
fn pins(snapshot: &crate::revision::ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn snapshot(o: &NativeObjectHistory) -> crate::revision::ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn plan(
    s: &crate::revision::ObjectSnapshot,
    frame: u32,
) -> crate::object_render_scene::ObjectPaintPlan {
    let p = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &p).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &p).unwrap(), &p).unwrap()
}
fn v1_drawing() -> Value {
    let mut doc = super::tests::drawing();
    doc["formatVersion"] = json!(1);
    doc.as_object_mut().unwrap().remove("compositionOrder");
    doc["layers"][0].as_object_mut().unwrap().remove("timing");
    doc
}
fn direct_scene(
    s: &crate::revision::ObjectSnapshot,
) -> crate::object_render_scene::ObjectRenderScene {
    let pin = pins(s, 0);
    let packet = crate::object_frame_packet::prepare_object_frame(s, &pin).unwrap();
    prepare_object_render_scene(&packet, &pin).unwrap()
}
fn direct_plan(s: &crate::revision::ObjectSnapshot) -> crate::object_render_scene::ObjectPaintPlan {
    prepare_object_paint_plan(&direct_scene(s), &pins(s, 0)).unwrap()
}
#[test]
fn real_v1_identity_keeps_native_scene_and_original_no_order_paint_refusal() {
    use vello::kurbo::Shape;
    let doc = v1_drawing();
    // Stored x/y are the centre (baseline serR), not the display-rect origin.
    let raw = &doc["items"]["raster"][0]["item"];
    assert_eq!(raw["x"], 40);
    assert_eq!(raw["y"], 40);
    assert_eq!(raw["width"], 2);
    assert_eq!(raw["height"], 2);
    let o = owner(&doc);
    let s = snapshot(&o);
    let before = crate::object_codec::encode_storage_project(s.document()).unwrap();
    let scene = direct_scene(&s);
    assert_eq!(s.document().format_version(), 1);
    assert!(s.document().composition_order.value().is_none());
    assert!(scene.texts.parent_chains.chain(0).unwrap().is_empty());
    assert_eq!(scene.entries().len(), 1);
    assert_eq!(scene.rasters.len(), 1);
    let raster = &scene.rasters[0].source;
    assert_eq!(raster.asset_id, "img-1");
    assert_eq!(
        (
            raster.rect.x,
            raster.rect.y,
            raster.rect.width,
            raster.rect.height
        ),
        (39.0, 39.0, 2.0, 2.0) // top-left = centre40 - extent2 / 2
    );
    assert_eq!(raster.opacity, 1.0);
    let t = &scene.texts.entries[0];
    let bounds = t
        .item
        .scene()
        .contours()
        .iter()
        .map(|c| c.contour.path().bounding_box())
        .reduce(|a, b| a.union(b))
        .unwrap();
    for (a, b) in [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .zip([10.33984375, 22.1375, 25.3515625, 39.2])
    {
        assert!((a - b).abs() < 1e-6, "original v1 glyph geometry changed");
    }
    assert!(t.item.painted().all(|(_, rgba)| rgba == [17, 34, 51, 255]));
    assert_eq!(scene.texts.resources.len(), 1);
    // Existing mixed paint requires an explicit v2 NumericOrderPlan. Preserve
    // this pre-existing availability boundary; do not invent order for raw v1.
    assert!(matches!(
        prepare_object_paint_plan(&scene, &pins(&s, 0)),
        Err(crate::object_render_scene::ObjectPaintPlanError::InvalidOrder)
    ));
    assert_eq!(
        crate::object_codec::encode_storage_project(s.document()).unwrap(),
        before
    );
    assert!(s.document().composition_order.value().is_none());
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn real_v1_parent_never_gains_a_fabricated_order_context_or_scene() {
    let mut doc = v1_drawing();
    doc["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P"}));
    doc["layers"][0]["parentLayerUid"] = json!("P");
    let input = doc.to_string().into_bytes();
    let rejected = crate::object_codec::decode_storage_project(&input).unwrap_err();
    assert_eq!(
        rejected.kind(),
        crate::object_codec::ObjectCodecErrorKind::Unsupported
    );
    assert_eq!(rejected.message(), "layer parenting is unavailable");
    assert_eq!(doc.to_string().into_bytes(), input);
}
#[test]
fn real_hidden_parent_geometry_and_actual_frame_raw_keys_move_both_consumers() {
    // Regression control for the original authoring error: order positions are
    // per layer, so P's single element cannot use L0's third global position.
    let mut gap = drawing();
    gap["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][2]["naturalPosition"] =
        json!(2);
    let error = crate::object_codec::decode_ordered_project(&serde_json::to_vec(&gap).unwrap())
        .unwrap_err();
    assert_eq!(
        error.kind(),
        crate::object_codec::ObjectCodecErrorKind::Invalid
    );
    assert_eq!(error.message(), "gapped element positions");
    eprintln!("ordered-gap control: {error:?}");
    let mut doc = drawing();
    doc["propertyHolders"][0]["properties"][0] = json!({"property":"position","track":{"keys":[
        {"frame":0,"v":[40,0],"hold":true},{"frame":7,"v":[80,10]}]}});
    let o = owner(&doc);
    let s = snapshot(&o);
    let before = crate::object_codec::encode_storage_project(s.document()).unwrap();
    for (frame, dx, dy) in [(0, 40.0, 0.0), (3, 40.0, 0.0), (7, 80.0, 10.0)] {
        let p = plan(&s, frame);
        assert_eq!(p.packet().source_frame_for_layer(1), Some(0));
        let chain = p.native_parent_chains().chain(0).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].pivot, [20.0, 10.0]);
        assert_eq!((chain[0].mat.dx, chain[0].mat.dy), (dx, dy));
        let base = super::tests::owner(&super::tests::drawing());
        let base_s = snapshot(&base);
        let base_plan = super::tests::plan(&base_s, frame);
        let first = |p: &crate::object_render_scene::ObjectPaintPlan| {
            ordered_entries(p)
                .find_map(|e| {
                    if let PaintEntry::Text(t) = e {
                        Some(t.item.scene().contours()[0].contour.segments()[0].point)
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        let a = first(&p);
        let b = first(&base_plan);
        assert!((a[0] - b[0] - dx).abs() < 1e-9);
        assert!((a[1] - b[1] - dy).abs() < 1e-9);
        assert_eq!(p.raster_sources().len(), 1); // real raster consumer uses this SAME plan chain.
    }
    assert_eq!(
        crate::object_codec::encode_storage_project(s.document()).unwrap(),
        before
    );
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn own_layer_opacity_is_not_multiplied_twice_and_parent_opacity_is_real() {
    let mut doc = drawing();
    doc["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"opacity","static":[50]}));
    doc["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(json!({"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"opacity","static":[50]}]}));
    let o = owner(&doc);
    let s = snapshot(&o);
    let p = plan(&s, 7);
    let chain = p.native_parent_chains().chain(0).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].mat.op, 0.5);
    let text = ordered_entries(&p)
        .find_map(|e| {
            if let PaintEntry::Text(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    assert!(text
        .item
        .scene()
        .contours()
        .iter()
        .all(|c| c.opacity == 0.25));
    assert!(text.item.painted().all(|(_, rgba)| rgba[3] == 64));
    assert_eq!(p.raster_sources()[0].source.opacity, 0.5);
}
#[test]
fn authoritative_parent_rotation_scale_anchor_and_negative_scale_use_real_pivot() {
    let mut doc = drawing();
    doc["propertyHolders"][0]["properties"] = json!([
        {"property":"position","static":[7,9]},{"property":"anchor","static":[3,-2]},
        {"property":"rotation","static":[90]},{"property":"scale","static":[-100,200]}]);
    let o = owner(&doc);
    let s = snapshot(&o);
    let p = plan(&s, 0);
    let c = p.native_parent_chains().chain(0).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].pivot, [23.0, 8.0]);
    assert_eq!(c[0].mat.sx, -1.0);
    assert_eq!(c[0].mat.sy, 2.0);
    // Literal D30 arithmetic at a point one unit right of the real pivot:
    // mirror x -> -1, rotate90 -> (0,-1), then translate (7,9).
    let moved = crate::evaluation::motion_parent_chain::apply_chain_to_segments(
        &[crate::evaluation::motion_parent_chain::Segment {
            point: [24.0, 8.0],
            handle_in: None,
            handle_out: None,
        }],
        &c,
    );
    assert!((moved[0].point[0] - 30.0).abs() < 1e-9);
    assert!((moved[0].point[1] - 16.0).abs() < 1e-9);
}
#[test]
fn no_parent_or_pose_keeps_the_original_native_geometry_and_alpha() {
    let mut doc = drawing();
    doc["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("parentLayerUid");
    doc.as_object_mut().unwrap().remove("propertyHolders");
    let o = owner(&doc);
    let s = snapshot(&o);
    let p = plan(&s, 7);
    assert!(p.native_parent_chains().chain(0).unwrap().is_empty());
    let t = ordered_entries(&p)
        .find_map(|e| {
            if let PaintEntry::Text(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    assert!(t.item.scene().contours().iter().all(|c| c.opacity == 1.0));
}
#[test]
fn missing_parent_cycle_and_enabled_expression_never_publish_a_scene() {
    for fault in ["missing", "cycle", "expression", "group"] {
        let mut doc = drawing();
        match fault {
            "missing" => doc["layers"][0]["parentLayerUid"] = json!("absent"),
            "cycle" => doc["layers"][1]["parentLayerUid"] = json!("L0"),
            "expression" => {
                doc["propertyHolders"][0]["properties"][0]["expression"] =
                    json!({"enabled":true,"code":"1"})
            }
            _ => {
                doc["layers"][1]["groups"] =
                    json!({"contextId":"scene-root","entries":[],"members":[]})
            }
        }
        // Malformed graphs now refuse atomically at storage, before any owner or scene.
        if fault == "missing" || fault == "cycle" {
            let bytes = serde_json::to_vec(&doc).unwrap();
            let error = crate::object_codec::decode_ordered_project(&bytes).unwrap_err();
            assert_eq!(
                error.kind(),
                crate::object_codec::ObjectCodecErrorKind::Invalid,
                "{fault}"
            );
            assert!(crate::object_codec::decode_storage_project(&bytes).is_err());
            assert_eq!(serde_json::to_vec(&doc).unwrap(), bytes);
            continue;
        }
        // Storage-valid pose/context faults must still reach the actual frame/scene refusal.
        crate::object_codec::decode_ordered_project(&serde_json::to_vec(&doc).unwrap())
            .unwrap_or_else(|error| panic!("{fault} fixture admission: {error:?}"));
        let o = owner(&doc);
        let s = snapshot(&o);
        let before = crate::object_codec::encode_storage_project(s.document()).unwrap();
        let pin = pins(&s, 7);
        match prepare_object_exposure_frame(&s, &pin) {
            Err(_) => {}
            Ok(packet) => assert!(
                prepare_object_render_scene(&packet, &pin).is_err(),
                "{fault}"
            ),
        }
        assert_eq!(
            crate::object_codec::encode_storage_project(s.document()).unwrap(),
            before
        );
    }
}

#[cfg(feature = "test-compositor")]
#[path = "object_scene_parent_chain_pixels.rs"]
mod actual_pixels;
