//! Independent authored-exposure and requested-time controls; no inferred keys.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFrameError, ObjectFramePacket,
    ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn timing(keys: &[u32], samples: Value) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|frame|json!({"frame":frame})).collect::<Vec<_>>(),"samples":samples})
}
fn record(layer: &str, id: &str, frame: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut result = cases["records"][0].clone();
    result["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id});
    result
}
fn plan(value: &mut Value) {
    let scopes: Vec<_> = (0..value["totalFrames"].as_u64().unwrap()).filter_map(|frame| {
        let mut positions = std::collections::HashMap::<String,usize>::new();
        let elements: Vec<_> = value["objects"].as_array().unwrap().iter()
            .filter(|r| r["target"]["frameScope"]["kind"]=="authored" && r["target"]["frameScope"]["frame"]==frame)
            .map(|r| {
                let layer=r["target"]["layerUid"].as_str().unwrap();
                let position=positions.entry(layer.into()).or_default();
                let entry=json!({"layerUid":layer,"strokeId":r["target"]["strokeId"],"naturalPosition":*position,"order":{}});
                *position+=1;entry
            }).collect();
        (!elements.is_empty()).then(|| json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}))
    }).collect();
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
        "layers":[{"layerUid":"A","naturalPosition":0,"order":{}},{"layerUid":"B","naturalPosition":1,"order":{}}],"scopes":scopes}]});
}
fn drawing(version: u32) -> Value {
    // Frozen drawing oracle: key1 A; ignored non-key2 X; sample3 C; blank key5;
    // manual sample6 D. After samples, held cells return to the last KEY.
    let mut reference = record("A", "A", 1);
    reference["target"]["frameScope"]["kind"] = json!("reference");
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":8,
        "layers":[{"layerUid":"A","timing":timing(&[1,5],json!([{"frame":3},{"frame":6,"manualEdit":true}]))},
                  {"layerUid":"B","timing":timing(&[0,2,6],json!([]))}],
        "objects":[record("A","A",1),record("A","X",2),record("A","C",3),record("A","D",6),record("B","B0",0),record("B","B2",2),reference]});
    if version == 2 {
        plan(&mut value);
    }
    value
}
fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "exposure",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn selector(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn contents(p: &ObjectFramePacket) -> Vec<(String, String, u32)> {
    p.records()
        .map(|r| {
            let t = r.record().target();
            (t.layer_uid().into(), t.stroke_id().into(), t.frame())
        })
        .collect()
}
#[test]
fn independently_frozen_exposure_matrix_keeps_each_layers_source_and_requested_identity() {
    let expected = [
        (None, Some(0), "", "B0"),
        (Some(1), Some(0), "A", "B0"),
        (Some(1), Some(2), "A", "B2"),
        (Some(3), Some(2), "C", "B2"),
        (Some(1), Some(2), "A", "B2"),
        (Some(5), Some(2), "", "B2"),
        (Some(6), Some(6), "D", ""),
        (Some(5), Some(6), "", ""),
    ];
    for version in [1, 2] {
        let s = snapshot(&drawing(version));
        let before = encode_storage_project(s.document()).unwrap();
        for (frame, (a, b, a_id, b_id)) in expected.iter().enumerate() {
            let p = prepare_object_exposure_frame(&s, &selector(&s, frame as u32)).unwrap();
            assert_eq!(
                (
                    p.frame(),
                    p.source_frame_for_layer(0),
                    p.source_frame_for_layer(1)
                ),
                (frame as u32, *a, *b)
            );
            let mut want = Vec::new();
            if !a_id.is_empty() {
                want.push(("A".into(), (*a_id).into(), a.unwrap()));
            }
            if !b_id.is_empty() {
                want.push(("B".into(), (*b_id).into(), b.unwrap()));
            }
            assert_eq!(contents(&p), want, "v{version}/frame{frame}");
            assert_eq!(
                (
                    p.instance_id(),
                    p.document_id(),
                    p.content_revision(),
                    p.document_snapshot_id()
                ),
                (
                    s.instance_id(),
                    s.document_id(),
                    s.content_revision(),
                    s.snapshot_id()
                )
            );
            for item in p.records() {
                assert!(std::ptr::eq(
                    item.record(),
                    &s.document().objects()[item.source_object_index()]
                ));
            }
            #[cfg(feature = "compositor")]
            if version == 2 {
                let scene = crate::object_render_scene::prepare_object_render_scene(
                    &p,
                    &selector(&s, frame as u32),
                )
                .unwrap();
                let paint = crate::object_render_scene::prepare_object_paint_plan(
                    &scene,
                    &selector(&s, frame as u32),
                )
                .unwrap();
                assert_eq!(
                    paint.commands().len(),
                    p.records().len(),
                    "actual complete paint join"
                );
            }
        }
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        // Negative control: the explicit exact query at2 sees X, exposure sees A.
        assert_ne!(
            contents(&prepare_object_frame(&s, &selector(&s, 2)).unwrap()),
            contents(&prepare_object_exposure_frame(&s, &selector(&s, 2)).unwrap())
        );
    }
}
#[test]
fn missing_mixed_hidden_missing_reference_objects_and_all_bad_pins_refuse_without_fallback() {
    for version in [1, 2] {
        for missing in [1, 2] {
            let mut value = drawing(version);
            for layer in value["layers"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .take(missing)
            {
                layer.as_object_mut().unwrap().remove("timing");
                layer["visible"] = json!(false);
            }
            let s = snapshot(&value);
            assert_eq!(
                prepare_object_exposure_frame(&s, &selector(&s, 2)).unwrap_err(),
                ObjectFrameError::Unavailable
            );
            assert!(
                prepare_object_frame(&s, &selector(&s, 2)).is_ok(),
                "distinct explicit scoped API"
            );
        }
        let s = snapshot(&drawing(version));
        let good = selector(&s, 4);
        for (field, error) in [
            (0, ObjectFrameError::WrongInstance),
            (1, ObjectFrameError::WrongDocument),
            (2, ObjectFrameError::WrongRevision),
            (3, ObjectFrameError::WrongSnapshot),
            (4, ObjectFrameError::Unavailable),
            (5, ObjectFrameError::Unavailable),
            (6, ObjectFrameError::FrameOutOfRange),
        ] {
            let mut bad = good.clone();
            match field {
                0 => bad.instance_id = "foreign",
                1 => bad.document_id = "foreign",
                2 => bad.content_revision += 1,
                3 => bad.document_snapshot_id = "foreign",
                4 => bad.context_id = "foreign",
                5 => bad.scope_kind = FrameScopeKind::Reference,
                _ => bad.frame = 8,
            };
            assert_eq!(prepare_object_exposure_frame(&s, &bad).unwrap_err(), error);
        }
        let mut hidden = drawing(version);
        hidden["layers"][0]["visible"] = json!(false);
        let h = snapshot(&hidden);
        assert_eq!(
            contents(&prepare_object_exposure_frame(&h, &selector(&h, 4)).unwrap()),
            vec![("B".into(), "B2".into(), 2)]
        );
    }
}
#[test]
fn preserved_reference_only_record_at_blank_key_cannot_poison_authored_exposure() {
    let mut value = drawing(2);
    // This key is explicitly BLANK. Only stored Reference data shares its frame.
    value["objects"].as_array_mut().unwrap().last_mut().unwrap()["target"]["frameScope"]["frame"] =
        json!(5);
    assert!(!value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope["frameScope"]["frame"] == 5));
    let s = snapshot(&value);
    let before = encode_storage_project(s.document()).unwrap();
    for (frame, expected) in [(5, vec![("B".into(), "B2".into(), 2)]), (7, vec![])] {
        let selected = selector(&s, frame);
        let p = prepare_object_exposure_frame(&s, &selected).unwrap();
        assert_eq!(p.source_frame_for_layer(0), Some(5));
        assert_eq!(contents(&p), expected);
        #[cfg(feature = "compositor")]
        {
            let scene =
                crate::object_render_scene::prepare_object_render_scene(&p, &selected).unwrap();
            let paint =
                crate::object_render_scene::prepare_object_paint_plan(&scene, &selected).unwrap();
            assert_eq!(paint.commands().len(), expected.len());
        }
    }
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    let saved: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        saved["objects"].as_array().unwrap().last().unwrap()["target"]["frameScope"],
        json!({"kind":"reference","frame":5})
    );
    let reopened = snapshot(&saved);
    assert_eq!(
        contents(&prepare_object_exposure_frame(&reopened, &selector(&reopened, 5)).unwrap()),
        vec![("B".into(), "B2".into(), 2)]
    );
}

fn order_drawing() -> Value {
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":18,
        "layers":[{"layerUid":"A","timing":timing(&[1],json!([]))},{"layerUid":"B","timing":timing(&[2],json!([]))}],
        "objects":[record("A","a",1),record("A","b",1),record("B","b",2)]});
    plan(&mut value);
    let context = &mut value["compositionOrder"]["contexts"][0];
    context["layers"][0]["order"] =
        json!({"track":{"keys":[{"frame":0,"v":[1],"curvePoints":[]},{"frame":10,"v":[3]}]}});
    context["scopes"][0]["elements"][0]["order"] = json!({"static":[1]});
    context["scopes"][0]["elements"][1]["order"] =
        json!({"track":{"keys":[{"frame":0,"v":[0],"curvePoints":[]},{"frame":10,"v":[2]}]}});
    context["scopes"][1]["elements"][0]["order"] = json!({"static":[100]});
    value
}
#[test]
fn actual_order_holder_comes_from_each_source_scope_but_tracks_use_requested_time() {
    let s = snapshot(&order_drawing());
    for (frame, rank, ids) in [(5, 2.0, vec!["a", "b"]), (10, 3.0, vec!["b", "a"])] {
        let p = prepare_object_exposure_frame(&s, &selector(&s, frame)).unwrap();
        let a = p
            .numeric_order()
            .unwrap()
            .layers()
            .iter()
            .find(|l| l.source_layer_index() == 0)
            .unwrap();
        assert_eq!(a.value(), rank);
        assert_eq!(
            a.elements()
                .iter()
                .map(|e| s.document().objects()[e.source_object_index()]
                    .target()
                    .stroke_id())
                .collect::<Vec<_>>(),
            ids
        );
        let b = p
            .numeric_order()
            .unwrap()
            .layers()
            .iter()
            .find(|l| l.source_layer_index() == 1)
            .unwrap();
        assert_eq!(b.elements()[0].value(), 100.0);
        assert_eq!(
            contents(&p),
            vec![
                ("A".into(), "a".into(), 1),
                ("A".into(), "b".into(), 1),
                ("B".into(), "b".into(), 2)
            ]
        );
    }
}
#[cfg(feature = "object_expression_runtime")]
#[test]
fn actual_expression_consumer_pins_source_holder_and_evaluates_requested_frame() {
    use crate::history::NativeObjectHistory;
    use crate::object_expression_runtime::{
        prepare_expression_order_exposure_frame_for_owner, ExpressionLimits,
    };
    let mut value = order_drawing();
    let expression =
        json!({"static":[2],"expression":{"enabled":true,"code":"[value + frame / 5]"}});
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = expression.clone();
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"] = expression;
    let owner = NativeObjectHistory::new(
        "exposure-expression",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let s = owner.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    assert_eq!(
        prepare_object_exposure_frame(&s, &selector(&s, 10)).unwrap_err(),
        ObjectFrameError::Unavailable,
        "no raw active-expression fallback"
    );
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap();
    let p =
        prepare_expression_order_exposure_frame_for_owner(&owner, &s, &selector(&s, 10), limits)
            .unwrap();
    let a = p
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .find(|l| l.source_layer_index() == 0)
        .unwrap();
    assert_eq!(a.value(), 4.0);
    assert_eq!(a.elements()[0].value(), 4.0);
    assert_eq!(a.elements()[1].value(), 2.0);
    assert_eq!(p.source_frame_for_layer(0), Some(1));
    assert_eq!(p.frame(), 10);
    let foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert!(prepare_expression_order_exposure_frame_for_owner(
        &foreign,
        &s,
        &selector(&s, 10),
        limits
    )
    .is_err());
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn fill_undo_redo_save_reopen_and_old_pins_preserve_exposure_without_copying_geometry() {
    use crate::history::NativeObjectHistory;
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":64,"canvasH":64,"fps":24},
        "drawing":drawing(2),"viewportReference":null,"assets":[]});
    let mut owner =
        NativeObjectHistory::from_project_file("exposure-history", project.to_string().as_bytes())
            .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let old_bytes = old.serialize_project_file().unwrap();
    let packet = prepare_object_exposure_frame(&old, &selector(&old, 4)).unwrap();
    let payload = json!({"command":"object.fill.set","stableTarget":drawing(2)["objects"][0]["target"],"fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":1}});
    let candidate = owner
        .prepare_fill_json(payload.to_string().as_bytes())
        .unwrap();
    owner.commit_fill(candidate).unwrap();
    let current = owner.acquire_snapshot(1).unwrap();
    let changed = prepare_object_exposure_frame(&current, &selector(&current, 4)).unwrap();
    assert_eq!(contents(&packet), contents(&changed));
    assert_ne!(
        packet.records().next().unwrap().record(),
        changed.records().next().unwrap().record()
    );
    assert_eq!(old.serialize_project_file().unwrap(), old_bytes);
    owner.undo(1).unwrap();
    assert_eq!(
        owner
            .acquire_snapshot(2)
            .unwrap()
            .serialize_project_file()
            .unwrap(),
        old_bytes
    );
    owner.redo(2).unwrap();
    let bytes = owner
        .acquire_snapshot(3)
        .unwrap()
        .serialize_project_file()
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("exposure-reopen", &bytes).unwrap();
    let fresh = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(
        contents(&prepare_object_exposure_frame(&fresh, &selector(&fresh, 4)).unwrap()),
        contents(&changed)
    );
    assert_eq!(
        contents(&packet),
        vec![("A".into(), "A".into(), 1), ("B".into(), "B2".into(), 2)]
    );
    let new = NativeObjectHistory::new_blank_project("new-exposure", 64, 64, 24.0).unwrap();
    let s = new.acquire_snapshot(0).unwrap();
    assert_eq!(
        prepare_object_exposure_frame(&s, &selector(&s, 17))
            .unwrap()
            .records()
            .len(),
        0
    );
}
