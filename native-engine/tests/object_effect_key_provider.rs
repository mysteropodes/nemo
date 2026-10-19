//! Real effect records/codec and the same sealed Shift/history. No renderer completion is inferred.
use super::{locked_keys, shift_layer};
use crate::commands::ObjectFillOwner;
use crate::object_codec::{decode_storage_project, encode_storage_project, ObjectCodecErrorKind};
use crate::object_document::{KeyLockMode, ObjectDocument};
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

fn value(document: &ObjectDocument) -> Value {
    serde_json::to_value(document).unwrap()
}
fn admit(v: Value) -> ObjectDocument {
    decode_storage_project(v.to_string().as_bytes()).unwrap()
}
fn effect() -> Value {
    json!({"type":"blur","enabled":false,"p1":12,"p8":null,"keys":{
        "p1":{"keys":[{"frame":1,"v":[3],"hold":true,"hIn":[0,0],"hOut":[0,0],"curvePoints":[],"lockTo":"in"},
            {"frame":4,"v":[7],"lockTo":null}]},
        "opaque":{"keys":[{"frame":2,"v":[9]}]},"":null}})
}
fn fixture() -> Value {
    let document = build::document(
        &rows::parse_layers("K=a.1.10;.;.;K=b.2.20;.;.|K;.;.;.;.;."),
        build::Build::V2Extras,
    );
    let mut v = value(&document);
    v["layers"][0]["effects"] = json!([effect()]);
    for record in v["objects"].as_array_mut().unwrap() {
        record["effects"] = json!([effect()]);
    }
    v
}
fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn payload() -> Value {
    json!({"command":"object.layer.range.drag","contextId":"scene-root","layerUid":"L0","activeLayerUid":"L1",
        "part":"both","barSelection":[],"samples":[2],"plainClick":false,"alt":false,"frame":0,"selectedKeys":[]})
}
fn shifted_effect() -> Value {
    let mut e = effect();
    e["keys"]["p1"]["keys"][0]["frame"] = json!(3.0);
    e["keys"]["p1"]["keys"][1]["frame"] = json!(5.0);
    e["keys"]["opaque"]["keys"][0]["frame"] = json!(4.0);
    e
}
#[test]
fn actual_layer_and_element_records_preserve_missing_null_empty_and_typed_key_metadata_on_reopen() {
    for stack in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([effect()])),
    ] {
        let mut v = fixture();
        let record = v["objects"].as_array_mut().unwrap().first_mut().unwrap();
        match stack {
            None => {
                record.as_object_mut().unwrap().remove("effects");
            }
            Some(stack) => record["effects"] = stack,
        }
        let d = admit(v.clone());
        assert_eq!(value(&d), v);
        let encoded = encode_storage_project(&d).unwrap();
        assert_eq!(
            encode_storage_project(&decode_storage_project(&encoded).unwrap()).unwrap(),
            encoded
        );
    }
    for keys in [None, Some(Value::Null), Some(json!({}))] {
        let mut v = fixture();
        match keys {
            None => {
                v["layers"][0]["effects"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("keys");
            }
            Some(keys) => v["layers"][0]["effects"][0]["keys"] = keys,
        }
        assert_eq!(value(&admit(v.clone())), v);
    }
}
#[test]
fn actual_codec_reuses_strict_scalar_track_sort_dimension_handle_and_curve_predicates() {
    let mut bad = Vec::new();
    for keys in [
        json!([{"frame":2,"v":[1]},{"frame":1,"v":[2]}]),
        json!([{"frame":1,"v":[1]},{"frame":1,"v":[2]}]),
        json!([{"frame":1,"v":[1,2]}]),
        json!([{"frame":1,"v":[1],"hIn":[0]}]),
        json!([{"frame":1,"v":[1],"curvePoints":[{"x":0.5,"y":0},{"x":1,"y":1}]}]),
    ] {
        let mut v = fixture();
        v["layers"][0]["effects"][0]["keys"]["p1"]["keys"] = keys.clone();
        bad.push(v);
        let mut v = fixture();
        v["objects"][0]["effects"][0]["keys"]["p1"]["keys"] = keys;
        bad.push(v);
    }
    for v in bad {
        assert_eq!(
            decode_storage_project(v.to_string().as_bytes())
                .unwrap_err()
                .kind(),
            ObjectCodecErrorKind::Invalid
        );
    }
    let raw = fixture().to_string();
    let raw = raw.replacen("\"p8\":null", "\"p8\":null,\"keys\":{},\"keys\":{}", 1);
    assert!(decode_storage_project(raw.as_bytes()).is_err());
}
#[test]
fn effect_curves_keep_actual_motion_short_array_admission_without_relaxing_order_curves() {
    let mut v = fixture();
    let short = json!([{"x":7,"y":-3,"tx":0,"ty":17}]);
    v["layers"][0]["effects"][0]["keys"]["p1"]["keys"][0]["curvePoints"] = short.clone();
    assert_eq!(value(&admit(v.clone())), v);
    v["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"] = json!({
        "keys":[{"frame":1,"v":[0],"curvePoints":short}]});
    assert_eq!(
        decode_storage_project(v.to_string().as_bytes())
            .unwrap_err()
            .kind(),
        ObjectCodecErrorKind::Invalid
    );
}
#[test]
fn whole_layer_motion_retimes_disabled_layer_and_element_effects_once_and_excludes_reference_other_context(
) {
    let input = fixture();
    let mut d = admit(input.clone());
    assert!(shift_layer(&mut d, "scene-root", "L0", 2).unwrap());
    let mut expected = input;
    expected["layers"][0]["effects"][0] = shifted_effect();
    for record in expected["objects"].as_array_mut().unwrap() {
        if record["target"]["contextId"] == "scene-root"
            && record["target"]["layerUid"] == "L0"
            && record["target"]["frameScope"]["kind"] == "authored"
        {
            record["effects"][0] = shifted_effect();
        }
    }
    assert_eq!(value(&d), expected);
    assert!(
        locked_keys(&d, "scene-root", "L0", KeyLockMode::In).is_empty(),
        "effect locks never enter original Motion selection"
    );
    assert_eq!(value(&admit(value(&d))), expected);
}
#[test]
fn actual_sealed_shift_moves_content_and_effect_keys_in_one_revision_without_touching_retained_source(
) {
    let input = admit(fixture());
    let mut o = ObjectFillOwner::new("effect-shift", input.clone()).unwrap();
    let retained = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(&input).unwrap();
    let p = o
        .prepare_layer_shift_json(payload().to_string().as_bytes())
        .unwrap();
    assert_eq!(encode_storage_project(&head(&o)).unwrap(), before);
    o.commit_layer_shift(p).unwrap();
    assert_eq!(o.content_revision(), 1);
    let out = value(&head(&o));
    assert_eq!(out["layers"][0]["effects"][0], shifted_effect());
    assert_eq!(out["layers"][0]["range"], json!({"inPoint":2,"outPoint":7}));
    assert_eq!(
        out["layers"][0]["timing"],
        build::timing_json(&rows::parse_layers(".;.;K=a.1.10;.;.;K=b.2.20")[0])
    );
    for record in out["objects"].as_array().unwrap() {
        if record["target"]["contextId"] == "scene-root"
            && record["target"]["layerUid"] == "L0"
            && record["target"]["frameScope"]["kind"] == "authored"
        {
            assert!([2, 5].contains(&record["target"]["frameScope"]["frame"].as_u64().unwrap()));
            assert_eq!(record["effects"][0], shifted_effect());
        }
    }
    assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
}
#[cfg(feature = "history")]
#[test]
fn real_history_restores_complete_effect_storage_bytes_and_redo_after_one_atomic_checkpoint() {
    use crate::history::NativeObjectHistory;
    let d = admit(fixture());
    let before = encode_storage_project(&d).unwrap();
    let mut h = NativeObjectHistory::new("effect-history", d).unwrap();
    let p = h
        .prepare_layer_range_drag_json(payload().to_string().as_bytes())
        .unwrap();
    h.commit_layer_range_drag(p).unwrap();
    assert_eq!(h.history_depths(), (1, 0));
    let after =
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap();
    h.undo(h.content_revision()).unwrap();
    assert_eq!(
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap(),
        before
    );
    h.redo(h.content_revision()).unwrap();
    assert_eq!(
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap(),
        after
    );
}
#[cfg(feature = "test-application")]
#[test]
fn actual_frame_packet_to_layer_plan_and_contour_producer_refuse_unimplemented_active_effect_consumers(
) {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::prepare_object_render_scene;
    use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
        layer_inputs, SceneEffectJoinError,
    };
    let mut v = fixture();
    v["layers"][0]["effects"][0]["enabled"] = json!(true);
    let o = ObjectFillOwner::new("effect-render", admit(v)).unwrap();
    let snap = o.acquire_snapshot(0).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: snap.instance_id(),
        document_id: snap.document_id(),
        content_revision: snap.content_revision(),
        document_snapshot_id: snap.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snap, &selector).unwrap();
    assert!(matches!(
        layer_inputs(&packet),
        Err(SceneEffectJoinError::EffectParametersUnavailable { layer: 0 })
    ));
    let mut v = fixture();
    v["objects"][0]["effects"][0]["enabled"] = json!(true);
    let o = ObjectFillOwner::new("element-effect-render", admit(v)).unwrap();
    let snap = o.acquire_snapshot(0).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: snap.instance_id(),
        document_id: snap.document_id(),
        content_revision: snap.content_revision(),
        document_snapshot_id: snap.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snap, &selector).unwrap();
    assert!(prepare_object_render_scene(&packet, &selector).is_err());
}

#[cfg(feature = "test-application")]
fn placed_effect_outcome(enabled: bool, selected: bool)
-> crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome{
    use crate::evaluation::object_component_kernels::object_component_evaluate::{
        evaluate_component_frame, ComponentFrame,
    };
    use crate::evaluation::object_component_kernels::object_component_exposed::{
        ExposedEntry, ExposedValue,
    };
    use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::NoPorts;
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::ObjectFrameSelector;
    // The fixture declares NO exposed properties. Any attempted external override is a bug.
    struct NoOverrides;
    impl OverridePort for NoOverrides {
        fn available(&mut self) -> bool {
            panic!("no override declared");
        }
        fn register(&mut self, _: &ExposedEntry) {
            panic!("no override declared");
        }
        fn read(&mut self, _: &str, _: &str, _: u32) -> Result<Vec<ExposedValue>, String> {
            panic!("no override declared");
        }
    }
    let plain = build::record_json("effect-symbol", "authored", "inside", "plain", 0, 1, 10);
    let mut fx = build::record_json("effect-symbol", "authored", "inside", "fx", 0, 2, 20);
    let mut stack = effect();
    stack["enabled"] = json!(enabled);
    fx["effects"] = json!([stack]);
    let mut v = fixture();
    v["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"effect-symbol","totalFrames":6,"layers":[{
            "layerUid":"inside","visible":selected,"timing":{"format":"nemo.native-layer-timing",
                "formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
            "objects":[plain,fx]}],
        "instances":[{"contextId":"scene-root","layerUid":"L1","symbolId":"effect-symbol",
            "symSpeed":1,"symPlacedAt":0,"symMatrix":[1,0,0,1,0,0]}]}});
    let d = admit(v);
    let before = encode_storage_project(&d).unwrap();
    let owner = ObjectFillOwner::new("placed-effect", d).unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let components = snapshot
        .document()
        .component_set()
        .expect("real stored component table");
    assert_eq!(
        components.components[0].objects[1].has_enabled_effects(),
        enabled
    );
    assert_eq!(
        components.components[0].objects[1].effects().unwrap()[0]
            .keys
            .value()
            .unwrap()
            .as_ref()
            .unwrap()
            .entries()
            .len(),
        3
    );
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let frame = evaluate_component_frame(
        &snapshot,
        &selector,
        Some((640.0, 360.0)),
        &mut NoPorts,
        &mut NoOverrides,
    )
    .expect("actual admitted frame/packet/placed-component chain");
    let ComponentFrame::Scene(scene) = frame else {
        panic!("real placed instance must retain its scene slot");
    };
    assert_eq!(
        scene.slots().len(),
        1,
        "placing layer keeps exactly one slot"
    );
    assert_eq!(scene.slots()[0].entry().layer_uid, "L1");
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(encode_storage_project(&head(&owner)).unwrap(), before);
    scene.slots()[0].entry().outcome.clone()
}
#[cfg(feature = "test-application")]
#[test]
fn placed_component_selected_enabled_element_effect_returns_typed_unavailable_without_partial_plain_shapes(
) {
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome;
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentUnavailable;
    // The effect record follows a valid plain record. Refuse the whole instance, not just the effect shape.
    assert_eq!(
        placed_effect_outcome(true, true),
        ComponentOutcome::Unavailable(ComponentUnavailable::InnerFeature {
            layer_uid: "inside".into(),
            feature: "per-element effects"
        })
    );
}
#[cfg(feature = "test-application")]
#[test]
fn placed_component_disabled_or_unselected_effects_keep_original_shapes_and_empty_selection_outcomes(
) {
    use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome;
    let ComponentOutcome::Shapes(placed) = placed_effect_outcome(false, true) else {
        panic!("disabled effects must remain available");
    };
    assert_eq!(
        placed
            .shapes
            .iter()
            .map(|s| s.target.stroke_id())
            .collect::<Vec<_>>(),
        ["plain", "fx"]
    );
    let ComponentOutcome::Shapes(placed) = placed_effect_outcome(true, false) else {
        panic!("unselected effects must not refuse");
    };
    assert!(placed.shapes.is_empty());
}

#[test]
fn storage_routes_v2_refusals_to_the_strict_ordered_codec_without_losing_raw_admission() {
    let mut invalid = fixture();
    invalid["layers"][0]["effects"][0]["keys"]["p1"]["keys"] = json!([{"frame":1,"v":[3,4]}]);
    let raw = invalid.to_string();
    let explicit = crate::object_codec::decode_ordered_project(raw.as_bytes()).unwrap_err();
    assert_eq!(explicit.kind(), ObjectCodecErrorKind::Invalid);
    assert_eq!(
        decode_storage_project(raw.as_bytes()).unwrap_err(),
        explicit
    );
    let mut unknown = fixture();
    unknown["unknownRootMember"] = json!(true);
    let raw = unknown.to_string();
    assert_eq!(
        decode_storage_project(raw.as_bytes()).unwrap_err(),
        crate::object_codec::decode_ordered_project(raw.as_bytes()).unwrap_err()
    );
    let raw = fixture().to_string();
    let duplicate = raw.replacen(
        "\"formatVersion\":2",
        "\"formatVersion\":2,\"formatVersion\":2",
        1,
    );
    assert_ne!(duplicate, raw);
    assert!(decode_storage_project(duplicate.as_bytes()).is_err());
    assert!(decode_storage_project(b"[2]").is_err());
    let legacy = build::document_json(&rows::parse_layers("K=a.1.10;.;."), build::Build::V1);
    let raw = legacy.to_string();
    assert_eq!(
        value(&decode_storage_project(raw.as_bytes()).unwrap()),
        value(&crate::object_codec::decode_project(raw.as_bytes()).unwrap())
    );
    let mut legacy_extra = legacy;
    legacy_extra["compositionOrder"] = Value::Null;
    let raw = legacy_extra.to_string();
    assert_eq!(
        decode_storage_project(raw.as_bytes()).unwrap_err(),
        crate::object_codec::decode_project(raw.as_bytes()).unwrap_err()
    );
}
