//! Ordering tripwire against the expression views' component refusal (D17, read-only
//! here, L04-I). `bind_parts` answers a scene-root target unless the order plan declares a
//! component context (`ComponentLayers`) and refuses every non-root target
//! (`ComponentContext`). Component inner layers live in their component records, so the
//! scene-root layer list stays knowable and the tables alone do not trigger the first
//! refusal; the second one stays until the nested-context resolver (L04-J) lands. When
//! L04-J lifts either refusal, the matching assertion below must be flipped in the same
//! change: that is the recorded order.
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
fn the_tables_leave_the_scene_root_views_available_and_component_targets_unavailable() {
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
    let refused = bind_for_test(
        &doc,
        Some(&meta),
        &target("sym_comp", "ly_comp_ia"),
        "position",
    );
    assert_eq!(refused.err(), Some(ViewUnavailable::ComponentContext));
}

#[test]
fn a_component_context_in_the_order_plan_still_refuses_the_scene_root() {
    let mut value = golden_value();
    let contexts = value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap();
    let mut component = contexts[0].clone();
    component["contextId"] = json!("sym_comp");
    contexts.push(component);
    let doc = admit(&value).unwrap().into_storage();
    let meta = metadata();
    let refused = bind_for_test(
        &doc,
        Some(&meta),
        &target("scene-root", "ly_toggle"),
        "position",
    );
    assert_eq!(refused.err(), Some(ViewUnavailable::ComponentLayers));
}
