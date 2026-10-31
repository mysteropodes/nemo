//! #2256 Q5: the closed `object.component.instance.set` payload and the `instance` read selector, against the shipped
//! transport schema and the contract table (literal payloads, spelled out independently of the table under test).
use super::*;
use serde_json::{json, Value};

const SET: &str = "command.document.object.component.instance.set";
const READ: &str = "query.document.object.component.read";

fn set(extra: Value) -> Value {
    let mut payload = json!({"command":"object.component.instance.set","layerUid":"layer-a"});
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}

#[test]
fn every_member_is_optional_and_bounded_by_its_kind() {
    for ok in [
        json!({}),
        json!({"mode":"loop"}),
        json!({"mode":"single","singleFrame":0,"atFrame":4_294_967_295u64}),
        json!({"singleFrame":4_294_967_295u64}),
        json!({"placedAt":-2_147_483_648i64}),
        json!({"placedAt":2_147_483_647}),
        json!({"speed":0.1}),
        json!({"speed":8,"mode":"pingpong","placedAt":3}),
    ] {
        assert!(request(SET, &set(ok.clone())), "{ok}");
        assert!(
            crate::native_contract::validate_request(SET, &set(ok.clone())),
            "{ok}"
        );
    }
    for bad in [
        json!({"mode":"bounce"}),
        json!({"mode":"Loop"}),
        json!({"mode":null}),
        json!({"singleFrame":-1}),
        json!({"singleFrame":1.5}),
        json!({"singleFrame":4_294_967_296u64}),
        json!({"singleFrame":"3"}),
        json!({"placedAt":2_147_483_648i64}),
        json!({"placedAt":-2_147_483_649i64}),
        json!({"placedAt":0.5}),
        json!({"speed":0}),
        json!({"speed":-1}),
        json!({"speed":"2"}),
        json!({"speed":null}),
        json!({"atFrame":-1}),
        json!({"symTrimIn":1}),
        json!({"symbolId":"sym"}),
    ] {
        assert!(!request(SET, &set(bad.clone())), "{bad}");
    }
    let mut anonymous = set(json!({}));
    anonymous.as_object_mut().unwrap().remove("layerUid");
    assert!(!request(SET, &anonymous));
    assert!(!request(
        SET,
        &json!({"command":"object.component.place","layerUid":"layer-a"})
    ));
}

#[test]
fn the_shipped_schema_and_the_capability_descriptor_declare_the_same_closed_payload() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let payload = &schema["$defs"]["ObjectComponentInstanceSetPayload"];
    assert_eq!(payload["additionalProperties"], false);
    assert_eq!(payload["required"], json!(["command", "layerUid"]));
    let mut members: Vec<&str> = payload["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    members.sort_unstable();
    let mut table: Vec<&str> = SPECS
        .iter()
        .find(|row| row.0 == "component.instance.set")
        .unwrap()
        .3
        .split(' ')
        .map(|m| m.split(':').next().unwrap())
        .collect();
    table.push("command");
    table.sort_unstable();
    assert_eq!(
        members, table,
        "the schema and the table name the same members"
    );
    let requests = schema["$defs"]["Request"]["oneOf"].as_array().unwrap();
    let branches = requests
        .iter()
        .filter(|b| b["allOf"][1]["properties"]["operation"]["const"] == SET)
        .count();
    assert_eq!(branches, 1);
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-component.json"
    ))
    .unwrap();
    assert!(descriptor["operations"]
        .as_array()
        .unwrap()
        .contains(&json!(SET)));
}

#[test]
fn the_instance_read_selector_names_one_layer_and_one_frame_and_nothing_else() {
    for selector in [
        json!({"kind":"instance","layerUid":"layer-a","frame":0}),
        json!({"kind":"instance","layerUid":"x".repeat(128),"frame":9_007_199_254_740_991u64}),
    ] {
        let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":selector});
        assert!(read::request(&payload), "{payload}");
        assert!(
            crate::native_contract::validate_request(READ, &payload),
            "{payload}"
        );
    }
    for selector in [
        json!({"kind":"instance","layerUid":"layer-a"}),
        json!({"kind":"instance","frame":0}),
        json!({"kind":"instance","layerUid":"","frame":0}),
        json!({"kind":"instance","layerUid":"x".repeat(129),"frame":0}),
        json!({"kind":"instance","layerUid":"layer-a","frame":-1}),
        json!({"kind":"instance","layerUid":"layer-a","frame":1.5}),
        json!({"kind":"instance","layerUid":"layer-a","frame":9_007_199_254_740_992u64}),
        json!({"kind":"instance","layerUid":"layer-a","frame":0,"cursor":0}),
    ] {
        let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":selector});
        assert!(!read::request(&payload), "{payload}");
    }
}

#[test]
fn a_malformed_instance_view_is_not_an_answer() {
    let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":{"kind":"instance","layerUid":"ly_inst","frame":3}});
    let view = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","kind":"instance","layerUid":"ly_inst",
        "componentId":"sym_a","mode":"once","singleFrame":0,"speed":1,"placedAt":0,"totalFrames":24,"resolvedFrame":3,"explicit":false});
    assert!(read::result(&payload, &view));
    let mut blank = view.clone();
    blank["resolvedFrame"] = Value::Null;
    assert!(read::result(&payload, &blank));
    let patch = |key: &str, value: Value| {
        let mut bad = view.clone();
        bad[key] = value;
        bad
    };
    for bad in [
        patch("layerUid", json!("other")),
        patch("componentId", json!("")),
        patch("mode", json!("bounce")),
        patch("singleFrame", json!(-1)),
        patch("speed", json!(0)),
        patch("speed", json!("1")),
        patch("placedAt", json!(null)),
        patch("totalFrames", json!(-1)),
        patch("resolvedFrame", json!(1.5)),
        patch("resolvedFrame", json!(-1)),
        patch("explicit", json!(0)),
        patch("kind", json!("duplicator")),
        patch("atRevision", json!(8)),
    ] {
        assert!(!read::result(&payload, &bad), "{bad}");
    }
    let mut extra = view.clone();
    extra["extra"] = json!(true);
    assert!(!read::result(&payload, &extra));
    let mut missing = view.clone();
    missing.as_object_mut().unwrap().remove("explicit");
    assert!(!read::result(&payload, &missing));
}
