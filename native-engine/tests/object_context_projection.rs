//! #2256: the component-as-scene projection on a hand-authored world (expected values are the authored literals).
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn layer(uid: &str, keys: &[u32]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|frame| json!({ "frame": frame })).collect();
    json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#4a9eff","parentLayerUid":null,
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":keys,"samples":[]}})
}
fn order(uid: &str, context: &str) -> Value {
    json!({"contextId":context,"layers":[{"layerUid":uid,"naturalPosition":0,"order":{}}],"scopes":[]})
}
fn world() -> ObjectDocument {
    let value = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":6,
        "layers":[layer("ly_scene",&[0])],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[order("ly_scene","scene-root"),order("ly_in","sym_a")],"migration":"explicit-plan-required","referencePolicy":"unresolved"},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"sym_a","name":"A","totalFrames":4,"fps":24,"layers":[layer("ly_in",&[0,2])],"objects":[]}],
            "instances":[]}}});
    decode_storage_project(value.to_string().as_bytes()).expect("admitted world")
}

#[test]
fn the_scene_root_projects_to_the_document_itself() {
    let document = world();
    assert_eq!(project(&document, "scene-root").unwrap(), document);
}

#[test]
fn a_component_projects_to_its_own_layers_length_and_order_scope() {
    let document = world();
    let view = project(&document, "sym_a").unwrap();
    assert_eq!(view.total_frames, 4);
    let uids: Vec<_> = view.layers.iter().map(|l| l.layer_uid.as_str()).collect();
    assert_eq!(uids, vec!["ly_in"]);
    let plan = view.composition_order.value().unwrap();
    assert_eq!(plan.contexts.len(), 1);
    assert_eq!(plan.contexts[0].context_id, "scene-root");
    assert!(
        view.component_tables.missing(),
        "the view declares only the scene root"
    );
}

#[test]
fn an_unedited_view_embeds_back_to_the_original() {
    let document = world();
    let view = project(&document, "sym_a").unwrap();
    assert_eq!(
        embed(&document, "sym_a", &view, view.clone()).unwrap(),
        document
    );
}

#[test]
fn an_undeclared_context_and_a_v1_document_are_refused() {
    let document = world();
    assert_eq!(
        project(&document, "sym_missing").unwrap_err(),
        DispatchErrorCode::NotFound
    );
    let v1 = decode_storage_project(
        br#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L"}],"objects":[]}"#,
    )
    .unwrap();
    assert_eq!(
        project(&v1, "sym_a").unwrap_err(),
        DispatchErrorCode::Unavailable
    );
}

#[test]
fn a_kernel_that_wrote_a_member_the_view_does_not_carry_is_refused() {
    let document = world();
    let view = project(&document, "sym_a").unwrap();
    let mut edited = view.clone();
    edited.component_tables = document.component_tables.clone();
    assert_eq!(
        embed(&document, "sym_a", &view, edited).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
}

#[test]
fn the_payload_context_defaults_to_the_scene_and_refuses_an_empty_id() {
    assert_eq!(context_of(&Presence::Missing).unwrap(), "scene-root");
    assert_eq!(
        context_of(&Presence::Present("sym_a".to_owned())).unwrap(),
        "sym_a"
    );
    assert_eq!(
        context_of(&Presence::Present(String::new())).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
}
