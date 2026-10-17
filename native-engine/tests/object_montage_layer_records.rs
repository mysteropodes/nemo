//! Record, admission and gesture tests of the montage layer join (L10-I): the persisted `montageId` /
//! `symPlacedAt` record, `MontageLayerSet` admission, retime exactness and `placeMontageAsLayer`'s
//! plan, over the real L04-G records. Oracle-driven row comparisons live in the parent test module.
use super::super::rows::{existing_of, world, worlds, World};
use super::super::{
    plan_place_montage, MontageLayer, MontageLayerError, MontageLayerSet, PlaceOutcome,
    ResolvedMontage,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use crate::object_document::ObjectLayer;
use serde_json::{json, Value};

fn scene(uids: &[&str]) -> Vec<ObjectLayer> {
    uids.iter()
        .map(|uid| serde_json::from_value(json!({"layerUid": uid, "name": uid})).expect("layer"))
        .collect()
}

fn set_of(layers: Value) -> MontageLayerSet {
    serde_json::from_value(json!({
        "format": "nemo.native-montage-layers", "formatVersion": 1, "layers": layers
    }))
    .expect("set")
}

/// The world's storyboard with `edit` applied to its JSON value.
fn edited_storyboard(w: &World, edit: impl FnOnce(&mut Value)) -> StoryboardRecord {
    let mut value = serde_json::to_value(&w.storyboard).expect("encode");
    edit(&mut value);
    serde_json::from_value(value).expect("decode")
}

fn module_index(value: &Value, id: &str) -> usize {
    value["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .position(|m| m["id"] == id)
        .expect("module")
}
#[test]
fn placement_and_montage_id_keep_absent_null_and_zero_apart() {
    let texts = [
        r#"{"layerUid":"a","montageId":"mt"}"#,
        r#"{"layerUid":"a","montageId":"mt","symPlacedAt":null}"#,
        r#"{"layerUid":"a","montageId":"mt","symPlacedAt":4}"#,
        r#"{"layerUid":"a","montageId":"mt","symPlacedAt":0}"#,
    ];
    let layers: Vec<MontageLayer> = texts
        .iter()
        .map(|t| serde_json::from_str(t).expect("layer"))
        .collect();
    for (layer, text) in layers.iter().zip(texts) {
        assert_eq!(serde_json::to_string(layer).expect("encode"), text);
    }
    assert_ne!(layers[0], layers[1]);
    assert_ne!(layers[1], layers[3]);
    assert_ne!(layers[2], layers[3]);
    assert_eq!(layers[0].placed_at(), Ok(None));
    assert_eq!(layers[1].placed_at(), Ok(None));
    assert_eq!(layers[2].placed_at(), Ok(Some(4)));
    assert_eq!(layers[3].placed_at(), Ok(Some(0)));
    for bad in [
        r#"{"layerUid":"a","montageId":"mt","extra":1}"#,
        r#"{"layerUid":"a","montageId":"mt","montageId":"m2"}"#,
        r#"["a","mt"]"#,
    ] {
        assert!(serde_json::from_str::<MontageLayer>(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_set_admits_scene_layers_that_are_not_component_instances() {
    let all = worlds();
    let w = world(&all, "plain");
    let scene = scene(&["ly_1", "ly_2"]);
    let ok = |layers: Value| set_of(layers).validate(&w.components, &scene);
    // Two layers of one montage, and a montage that was deleted, are what the baseline allows.
    assert_eq!(
        ok(
            json!([{"layerUid": "ly_1", "montageId": "mt"}, {"layerUid": "ly_2", "montageId": "mt"}])
        ),
        Ok(())
    );
    assert_eq!(
        ok(json!([{"layerUid": "ly_1", "montageId": "gone"}])),
        Ok(())
    );
    assert_eq!(ok(json!([])), Ok(()));
    assert_eq!(
        ok(
            json!([{"layerUid": "ly_1", "montageId": "mt"}, {"layerUid": "ly_1", "montageId": "mt2"}])
        ),
        Err(MontageLayerError::DuplicateLayer)
    );
    assert!(matches!(
        ok(json!([{"layerUid": "zz", "montageId": "mt"}])),
        Err(MontageLayerError::Unresolved(_))
    ));
    assert!(matches!(
        ok(json!([{"layerUid": "ly_1", "montageId": " "}])),
        Err(MontageLayerError::Invalid(_))
    ));
    assert_eq!(
        ok(json!([{"layerUid": "ly_1", "montageId": "mt", "symPlacedAt": 4.5}])),
        Err(MontageLayerError::NonInteger("symPlacedAt"))
    );
    // 2^53 - 1 is the last accepted placement; the next integer is refused.
    assert_eq!(
        ok(json!([{"layerUid": "ly_1", "montageId": "mt", "symPlacedAt": 9007199254740991_i64}])),
        Ok(())
    );
    assert_eq!(
        ok(json!([{"layerUid": "ly_1", "montageId": "mt", "symPlacedAt": 9007199254740992_i64}])),
        Err(MontageLayerError::NonInteger("symPlacedAt"))
    );
    let mut wrong = set_of(json!([]));
    wrong.format_version = 2;
    assert!(matches!(
        wrong.validate(&w.components, &scene),
        Err(MontageLayerError::Unsupported(_))
    ));
    let mut with_instance = serde_json::to_value(&w.components).expect("encode");
    with_instance["instances"] =
        json!([{"contextId": "scene-root", "layerUid": "ly_1", "symbolId": "sa"}]);
    let components: ComponentSet = serde_json::from_value(with_instance).expect("decode");
    assert!(matches!(
        set_of(json!([{"layerUid": "ly_1", "montageId": "mt"}])).validate(&components, &scene),
        Err(MontageLayerError::Invalid(_))
    ));
    assert_eq!(
        set_of(json!([{"layerUid": "ly_2", "montageId": "mt"}])).validate(&components, &scene),
        Ok(())
    );
}

#[test]
fn a_retime_that_is_not_an_exact_integer_is_a_typed_error_for_its_montage_only() {
    let all = worlds();
    let w = world(&all, "plain");
    let fractional = edited_storyboard(w, |v| {
        let i = module_index(v, "i2");
        v["modules"][i]["duration"] = json!(2.5);
    });
    assert_eq!(
        ResolvedMontage::resolve(&fractional, &w.components, "mt").err(),
        Some(MontageLayerError::NonInteger("duration"))
    );
    assert!(ResolvedMontage::resolve(&fractional, &w.components, "mt2").is_ok());
    let beyond = edited_storyboard(w, |v| {
        let i = module_index(v, "i2");
        v["modules"][i]["trimOut"] = json!(9007199254740993_i64);
    });
    assert_eq!(
        ResolvedMontage::resolve(&beyond, &w.components, "mt").err(),
        Some(MontageLayerError::NonInteger("trimOut"))
    );
}

#[test]
fn the_gesture_refuses_before_it_focuses_and_names_its_refusals() {
    let all = worlds();
    let w = world(&all, "plain");
    let plan = |placed: Value, uids: &[&str], montage: &str| {
        plan_place_montage(
            &w.storyboard,
            &w.components,
            &set_of(placed),
            &scene(uids),
            montage,
        )
    };
    let empty = plan(json!([]), &[], "mt3");
    assert_eq!(empty, Err(MontageLayerError::EmptyMontage));
    assert_eq!(
        empty.expect_err("refused").toast_key(),
        Some("toastMontageEmptySnapHint")
    );
    // An emptied chain beats "already placed": the baseline checks the chain first.
    assert_eq!(
        plan(
            json!([{"layerUid": "ly_1", "montageId": "mt3"}]),
            &["ly_1"],
            "mt3"
        ),
        Err(MontageLayerError::EmptyMontage)
    );
    assert_eq!(
        plan(json!([]), &[], "zzz"),
        Err(MontageLayerError::UnknownMontage)
    );
    assert_eq!(
        plan(json!([]), &[], "i1"),
        Err(MontageLayerError::UnknownMontage)
    );
    assert_eq!(MontageLayerError::UnknownMontage.toast_key(), None);
    let unnamed = edited_storyboard(w, |v| {
        let i = module_index(v, "mt");
        v["modules"][i]
            .as_object_mut()
            .expect("module")
            .remove("name");
    });
    assert_eq!(
        plan_place_montage(&unnamed, &w.components, &set_of(json!([])), &[], "mt"),
        Err(MontageLayerError::MissingName)
    );
    // The scene layer order, not the set order, decides which layer is focused.
    let focus = plan(
        json!([{"layerUid": "ly_2", "montageId": "mt"}, {"layerUid": "ly_1", "montageId": "mt"}]),
        &["ly_1", "ly_2"],
        "mt",
    );
    assert_eq!(
        focus,
        Ok(PlaceOutcome::Focus {
            layer_uid: "ly_1".to_string(),
            layer_name: Some("ly_1".to_string())
        })
    );
}

#[test]
fn a_create_plan_binds_to_the_minted_layer_without_a_placement() {
    let all = worlds();
    let w = world(&all, "plain");
    let existing = existing_of("[]");
    let outcome = plan_place_montage(
        &w.storyboard,
        &w.components,
        &existing.placed,
        &existing.scene,
        "mt",
    );
    let plan = match outcome {
        Ok(PlaceOutcome::Create(plan)) => plan,
        other => panic!("expected a create plan, got {other:?}"),
    };
    assert_eq!(plan.layer_name, "Montage 1");
    assert_eq!(plan.montage_id, "mt");
    assert_eq!(plan.total_frames, 35);
    let record = plan.bind("native-layer:7:0".to_string());
    assert_eq!(
        serde_json::to_string(&record).expect("encode"),
        r#"{"layerUid":"native-layer:7:0","montageId":"mt"}"#
    );
}

#[test]
fn the_toast_templates_keep_the_baseline_text() {
    let t = |key: &str| format!("<{key}>");
    let focus = PlaceOutcome::Focus {
        layer_uid: "u".to_string(),
        layer_name: Some("Placed MT".to_string()),
    };
    assert_eq!(
        focus.toast(&t),
        "<toastMontageAlreadyPlacedLayer>Placed MT \u{bb}"
    );
    let nameless = PlaceOutcome::Focus {
        layer_uid: "u".to_string(),
        layer_name: None,
    };
    assert_eq!(
        nameless.toast(&t),
        "<toastMontageAlreadyPlacedLayer>undefined \u{bb}"
    );
    let all = worlds();
    let w = world(&all, "plain");
    let existing = existing_of("[]");
    let created = plan_place_montage(
        &w.storyboard,
        &w.components,
        &existing.placed,
        &existing.scene,
        "mt",
    )
    .expect("create");
    assert_eq!(
        created.toast(&t),
        "<toastMontagePlacedAsLayerSuffix>Montage 1 \u{bb} (35 images, en boucle)"
    );
}
