//! Negative and guard controls of the widget transactions: single-flaw payloads against accepted twins,
//! missing controls, sealed candidates, dispatch envelope refusals and oracle non-vacuity.
use super::object_widget_edit_rows::*;
use super::*;
use serde_json::{json, Value};

/// Every flaw is refused as `invalid_request` and leaves wire, revision and history untouched.
fn assert_refused(make: impl Fn() -> NativeObjectHistory, op: &str, flaws: Vec<(&str, Value)>) {
    for (name, payload) in flaws {
        let mut owner = make();
        let before = state(&owner);
        let reply = run(&mut owner, name, op, payload);
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), "invalid_request", "{name}");
        assert_eq!(state(&owner), before, "{name}");
    }
}

#[test]
fn single_flaw_creation_payloads_are_refused_and_their_twin_is_accepted() {
    let twin = || create_payload("joystick", "Joystick 1", [960., 540.]);
    let mut accepted = owner_of(&base(1));
    assert_eq!(run(&mut accepted, "twin", "create", twin())["ok"], true);
    let flaws = vec![
        ("kind_unknown", with(twin(), |p| p["kind"] = json!("dial"))),
        ("kind_case", with(twin(), |p| p["kind"] = json!("Joystick"))),
        ("kind_empty", with(twin(), |p| p["kind"] = json!(""))),
        ("kind_number", with(twin(), |p| p["kind"] = json!(3))),
        (
            "kind_missing",
            with(twin(), |p| drop(p.as_object_mut().unwrap().remove("kind"))),
        ),
        (
            "name_missing",
            with(twin(), |p| drop(p.as_object_mut().unwrap().remove("name"))),
        ),
        ("name_number", with(twin(), |p| p["name"] = json!(7))),
        (
            "pos_missing",
            with(twin(), |p| drop(p.as_object_mut().unwrap().remove("pos"))),
        ),
        ("pos_short", with(twin(), |p| p["pos"] = json!([1]))),
        ("pos_long", with(twin(), |p| p["pos"] = json!([1, 2, 3]))),
        ("pos_null", with(twin(), |p| p["pos"] = json!(null))),
        (
            "pos_strings",
            with(twin(), |p| p["pos"] = json!(["a", "b"])),
        ),
        ("pos_huge", with(twin(), |p| p["pos"] = json!([1e10, 0]))),
        (
            "pos_negative_huge",
            with(twin(), |p| p["pos"] = json!([0, -1e10])),
        ),
        (
            "unknown_member",
            with(twin(), |p| p["layerUid"] = json!("chosen")),
        ),
        (
            "command_wrong",
            with(twin(), |p| p["command"] = json!("object.layer.create")),
        ),
        ("not_an_object", json!(["object.widget.create"])),
    ];
    assert_refused(|| owner_of(&base(1)), "create", flaws);
}

#[test]
fn raw_creation_bytes_refuse_duplicates_and_oversize_and_accept_the_padded_twin() {
    let owner = owner_of(&base(1));
    let original = state(&owner);
    let duplicate = r#"{"command":"object.widget.create","kind":"joystick","kind":"slider","name":"A","pos":[1,2]}"#;
    // The first member name is spelled with an escape (u006d is m) yet still duplicates the second.
    let escaped = "{\"command\":\"object.widget.create\",\"kind\":\"joystick\",\"na\\u006de\":\"A\",\"name\":\"B\",\"pos\":[1,2]}";
    for raw in [duplicate, escaped, "{}", "[]", "null"] {
        assert_eq!(
            owner
                .prepare_widget_create_json(raw.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{raw}"
        );
    }
    let mut padded = create_payload("slider", "Slider 1", [960., 540.]).to_string();
    padded.push_str(&" ".repeat(4096 - padded.len()));
    assert!(owner.prepare_widget_create_json(padded.as_bytes()).is_ok());
    padded.push(' ');
    assert_eq!(
        owner
            .prepare_widget_create_json(padded.as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(state(&owner), original, "preparation changes nothing");
}

#[test]
fn single_flaw_write_payloads_are_refused_before_any_control_is_touched() {
    let twin = || write_payload(UID, json!(3), local([10., -10.]));
    let world = |to: Value| write_payload(UID, json!(3), to);
    let flaws = vec![
        (
            "to_kind_unknown",
            world(json!({"kind":"polar","point":[1,2]})),
        ),
        ("to_kind_missing", world(json!({"point":[1,2]}))),
        ("to_not_an_object", world(json!("local"))),
        ("local_without_point", world(json!({"kind":"local"}))),
        (
            "local_with_anchor",
            world(json!({"kind":"local","point":[1,2],"anchor":[0,0]})),
        ),
        (
            "local_with_angle",
            world(json!({"kind":"local","point":[1,2],"angleDegrees":30})),
        ),
        (
            "world_without_anchor",
            world(json!({"kind":"world","point":[1,2],"angleDegrees":30})),
        ),
        (
            "world_without_angle",
            world(json!({"kind":"world","point":[1,2],"anchor":[0,0]})),
        ),
        (
            "world_without_point",
            world(json!({"kind":"world","anchor":[0,0],"angleDegrees":30})),
        ),
        (
            "rest_with_point",
            world(json!({"kind":"rest","point":[1,2]})),
        ),
        (
            "point_one_number",
            world(json!({"kind":"local","point":[1]})),
        ),
        ("point_null", world(json!({"kind":"local","point":null}))),
        (
            "point_huge",
            world(json!({"kind":"local","point":[1e10,0]})),
        ),
        (
            "anchor_huge",
            world(json!({"kind":"world","point":[1,2],"anchor":[0,-1e10],"angleDegrees":0})),
        ),
        (
            "angle_huge",
            world(json!({"kind":"world","point":[1,2],"anchor":[0,0],"angleDegrees":1e10})),
        ),
        (
            "unknown_to_member",
            world(json!({"kind":"local","point":[1,2],"snap":true})),
        ),
        ("frame_null", with(twin(), |p| p["frame"] = json!(null))),
        ("frame_string", with(twin(), |p| p["frame"] = json!("3"))),
        (
            "frame_missing",
            with(twin(), |p| drop(p.as_object_mut().unwrap().remove("frame"))),
        ),
        ("unknown_member", with(twin(), |p| p["extra"] = json!(1))),
        (
            "layer_uid_empty",
            with(twin(), |p| p["layerUid"] = json!("")),
        ),
        (
            "command_wrong",
            with(twin(), |p| p["command"] = json!("object.widget.settings")),
        ),
    ];
    assert_refused(|| joystick(None), "write", flaws);
    #[cfg(feature = "evaluation")]
    {
        let mut owner = joystick(None);
        assert_eq!(run(&mut owner, "twin", "write", twin())["ok"], true);
    }
}

#[test]
fn single_flaw_settings_payloads_are_refused_and_their_twin_is_accepted() {
    let twin = || settings_payload(UID, json!({"size":[200,200]}));
    let set = |value: Value| settings_payload(UID, value);
    let flaws = vec![
        ("set_empty", set(json!({}))),
        (
            "set_missing",
            with(twin(), |p| drop(p.as_object_mut().unwrap().remove("set"))),
        ),
        ("x_empty", set(json!({"x":{}}))),
        ("y_empty", set(json!({"y":{}}))),
        ("unknown_set_member", set(json!({"fontSize":1}))),
        ("unknown_axis_member", set(json!({"x":{"ease":1}}))),
        ("size_one", set(json!({"size":[200]}))),
        ("size_three", set(json!({"size":[1, 2, 3]}))),
        ("size_null", set(json!({"size":null}))),
        ("size_strings", set(json!({"size":["a", "b"]}))),
        ("size_huge", set(json!({"size":[1e10, 200]}))),
        ("pos_huge", set(json!({"pos":[0, 1e10]}))),
        ("pos_string", set(json!({"pos":"centre"}))),
        ("min_string", set(json!({"x":{"min":"0"}}))),
        ("max_null", set(json!({"x":{"max":null}}))),
        ("rest_null", set(json!({"x":{"rest":null}}))),
        ("rest_huge", set(json!({"y":{"rest":-1e10}}))),
        ("label_number", set(json!({"x":{"label":5}}))),
        ("label_null", set(json!({"x":{"label":null}}))),
        (
            "command_wrong",
            with(twin(), |p| p["command"] = json!("object.widget.write")),
        ),
        (
            "layer_uid_empty",
            with(twin(), |p| p["layerUid"] = json!("")),
        ),
    ];
    assert_refused(|| joystick(None), "settings", flaws);
    let mut owner = joystick(None);
    assert_eq!(run(&mut owner, "twin", "settings", twin())["ok"], true);
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn a_slider_has_no_y_axis_to_edit_and_a_refused_settings_request_changes_no_axis() {
    let make = || {
        let mut owner = owner_of(&base(1));
        run_ok(
            &mut owner,
            "c",
            "create",
            create_payload("slider", "Slider 1", [960., 540.]),
        );
        owner
    };
    let flaws = vec![
        ("slider_y", settings_payload(UID, json!({"y":{"min":0}}))),
        (
            "slider_x_then_y",
            settings_payload(UID, json!({"x":{"min":5},"y":{"max":1}})),
        ),
    ];
    assert_refused(make, "settings", flaws);
}

#[test]
fn absent_or_foreign_targets_are_not_found_and_publish_nothing() {
    let mut owner = joystick(None);
    run_ok(
        &mut owner,
        "plain",
        "create",
        create_payload("slider", "Slider 2", [10., 10.]),
    );
    let before = state(&owner);
    for (id, layer) in [("absent", "nope"), ("plain-drawing-layer", "L0")] {
        for (op, payload) in [
            (
                "settings",
                settings_payload(layer, json!({"size":[200,200]})),
            ),
            ("write", write_payload(layer, json!(0), local([1., 2.]))),
        ] {
            let reply = run(&mut owner, &format!("{id}-{op}"), op, payload);
            assert_eq!(code(&reply), "not_found", "{id} {op}");
            assert_eq!(state(&owner), before, "{id} {op}");
        }
    }
}

#[test]
fn a_missing_or_non_number_control_refuses_the_whole_write_and_publishes_nothing() {
    let (kx, ky) = (number_control("kx", "Turn"), number_control("ky", "Lift"));
    let angle_y = json!({"key":"ky","name":"Lift","type":"angle","default":[0]});
    let cases = [
        ("y_deleted", json!([kx.clone()])),
        ("x_deleted", json!([ky.clone()])),
        ("both_deleted", json!([])),
        ("y_is_an_angle_control", json!([kx.clone(), angle_y])),
    ];
    for (name, controls) in cases {
        let mut owner = owner_of(&broken_document(controls));
        let before = state(&owner);
        let reply = run(
            &mut owner,
            name,
            "write",
            write_payload("W", json!(0), local([10., -10.])),
        );
        assert_eq!(code(&reply), "unavailable", "{name}");
        assert_eq!(
            state(&owner),
            before,
            "{name}: no partial publication, not even the intact axis"
        );
    }
    #[cfg(feature = "evaluation")]
    {
        let mut owner = owner_of(&broken_document(json!([kx, ky])));
        run_ok(
            &mut owner,
            "twin",
            "write",
            write_payload("W", json!(0), local([10., -10.])),
        );
        assert_eq!(
            wire(&owner)["propertyHolders"][0]["properties"],
            json!([{"property":"kx","static":[12.5]},{"property":"ky","static":[12.5]}])
        );
    }
}
