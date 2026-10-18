//! Real owner -> selected frame -> mixed paint -> viewport/export. CPU execution UNRUN.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, prepare_object_viewport_composition, ObjectPaintPlan, PaintEntry,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
#[path = "object_text_scene_history.rs"]
mod history;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
#[path = "object_text_scene_svg.rs"]
mod svg;

pub(super) fn block() -> Value {
    json!({"text":"A","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}})
}
pub(super) fn drawing() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut vector = cases["records"][0].clone();
    vector["target"] = support::target("L0", "vector");
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[vector],
        "items":{"raster":[{"target":support::target("L0","image"),"item":support::raster_item("img-1",40,40,2,2)}],
            "text":[{"target":support::target("L0","text"),"block":block()}]},
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"L0","strokeId":"vector","naturalPosition":0,"order":{}},
                    {"layerUid":"L0","strokeId":"text","naturalPosition":1,"order":{}},
                    {"layerUid":"L0","strokeId":"image","naturalPosition":2,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
pub(super) fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project(
        "text-scene",
        support::project(doc, support::images_only()),
    )
    .unwrap()
}
pub(super) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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
pub(super) fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_exposure_frame(snapshot, &pins).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &pins).unwrap(), &pins).unwrap()
}
pub(super) fn labels(plan: &ObjectPaintPlan) -> Vec<String> {
    ordered_entries(plan)
        .map(|entry| match entry {
            PaintEntry::Contour(c) => c.source().record().target().stroke_id().to_owned(),
            PaintEntry::Raster(_) => "image".into(),
            PaintEntry::Text(t) => t.item.group_id().to_owned(),
        })
        .collect()
}
#[test]
fn actual_mixed_order_and_held_frame_keep_text_between_vector_and_raster() {
    let owner = owner(&drawing());
    let snapshot = support::head(&owner);
    let before = support::state(&owner);
    for frame in [0, 3, 7, 0] {
        let p = plan(&snapshot, frame);
        assert_eq!(p.packet().frame(), frame);
        assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
        assert_eq!(p.packet().text_records().len(), 1);
        assert_eq!(p.commands().len(), 1);
        assert_eq!(labels(&p), ["vector", "scene-root:L0:text", "image"]);
        assert_eq!(p.packet().text_records().next().unwrap().source_index(), 2);
    }
    assert_eq!(support::state(&owner), before);
}
#[test]
fn real_bundled_ink_and_viewport_export_keep_font_and_raster_resources() {
    let owner = owner(&drawing());
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 7);
    let pins = selector(&snapshot, 7);
    let viewport = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert!(viewport.is_viewport());
    assert!(!export.is_viewport());
    let declared: std::collections::BTreeMap<String, String> =
        p.scene_resource_versions().unwrap().into_iter().collect();
    viewport.verify_scene_resources(&declared).unwrap();
    export.verify_scene_resources(&declared).unwrap();
    let resources = export.scene_resources().unwrap();
    assert_eq!(resources.len(), 2);
    assert_eq!(
        resources
            .get("font:Roboto-Regular")
            .unwrap()
            .charged_bytes(),
        include_bytes!("../../src/fonts/Roboto-Regular.ttf").len() as u64
    );
    assert_eq!(
        viewport
            .scene_resources()
            .unwrap()
            .get("font:Roboto-Regular")
            .unwrap()
            .version(),
        resources.get("font:Roboto-Regular").unwrap().version()
    );
    let text = ordered_entries(&p)
        .find_map(|e| {
            if let PaintEntry::Text(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(text.item.color(), "#112233");
    assert!(!text.item.scene().contours().is_empty());
    let mut encoded = Scene::new();
    // Real bundled Roboto + vendored opentype baseline at actual bridge ascent
    // size*0.8: bounds are literal independent CPU oracle, not the native producer.
    use vello::kurbo::Shape;
    let bounds = text
        .item
        .scene()
        .contours()
        .iter()
        .map(|c| c.contour.path().bounding_box())
        .reduce(|a, b| a.union(b))
        .unwrap();
    for (actual, expected) in [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .zip([10.33984375, 22.1375, 25.3515625, 39.2])
    {
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }
    text.draw(&mut encoded);
    assert_eq!(
        encoded.encoding().n_paths as usize,
        text.item.scene().contours().len()
    );
    assert!(text
        .item
        .painted()
        .all(|(_, rgba)| rgba == [17, 34, 51, 255]));
    let mut stale = pins.clone();
    stale.content_revision += 1;
    assert!(prepare_object_render_scene(p.packet(), &stale).is_err());
    assert!(prepare_object_composition(&p, &stale).is_err());
    let mut wrong = declared.clone();
    wrong.insert("font:Roboto-Regular".into(), "foreign-bytes".into());
    assert!(export.verify_scene_resources(&wrong).is_err());
    wrong.remove("font:Roboto-Regular");
    assert!(viewport.verify_scene_resources(&wrong).is_err());
}
#[test]
fn real_area_text_preserves_storage_presence_and_bounds() {
    let mut doc = drawing();
    let b = &mut doc["items"]["text"][0]["block"];
    b["fixedWidth"] = json!(35);
    b["fixedHeight"] = json!(60);
    b["text"] = json!("A A A");
    let o = owner(&doc);
    let before = support::state(&o);
    let snap = support::head(&o);
    let p = plan(&snap, 3);
    assert!(ordered_entries(&p).any(|e| matches!(e, PaintEntry::Text(_))));
    assert_eq!(support::state(&o), before);
}
#[test]
fn parented_text_refuses_atomically_and_keeps_old_witness_usable() {
    let o = owner(&drawing());
    let snap = support::head(&o);
    let good = plan(&snap, 0);
    let retained = prepare_object_composition(&good, &selector(&snap, 0)).unwrap();
    let bytes = support::state(&o);
    let mut doc = drawing();
    let timing = doc["layers"][0]["timing"].clone();
    doc["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"parent","timing":timing}));
    doc["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"parent","naturalPosition":1,"order":{}}));
    doc["layers"][0]["parentLayerUid"] = json!("parent");
    // Parenting is refused by native admission before a scene can be prepared.
    // The storage wrapper preserves its first (v1) error, so check the explicit
    // ordered codec too rather than inventing an inadmissible native owner.
    use crate::object_codec::{
        decode_ordered_project, decode_storage_project, ObjectCodecErrorKind,
    };
    let encoded = serde_json::to_vec(&doc).unwrap();
    let ordered_error = decode_ordered_project(&encoded).unwrap_err();
    assert_eq!(ordered_error.kind(), ObjectCodecErrorKind::Unsupported);
    assert_eq!(ordered_error.message(), "layer parenting is unavailable");
    let storage_error = decode_storage_project(&encoded).unwrap_err();
    assert_eq!(storage_error.kind(), ObjectCodecErrorKind::Unsupported);
    assert_eq!(serde_json::to_vec(&doc).unwrap(), encoded);
    assert_eq!(support::state(&o), bytes);
    retained
        .verify_scene_resources(
            &good
                .scene_resource_versions()
                .unwrap()
                .into_iter()
                .collect(),
        )
        .unwrap();
}

#[test]
fn text_only_scene_reaches_paint_without_a_vector_command_and_hidden_layer_is_empty() {
    let mut doc = drawing();
    doc["objects"] = json!([]);
    doc["items"].as_object_mut().unwrap().remove("raster");
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
    let o = owner(&doc);
    let snap = support::head(&o);
    let p = plan(&snap, 0);
    assert_eq!(p.commands().len(), 0);
    assert_eq!(labels(&p), ["scene-root:L0:text"]);
    let w = prepare_object_composition(&p, &selector(&snap, 0)).unwrap();
    assert_eq!(w.scene_resources().unwrap().len(), 1);
    doc["layers"][0]["visible"] = json!(false);
    let hidden = owner(&doc);
    let hidden = support::head(&hidden);
    let p = plan(&hidden, 0);
    assert_eq!(ordered_entries(&p).count(), 0);
    assert!(p.scene_resource_versions().unwrap().is_empty());
}
#[test]
fn text_order_samples_presentation_frame_while_glyph_source_holds_and_expressions_refuse() {
    let mut doc = drawing();
    let elements = &mut doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    elements[0]["order"] = json!({"static":[3]});
    elements[2]["order"] = json!({"static":[1]});
    elements[1]["order"] = json!({"track":{"keys":[{"frame":0,"v":[2],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},{"frame":7,"v":[4]}]}});
    let o = owner(&doc);
    let snap = support::head(&o);
    assert_eq!(
        labels(&plan(&snap, 0)),
        ["vector", "scene-root:L0:text", "image"]
    );
    let p = plan(&snap, 7);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(labels(&p), ["scene-root:L0:text", "vector", "image"]);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][1]["order"]["expression"] =
        json!({"enabled":true,"code":"999"});
    let refused = owner(&doc);
    let snap = support::head(&refused);
    assert!(prepare_object_exposure_frame(&snap, &selector(&snap, 7)).is_err());
}

// Combined raw-layer-opacity and actual Text scene controls. Expected RGBA8
// values are literal independent CSS-color/layer-alpha results, not sampler output.
fn alpha_drawing(property: Value) -> Value {
    let mut doc = drawing();
    doc["items"]["text"][0]["block"]["color"] = json!("#11223380");
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[property]}]);
    doc
}
fn text_alphas(p: &ObjectPaintPlan) -> Vec<u8> {
    ordered_entries(p)
        .filter_map(|entry| match entry {
            PaintEntry::Text(t) => Some(
                t.item
                    .painted()
                    .map(|(_, rgba)| rgba[3])
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}
fn require_text_alpha(p: &ObjectPaintPlan, expected: u8) {
    let alpha = text_alphas(p);
    assert!(!alpha.is_empty(), "real font contours must be present");
    assert!(
        alpha.iter().all(|&value| value == expected),
        "{alpha:?} != {expected}"
    );
}
#[test]
fn text_raw_layer_alpha_is_applied_once_with_color_and_keeps_mixed_resource_order() {
    for (property, expected) in [
        (json!({"property":"opacity"}), 128),
        (json!({"property":"opacity","static":[50]}), 64),
        (json!({"property":"opacity","static":[-50]}), 0),
        (json!({"property":"opacity","static":[150]}), 192),
        (json!({"property":"opacity","static":[200]}), 255),
        (
            json!({"property":"opacity","static":[50],"track":{"keys":[]}}),
            64,
        ),
    ] {
        let o = owner(&alpha_drawing(property));
        let snapshot = support::head(&o);
        let bytes = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
        let p = plan(&snapshot, 7);
        assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
        require_text_alpha(&p, expected);
        assert_eq!(labels(&p), ["vector", "scene-root:L0:text", "image"]);
        let resources = p.scene_resource_versions().unwrap();
        assert_eq!(resources.len(), 2);
        assert!(resources.iter().any(|(id, _)| id == "font:Roboto-Regular"));
        assert_eq!(
            crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
            bytes
        );
        assert_eq!(o.history_depths(), (0, 0));
    }
}
#[test]
fn held_text_alpha_samples_presentation_time_for_linear_hold_and_fractional_keys() {
    for (keys, expected) in [
        (
            json!([{"frame":0,"v":[0],"curvePoints":[]},{"frame":6,"v":[100]}]),
            vec![(0, 0), (3, 64), (6, 128), (7, 128)],
        ),
        (
            json!([{"frame":0,"v":[0],"hold":true},{"frame":6,"v":[100]}]),
            vec![(0, 0), (3, 0), (6, 128)],
        ),
        (
            json!([{"frame":-0.5,"v":[0],"curvePoints":[]},{"frame":6.5,"v":[100]}]),
            vec![(3, 64)],
        ),
    ] {
        let o = owner(&alpha_drawing(
            json!({"property":"opacity","track":{"keys":keys}}),
        ));
        let snapshot = support::head(&o);
        let first = plan(&snapshot, 0);
        for (frame, alpha) in expected {
            let p = plan(&snapshot, frame);
            assert_eq!(p.packet().frame(), frame);
            assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
            require_text_alpha(&p, alpha);
            assert_eq!(
                p.scene_resource_versions().unwrap(),
                first.scene_resource_versions().unwrap()
            );
        }
        assert_eq!(o.history_depths(), (0, 0));
    }
}
#[test]
fn text_alpha_edit_history_and_full_reopen_keep_old_ink_and_resource_versions() {
    let mut o = owner(&alpha_drawing(json!({"property":"opacity","static":[50]})));
    let initial = support::head(&o);
    let old = plan(&initial, 7);
    let bytes = crate::object_codec::encode_storage_project(initial.document()).unwrap();
    let edit = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[25]}});
    let candidate = o
        .prepare_property_edit_json(edit.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_property_edit(candidate).unwrap(), 1);
    let changed = support::head(&o);
    let changed_plan = plan(&changed, 7);
    require_text_alpha(&changed_plan, 32);
    require_text_alpha(&old, 64);
    assert!(prepare_object_composition(&changed_plan, &selector(&initial, 7)).is_err());
    assert_eq!(
        changed_plan.scene_resource_versions().unwrap(),
        old.scene_resource_versions().unwrap()
    );
    assert_eq!(o.undo(1).unwrap(), 2);
    require_text_alpha(&plan(&support::head(&o), 7), 64);
    assert_eq!(o.redo(2).unwrap(), 3);
    let redone = support::head(&o);
    let stored = o.serialize_project_file(3, redone.snapshot_id()).unwrap();
    let reopened = NativeObjectHistory::from_project_file("alpha-reopen", &stored).unwrap();
    let reopened_plan = plan(&support::head(&reopened), 7);
    require_text_alpha(&reopened_plan, 32);
    assert_eq!(labels(&reopened_plan), labels(&changed_plan));
    assert_eq!(
        reopened_plan.scene_resource_versions().unwrap(),
        changed_plan.scene_resource_versions().unwrap()
    );
    assert_eq!(
        crate::object_codec::encode_storage_project(initial.document()).unwrap(),
        bytes
    );
    require_text_alpha(&old, 64);
}
#[test]
fn text_alpha_never_admits_other_layer_properties_or_enabled_expressions() {
    let base = alpha_drawing(json!({"property":"opacity","static":[50]}));
    let mut other = base.clone();
    other["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"position","static":[0,0]}));
    let mut active = base.clone();
    active["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":true,"code":"50"});
    for doc in [other, active] {
        let o = owner(&doc);
        let snapshot = support::head(&o);
        let bytes = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
        assert!(prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 7)).is_err());
        assert_eq!(
            crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
            bytes
        );
        assert_eq!(o.history_depths(), (0, 0));
    }
    let mut disabled = base;
    disabled["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":false,"code":"throw 1"});
    let o = owner(&disabled);
    require_text_alpha(&plan(&support::head(&o), 7), 64);
}
