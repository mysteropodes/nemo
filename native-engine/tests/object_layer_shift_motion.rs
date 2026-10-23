//! Hand-expected Motion frames through actual storage admission, never a parallel track store.
// Same-lexical fixture/support extraction preserves original test names and bodies.
include!("object_layer_shift_motion_fixture.rs");
#[test]
fn whole_layer_ignores_layer_lock_and_shifts_all_native_holder_and_order_tracks_once() {
    let source = fixture();
    let mut document = admit(source.clone());
    assert!(shift_layer(&mut document, "scene-root", "L0", 2).unwrap());
    let mut expected = source;
    expected
        .pointer_mut(PROPERTY)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .pop();
    set_frames(&mut expected, PROPERTY, &[0.0, 2.0, 4.25, 9.0]);
    set_frames(&mut expected, ELEMENT, &[3.0, 9.0]);
    set_frames(&mut expected, LAYER_ORDER, &[2.0, 8.0]);
    set_frames(&mut expected, ELEMENT_ORDER, &[4.0, 9.0]);
    // A clamped unchanged frame must preserve its original integer Number spelling.
    expected.pointer_mut(ELEMENT).unwrap()[1]["frame"] = json!(9);
    assert_eq!(wire(&document), expected);
    assert_eq!(wire(&admit(wire(&document))), expected);
}

#[test]
fn nonroot_existing_context_holder_layer_order_and_scoped_order_have_isolated_retiming() {
    let source = fixture();
    let mut document = admit(source.clone());
    assert!(shift_layer(&mut document, "ctx-x", "L0", -3).unwrap());
    let mut expected = source;
    set_frames(
        &mut expected,
        "/propertyHolders/2/properties/0/track/keys",
        &[0.0, 1.5],
    );
    set_frames(
        &mut expected,
        "/compositionOrder/contexts/1/layers/0/order/track/keys",
        &[0.0],
    );
    set_frames(
        &mut expected,
        "/compositionOrder/contexts/1/scopes/0/elements/0/order/track/keys",
        &[0.0, 2.0],
    );
    assert_eq!(wire(&document), expected);
}

#[test]
fn genuine_component_time_remap_shifts_outside_property_holders_and_preserves_instance_fields() {
    let source = fixture();
    let mut document = admit(source.clone());
    assert!(shift_layer(&mut document, "scene-root", "L1", -2).unwrap());
    let mut expected = source;
    expected
        .pointer_mut(REMAP)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .remove(1);
    set_frames(&mut expected, REMAP, &[0.0, 6.0]);
    expected.pointer_mut(REMAP).unwrap()[0]["frame"] = json!(0);
    assert_eq!(wire(&document), expected);
    assert_eq!(wire(&admit(wire(&document))), expected);
}

#[test]
fn selected_original_key_addresses_are_captured_before_any_key_can_reach_another_selected_frame() {
    let mut source = fixture();
    *source.pointer_mut(PROPERTY).unwrap() =
        json!([{"frame":0,"v":[10]},{"frame":2,"v":[20]},{"frame":5,"v":[30]}]);
    let target = property("scene-root", "L0", None, "opacity");
    let mut document = admit(source.clone());
    assert!(shift_selected(
        &mut document,
        &[key(target.clone(), json!(0)), key(target, json!(2))],
        2
    )
    .unwrap());
    let mut expected = source;
    set_frames(&mut expected, PROPERTY, &[2.0, 4.0, 5.0]);
    expected.pointer_mut(PROPERTY).unwrap()[2]["frame"] = json!(5);
    assert_eq!(wire(&document), expected);
}

#[test]
fn selected_key_collision_keeps_earliest_original_key_even_if_unselected_or_shifted() {
    let target = property("scene-root", "L0", None, "opacity");
    for (selected, dx, survivor) in [(0, 2, 10), (4, -2, 20)] {
        let mut source = fixture();
        *source.pointer_mut(PROPERTY).unwrap() = json!([{"frame":0,"v":[10],"hold":true},{"frame":2,"v":[20],"lockTo":"in"},{"frame":4,"v":[30]}]);
        let mut document = admit(source.clone());
        assert!(
            shift_selected(&mut document, &[key(target.clone(), json!(selected))], dx).unwrap()
        );
        let actual = wire(&document);
        let keys = actual.pointer(PROPERTY).unwrap().as_array().unwrap();
        assert_eq!(keys.len(), 2);
        let collision = keys
            .iter()
            .find(|entry| entry["frame"].as_f64() == Some(2.0))
            .unwrap();
        assert_eq!(collision["v"], json!([survivor]));
        if selected == 0 {
            assert_eq!(collision["hold"], true);
            assert!(collision.get("lockTo").is_none());
        } else {
            assert_eq!(collision["lockTo"], "in");
        }
        let mut expected = source;
        *expected.pointer_mut(PROPERTY).unwrap() = if selected == 0 {
            json!([{"frame":2.0,"v":[10],"hold":true},{"frame":4,"v":[30]}])
        } else {
            json!([{"frame":0,"v":[10],"hold":true},{"frame":2,"v":[20],"lockTo":"in"}])
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn numeric_negative_zero_collision_uses_js_numeric_ties_not_total_float_order() {
    let mut source = fixture();
    *source.pointer_mut(PROPERTY).unwrap() = json!([{"frame":-1,"v":[11]},{"frame":-0.0,"v":[22]}]);
    let mut document = admit(source.clone());
    assert!(shift_selected(
        &mut document,
        &[key(
            property("scene-root", "L0", None, "opacity"),
            json!(-1)
        )],
        1
    )
    .unwrap());
    let mut expected = source;
    *expected.pointer_mut(PROPERTY).unwrap() = json!([{"frame":0.0,"v":[11]}]);
    assert_eq!(wire(&document), expected);
}

#[test]
fn selected_mixed_native_targets_shift_only_listed_keys_with_literal_frame_and_metadata_preservation(
) {
    let source = fixture();
    let mut document = admit(source.clone());
    let selected = vec![
        key(
            property("scene-root", "L0", Some("a"), "rotation"),
            json!(1),
        ),
        key(layer_order("scene-root", "L0"), json!(6)),
        key(element_order("scene-root", "L0", 0, "a"), json!(8.5)),
        key(remap(), json!(8)),
    ];
    assert!(shift_selected(&mut document, &selected, -1).unwrap());
    let mut expected = source;
    expected.pointer_mut(ELEMENT).unwrap()[0]["frame"] = json!(0.0);
    expected.pointer_mut(LAYER_ORDER).unwrap()[1]["frame"] = json!(5.0);
    expected.pointer_mut(ELEMENT_ORDER).unwrap()[1]["frame"] = json!(7.5);
    expected.pointer_mut(REMAP).unwrap()[2]["frame"] = json!(7.0);
    assert_eq!(wire(&document), expected);
}

#[test]
fn stale_missing_and_duplicate_selected_addresses_refuse_atomically_even_for_zero_delta() {
    let target = property("scene-root", "L0", None, "opacity");
    let good = key(target.clone(), json!(0));
    for missing in [
        key(target.clone(), json!(0.25)),
        key(property("scene-root", "L0", None, "missing"), json!(0)),
        key(property("absent", "L0", None, "opacity"), json!(0)),
        key(element_order("scene-root", "L0", 3, "absent"), json!(2)),
    ] {
        for dx in [0, 1] {
            let mut document = admit(fixture());
            let before = bytes(&document);
            assert_eq!(
                shift_selected(&mut document, &[good.clone(), missing.clone()], dx),
                Err(DispatchErrorCode::NotFound)
            );
            assert_eq!(bytes(&document), before);
        }
    }
    for dx in [0, 1] {
        let mut document = admit(fixture());
        let before = bytes(&document);
        let duplicate = key(target.clone(), json!(0.0));
        assert_eq!(
            shift_selected(&mut document, &[good.clone(), duplicate], dx),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(bytes(&document), before);
    }
}

#[test]
fn selected_wire_is_map_only_closed_duplicate_safe_and_supports_all_typed_target_variants() {
    let source = admit(fixture());
    let targets = [
        property("scene-root", "L0", None, "opacity"),
        layer_order("scene-root", "L0"),
        element_order("scene-root", "L0", 0, "a"),
        remap(),
    ];
    for (target, frame) in targets
        .into_iter()
        .zip([json!(0), json!(0), json!(2), json!(0)])
    {
        let raw = json!({"target":target,"frame":frame}).to_string();
        let selected: SelectedMotionKey = serde_json::from_slice(raw.as_bytes()).unwrap();
        validate_selected(&source, &[selected.clone()]).unwrap();
        assert_eq!(
            serde_json::to_value(selected).unwrap(),
            serde_json::from_str::<Value>(&raw).unwrap()
        );
    }
    for raw in [
        r#"[]"#,
        r#"[{},0]"#,
        r#"{"target":null,"frame":0}"#,
        r#"{"target":["layerOrder","scene-root","L0"],"frame":0}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0","extra":1},"frame":0}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0","layerUid":"L1"},"frame":0}"#,
        r#"{"target":{"kind":"layerOrder","kind":"timeRemap","contextId":"scene-root","layerUid":"L0"},"frame":0}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"frame":0,"frame":1}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"target":{},"frame":0}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"frame":false}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"frame":null}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"frame":"0"}"#,
        r#"{"target":{"kind":"layerOrder","contextId":"scene-root","layerUid":"L0"},"frame":0,"extra":0}"#,
    ] {
        assert!(
            serde_json::from_slice::<SelectedMotionKey>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn standing_key_modes_collect_property_and_order_tracks_but_never_time_remap_or_null_locks() {
    let document = admit(fixture());
    let serialize = |keys: Vec<SelectedMotionKey>| {
        keys.into_iter()
            .map(|key| serde_json::to_value(key).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        serialize(locked_keys(&document, "scene-root", "L0", KeyLockMode::In)),
        vec![
            json!({"target":property("scene-root","L0",None,"opacity"),"frame":0}),
            json!({"target":property("scene-root","L0",Some("a"),"rotation"),"frame":1}),
            json!({"target":layer_order("scene-root","L0"),"frame":0}),
        ]
    );
    assert_eq!(
        serialize(locked_keys(&document, "scene-root", "L0", KeyLockMode::Out)),
        vec![
            json!({"target":property("scene-root","L0",None,"opacity"),"frame":8}),
            json!({"target":layer_order("scene-root","L0"),"frame":6}),
        ]
    );
    assert_eq!(
        serialize(locked_keys(
            &document,
            "scene-root",
            "L0",
            KeyLockMode::Layer
        )),
        vec![
            json!({"target":property("scene-root","L0",None,"opacity"),"frame":12}),
            json!({"target":property("scene-root","L0",Some("a"),"rotation"),"frame":9}),
            json!({"target":element_order("scene-root","L0",0,"a"),"frame":2}),
        ]
    );
    assert!(locked_keys(&document, "scene-root", "L1", KeyLockMode::In).is_empty());
    assert!(locked_keys(&document, "ctx-x", "L0", KeyLockMode::In).is_empty());
}

#[test]
fn whole_then_standing_lock_pass_can_intentionally_shift_the_same_actual_key_twice() {
    let mut source = fixture();
    *source.pointer_mut(PROPERTY).unwrap() =
        json!([{"frame":0,"v":[1],"lockTo":"in"},{"frame":4,"v":[2]}]);
    let mut document = admit(source.clone());
    assert!(shift_layer(&mut document, "scene-root", "L0", 1).unwrap());
    let selected = locked_keys(&document, "scene-root", "L0", KeyLockMode::In);
    assert!(shift_selected(&mut document, &selected, 1).unwrap());
    let actual = wire(&document);
    assert_eq!(
        actual.pointer(PROPERTY).unwrap(),
        &json!([{"frame":2.0,"v":[1],"lockTo":"in"},{"frame":5.0,"v":[2]}])
    );
    assert_eq!(actual.pointer(ELEMENT).unwrap()[0]["frame"], json!(3.0));
    assert_eq!(actual.pointer(LAYER_ORDER).unwrap()[0]["frame"], json!(2.0));
    assert_eq!(actual["motionArcs"], source["motionArcs"]);
}

#[test]
fn zero_empty_and_clamped_nonempty_tracks_keep_baseline_touched_semantics_and_number_spelling() {
    let mut source = fixture();
    *source.pointer_mut(PROPERTY).unwrap() = json!([{"frame":0.0,"v":[1]}]);
    source["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    source["compositionOrder"]["contexts"][0]["layers"][0]["order"]
        .as_object_mut()
        .unwrap()
        .remove("track");
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]
        .as_object_mut()
        .unwrap()
        .remove("track");
    let mut document = admit(source.clone());
    let before = bytes(&document);
    assert!(!shift_layer(&mut document, "scene-root", "L0", 0).unwrap());
    assert!(shift_layer(&mut document, "scene-root", "L0", -1).unwrap());
    assert_eq!(
        bytes(&document),
        before,
        "0.0 spelling and no-op metadata survive"
    );
    assert!(shift_selected(
        &mut document,
        &[key(property("scene-root", "L0", None, "opacity"), json!(0))],
        -1
    )
    .unwrap());
    assert_eq!(bytes(&document), before);
    assert!(!shift_selected(&mut document, &[], 1).unwrap());
    assert!(
        !shift_layer(&mut document, "symbol", "inside", 1).unwrap(),
        "existing inner context has no native Motion tracks"
    );
    *source.pointer_mut(PROPERTY).unwrap() = json!([]);
    let mut empty = admit(source);
    let before = bytes(&empty);
    assert!(!shift_layer(&mut empty, "scene-root", "L0", 1).unwrap());
    assert_eq!(bytes(&empty), before);
}

#[test]
fn fractional_frame_clamps_at_u32_max_span_without_dense_timeline_or_integer_truncation() {
    let mut source = fixture();
    source["totalFrames"] = json!(u32::MAX);
    *source.pointer_mut(PROPERTY).unwrap() = json!([{"frame":4294967293.5,"v":[1],"lockTo":null}]);
    let mut document = admit(source.clone());
    assert!(shift_selected(
        &mut document,
        &[key(
            property("scene-root", "L0", None, "opacity"),
            json!(4294967293.5)
        )],
        5
    )
    .unwrap());
    let mut expected = source;
    expected.pointer_mut(PROPERTY).unwrap()[0]["frame"] = json!(4294967294.0);
    assert_eq!(wire(&document), expected);
}

#[test]
fn unsafe_delta_or_unknown_scope_refuses_without_any_preceding_track_mutation() {
    let mut document = admit(fixture());
    let before = bytes(&document);
    for dx in [i64::MIN, i64::MAX, -9007199254740992, 9007199254740992] {
        assert_eq!(
            shift_layer(&mut document, "scene-root", "L0", dx),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(
            shift_selected(
                &mut document,
                &[key(property("scene-root", "L0", None, "opacity"), json!(0))],
                dx
            ),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(bytes(&document), before);
    }
    assert_eq!(
        shift_layer(&mut document, "missing", "L0", 1),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        shift_layer(&mut document, "scene-root", "missing", 1),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        shift_layer(&mut document, "", "L0", 1),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(bytes(&document), before);
}

#[test]
fn whole_layer_reaches_extended_3d_and_custom_control_properties_beyond_the_base_five() {
    let source = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":10,
        "layers":[{"layerUid":"L0","locked":true}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},
        "controls":[{"key":"xc_gain","name":"Gain","type":"number","default":[3]}],
        "properties":[
            {"property":"rotationX","track":{"keys":[{"frame":1.5,"v":[30]}]}},
            {"property":"positionZ","static":[4],"track":{"keys":[{"frame":2,"v":[20]}]}},
            {"property":"xc_gain","static":[3],"expression":{"enabled":false,"code":"value"},"track":{"keys":[{"frame":3,"v":[8],"lockTo":"out"}]}},
            {"property":"opacity","static":[17]}]}]});
    let mut document = admit(source.clone());
    assert!(shift_layer(&mut document, "scene-root", "L0", 2).unwrap());
    let mut expected = source;
    for (index, frame) in [3.5, 4.0, 5.0].into_iter().enumerate() {
        expected["propertyHolders"][0]["properties"][index]["track"]["keys"][0]["frame"] =
            json!(frame);
    }
    assert_eq!(wire(&document), expected);
    assert_eq!(
        locked_keys(&document, "scene-root", "L0", KeyLockMode::Out).len(),
        1
    );
}
