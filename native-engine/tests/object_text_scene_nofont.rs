//! #2230 L07-L: without `font_asset_admission` the production Text join has no font provider. A selected
//! Text record must be the typed refusal, never a scene with the glyphs silently absent.
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_render_scene, ObjectSceneError};
use serde_json::{json, Value};
#[path = "object_raster_asset_commands_support.rs"]
mod support;

const REASON: &str = "native font parser is unavailable";

fn text_only(with_vector: bool) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut vector = cases["records"][0].clone();
    vector["target"] = support::target("L0", "vector");
    let mut elements = vec![json!({"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}})];
    if with_vector {
        elements.push(json!({"layerUid":"L0","strokeId":"vector","naturalPosition":1,"order":{}}));
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects": if with_vector { json!([vector]) } else { json!([]) },
        "items":{"text":[{"target":support::target("L0","text"),"block":{"text":"A",
            "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,"color":"#112233",
            "align":"left","anchorTopLeft":{"x":10,"y":20}}}]},
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}

fn refusal(doc: &Value) -> Result<(), ObjectSceneError> {
    let owner = NativeObjectHistory::from_admitted_project(
        "text-no-font",
        support::project(doc, support::images_only()),
    )
    .unwrap();
    let snapshot = support::head(&owner);
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    assert_eq!(packet.text_records().count(), 1, "the Text record is selected, not filtered away");
    prepare_object_render_scene(&packet, &pins).map(|_| ())
}

fn assert_typed(error: Result<(), ObjectSceneError>) {
    match error {
        Err(ObjectSceneError::UnsupportedGeometry { reason, .. }) => assert_eq!(reason, REASON),
        other => panic!("expected the typed no-font refusal, got {other:?}"),
    }
}

#[test]
fn text_without_a_font_parser_is_the_exact_typed_refusal_never_an_empty_text_scene() {
    assert_typed(refusal(&text_only(false)));
}

#[test]
fn a_vector_beside_unavailable_text_does_not_yield_a_shorter_scene() {
    assert_typed(refusal(&text_only(true)));
}
