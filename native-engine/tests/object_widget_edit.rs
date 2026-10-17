//! Widget creation, write-from-local and settings through the actual owner, history and dispatch.
use super::object_widget_edit_rows::*;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::AxisBinding;
use serde_json::{json, Value};

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

fn joystick_layer(name: &str, pos: Value) -> Value {
    json!({"layerUid":UID,"name":name,"visible":true,"locked":false,"color":"#ffb86c",
        "parentLayerUid":null,"timing":timing(),
        "widget":{"kind":"joystick","pos":pos,"size":[160,160],"screenSpace":false,
            "x":{"key":key(UID,"x"),"min":-100,"max":100,"rest":0,"label":"X"},
            "y":{"key":key(UID,"y"),"min":-100,"max":100,"rest":0,"label":"Y"}}})
}

fn joystick_holder() -> Value {
    json!([{"target":{"contextId":"scene-root","layerUid":UID},
        "controls":[{"key":key(UID,"x"),"name":"X","type":"number","default":[0]},
            {"key":key(UID,"y"),"name":"Y","type":"number","default":[0]}],
        "properties":[{"property":key(UID,"x"),"static":[0]},{"property":key(UID,"y"),"static":[0]}]}])
}

#[test]
fn oracle_row_counts_are_pinned() {
    let counts = ["cr", "st", "sm", "wr"].map(|tag| rows(tag).len());
    assert_eq!(counts, [6, 51, 10, 241]);
}

#[test]
fn creation_rows_match_the_executed_baseline_and_unknown_kinds_are_refused() {
    for row in rows("cr") {
        let (name, kind) = (row[1], row[2]);
        let (w, h): (f64, f64) = (row[3].parse().unwrap(), row[4].parse().unwrap());
        let name_json: Value = serde_json::from_str(row[8]).unwrap();
        let mut owner = owner_of(&base(1));
        let mut payload = json!({"command":"object.widget.create","name":name_json,
            "pos":[jnum(w / 2.), jnum(h / 2.)]});
        if kind != "undefined" {
            payload["kind"] = serde_json::from_str(kind).unwrap();
        }
        if kind != "\"joystick\"" && kind != "\"slider\"" {
            // The baseline falls back to a joystick; native refuses and publishes nothing.
            let before = state(&owner);
            let reply = run(&mut owner, name, "create", payload);
            assert_eq!(reply["error"]["code"], "invalid_request", "{name}");
            assert_eq!(state(&owner), before, "{name}");
            continue;
        }
        run_ok(&mut owner, name, "create", payload);
        let after = wire(&owner);
        let created = layer(&after, UID);
        let want = |index: usize| serde_json::from_str::<Value>(row[index]).unwrap();
        assert_eq!(stripped(&created["widget"]), want(5), "{name} widget");
        let (controls, seeds) = controls_and_seeds(&after);
        assert_eq!((controls, seeds), (want(6), want(7)), "{name} controls");
        assert_eq!(
            (&created["name"], &created["color"]),
            (&want(8), &want(9)),
            "{name}"
        );
    }
}

#[test]
fn joystick_creation_is_one_history_entry_whose_undo_removes_layer_controls_and_record_together() {
    let mut owner = owner_of(&base(1));
    let original = wire(&owner);
    let reply = run_ok(
        &mut owner,
        "create",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":UID})
    );
    assert_eq!(key(UID, "x"), "xc_native_layer_1_0_x");
    let mut expected = original.clone();
    expected["layers"]
        .as_array_mut()
        .unwrap()
        .push(joystick_layer("Joystick 1", json!([960, 540])));
    expected["propertyHolders"] = joystick_holder();
    assert_eq!(wire(&owner), expected);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(
        wire(&owner),
        original,
        "layer, controls and record leave together"
    );
    assert!(wire(&owner).get("propertyHolders").is_none());
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(wire(&owner), expected);
}

#[test]
fn slider_creation_declares_exactly_one_control_named_value_with_the_slider_defaults() {
    let mut owner = owner_of(&base(1));
    run_ok(
        &mut owner,
        "create",
        "create",
        create_payload("slider", "Slider 1", [960., 540.]),
    );
    let created = wire(&owner);
    let widget = layer(&created, UID)["widget"].clone();
    assert_eq!(
        widget,
        json!({"kind":"slider","pos":[960,540],"size":[200,26],"screenSpace":false,
        "x":{"key":"xc_native_layer_1_0_value","min":0,"max":100,"rest":0,"label":"Value"}})
    );
    assert_eq!(
        created["propertyHolders"][0]["controls"],
        json!([{"key":"xc_native_layer_1_0_value",
        "name":"Value","type":"number","default":[0]}])
    );
    assert_eq!(
        created["propertyHolders"][0]["properties"],
        json!([{"property":"xc_native_layer_1_0_value","static":[0]}])
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn creation_in_a_v2_document_joins_every_context_and_declares_the_holder_in_scene_root() {
    let mut owner = owner_of(&base(2));
    let original = wire(&owner);
    run_ok(
        &mut owner,
        "create",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    let created = wire(&owner);
    for context in created["compositionOrder"]["contexts"].as_array().unwrap() {
        assert_eq!(
            context["layers"][1],
            json!({"layerUid":UID,"naturalPosition":1,"order":{}})
        );
    }
    assert_eq!(created["propertyHolders"], joystick_holder());
    owner.undo(1).unwrap();
    assert_eq!(wire(&owner), original);
}

#[test]
fn a_second_creation_after_undo_gets_a_fresh_layer_and_control_identity() {
    let mut owner = owner_of(&base(1));
    run_ok(
        &mut owner,
        "one",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    owner.undo(1).unwrap();
    let reply = run_ok(
        &mut owner,
        "two",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    assert_eq!(reply["result"]["layerUid"], "native-layer:3:0");
    let created = wire(&owner);
    assert_eq!(
        created["propertyHolders"][0]["controls"][0]["key"],
        key("native-layer:3:0", "x")
    );
    assert_ne!(key("native-layer:3:0", "x"), key(UID, "x"));
}

#[test]
fn settings_rows_match_the_executed_baseline_panel_and_menu_edits() {
    for row in rows("st") {
        let (name, kind, field, value) = (row[1], row[2], row[3], f(row[4]));
        let (w0, h0) = if kind == "slider" {
            (200., 26.)
        } else {
            (160., 160.)
        };
        let set = match field {
            "w" => json!({"size":[jnum(value), jnum(h0)]}),
            "h" => json!({"size":[jnum(w0), jnum(value)]}),
            "xmin" => json!({"x":{"min":jnum(value)}}),
            "xmax" => json!({"x":{"max":jnum(value)}}),
            "xrest" => json!({"x":{"rest":jnum(value)}}),
            "ymin" => json!({"y":{"min":jnum(value)}}),
            "ymax" => json!({"y":{"max":jnum(value)}}),
            "yrest" => json!({"y":{"rest":jnum(value)}}),
            other => panic!("unknown field {other}"),
        };
        let mut owner = owner_of(&base(1));
        run_ok(
            &mut owner,
            "c",
            "create",
            create_payload(kind, "W", [960., 540.]),
        );
        run_ok(&mut owner, "s", "settings", settings_payload(UID, set));
        let widget = stripped(&layer(&wire(&owner), UID)["widget"]);
        assert_eq!(
            widget,
            serde_json::from_str::<Value>(row[5]).unwrap(),
            "{name}"
        );
    }
    for row in rows("sm") {
        let (name, kind, size) = (row[1], row[2], [jnum(f(row[3])), jnum(f(row[4]))]);
        let mut owner = owner_of(&base(1));
        run_ok(
            &mut owner,
            "c",
            "create",
            create_payload(kind, "W", [960., 540.]),
        );
        run_ok(
            &mut owner,
            "s",
            "settings",
            settings_payload(UID, json!({"size":size})),
        );
        let widget = stripped(&layer(&wire(&owner), UID)["widget"]);
        assert_eq!(
            widget,
            serde_json::from_str::<Value>(row[5]).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn settings_never_touch_control_values_or_the_holder() {
    let mut owner = joystick(None);
    property_edit(
        &mut owner,
        "keep",
        &key(UID, "x"),
        json!({"kind":"setValue","frame":0,"values":[37.5]}),
    );
    let holders = wire(&owner)["propertyHolders"].clone();
    run_ok(
        &mut owner,
        "s",
        "settings",
        settings_payload(
            UID,
            json!({"pos":[10,20.5],
        "size":[3,500],"x":{"min":-1,"max":9,"rest":2,"label":"Turn"},"y":{"min":0,"max":50,"rest":25}}),
        ),
    );
    let after = wire(&owner);
    assert_eq!(after["propertyHolders"], holders);
    assert_eq!(static_of(&after, &key(UID, "x")), 37.5);
    assert_eq!(
        layer(&after, UID)["widget"],
        json!({"kind":"joystick","pos":[10,20.5],"size":[8,500],
        "screenSpace":false,"x":{"key":key(UID,"x"),"min":-1,"max":9,"rest":2,"label":"Turn"},
        "y":{"key":key(UID,"y"),"min":0,"max":50,"rest":25,"label":"Y"}})
    );
}

#[test]
fn a_settings_no_op_preserves_bytes_history_redo_and_authored_spelling() {
    let mut owner = joystick(Some(json!({"size":[200,200]})));
    assert_eq!(owner.history_depths(), (2, 0));
    owner.undo(2).unwrap();
    assert_eq!(owner.history_depths(), (1, 1));
    let before = state(&owner);
    let bytes = encode_storage_project(current(&owner).document()).unwrap();
    // 160.0 is the same JavaScript number as the stored 160: nothing changes, not even the spelling.
    let reply = run_ok(
        &mut owner,
        "noop",
        "settings",
        settings_payload(
            UID,
            json!({"size":[160.0,160.0],"pos":[960.0,540.0],"x":{"min":-100.0,"label":"X"}}),
        ),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), before);
    assert_eq!(
        encode_storage_project(current(&owner).document()).unwrap(),
        bytes
    );
    run_ok(
        &mut owner,
        "real",
        "settings",
        settings_payload(UID, json!({"size":[161,160]})),
    );
    assert_eq!(owner.history_depths(), (2, 0), "a real edit clears redo");
}

#[test]
fn widget_edits_survive_save_and_reload_with_axes_still_bound() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        run_ok(
            &mut owner,
            "c",
            "create",
            create_payload("joystick", "Joystick 1", [960., 540.]),
        );
        run_ok(
            &mut owner,
            "s",
            "settings",
            settings_payload(UID, json!({"size":[200,200],"x":{"rest":25}})),
        );
        let bytes = encode_storage_project(current(&owner).document()).unwrap();
        let reopened = decode_storage_project(&bytes).unwrap();
        assert_eq!(encode_storage_project(&reopened).unwrap(), bytes);
        let bindings = reopened.widget_bindings(UID).unwrap();
        assert_eq!(
            (bindings.x, bindings.y),
            (AxisBinding::Resolved, Some(AxisBinding::Resolved))
        );
        assert!(NativeObjectHistory::new("reopened", reopened).is_ok());
    }
}
