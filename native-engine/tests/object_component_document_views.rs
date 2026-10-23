//! Positive expression context identity after the real nested-context provider.
//! Root's valid scoped-order fixture is retained. The old ComponentContext and
//! ComponentLayers tripwires evolve here with that provider; unknown contexts,
//! unresolved order declarations and missing metadata retain typed refusals.
use super::tests::{admit, count, document, golden_value};
use crate::evaluation::property_expression_views::{bind_for_test, ViewUnavailable};
use crate::object_document::{ProjectMetadata, PropertyHolderTarget};
use serde_json::json;

fn target(context: &str, layer: &str) -> PropertyHolderTarget {
    serde_json::from_value(json!({"contextId": context, "layerUid": layer})).unwrap()
}
fn metadata() -> ProjectMetadata {
    ProjectMetadata::new(640, 360, 24.).unwrap()
}

#[test]
fn the_tables_bind_root_and_component_identity_from_their_own_layers() {
    let doc = document();
    let meta = metadata();
    let view = bind_for_test(
        &doc,
        Some(&meta),
        &target("scene-root", "ly_toggle"),
        "position",
    )
    .unwrap_or_else(|error| panic!("scene-root view refused: {error:?}"));
    let comp = view.comp().unwrap();
    assert_eq!(comp.layers(), count("scene_layers"));
    assert_eq!(comp.frames() as usize, count("total_frames"));
    let nested = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ia"),
        "position",
    )
    .unwrap();
    let comp = nested.comp().unwrap();
    assert_eq!((comp.name(), comp.frames(), comp.layers()), ("Comp", 12, 2));
    let me = nested.self_view().unwrap();
    assert_eq!(
        (me.name(), me.index(), me.is_shape()),
        (Some("Inner A"), 0, false)
    );
}

#[test]
fn a_valid_component_order_plan_keeps_both_context_views_available() {
    let mut value = golden_value();
    // A declared inner plan must name its OWN admitted layers and cover all
    // its records. The old root-stack clone is no longer a valid fixture.
    let inner = &value["componentTables"]["components"]["components"][0];
    let layers = inner["layers"].as_array().unwrap().iter().enumerate().map(|(index,layer)|
        json!({"layerUid":layer["layerUid"],"naturalPosition":index,"order":{}})).collect::<Vec<_>>();
    let mut scopes = Vec::new();
    for frame in 0..inner["totalFrames"].as_u64().unwrap() {
        let mut positions = std::collections::HashMap::new();
        let elements=inner["objects"].as_array().unwrap().iter()
            .filter(|record|record["target"]["frameScope"]["frame"]==frame)
            .map(|record| {
                let layer=record["target"]["layerUid"].as_str().unwrap();
                let position=positions.entry(layer).or_insert(0usize);
                let element=json!({"layerUid":layer,"strokeId":record["target"]["strokeId"],"naturalPosition":*position,"order":{}});
                *position+=1;element
            }).collect::<Vec<_>>();
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
    }
    let component = json!({"contextId":inner["symbolId"],"layers":layers,"scopes":scopes});
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(component);
    let doc = admit(&value).unwrap().into_storage();
    let meta = metadata();
    let before = crate::object_codec::encode_storage_project(&doc).unwrap();
    let root = bind_for_test(
        &doc,
        Some(&meta),
        &target("scene-root", "ly_toggle"),
        "position",
    )
    .unwrap();
    let nested = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ia"),
        "position",
    )
    .unwrap();
    assert_eq!(
        (root.comp().unwrap().name(), root.comp().unwrap().frames()),
        ("Scene", 24)
    );
    assert_eq!(
        (
            nested.comp().unwrap().name(),
            nested.comp().unwrap().frames()
        ),
        ("Comp", 12)
    );
    assert_eq!(
        crate::object_codec::encode_storage_project(&doc).unwrap(),
        before
    );
}

#[test]
fn component_comp_aliases_use_own_fps_frames_and_name_with_inherited_canvas() {
    let mut value = golden_value();
    value["componentTables"]["components"]["components"][0]["fps"] = json!(48);
    value["componentTables"]["components"]["components"][0]["name"] = json!("Nested 48");
    let doc = admit(&value).unwrap().into_storage();
    let meta = metadata();
    let before = crate::object_codec::encode_storage_project(&doc).unwrap();
    let a = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ia"),
        "position",
    )
    .unwrap();
    let b = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp_b", "ly_comp_ia"),
        "position",
    )
    .unwrap();
    let scene = bind_for_test(
        &doc,
        Some(&meta),
        &target("scene-root", "ly_toggle"),
        "position",
    )
    .unwrap();
    let comp = a.comp().unwrap();
    assert_eq!(
        (
            comp.width(),
            comp.height(),
            comp.fps(),
            comp.frames(),
            comp.layers(),
            comp.name()
        ),
        (640, 360, 48., 12, 2, "Nested 48")
    );
    assert_eq!(
        (comp.frame_duration(), comp.duration(), comp.num_layers()),
        (1. / 48., 0.25, 2)
    );
    assert_eq!(
        (b.comp().unwrap().name(), b.comp().unwrap().fps()),
        ("Comp B", 24.)
    );
    assert_eq!(
        (
            scene.comp().unwrap().name(),
            scene.comp().unwrap().fps(),
            scene.comp().unwrap().frames()
        ),
        ("Scene", 24., 24)
    );
    assert_eq!(
        crate::object_codec::encode_storage_project(&doc).unwrap(),
        before
    );
}

#[test]
fn absent_component_fps_and_empty_name_follow_baseline_enter_symbol_defaults() {
    let mut value = golden_value();
    let component = value["componentTables"]["components"]["components"][0]
        .as_object_mut()
        .unwrap();
    component.remove("fps");
    component.insert("name".into(), json!(""));
    let doc = admit(&value).unwrap().into_storage();
    let meta = ProjectMetadata::new(640, 360, 30.).unwrap();
    let view = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ia"),
        "opacity",
    )
    .unwrap();
    assert_eq!(
        (
            view.comp().unwrap().name(),
            view.comp().unwrap().fps(),
            view.comp().unwrap().duration()
        ),
        ("Component", 30., 0.4)
    );
    let bare = bind_for_test(&doc, None, &target("sym_comp", "ly_comp_ia"), "opacity").unwrap();
    assert_eq!(bare.comp(), Err(ViewUnavailable::ProjectMetadata));
    assert_eq!(bare.self_view().unwrap().name(), Some("Inner A"));
}

#[test]
fn equal_layer_uids_in_two_components_keep_self_views_in_their_own_context() {
    let mut value = golden_value();
    let components = value["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap();
    components[0]["layers"][0]["name"] = json!("Local Parent");
    components[0]["layers"][1]["name"] = json!("Local Child");
    components[1]["layers"][0]["name"] = json!("Other Context");
    let doc = admit(&value).unwrap().into_storage();
    let meta = metadata();
    let child = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ib"),
        "position",
    )
    .unwrap();
    let other = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp_b", "ly_comp_ia"),
        "position",
    )
    .unwrap();
    let me = child.self_view().unwrap();
    assert_eq!(
        (me.name(), me.index(), me.has_parent()),
        (Some("Local Child"), 1, false)
    );
    assert!(me.parent().is_none());
    assert_eq!(other.self_view().unwrap().name(), Some("Other Context"));
    assert!(other.self_view().unwrap().parent().is_none());
    let root = bind_for_test(
        &doc,
        Some(&meta),
        &target("scene-root", "ly_comp_ib"),
        "position",
    )
    .unwrap();
    assert!(
        root.self_view().is_none(),
        "a component layer cannot leak into root self"
    );
}

#[test]
fn component_views_do_not_infer_missing_holders_or_fall_back_to_root_layers() {
    let doc = document();
    let meta = metadata();
    let component = doc
        .component_set()
        .unwrap()
        .components
        .iter()
        .find(|c| c.symbol_id() == "sym_comp")
        .unwrap();
    let record = &component.objects()[0];
    let element: PropertyHolderTarget = serde_json::from_value(json!({"contextId":"sym_comp", "layerUid":record.target().layer_uid(), "strokeId":record.target().stroke_id()})).unwrap();
    let view = bind_for_test(&doc, Some(&meta), &element, "opacity").unwrap();
    assert!(
        view.self_view().is_none(),
        "a real stroke without its exact property holder is not an element expression holder"
    );
    let missing = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_toggle"),
        "position",
    )
    .unwrap();
    assert!(
        missing.self_view().is_none(),
        "root layers are not component layers"
    );
    assert_eq!(
        bind_for_test(
            &doc,
            Some(&meta),
            &target("missing-component", "ly_toggle"),
            "position"
        )
        .err(),
        Some(ViewUnavailable::ComponentContext)
    );
}

#[test]
fn a_retained_component_layer_view_keeps_identity_and_owned_values_after_snapshot_drop() {
    use crate::evaluation::property_expression_views::ExpressionContextView;
    use crate::revision::ObjectSnapshot;
    let doc = document();
    let first = ObjectSnapshot::from_admitted("component-view".into(), doc.clone()).unwrap();
    let other = ObjectSnapshot::from_admitted("component-view".into(), doc).unwrap();
    let view =
        ExpressionContextView::bind(&first, &target("sym_comp", "ly_comp_ia"), "position").unwrap();
    assert!(view.is_bound_to(&first) && view.is_bound_to(&first.clone()));
    assert!(!view.is_bound_to(&other));
    assert_eq!(view.comp(), Err(ViewUnavailable::ProjectMetadata));
    assert_eq!(view.property(), "position");
    drop(first);
    drop(other);
    assert_eq!(
        (
            view.self_view().unwrap().name(),
            view.self_view().unwrap().index()
        ),
        (Some("Inner A"), 0)
    );
}

#[cfg(feature = "history")]
#[test]
fn retained_project_snapshot_provides_real_nested_comp_alias_metadata_without_publication() {
    use crate::evaluation::property_expression_views::ExpressionContextView;
    use crate::object_document::ProjectEnvelope;
    use crate::revision::ObjectSnapshot;
    // Use the real component records in a component-only project with no audio assets.
    let mut value = golden_value();
    let components = value["componentTables"]["components"].clone();
    value["componentTables"] = json!({"components":components});
    let doc = admit(&value).unwrap().into_storage();
    let before = crate::object_codec::encode_storage_project(&doc).unwrap();
    let project = ProjectEnvelope::no_media(metadata(), doc);
    let snapshot =
        ObjectSnapshot::from_admitted_project("component-project".into(), project).unwrap();
    let view =
        ExpressionContextView::bind(&snapshot, &target("sym_comp", "ly_comp_ia"), "position")
            .unwrap();
    let comp = view.comp().unwrap();
    assert_eq!(
        (
            comp.name(),
            comp.width(),
            comp.height(),
            comp.fps(),
            comp.frames(),
            comp.layers()
        ),
        ("Comp", 640, 360, 24., 12, 2)
    );
    assert_eq!(
        (comp.frame_duration(), comp.duration(), comp.num_layers()),
        (1. / 24., 0.5, 2)
    );
    assert!(view.is_bound_to(&snapshot));
    assert_eq!(snapshot.content_revision(), 0);
    assert_eq!(
        crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
        before
    );
    drop(snapshot);
    assert_eq!(view.comp().unwrap().name(), "Comp");
}

#[test]
fn component_parent_links_remain_an_explicit_unsupported_storage_provider() {
    let mut value = golden_value();
    value["componentTables"]["components"]["components"][0]["layers"][1]["parentLayerUid"] =
        json!("ly_comp_ia");
    let error = admit(&value).unwrap_err();
    assert_eq!(
        error.kind(),
        crate::object_codec::ObjectCodecErrorKind::Unsupported
    );
}

#[test]
fn admitted_component_element_holder_binds_exact_context_and_survives_save_reopen() {
    use crate::evaluation::property_expression_views::ExpressionContextView;
    use crate::revision::ObjectSnapshot;
    let mut value = golden_value();
    // This is the existing G-oracle record, not a replacement test-local object type.
    assert_eq!(
        value["componentTables"]["components"]["components"][0]["objects"][0]["target"],
        json!({"contextId":"sym_comp", "frameScope":{"kind":"authored","frame":0}, "layerUid":"ly_comp_ia", "strokeId":"s_comp_a_0"})
    );
    assert_eq!(
        value["componentTables"]["components"]["components"][1]["objects"][0]["target"]["strokeId"],
        json!("s_comp_a_0")
    );
    let holder_target =
        json!({"contextId":"sym_comp", "layerUid":"ly_comp_ia", "strokeId":"s_comp_a_0"});
    value["propertyHolders"] = json!([{"target":holder_target, "properties":[]}]);
    // Requires the actual declared-component admission join claimed in #2244/6261877434.
    let doc = admit(&value)
        .expect("real component holder admission")
        .into_storage();
    let target: PropertyHolderTarget = serde_json::from_value(holder_target).unwrap();
    let meta = metadata();
    let view = bind_for_test(&doc, Some(&meta), &target, "position").unwrap();
    let me = view
        .self_view()
        .expect("the exact native element holder resolves");
    assert_eq!(
        (
            me.layer_uid(),
            me.stroke_id(),
            me.name(),
            me.index(),
            me.is_shape()
        ),
        ("ly_comp_ia", Some("s_comp_a_0"), Some("Inner A"), 0, true)
    );
    assert_eq!(view.target(), &target);
    assert_eq!(
        (view.comp().unwrap().name(), view.comp().unwrap().frames()),
        ("Comp", 12)
    );
    let other: PropertyHolderTarget = serde_json::from_value(
        json!({"contextId":"sym_comp_b", "layerUid":"ly_comp_ia", "strokeId":"s_comp_a_0"}),
    )
    .unwrap();
    assert!(
        bind_for_test(&doc, Some(&meta), &other, "position")
            .unwrap()
            .self_view()
            .is_none(),
        "an actual same-ID stroke in another component cannot borrow this holder"
    );
    let saved = crate::object_codec::encode_storage_project(&doc).unwrap();
    let reopened = crate::object_codec::decode_storage_project(&saved).unwrap();
    assert_eq!(reopened.property_holders().unwrap()[0].target(), &target);
    assert_eq!(
        crate::object_codec::encode_storage_project(&reopened).unwrap(),
        saved
    );
    let snapshot = ObjectSnapshot::from_admitted("retained-element".into(), reopened).unwrap();
    let retained = ExpressionContextView::bind(&snapshot, &target, "position").unwrap();
    assert!(retained.is_bound_to(&snapshot));
    assert_eq!(snapshot.content_revision(), 0);
    assert_eq!(
        crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
        saved
    );
    drop(snapshot);
    assert_eq!(
        (
            retained.self_view().unwrap().stroke_id(),
            retained.self_view().unwrap().name()
        ),
        (Some("s_comp_a_0"), Some("Inner A"))
    );
}
