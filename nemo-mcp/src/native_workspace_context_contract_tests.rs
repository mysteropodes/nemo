use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{decode_native_request, validate_native_response};
use serde_json::json;

const SNAPSHOT: &str = "native-object:document:7";

fn view(frame: u64, lo: u64, hi: u64) -> Value {
    json!({"frame":frame,"workIn":lo,"workOut":hi})
}
fn scene(tabs: Value) -> Value {
    json!({"activeContextId":null,"openTabs":tabs,"view":view(5, 0, 23),"sceneView":null})
}
fn inside(tabs: Value) -> Value {
    json!({"activeContextId":"sym_a","openTabs":tabs,"view":view(0, 0, 11),"sceneView":view(5, 0, 23)})
}
fn enter() -> Value {
    json!({"kind":"enter","contextId":"sym_a","entryFrame":null})
}
fn payload(workspace: Value, action: Value) -> Value {
    json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"workspace":workspace,"action":action})
}
fn envelope(workspace: Value, action: Value) -> Value {
    json!({"apiVersion":2,"requestId":"workspace-context","instanceId":"instance","documentId":"document",
        "operation":OP,"payload":payload(workspace, action)})
}
fn answer(result: Value) -> Value {
    json!({"apiVersion":2,"requestId":"workspace-context","instanceId":"instance","documentId":"document",
        "contentRevision":7,"ok":true,"result":result})
}
fn entered() -> Value {
    json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"outcome":"entered","contextId":"sym_a",
        "totalFrames":12,"fps":24.0,"workspace":inside(json!(["sym_a"]))})
}
fn exited() -> Value {
    json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"outcome":"exited","contextId":"scene-root",
        "totalFrames":24,"fps":24.0,"workspace":scene(json!(["sym_a"]))})
}

#[test]
fn published_schema_declares_the_forbidden_revision_request_and_closed_shapes() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let requests = schema["$defs"]["Request"]["oneOf"].as_array().unwrap();
    let matching: Vec<_> = requests
        .iter()
        .filter(|branch| branch["allOf"][1]["properties"]["operation"]["const"] == OP)
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0]["allOf"][0],
        json!({"$ref":"#/$defs/ForbiddenRevisionRequest"})
    );
    let mut resources = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let document: Value = serde_json::from_str(text).unwrap();
        let id = document["$id"].as_str().unwrap().to_owned();
        resources = resources
            .add(id, jsonschema::Resource::from_contents(document))
            .unwrap();
    }
    let registry = resources.prepare().unwrap();
    let validator = jsonschema::draft202012::options()
        .with_registry(&registry)
        .build(&schema)
        .unwrap();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert!(branches.contains(&json!({"$ref":"#/$defs/ObjectWorkspaceContextResult"})));
    for action in [
        enter(),
        json!({"kind":"enter","contextId":"sym_a","entryFrame":3}),
        json!({"kind":"exit"}),
        json!({"kind":"close","contextId":"sym_a"}),
        json!({"kind":"refresh"}),
    ] {
        assert!(
            validator.is_valid(&envelope(scene(json!([])), action.clone())),
            "{action}"
        );
    }
    assert!(validator.is_valid(&envelope(inside(json!(["sym_a"])), json!({"kind":"exit"}))));
    assert!(validator.is_valid(&answer(entered())));
    assert!(validator.is_valid(&answer(exited())));
    let mut revisioned = envelope(scene(json!([])), enter());
    revisioned["expectedRevision"] = json!(7);
    assert!(!validator.is_valid(&revisioned));
    for action in [
        json!({"kind":"enter","contextId":"","entryFrame":null}),
        json!({"kind":"enter","contextId":"a"}),
        json!({"kind":"exit","contextId":"a"}),
        json!({"kind":"teleport"}),
    ] {
        assert!(
            !validator.is_valid(&envelope(scene(json!([])), action.clone())),
            "{action}"
        );
    }
    let nine: Vec<String> = (0..9).map(|n| format!("t{n}")).collect();
    assert!(!validator.is_valid(&envelope(scene(json!(nine)), enter())));
    assert!(!validator.is_valid(&envelope(scene(json!(["a", "a"])), enter())));
    let mut bad = answer(entered());
    bad["result"]["outcome"] = json!("teleported");
    assert!(!validator.is_valid(&bad));
    let mut bad = answer(entered());
    bad["result"]["fps"] = json!(0);
    assert!(!validator.is_valid(&bad));
    let mut bad = answer(entered());
    bad["result"]["extra"] = json!(true);
    assert!(!validator.is_valid(&bad));
}

#[test]
fn the_descriptor_example_is_a_valid_request_and_answer() {
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-workspace-context.json"
    ))
    .unwrap();
    assert_eq!(descriptor["operations"], json!([OP]));
    let example = &descriptor["examples"][0];
    assert!(request(&example["request"]["payload"]));
    assert!(result(
        &example["request"]["payload"],
        &example["response"]["result"]
    ));
    let r: NativeApplicationRequest = serde_json::from_value(example["request"].clone()).unwrap();
    let a: NativeApplicationResponse = serde_json::from_value(example["response"].clone()).unwrap();
    r.validate().unwrap();
    validate_native_response(&a, &r).unwrap();
}

#[test]
fn request_is_exact_pinned_and_bounded() {
    for (workspace, action) in [
        (scene(json!([])), enter()),
        (
            scene(json!(["sym_a"])),
            json!({"kind":"enter","contextId":"sym_a","entryFrame":4294967295u64}),
        ),
        (inside(json!(["sym_a"])), json!({"kind":"exit"})),
        (
            inside(json!(["sym_b", "sym_a"])),
            json!({"kind":"close","contextId":"sym_b"}),
        ),
        (scene(json!([])), json!({"kind":"refresh"})),
    ] {
        assert!(
            request(&payload(workspace.clone(), action.clone())),
            "{workspace} {action}"
        );
    }
    let nine: Vec<String> = (0..9).map(|n| format!("t{n}")).collect();
    let mut no_scene_view = inside(json!(["sym_a"]));
    no_scene_view["sceneView"] = Value::Null;
    let mut stray_scene_view = scene(json!([]));
    stray_scene_view["sceneView"] = view(1, 0, 2);
    let mut reversed = scene(json!([]));
    reversed["view"] = view(1, 9, 3);
    let mut extra = scene(json!([]));
    extra["extra"] = json!(1);
    for workspace in [
        scene(json!(nine)),
        scene(json!(["a", "a"])),
        scene(json!([""])),
        scene(json!([7])),
        inside(json!(["other"])),
        no_scene_view,
        stray_scene_view,
        reversed,
        extra,
        json!({"activeContextId":"x".repeat(129),"openTabs":[],"view":view(0, 0, 1),"sceneView":view(0, 0, 1)}),
        json!({"activeContextId":null,"openTabs":[],"view":view(4294967296, 0, 1),"sceneView":null}),
        Value::Null,
    ] {
        assert!(
            !request(&payload(workspace.clone(), enter())),
            "{workspace}"
        );
    }
    for action in [
        json!({"kind":"enter","contextId":"sym_a","entryFrame":-1}),
        json!({"kind":"enter","contextId":"sym_a"}),
        json!({"kind":"close"}),
        json!({"kind":"exit","contextId":"sym_a"}),
        json!({"kind":"refresh","x":1}),
        json!({"kind":"other"}),
        json!("exit"),
        Value::Null,
    ] {
        assert!(
            !request(&payload(scene(json!([])), action.clone())),
            "{action}"
        );
    }
    for bad in [
        Value::Null,
        json!({}),
        json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"workspace":scene(json!([]))}),
        json!({"atRevision":-1,"documentSnapshotId":SNAPSHOT,"workspace":scene(json!([])),"action":enter()}),
        json!({"atRevision":7,"documentSnapshotId":"","workspace":scene(json!([])),"action":enter()}),
        json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"workspace":scene(json!([])),"action":enter(),"extra":1}),
    ] {
        assert!(!request(&bad), "{bad}");
    }
}

#[test]
fn result_answers_its_own_request_pin_and_presents_the_active_context() {
    let to_enter = payload(scene(json!([])), enter());
    assert!(result(&to_enter, &entered()));
    let to_exit = payload(inside(json!(["sym_a"])), json!({"kind":"exit"}));
    assert!(result(&to_exit, &exited()));
    for (key, value) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("foreign")),
        ("outcome", json!("teleported")),
        ("contextId", json!("scene-root")),
        ("totalFrames", json!(0)),
        ("fps", json!(0)),
        ("fps", json!("24")),
        ("extra", json!(true)),
    ] {
        let mut bad = entered();
        bad[key] = value;
        assert!(!result(&to_enter, &bad), "{key}");
    }
    // The scene is presented exactly when no component is active.
    let mut bad = exited();
    bad["contextId"] = json!("sym_a");
    assert!(!result(&to_exit, &bad));
    let mut bad = entered();
    bad["workspace"]["sceneView"] = Value::Null;
    assert!(
        !result(&to_enter, &bad),
        "an active context saves the scene view"
    );
    let mut bad = entered();
    bad["workspace"]["openTabs"] = json!([]);
    assert!(
        !result(&to_enter, &bad),
        "the active context is an open tab"
    );
    assert!(!result(&Value::Null, &entered()));
}

#[test]
fn registered_public_input_forbids_expected_revision_and_duplicates() {
    let r: NativeApplicationRequest =
        decode_native_request(envelope(scene(json!([])), enter()).to_string().as_bytes()).unwrap();
    r.validate().unwrap();
    assert!(crate::native_contract::forbids_expected_revision(OP));
    assert!(crate::native_contract::validate_request(
        OP,
        &payload(scene(json!([])), enter())
    ));
    let mut bad = envelope(scene(json!([])), enter());
    bad["expectedRevision"] = json!(7);
    assert!(decode_native_request(bad.to_string().as_bytes())
        .and_then(|r| r
            .validate()
            .map_err(|e| std::io::Error::other(e.to_string())))
        .is_err());
    let mut bad = envelope(scene(json!([])), enter());
    bad["payload"]["action"]["kind"] = json!("teleport");
    assert!(!crate::native_contract::validate_request(
        OP,
        &bad["payload"]
    ));
    let dup = envelope(scene(json!([])), enter()).to_string().replacen(
        "\"atRevision\":7",
        "\"atRevision\":7,\"atRevision\":7",
        1,
    );
    assert!(decode_native_request(dup.as_bytes()).is_err());
}

#[test]
fn wire_result_rejects_forged_owner_revision_and_shape() {
    let r: NativeApplicationRequest =
        serde_json::from_value(envelope(scene(json!([])), enter())).unwrap();
    let good: NativeApplicationResponse = serde_json::from_value(answer(entered())).unwrap();
    validate_native_response(&good, &r).unwrap();
    for (key, value) in [
        ("requestId", json!("foreign")),
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(8)),
    ] {
        let mut bad = answer(entered());
        bad[key] = value;
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("foreign")),
        ("extra", json!(true)),
        ("workspace", json!("none")),
    ] {
        let mut bad = answer(entered());
        bad["result"][key] = value;
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err(),
            "{key}"
        );
    }
    let refusal = json!({"apiVersion":2,"requestId":"workspace-context","instanceId":"instance",
        "documentId":"document","contentRevision":7,"ok":false,
        "error":{"code":"not_found","message":"no such context"}});
    validate_native_response(&serde_json::from_value(refusal).unwrap(), &r).unwrap();
    let replaced = json!({"apiVersion":2,"requestId":"workspace-context","instanceId":"instance",
        "documentId":"other","contentRevision":7,"ok":false,
        "error":{"code":"wrong_document","message":"replaced"}});
    assert!(response_matches(
        &r,
        &serde_json::from_value(replaced).unwrap()
    ));
    let foreign = json!({"apiVersion":2,"requestId":"workspace-context","instanceId":"instance",
        "documentId":"other","contentRevision":7,"ok":false,
        "error":{"code":"not_found","message":"x"}});
    assert!(!response_matches(
        &r,
        &serde_json::from_value(foreign).unwrap()
    ));
}
