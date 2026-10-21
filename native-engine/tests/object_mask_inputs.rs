//! Real retained owner -> Frame -> scene -> paint/compose inputs. Runtime UNRUN.
//! Literal geometry: build_bezpath_from_segments, then Position [7,-3].
//! Original mask contract: engine-bridge.js:925-934,2024-2035 (fbe4b8a3).
use super::{ObjectMaskInputs, ObjectMaskMode};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, ScenePin,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    plan_scene_effects, JoinOptions,
};
use crate::object_render_scene::{
    ordered_entries, paint_test_document, prepare_object_paint_plan, prepare_object_render_scene,
    ObjectRenderScene, ObjectSceneError,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::{PathEl, Point};

fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "native-mask-inputs",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn vector(record: &mut Value) {
    record["family"] = json!("editable-vector-path");
    record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
        "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":0});
}
fn document() -> Value {
    let mut v = paint_test_document(&[7]);
    for record in v["objects"].as_array_mut().unwrap() {
        if record["target"]["layerUid"] != "L0" {
            continue;
        }
        vector(record);
        let (mode, feather) = match record["target"]["strokeId"].as_str().unwrap() {
            "a" => ("unknown-mode", -3.),
            "b" => ("subtract", 4.),
            "c" => ("intersect", 2.),
            _ => unreachable!(),
        };
        record["vectorExt"] = json!({"isMask":true,"maskMode":mode,"maskFeather":feather});
        // No normal paint; native clipping must still use this exact curved path.
        record["fill"] = Value::Null;
        record["geometry"]["segments"][0]["handleOut"] = json!({"x":3,"y":4});
        record["geometry"]["segments"][1]["handleIn"] = json!({"x":-2,"y":1});
        record["effects"] = json!([{"type":"blur","enabled":true,"p1":3}]);
    }
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","static":[7,-3]}]}]);
    v
}
fn scene(s: &ObjectSnapshot) -> ObjectRenderScene {
    let p = prepare_object_frame(s, &pins(s, 7)).unwrap();
    prepare_object_render_scene(&p, &pins(s, 7)).unwrap()
}

#[test]
fn retained_owner_masks_reach_actual_curved_scene_and_leave_ordinary_paint_only() {
    let o = owner(&document());
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let scene = scene(&s);
    let plan = prepare_object_paint_plan(&scene, &pins(&s, 7)).unwrap();
    let inputs = plan.mask_inputs();
    assert!(inputs.is_for_packet(plan.packet()));
    let layer = inputs.layer(1).unwrap();
    assert_eq!(layer.source_layer_index(), 1);
    assert_eq!(layer.masks().len(), 3);
    assert_eq!(layer.feather(), 4.);
    // Authored source encounter c,b,a; element Order is a,b,c. No mask re-sort.
    assert_eq!(
        layer
            .masks()
            .iter()
            .map(|m| m.source_object_index())
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(
        layer.masks().iter().map(|m| m.mode()).collect::<Vec<_>>(),
        [
            ObjectMaskMode::Intersect,
            ObjectMaskMode::Subtract,
            ObjectMaskMode::Add
        ]
    );
    for mask in layer.masks() {
        assert_eq!(mask.rgba(), [1.; 4]);
        assert!(mask.duplicator_paint_index().is_none());
        let path = plan.mask_path(mask).unwrap();
        assert_eq!(path.elements()[0], PathEl::MoveTo(Point::new(7., -3.)));
        assert_eq!(
            path.elements()[1],
            PathEl::CurveTo(
                Point::new(10., 1.),
                Point::new(15., -2.),
                Point::new(17., -3.)
            )
        );
        let entry = scene.entry(mask.ordinary_entry_index().unwrap()).unwrap();
        assert!(
            std::ptr::eq(path, entry.path()),
            "borrow the original path allocation"
        );
    }
    assert_eq!(
        scene.entries().len(),
        5,
        "mask sources remain produced/accounted"
    );
    assert_eq!(plan.commands().len(), 2);
    assert_eq!(ordered_entries(&plan).count(), 2);
    assert!(plan.commands().all(|c| !c.source().record().is_mask()));
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    drop(scene);
    drop(s);
    drop(o);
    assert_eq!(
        plan.mask_path(&plan.mask_inputs().layer(1).unwrap().masks()[0])
            .unwrap()
            .elements()[0],
        PathEl::MoveTo(Point::new(7., -3.))
    );
    assert_eq!(
        encode_storage_project(plan.packet().snapshot().document()).unwrap(),
        before
    );
}

#[test]
fn actual_source_claims_and_layer_composition_count_the_produced_masks() {
    let o = owner(&document());
    let s = o.acquire_snapshot(0).unwrap();
    let scene = scene(&s);
    let mut claims = ProducerOutput::new("native-contour", ScenePin::of_packet(scene.packet()));
    for (index, entry) in scene.entries().enumerate() {
        claims = claims.with_item(entry.source().source_object_index(), index);
    }
    let joined = join_scene(
        scene.packet(),
        &ScenePin::of_packet(scene.packet()),
        vec![claims],
    )
    .unwrap();
    let effects =
        plan_scene_effects(scene.packet(), &joined, &JoinOptions::new(1., &|_| false)).unwrap();
    assert!(effects.is_complete());
    assert_eq!(
        effects
            .layers()
            .iter()
            .map(|l| l.entries.len())
            .sum::<usize>(),
        5
    );
    assert_eq!(effects.mask_count(1), Some(3));
    assert_eq!(effects.mask_feather(1), Some(4.));
    let wire = effects.wire_index_of_source(1).unwrap();
    assert_eq!(effects.composition()[wire].mask_count, 3);
    assert_eq!(effects.mask_count(0), Some(0));
    assert_eq!(effects.mask_feather(0), Some(0.));
    assert_eq!(effects.mask_count(99), None);
}

#[test]
fn absence_null_false_and_zero_keep_source_presence_but_do_not_invent_masks_or_feather() {
    for flag in [None, Some(Value::Null), Some(json!(false))] {
        let mut v = document();
        v.as_object_mut().unwrap().remove("propertyHolders");
        for record in v["objects"].as_array_mut().unwrap() {
            if record["target"]["layerUid"] != "L0" {
                continue;
            }
            record.as_object_mut().unwrap().remove("effects");
            record["vectorExt"] = json!({"maskMode":"subtract","maskFeather":4});
            if let Some(flag) = &flag {
                record["vectorExt"]["isMask"] = flag.clone();
            }
        }
        let o = owner(&v);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        let scene = scene(&s);
        let plan = prepare_object_paint_plan(&scene, &pins(&s, 7)).unwrap();
        assert!(plan.mask_inputs().layers().is_empty());
        assert_eq!(plan.commands().len(), 5);
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    }
    for mode in [None, Some(Value::Null), Some(json!("")), Some(json!("ADD"))] {
        let mut v = document();
        for record in v["objects"].as_array_mut().unwrap() {
            if record["target"]["layerUid"] != "L0" {
                continue;
            }
            record["vectorExt"] = json!({"isMask":true,"maskFeather":0});
            if let Some(mode) = &mode {
                record["vectorExt"]["maskMode"] = mode.clone();
            }
        }
        let o = owner(&v);
        let s = o.acquire_snapshot(0).unwrap();
        let scene = scene(&s);
        let plan = prepare_object_paint_plan(&scene, &pins(&s, 7)).unwrap();
        let layer = plan.mask_inputs().layer(1).unwrap();
        assert_eq!(layer.feather(), 0.);
        assert!(layer
            .masks()
            .iter()
            .all(|m| m.mode() == ObjectMaskMode::Add));
    }
}

#[test]
fn hidden_folder_widget_and_effect_slots_have_no_own_masks() {
    for flags in [
        json!({"visible":false}),
        json!({"isFolderLayer":true}),
        json!({"isEffectLayer":true}),
    ] {
        let mut v = document();
        v.as_object_mut().unwrap().remove("propertyHolders");
        // Use the original role field spellings; storage, not this test, owns admission.
        for (key, value) in flags.as_object().unwrap() {
            v["layers"][1][key] = value.clone();
        }
        if flags["isFolderLayer"].as_bool() == Some(true)
            || flags["isEffectLayer"].as_bool() == Some(true)
        {
            let rejected = decode_storage_project(&serde_json::to_vec(&v).unwrap()).unwrap_err();
            assert_eq!(
                rejected.kind(),
                crate::object_codec::ObjectCodecErrorKind::Invalid
            );
            assert_eq!(
                rejected.message(),
                "an effect layer or a folder layer carries no objects"
            );
            v["objects"]
                .as_array_mut()
                .unwrap()
                .retain(|record| record["target"]["layerUid"] != "L0");
            for scope in v["compositionOrder"]["contexts"][0]["scopes"]
                .as_array_mut()
                .unwrap()
            {
                scope["elements"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|element| element["layerUid"] != "L0");
            }
        } else {
            assert_eq!(
                v["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|record| record["target"]["layerUid"] == "L0")
                    .count(),
                3
            );
        }
        let o = owner(&v);
        let s = o.acquire_snapshot(0).unwrap();
        let scene = scene(&s);
        let inputs = ObjectMaskInputs::from_scene(&scene).unwrap();
        assert!(inputs.layers().is_empty(), "{flags}");
        let summary = super::mask_summary(scene.packet()).unwrap();
        assert!(summary.is_empty());
    }
    let mut v = document();
    v["layers"][1]["widget"] = json!({"kind":"slider","x":{"key":"kx","min":0,"max":1}});
    // Existing codec forbids ANY object on a widget; do not manufacture that fixture.
    assert!(decode_storage_project(&serde_json::to_vec(&v).unwrap()).is_err());
    v["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["target"]["layerUid"] != "L0");
    v["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["layerUid"] != "L0");
    v.as_object_mut().unwrap().remove("propertyHolders");
    let o = owner(&v);
    let s = o.acquire_snapshot(0).unwrap();
    assert!(ObjectMaskInputs::from_scene(&scene(&s))
        .unwrap()
        .layers()
        .is_empty());
}

#[test]
fn mask_indices_cannot_resolve_foreign_scene_or_caller_cloned_input() {
    let o = owner(&document());
    let s = o.acquire_snapshot(0).unwrap();
    let first = scene(&s);
    let inputs = ObjectMaskInputs::from_scene(&first).unwrap();
    let mask = &inputs.layer(1).unwrap().masks()[0];
    let other = owner(&document());
    let other_s = other.acquire_snapshot(0).unwrap();
    let second = scene(&other_s);
    assert!(!inputs.is_for_packet(second.packet()));
    assert!(inputs.path_in_scene(&second, mask).is_none());
    let detached = (*mask).clone();
    assert!(inputs.path_in_scene(&first, &detached).is_none());
    assert!(inputs.path_in_scene(&first, mask).is_some());
}

#[test]
fn unsupported_mask_geometry_refuses_instead_of_drawing_an_unmasked_scene() {
    let mut v = document();
    for record in v["objects"].as_array_mut().unwrap() {
        if record["target"]["layerUid"] != "L0" {
            continue;
        }
        for segment in record["geometry"]["segments"].as_array_mut().unwrap() {
            *segment =
                json!({"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
        }
    }
    let o = owner(&v);
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let p = prepare_object_frame(&s, &pins(&s, 7)).unwrap();
    assert!(matches!(
        prepare_object_render_scene(&p, &pins(&s, 7)),
        Err(ObjectSceneError::UnsupportedGeometry {
            reason: "vector path has no extent",
            ..
        })
    ));
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn original_mask_translation_does_not_admit_expanded_or_parent_batch_transforms() {
    for kind in ["rotation", "scale", "parent"] {
        let mut v = document();
        if kind == "parent" {
            v["layers"][1]["parentLayerUid"] = json!("L1");
            v["propertyHolders"].as_array_mut().unwrap().push(json!({
                "target":{"contextId":"scene-root","layerUid":"L1"},
                "properties":[{"property":"position","static":[1,0]}]}));
        } else {
            let value = if kind == "rotation" {
                json!([30])
            } else {
                json!([110, 100])
            };
            v["propertyHolders"][0]["properties"]
                .as_array_mut()
                .unwrap()
                .push(json!({"property":kind,"static":value}));
        }
        let o = owner(&v);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        let p = prepare_object_frame(&s, &pins(&s, 7)).unwrap();
        assert!(matches!(
            prepare_object_render_scene(&p, &pins(&s, 7)),
            Err(ObjectSceneError::UnsupportedGeometry {
                reason:
                    "layer Motion on mixed mask, Text or raster geometry awaits one complete batch",
                ..
            })
        ));
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}

#[test]
fn actual_temporal_duplicator_masks_use_derived_paths_and_never_the_extra_host_seed() {
    use crate::object_frame_packet::duplicators::tests::{fixture, selector, snapshot};
    use crate::object_frame_packet::prepare_object_exposure_frame;
    let mut v = fixture(1);
    for record in v["objects"].as_array_mut().unwrap() {
        vector(record);
        record["vectorExt"] = json!({"isMask":true,"maskMode":"subtract","maskFeather":3});
        record["fill"] = Value::Null;
    }
    let s = snapshot(v);
    let before = encode_storage_project(s.document()).unwrap();
    let p = prepare_object_exposure_frame(&s, &selector(&s, 2)).unwrap();
    let scene = prepare_object_render_scene(&p, &selector(&s, 2)).unwrap();
    let plan = prepare_object_paint_plan(&scene, &selector(&s, 2)).unwrap();
    let layer = plan.mask_inputs().layer(0).unwrap();
    assert_eq!(layer.masks().len(), 3);
    assert_eq!(layer.feather(), 3.);
    assert_eq!(
        layer
            .masks()
            .iter()
            .map(|m| m.source_object_index())
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
    let starts = layer
        .masks()
        .iter()
        .map(|mask| {
            assert!(mask.ordinary_entry_index().is_none());
            assert_eq!(mask.mode(), ObjectMaskMode::Subtract);
            match plan.mask_path(mask).unwrap().elements()[0] {
                PathEl::MoveTo(p) => [p.x, p.y],
                _ => panic!("cubic start"),
            }
        })
        .collect::<Vec<_>>();
    // Original production duplicator fixture literals, not a call to its evaluator.
    assert_eq!(starts, [[20., 20.], [20., 20.], [30., 20.]]);
    assert_eq!(plan.commands().len(), 0);
    assert_eq!(ordered_entries(&plan).count(), 0);
    assert_eq!(plan.packet().records().len(), 1);
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
}
