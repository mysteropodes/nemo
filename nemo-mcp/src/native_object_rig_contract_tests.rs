use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

const CORPUS: &str = include_str!("../../tests/native-object-rig-fixtures/public-corpus.json");
pub(super) const P: &str = "command.document.object.";
pub(super) const OPERATIONS: [&str; 6] = [
    "image.mesh.edit",
    "widget.create",
    "widget.write",
    "widget.settings",
    "rig.edit",
    "rig.pose",
];
/// U36 (#2362): the Labs generators share the table; the agreement corpus stays the original six.
pub(super) const LABS: [&str; 8] = [
    "labs.speedLines",
    "labs.boil",
    "labs.followPath",
    "labs.referenceFill",
    "labs.pingpong",
    "labs.retime",
    "labs.interval",
    "labs.moveToLayer",
];
/// D20-D22 (#2358): the four tween commands share the table too; their own shapes are pinned in the tween tests.
pub(super) const TWEEN: [&str; 4] = [
    "tween.generate",
    "tween.regenerate",
    "tween.split",
    "tween.reassign",
];

/// G39 (#2376): the Motion preset apply shares the table; its own shapes are pinned in the preset tests.
pub(super) const PRESET: [&str; 5] = [
    "property.preset.apply",
    "layer.compositing.edit",
    "layer.parent.set",
    "layer.timeLink.set",
    "layer.followPath.set",
];

pub(super) fn corpus() -> Value {
    serde_json::from_str(CORPUS).expect("the agreement corpus parses")
}

pub(super) fn envelope(operation: &str, payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
        "expectedRevision":7,"operation":operation,"payload":payload}))
    .expect("a request envelope")
}

#[test]
fn the_table_declares_exactly_the_twenty_three_operations_and_nothing_else() {
    assert_eq!(table().len(), 23, "an unparsable table admits nothing");
    for &suffix in OPERATIONS.iter().chain(&LABS).chain(&TWEEN).chain(&PRESET) {
        assert!(admitted(&format!("{P}{suffix}")), "{suffix}");
    }
    for other in [
        "command.document.object.layer.create",
        "command.document.object.layer.policy.edit",
        "command.document.object.image.mesh",
        "command.document.object.rig",
        "command.document.object.widget.create ",
        "command.document.object.__proto__",
        "query.document.object",
        "image.mesh.edit",
        "",
    ] {
        assert!(!admitted(other), "{other}");
        assert!(
            !request(other, &json!({"command":"object.rig.pose"})),
            "{other}"
        );
    }
}

#[test]
fn every_valid_payload_of_the_corpus_is_admitted() {
    let corpus = corpus();
    let valid = corpus["valid"].as_array().expect("valid rows");
    assert_eq!(valid.len(), 28);
    for row in valid {
        let operation = row["operation"].as_str().unwrap();
        assert!(
            request(operation, &row["payload"]),
            "{operation}: {}",
            row["payload"]
        );
    }
}

#[test]
fn every_single_flaw_of_the_corpus_is_refused() {
    let corpus = corpus();
    let flaws = corpus["flaws"].as_array().expect("flaw rows");
    assert!(
        flaws.len() >= 700,
        "the corpus is not vacuous: {}",
        flaws.len()
    );
    for row in flaws {
        let operation = row["operation"].as_str().unwrap();
        assert!(
            !request(operation, &row["payload"]),
            "{operation} {} ({}): {}",
            row["path"],
            row["why"],
            row["payload"]
        );
    }
}

#[test]
fn a_payload_of_another_operation_is_refused_by_its_command_member() {
    let corpus = corpus();
    let valid = corpus["valid"].as_array().unwrap();
    for left in valid {
        for right in valid {
            let same = left["payload"]["command"] == right["payload"]["command"];
            let op = left["operation"].as_str().unwrap();
            if !same {
                assert!(
                    !request(op, &right["payload"]),
                    "{op} accepted a foreign payload"
                );
            }
        }
    }
}

#[test]
fn array_bounds_the_corpus_leaves_out_are_pinned_here() {
    let op = format!("{P}image.mesh.edit");
    let outline = |count: usize| {
        let points: Vec<Value> = (0..count).map(|_| json!([0.5, 0.5])).collect();
        json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOutline","outline":points}})
    };
    assert!(request(&op, &outline(512)));
    assert!(!request(&op, &outline(513)));
    assert!(!request(&op, &outline(2)));
    let op = format!("{P}rig.edit");
    let bone = |count: usize| {
        let segments: Vec<Value> = (0..count)
            .map(|_| json!({"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]}))
            .collect();
        json!({"command":"object.rig.edit","layerUid":"layer-a","action":{"kind":"drawBone","segments":segments,"closed":false}})
    };
    assert!(request(&op, &bone(256)));
    assert!(!request(&op, &bone(257)));
    assert!(!request(&op, &bone(1)));
}

#[test]
fn the_outline_perimeter_is_the_owners_bound() {
    let op = format!("{P}image.mesh.edit");
    let outline = |points: Value| json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOutline","outline":points}});
    assert!(request(
        &op,
        &outline(json!([[0, 0], [1, 0], [1, 1], [0, 1]]))
    ));
    assert!(
        request(&op, &outline(json!([[-4, 0], [4, 0], [-4, 0]]))),
        "a perimeter of exactly 16"
    );
    assert!(!request(&op, &outline(json!([[-4, 0], [4, 0], [-4, 0.5]]))));
    assert!(!request(&op, &outline(json!([[-4, -4], [4, -4], [4, 4]]))));
}

#[test]
fn integers_spelled_with_a_zero_fraction_are_integers_and_other_fractions_are_not() {
    let op = format!("{P}rig.pose");
    let parse = |text: &str| serde_json::from_str::<Value>(text).unwrap();
    assert!(request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4.0}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4.5}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4294967296}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":-1}}"#
        )
    ));
}

#[test]
fn identities_are_bounded_in_utf8_bytes() {
    let op = format!("{P}rig.pose");
    let pose =
        |uid: String| json!({"command":"object.rig.pose","layerUid":uid,"action":{"kind":"reset"}});
    assert!(request(&op, &pose("g".repeat(128))));
    assert!(!request(&op, &pose("g".repeat(129))));
    assert!(request(&op, &pose("é".repeat(64))));
    assert!(!request(&op, &pose("é".repeat(65))));
    assert!(!request(&op, &pose(String::new())));
    let write = |uid: String| json!({"command":"object.widget.write","layerUid":uid,"frame":0,"to":{"kind":"rest"}});
    let op = format!("{P}widget.write");
    assert!(
        request(&op, &write("g".repeat(1000))),
        "a widget layer uid is any non-empty string"
    );
    assert!(request(&op, &write("data:opaque authored layer".into())));
    assert!(!request(&op, &write(String::new())));
}

#[test]
fn malformed_or_partial_table_keeps_known_operations_on_the_closed_validator() {
    for source in ["not json", "[]", "null", "{}"] {
        let empty = parse_table(source);
        assert!(empty.is_empty());
        for row in corpus()["valid"].as_array().unwrap() {
            let op = row["operation"].as_str().unwrap();
            assert!(
                admitted(op),
                "known family must not fall through generic admission"
            );
            assert!(!request_with_table(&empty, op, &row["payload"]));
        }
    }
    let mut partial = parse_table(SPEC);
    partial.remove("rig.pose");
    let op = "command.document.object.rig.pose";
    let cases = corpus();
    let pose = cases["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == op)
        .unwrap();
    assert!(admitted(op));
    assert!(request(op, &pose["payload"]));
    assert!(!request_with_table(&partial, op, &pose["payload"]));
}

// Binding agreement is additive: the original 28-row corpus and its flaw count stay unchanged.
fn binding_target() -> Value {
    json!({"contextId":"component-A","layerUid":"inner","strokeId":"pixels",
        "frameScope":{"kind":"authored","frame":23}})
}
fn binding_mesh() -> Value {
    json!({"outline":[[0,0],[1,0],[0,1]],"verts":[[0,0],[1,0],[0,1]],
        "tris":[0,1,2],"offsets":[[0,0],[0,0],[0,0]],"cols":1,"rows":64})
}
fn binding_action(action: Value) -> Value {
    json!({"command":"object.image.mesh.edit","action":action})
}
fn binding_attach() -> Value {
    binding_action(json!({"kind":"attach","target":binding_target(),"mesh":binding_mesh()}))
}

#[test]
fn binding_actions_have_closed_public_shapes_and_real_original_byte_admission() {
    let operation = format!("{P}image.mesh.edit");
    let mut reference = binding_target();
    reference["frameScope"] = json!({"kind":"reference","frame":4294967295u64});
    let mut valid = vec![
        binding_attach(),
        binding_action(json!({"kind":"detach","target":reference})),
        binding_action(json!({"kind":"duplicate","contextId":"component-A","layerUid":"inner"})),
        binding_action(json!({"kind":"releaseIfUnused","meshId":"im_1"})),
        binding_action(json!({"kind":"releaseIfUnused","meshId":""})),
    ];
    for mesh_id in [json!("im_1"), json!(""), Value::Null] {
        valid.push(binding_action(
            json!({"kind":"propagate","target":binding_target(),"meshId":mesh_id}),
        ));
    }
    // Shape admission must not pretend to perform the native owner's topology validation.
    let mut owner_invalid = binding_attach();
    owner_invalid["action"]["mesh"]["tris"] = json!([999]);
    valid.push(owner_invalid);
    let mut empty_arrays = binding_attach();
    empty_arrays["action"]["mesh"] = json!({"outline":[],"verts":[],"tris":[]});
    valid.push(empty_arrays);
    for payload in valid {
        assert!(request(&operation, &payload), "{payload}");
        let command = envelope(&operation, payload);
        assert!(command.validate_input().is_ok());
        assert!(command.validate().is_ok());
        let raw = serde_json::to_vec(&command).unwrap();
        assert!(raw.len() <= 4096);
        assert!(crate::wire::decode_native_request(&raw).is_ok());
    }
}

#[test]
fn binding_single_faults_are_refused_without_widening_legacy_or_video_actions() {
    let operation = format!("{P}image.mesh.edit");
    let mut faults = Vec::new();
    for pointer in [
        "/command",
        "/action",
        "/action/kind",
        "/action/target",
        "/action/target/contextId",
        "/action/target/layerUid",
        "/action/target/strokeId",
        "/action/target/frameScope",
        "/action/target/frameScope/kind",
        "/action/target/frameScope/frame",
        "/action/mesh",
        "/action/mesh/outline",
        "/action/mesh/verts",
        "/action/mesh/tris",
    ] {
        let mut payload = binding_attach();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        payload
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        faults.push(payload);
    }
    for (pointer, value) in [
        ("/command", json!("object.rig.edit")),
        ("/action", Value::Null),
        ("/action/kind", json!("attachVideo")),
        ("/action/kind", json!("detachVideo")),
        ("/action/target", json!([])),
        ("/action/target/contextId", json!("")),
        ("/action/target/layerUid", json!("")),
        ("/action/target/strokeId", json!("")),
        ("/action/target/frameScope/kind", json!("unknown")),
        ("/action/target/frameScope/frame", json!(-1)),
        ("/action/target/frameScope/frame", json!(0.5)),
        ("/action/target/frameScope/frame", json!(4294967296u64)),
        ("/action/mesh/verts", json!([[0]])),
        ("/action/mesh/outline", json!([[0, 1, 2]])),
        ("/action/mesh/tris", json!([-1])),
        ("/action/mesh/tris", json!([0.5])),
        ("/action/mesh/tris", json!([4294967296u64])),
        ("/action/mesh/offsets", Value::Null),
        ("/action/mesh/cols", json!(0)),
        ("/action/mesh/cols", json!(65)),
        ("/action/mesh/rows", json!(1.5)),
    ] {
        let mut payload = binding_attach();
        *payload.pointer_mut(pointer).unwrap() = value;
        faults.push(payload);
    }
    for pointer in [
        "",
        "/action",
        "/action/target",
        "/action/target/frameScope",
        "/action/mesh",
    ] {
        let mut payload = binding_attach();
        payload
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        faults.push(payload);
    }
    let mut outer_id = binding_attach();
    outer_id["meshId"] = json!("im_1");
    faults.push(outer_id);
    faults.push(binding_action(
        json!({"kind":"propagate","target":binding_target()}),
    ));
    faults.push(binding_action(
        json!({"kind":"duplicate","contextId":"","layerUid":"inner"}),
    ));
    faults.push(json!({"command":"object.image.mesh.edit","action":{"kind":"resetOffsets"}}));
    for payload in faults {
        assert!(!request(&operation, &payload), "{payload}");
        let command = envelope(&operation, payload);
        assert!(command.validate_input().is_err());
        assert!(
            crate::wire::decode_native_request(&serde_json::to_vec(&command).unwrap()).is_err()
        );
    }
}

#[test]
fn binding_duplicate_members_are_refused_from_original_bytes_at_every_new_object_level() {
    let operation = format!("{P}image.mesh.edit");
    let raw = serde_json::to_string(&envelope(&operation, binding_attach())).unwrap();
    for (needle, replacement) in [
        (
            "\"requestId\":",
            "\"requestId\":\"duplicate\",\"requestId\":",
        ),
        ("\"command\":", "\"command\":\"duplicate\",\"command\":"),
        (
            "\"kind\":\"attach\"",
            "\"kind\":\"attach\",\"kind\":\"attach\"",
        ),
        (
            "\"contextId\":",
            "\"contextId\":\"duplicate\",\"contextId\":",
        ),
        ("\"cols\":", "\"cols\":1,\"cols\":"),
    ] {
        assert!(raw.contains(needle));
        let duplicate = raw.replacen(needle, replacement, 1);
        assert!(
            crate::wire::decode_native_request(duplicate.as_bytes()).is_err(),
            "{duplicate}"
        );
    }
}

/// G39 (#2376): the preset payload is closed; the engine repeats every rule on original bytes.
#[test]
fn the_motion_preset_payload_is_closed_and_bounded() {
    let op = format!("{P}property.preset.apply");
    let target = json!({"contextId":"scene-root","layerUid":"L"});
    let ok = |properties: Value| {
        let request = envelope(
            &op,
            json!({"command":"object.property.preset.apply","target":target,"properties":properties}),
        );
        crate::native_contract::validate_request(&request.operation, &request.payload)
    };
    let key = json!({"frame":0,"v":[1,2],"hold":true,"curvePoints":[{"x":0,"y":0},{"x":1,"y":1,"tx":0.5}],"hIn":[0,0],"hOut":[1,1]});
    assert!(ok(json!([{"property":"position","keys":[key]},{"property":"opacity","keys":[],"static":[75]}])));
    assert!(!ok(json!([])), "at least one property");
    assert!(!ok(json!([{"property":"order","keys":[]}])), "another owner");
    assert!(!ok(json!([{"property":"position"}])), "keys are required");
    assert!(!ok(json!([{"property":"position","keys":[{"frame":0,"v":[]}]}])));
    assert!(!ok(json!([{"property":"position","keys":[{"frame":0,"v":[1],"x":1}]}])));
    assert!(!ok(json!([{"property":"opacity","keys":[],"static":[1,2,3,4,5]}])));
    assert!(!ok(json!([{"property":"opacity","keys":[{"frame":0,"v":[1],"lockTo":"up"}]}])));
}

/// #2372: the layer compositing edit is one closed shape per edit kind; the engine repeats every rule on original bytes.
#[test]
fn the_layer_compositing_edit_is_a_closed_union_by_edit() {
    let op = format!("{P}layer.compositing.edit");
    let ok = |extra: Value| {
        let mut payload = json!({"command":"object.layer.compositing.edit","layerUid":"L"});
        for (key, member) in extra.as_object().unwrap() {
            payload[key] = member.clone();
        }
        crate::native_contract::validate_request(&op, &payload)
    };
    assert!(ok(json!({"edit":"blend","mode":"multiply"})));
    assert!(ok(json!({"edit":"matteMode","mode":"luma","source":"S"})));
    assert!(ok(json!({"edit":"matteClear"})));
    assert!(ok(json!({"edit":"matteMoreAdd","source":"S"})));
    assert!(ok(json!({"edit":"matteMoreSet","index":0,"mode":"alpha"})));
    assert!(ok(json!({"edit":"folderCollapsed","collapsed":true})));
    let blur = json!({"type":"blur","enabled":true,"p1":4,"p2":null,"keys":{"p1":{"keys":[]}}});
    assert!(ok(json!({"edit":"effects","effects":[blur, {"type":"vignette","enabled":false}]})));
    assert!(ok(json!({"edit":"effects","effects":[]})));
    assert!(!ok(json!({"edit":"blend","mode":"Multiply"})), "unknown blend");
    assert!(!ok(json!({"edit":"blend"})), "mode is required");
    assert!(!ok(json!({"edit":"matteMode","mode":"none"})), "none is matteClear");
    assert!(!ok(json!({"edit":"blend","mode":"screen","source":"S"})), "closed members");
    assert!(!ok(json!({"edit":"matteClear","mode":"alpha"})));
    assert!(!ok(json!({"edit":"matteMoreRemove","index":-1})));
    assert!(!ok(json!({"edit":"effects"})), "the stack is required");
    assert!(!ok(json!({"edit":"effects","effects":[{"type":"blur"}]})));
    assert!(!ok(json!({"edit":"effects","effects":[{"type":"blur","enabled":true,"p9":1}]})));
    assert!(!ok(json!({"edit":"effects","effects":[{"type":"blur","enabled":true,"keys":[]}]})));
    assert!(!ok(json!({"edit":"effects","effects":[{"type":"blur","enabled":true,"p1":"4"}]})));
    assert!(!ok(json!({"edit":"unknown"})));
    assert!(!ok(json!({"layerUid":"L"})), "edit is required");
    let wide = json!({"type":"blur","enabled":true,"keys":{"p1":{"keys":"x".repeat(2100)}}});
    assert!(!ok(json!({"edit":"effects","effects":[wide]})), "key tracks are bounded");
}

/// #2372 U12: the Parent A write is one closed shape; the engine repeats every rule on original bytes.
#[test]
fn the_layer_parent_set_names_one_layer_one_parent_or_null_and_one_frame() {
    let op = format!("{P}layer.parent.set");
    let base = || json!({"command":"object.layer.parent.set","contextId":"scene-root","layerUid":"L","parentLayerUid":"P","frame":3});
    let ok = |edit: &dyn Fn(&mut Value)| {
        let mut payload = base();
        edit(&mut payload);
        crate::native_contract::validate_request(&op, &payload)
    };
    assert!(ok(&|_| {}));
    assert!(ok(&|p| p["parentLayerUid"] = Value::Null), "Parent A : None");
    assert!(ok(&|p| p["frame"] = json!(4294967295u64)));
    assert!(!ok(&|p| p["parentLayerUid"] = json!("")), "an empty uid is not a parent");
    assert!(!ok(&|p| p["parentLayerUid"] = json!(7)));
    assert!(!ok(&|p| { p.as_object_mut().unwrap().remove("parentLayerUid"); }), "null is explicit");
    assert!(!ok(&|p| { p.as_object_mut().unwrap().remove("contextId"); }));
    assert!(!ok(&|p| p["frame"] = json!(-1)));
    assert!(!ok(&|p| p["frame"] = json!(1.5)));
    assert!(!ok(&|p| p["extra"] = json!(1)), "closed members");
    assert!(!ok(&|p| p["layerUid"] = json!("x".repeat(129))));
    assert!(!ok(&|p| p["command"] = json!("object.layer.rename")));
}

/// #2372: Link time and Follow path are closed shapes; the engine repeats every rule on original bytes.
#[test]
fn the_layer_time_link_and_follow_path_writes_are_closed_shapes() {
    let ok = |op: &str, payload: Value| {
        crate::native_contract::validate_request(&format!("{P}{op}"), &payload)
    };
    let link = || json!({"command":"object.layer.timeLink.set","contextId":"scene-root","layerUid":"L","sourceLayerUid":"S","frame":3});
    let with = |key: &str, value: Value| {
        let mut payload = link();
        payload[key] = value;
        payload
    };
    assert!(ok("layer.timeLink.set", link()));
    assert!(ok("layer.timeLink.set", with("sourceLayerUid", Value::Null)), "Time: None");
    assert!(ok("layer.timeLink.set", with("mode", json!("in"))));
    assert!(ok("layer.timeLink.set", with("srcAnchor", json!("out"))));
    assert!(!ok("layer.timeLink.set", with("mode", json!("sideways"))));
    assert!(!ok("layer.timeLink.set", with("srcAnchor", json!("both"))));
    assert!(!ok("layer.timeLink.set", with("sourceLayerUid", json!(""))), "an empty uid is not a source");
    assert!(!ok("layer.timeLink.set", with("frame", json!(-1))));
    assert!(!ok("layer.timeLink.set", with("extra", json!(1))), "closed members");
    assert!(!ok("layer.timeLink.set", with("command", json!("object.layer.parent.set"))));
    let mut absent = link();
    absent.as_object_mut().unwrap().remove("sourceLayerUid");
    assert!(!ok("layer.timeLink.set", absent), "null is explicit");
    let follow = || json!({"command":"object.layer.followPath.set","contextId":"scene-root","layerUid":"L","targetLayerUid":"P"});
    assert!(ok("layer.followPath.set", follow()));
    let mut none = follow();
    none["targetLayerUid"] = Value::Null;
    assert!(ok("layer.followPath.set", none), "Chemin : Aucun");
    let mut empty = follow();
    empty["targetLayerUid"] = json!("");
    assert!(!ok("layer.followPath.set", empty));
    let mut extra = follow();
    extra["frame"] = json!(0);
    assert!(!ok("layer.followPath.set", extra), "no frame member");
    let mut missing = follow();
    missing.as_object_mut().unwrap().remove("targetLayerUid");
    assert!(!ok("layer.followPath.set", missing));
}
