//! Real storage admission/encoding controls, independent of the scene's later transform evaluation.
use crate::object_codec::{
    decode_ordered_project, decode_storage_project, encode_storage_project, ObjectCodecErrorKind,
};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

fn fixture(version: u32) -> Value {
    let mut source = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":2,"layers":[{"layerUid":"A"},{"layerUid":"B"},{"layerUid":"C"}],"objects":[]});
    if version == 2 {
        source["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"A","naturalPosition":0,"order":{}},
                {"layerUid":"B","naturalPosition":1,"order":{}},
                {"layerUid":"C","naturalPosition":2,"order":{}}],"scopes":[]}]});
    }
    source
}
fn roundtrip(source: &Value) -> ObjectDocument {
    let document = decode_storage_project(&serde_json::to_vec(source).unwrap()).unwrap();
    let saved = encode_storage_project(&document).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), *source);
    let reopened = decode_storage_project(&saved).unwrap();
    assert_eq!(encode_storage_project(&reopened).unwrap(), saved);
    reopened
}
#[test]
fn ordered_valid_forward_and_multilevel_links_preserve_missing_null_and_opaque_uids() {
    let mut source = fixture(2);
    source["layers"][0]["parentLayerUid"] = Value::Null;
    source["layers"][1]["parentLayerUid"] = json!("A");
    source["layers"][2]["parentLayerUid"] = json!("B");
    roundtrip(&source);
    let mut forward = fixture(2);
    forward["layers"][0]["parentLayerUid"] = json!("C");
    forward["layers"][1]["parentLayerUid"] = json!("C");
    roundtrip(&forward);
    let text = serde_json::to_string(&source)
        .unwrap()
        .replace("\"A\"", "\" 親:opaque / UID \"");
    let opaque: Value = serde_json::from_str(&text).unwrap();
    roundtrip(&opaque);
    for version in [1, 2] {
        roundtrip(&fixture(version));
        let mut null = fixture(version);
        null["layers"][1]["parentLayerUid"] = Value::Null;
        roundtrip(&null);
    }
}
#[test]
fn malformed_parent_graphs_refuse_decode_and_encode_without_changing_input() {
    for edges in [
        vec![(0, "")],
        vec![(0, "missing")],
        vec![(0, "A")],
        vec![(0, "B"), (1, "A")],
        vec![(0, "B"), (1, "C"), (2, "A")],
    ] {
        let mut source = fixture(2);
        for (layer, parent) in edges {
            source["layers"][layer]["parentLayerUid"] = json!(parent);
        }
        let bytes = serde_json::to_vec(&source).unwrap();
        assert_eq!(
            decode_ordered_project(&bytes).unwrap_err().kind(),
            ObjectCodecErrorKind::Invalid,
            "{source}"
        );
        assert!(decode_storage_project(&bytes).is_err());
        let typed: ObjectDocument = serde_json::from_value(source.clone()).unwrap();
        assert!(encode_storage_project(&typed).is_err());
        assert_eq!(serde_json::to_value(&typed).unwrap(), source);
        assert_eq!(serde_json::to_vec(&source).unwrap(), bytes);
    }
}
#[test]
fn legacy_and_component_symbols_still_refuse_even_a_valid_parent_link() {
    let mut legacy = fixture(1);
    legacy["layers"][1]["parentLayerUid"] = json!("A");
    assert_eq!(
        decode_storage_project(&serde_json::to_vec(&legacy).unwrap())
            .unwrap_err()
            .kind(),
        ObjectCodecErrorKind::Unsupported
    );
    let mut scene = fixture(2);
    scene["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"symbol","totalFrames":2,"layers":[
            {"layerUid":"inner-parent"},{"layerUid":"inner-child","parentLayerUid":"inner-parent"}],"objects":[]}],"instances":[]}});
    assert_eq!(
        decode_ordered_project(&serde_json::to_vec(&scene).unwrap())
            .unwrap_err()
            .kind(),
        ObjectCodecErrorKind::Unsupported
    );
}
#[cfg(all(feature = "history", feature = "compositor"))]
#[test]
fn valid_chain_survives_real_owner_reopen_and_pinned_frame_scene_preparation() {
    use crate::history::NativeObjectHistory;
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    };
    let mut source = fixture(2);
    source["layers"][1]["parentLayerUid"] = json!("A");
    source["layers"][2]["parentLayerUid"] = json!("B");
    let examples: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = examples["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","layerUid":"A","strokeId":"ink","frameScope":{"kind":"authored","frame":0}});
    source["objects"] = json!([record]);
    source["compositionOrder"]["contexts"][0]["scopes"] = json!([{"frameScope":{"kind":"authored","frame":0},
        "elements":[{"layerUid":"A","strokeId":"ink","naturalPosition":0,"order":{}}]}]);
    let document = roundtrip(&source);
    let before = encode_storage_project(&document).unwrap();
    let owner = NativeObjectHistory::new("parent-links", document).unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: 0,
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    assert_eq!(scene.entries().len(), 1);
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let witness = prepare_object_composition(&plan, &pins).unwrap();
    assert_eq!(witness.document_snapshot_id(), snapshot.snapshot_id());
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    // Storage-valid links do not activate the still-unavailable visible vector parent consumer.
    source["objects"][0]["target"]["layerUid"] = json!("C");
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["layerUid"] = json!("C");
    let parented = roundtrip(&source);
    let parented_before = encode_storage_project(&parented).unwrap();
    let parented_owner = NativeObjectHistory::new("parented-vector", parented).unwrap();
    let parented_snapshot = parented_owner.acquire_snapshot(0).unwrap();
    let parented_pins = ObjectFrameSelector {
        instance_id: parented_snapshot.instance_id(),
        document_id: parented_snapshot.document_id(),
        content_revision: 0,
        document_snapshot_id: parented_snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    assert!(matches!(
        prepare_object_frame(&parented_snapshot, &parented_pins),
        Err(crate::object_frame_packet::ObjectFrameError::Unavailable)
    ));
    assert_eq!(
        encode_storage_project(parented_snapshot.document()).unwrap(),
        parented_before
    );
    assert_eq!(
        (
            parented_owner.content_revision(),
            parented_owner.history_depths()
        ),
        (0, (0, 0))
    );
}
