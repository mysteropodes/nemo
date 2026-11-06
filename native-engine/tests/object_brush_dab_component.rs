//! #2361 B-comp-edit-rest: a seeded brush-texture anchor INSIDE a placed component paints its dabs, right after the placed anchor and
//! through the instance placement, exactly the dabs the same anchor paints at the scene root (the baseline copies the stored dabs of a
//! symbol layer through `applyMatrixToStrokeData`; the scene brush producer used to skip component contours, leaving only the camouflaged
//! anchor). Every plan comes from the real codec, history owner, frame packet, render scene and paint plan.
use super::{line_record, plan_of, seeded};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan, PaintEntry,
};
use serde_json::{json, Value};

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}
fn layers(uids: &[&str]) -> Value {
    json!(uids.iter().enumerate().map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}})).collect::<Vec<_>>())
}
/// `comp-a` holds one inner layer with `records` at frame 0; the scene has one host layer per entry of `instances`.
fn component_plan(records: Vec<Value>, instances: Vec<Value>, opacity: Option<f64>) -> ObjectPaintPlan {
    let hosts: Vec<String> = (0..instances.len()).map(|i| format!("host-{i}")).collect();
    let host_refs: Vec<&str> = hosts.iter().map(String::as_str).collect();
    let moved: Vec<Value> = records
        .into_iter()
        .map(|mut r| {
            r["target"]["contextId"] = json!("comp-a");
            r["target"]["layerUid"] = json!("inner");
            r
        })
        .collect();
    let elements: Vec<Value> = moved.iter().enumerate().map(|(k, r)| json!({"layerUid":"inner","strokeId":r["target"]["strokeId"],"naturalPosition":k,"order":{}})).collect();
    let inner = json!({"layerUid":"inner","timing":timing()});
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":hosts.iter().map(|h| json!({"layerUid":h,"timing":timing()})).collect::<Vec<_>>(),"objects":[]});
    doc["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
            {"contextId":"scene-root","layers":layers(&host_refs),"scopes":[]},
            {"contextId":"comp-a","layers":layers(&["inner"]),"scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]});
    if let Some(value) = opacity {
        doc["propertyHolders"] = json!([{"target":{"contextId":"comp-a","layerUid":"inner"},"properties":[{"property":"opacity","static":[value]}]}]);
    }
    doc["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-a","totalFrames":8,"layers":[inner],"objects":moved}],
        "instances":hosts.iter().zip(instances).map(|(h, extra)| {
            let mut i = json!({"contextId":"scene-root","layerUid":h,"symbolId":"comp-a","symPlayMode":"loop"});
            if let Some(map) = extra.as_object() { for (k, v) in map { i[k] = v.clone(); } }
            i }).collect::<Vec<_>>()}});
    let history = NativeObjectHistory::new("dabs-component", decode_storage_project(&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(), document_id: snapshot.document_id(), content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(), context_id: "scene-root", scope_kind: FrameScopeKind::Authored, frame: 0,
    };
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
fn dab_paths(plan: &ObjectPaintPlan) -> Vec<vello::kurbo::BezPath> {
    ordered_entries(plan).filter_map(|e| match e { PaintEntry::Dab(d) => Some(d.path.clone()), _ => None }).collect()
}
fn dab_alphas(plan: &ObjectPaintPlan) -> Vec<f64> {
    ordered_entries(plan).filter_map(|e| match e { PaintEntry::Dab(d) => d.svg_rgba.map(|c| c[3]), _ => None }).collect()
}

#[test]
fn a_placed_anchor_paints_the_dabs_the_scene_paints_for_the_same_anchor() {
    let scene = plan_of(line_record(Some(seeded()), false)).unwrap();
    let want = dab_paths(&scene);
    assert!(want.len() > 3, "the oracle anchor stamps several dabs");
    let plan = component_plan(vec![line_record(Some(seeded()), false)], vec![json!({})], None);
    let entries: Vec<_> = ordered_entries(&plan).collect();
    assert!(matches!(entries[0], PaintEntry::Component(_)), "the placed (camouflaged) anchor first, then its dabs");
    assert_eq!(dab_paths(&plan), want, "identity placement: the same dabs, in the same order");
    assert_eq!(dab_alphas(&plan), dab_alphas(&scene), "and the same opacities");
    assert_eq!(entries.len(), 1 + want.len());
}

#[test]
fn every_instance_places_its_own_dabs_and_the_instance_matrix_moves_them() {
    let plan = component_plan(vec![line_record(Some(seeded()), false)], vec![json!({}), json!({"symMatrix":[1,0,0,1,40,10]})], None);
    let all = dab_paths(&plan);
    let each = all.len() / 2;
    assert_eq!(all.len(), each * 2, "two instances, the same count each");
    let (plain, moved) = (&all[..each], &all[each..]);
    for (a, b) in plain.iter().zip(moved) {
        let (ba, bb) = (vello::kurbo::Shape::bounding_box(a), vello::kurbo::Shape::bounding_box(b));
        assert!((bb.x0 - ba.x0 - 40.0).abs() < 1e-6 && (bb.y0 - ba.y0 - 10.0).abs() < 1e-6, "{ba:?} -> {bb:?}");
        assert!(((bb.x1 - bb.x0) - (ba.x1 - ba.x0)).abs() < 1e-6, "a translation does not resize a dab");
    }
}

#[test]
fn the_inner_layer_opacity_scales_the_dabs_and_an_unseeded_or_stored_anchor_paints_none() {
    let full = component_plan(vec![line_record(Some(seeded()), false)], vec![json!({})], None);
    let half = component_plan(vec![line_record(Some(seeded()), false)], vec![json!({})], Some(50.0));
    for (a, b) in dab_alphas(&full).iter().zip(dab_alphas(&half)) {
        assert!((b - a * 0.5).abs() < 1e-9, "{a} -> {b}");
    }
    let none = component_plan(vec![line_record(Some(json!({"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1})), false)], vec![json!({})], None);
    assert!(dab_paths(&none).is_empty(), "an unseeded anchor");
    let mut stored = line_record(Some(json!({"brushGroupId":"g","isBrushTextureCopy":true})), false);
    stored["target"]["strokeId"] = json!("stored-dab");
    stored["vector"]["opacity"] = json!(0.5);
    let plan = component_plan(vec![line_record(Some(seeded()), false), stored], vec![json!({})], None);
    assert!(dab_paths(&plan).is_empty(), "the group's dabs are stored records: nothing is stamped twice");
}
